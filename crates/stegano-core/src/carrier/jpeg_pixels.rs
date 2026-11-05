//! Pixel decode and encode for JPEG resize and thumbnails.
//!
//! The embedding path never comes through here: it edits DCT coefficients in
//! place. This module decodes to samples with libjpeg (optionally DCT-scaled),
//! and writes a new baseline file with mozjpeg.

use std::ffi::{c_int, c_uint, c_ulong};
use std::mem;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;
use std::sync::atomic::AtomicBool;

use mozjpeg_sys::*;

use super::jpeg::{new_error_mgr, panic_message};
use crate::error::{Error, Result};

pub(crate) struct DecodedJpeg {
    pub width: u32,
    pub height: u32,
    /// 1 for greyscale, 3 for RGB.
    pub components: usize,
    pub pixels: Vec<u8>,
    pub icc: Vec<u8>,
    /// EXIF orientation 1–8. Missing or unreadable tags are 1.
    pub orientation: u8,
}

/// Decode to 8-bit samples.
///
/// `max_side` selects libjpeg's DCT scale (`1/1`, `1/2`, `1/4`, `1/8`) so a
/// large photo is not expanded to full resolution. `None` decodes 1:1.
pub(crate) fn decode_jpeg(
    data: &[u8],
    max_side: Option<u32>,
    cancel: Option<&AtomicBool>,
) -> Result<DecodedJpeg> {
    let mut err = new_error_mgr();
    let mut d: Box<jpeg_decompress_struct> = Box::new(unsafe { mem::zeroed() });
    d.common.err = &mut *err;
    let result = catch_unwind(AssertUnwindSafe(|| unsafe {
        jpeg_create_decompress(&mut *d);
        jpeg_mem_src(&mut d, data.as_ptr(), data.len() as c_ulong);
        jpeg_save_markers(&mut d, jpeg_marker::APP0 as c_int + 1, 0xFFFF);
        jpeg_save_markers(&mut d, jpeg_marker::APP0 as c_int + 2, 0xFFFF);
        jpeg_read_header(&mut d, 1);
        let (app1, app2) = copy_markers(&d);
        let orientation = exif_orientation(&app1);
        let icc = assemble_icc(&app2);
        match d.jpeg_color_space {
            JCS_CMYK | JCS_YCCK => {
                return Err(Error::UnsuitableCarrier(
                    "CMYK JPEG is not supported; convert it to RGB first".into(),
                ));
            }
            JCS_GRAYSCALE => d.out_color_space = JCS_GRAYSCALE,
            _ => d.out_color_space = JCS_RGB,
        }
        let (num, denom) = match max_side {
            Some(side) => (1, dct_denom(d.image_width, d.image_height, side)),
            None => (1, 1),
        };
        d.scale_num = num;
        d.scale_denom = denom;
        if jpeg_start_decompress(&mut d) == 0 {
            return Err(Error::Decode("could not start JPEG decode".into()));
        }
        let width = d.output_width;
        let height = d.output_height;
        let components = d.output_components as usize;
        if width == 0 || height == 0 || (components != 1 && components != 3) {
            return Err(Error::Decode(
                "JPEG decoded to an unsupported pixel layout".into(),
            ));
        }
        let row_bytes = width as usize * components;
        let mut pixels = vec![0u8; row_bytes * height as usize];
        while d.output_scanline < d.output_height {
            if d.output_scanline.is_multiple_of(32)
                && cancel.is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed))
            {
                return Err(Error::Cancelled);
            }
            let y = d.output_scanline as usize;
            let mut row = pixels[y * row_bytes..].as_mut_ptr();
            let n = jpeg_read_scanlines(&mut d, &mut row, 1);
            if n != 1 {
                return Err(Error::Decode("truncated JPEG scan".into()));
            }
        }
        if jpeg_finish_decompress(&mut d) == 0 {
            return Err(Error::Decode("could not finish JPEG decode".into()));
        }
        Ok(DecodedJpeg {
            width,
            height,
            components,
            pixels,
            icc,
            orientation,
        })
    }));
    unsafe { jpeg_destroy_decompress(&mut d) };
    match result {
        Ok(inner) => inner,
        Err(panic) => Err(Error::Decode(panic_message(panic))),
    }
}

/// Encode 8-bit greyscale or RGB. Quality is the libjpeg scale; RGB is 4:2:0
/// when `subsample_420` is set. `icc` is written as APP2 and nothing else
/// (no EXIF).
pub(crate) fn encode_jpeg(
    width: u32,
    height: u32,
    components: usize,
    pixels: &[u8],
    icc: &[u8],
    quality: u8,
    subsample_420: bool,
) -> Result<Vec<u8>> {
    if width == 0 || height == 0 || (components != 1 && components != 3) {
        return Err(Error::Encode("unsupported JPEG pixel layout".into()));
    }
    let row_bytes = width as usize * components;
    if pixels.len() != row_bytes * height as usize {
        return Err(Error::Encode("pixel buffer size mismatch".into()));
    }

    let mut err = new_error_mgr();
    let mut c: Box<jpeg_compress_struct> = Box::new(unsafe { mem::zeroed() });
    c.common.err = &mut *err;
    let mut out_ptr: *mut u8 = ptr::null_mut();
    let mut out_len: c_ulong = 0;
    let result = catch_unwind(AssertUnwindSafe(|| unsafe {
        jpeg_create_compress(&mut *c);
        jpeg_mem_dest(&mut c, &mut out_ptr, &mut out_len);
        c.image_width = width;
        c.image_height = height;
        c.input_components = components as c_int;
        c.in_color_space = if components == 1 {
            JCS_GRAYSCALE
        } else {
            JCS_RGB
        };
        jpeg_set_defaults(&mut c);
        jpeg_set_quality(&mut c, quality as c_int, 1);
        if components == 3 && subsample_420 {
            let info = c.comp_info;
            (*info).h_samp_factor = 2;
            (*info).v_samp_factor = 2;
            (*info.add(1)).h_samp_factor = 1;
            (*info.add(1)).v_samp_factor = 1;
            (*info.add(2)).h_samp_factor = 1;
            (*info.add(2)).v_samp_factor = 1;
        }
        jpeg_start_compress(&mut c, 1);
        write_icc(&mut c, icc);
        for y in 0..height as usize {
            let row_ptr: JSAMPROW = pixels[y * row_bytes..].as_ptr();
            if jpeg_write_scanlines(&mut c, &row_ptr, 1) != 1 {
                return Err(Error::Encode("could not write JPEG scan".into()));
            }
        }
        jpeg_finish_compress(&mut c);
        Ok(())
    }));
    unsafe { jpeg_destroy_compress(&mut c) };
    let bytes = if out_ptr.is_null() {
        Vec::new()
    } else {
        let v = unsafe { std::slice::from_raw_parts(out_ptr, out_len as usize).to_vec() };
        unsafe { libc::free(out_ptr.cast()) };
        v
    };
    match result {
        Ok(Ok(())) => Ok(bytes),
        Ok(Err(e)) => Err(e),
        Err(panic) => Err(Error::Encode(panic_message(panic))),
    }
}

/// Largest `1/d` libjpeg scale (`d` in 1, 2, 4, 8) that still covers `max_side`.
fn dct_denom(width: u32, height: u32, max_side: u32) -> u32 {
    let long = width.max(height);
    if max_side == 0 || long <= max_side {
        return 1;
    }
    let needed = max_side as f32 / long as f32;
    let mut chosen = 1u32;
    for denom in [1u32, 2, 4, 8] {
        if 1.0 / denom as f32 >= needed {
            chosen = denom;
        }
    }
    chosen
}

unsafe fn copy_markers(d: &jpeg_decompress_struct) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
    let mut app1 = Vec::new();
    let mut app2 = Vec::new();
    unsafe {
        let mut marker = d.marker_list;
        while !marker.is_null() {
            let m = &*marker;
            let data = std::slice::from_raw_parts(m.data, m.data_length as usize).to_vec();
            match m.marker {
                0xE1 => app1.push(data),
                0xE2 => app2.push(data),
                _ => {}
            }
            marker = m.next;
        }
    }
    (app1, app2)
}

fn exif_orientation(app1: &[Vec<u8>]) -> u8 {
    app1.iter()
        .find_map(|seg| orientation_in_exif(seg))
        .filter(|v| (1..=8).contains(v))
        .unwrap_or(1)
}

fn orientation_in_exif(data: &[u8]) -> Option<u8> {
    let rest = data.strip_prefix(b"Exif\0\0")?;
    if rest.len() < 8 {
        return None;
    }
    let le = match &rest[..2] {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let u16_at = |i: usize| -> Option<u16> {
        let b = rest.get(i..i + 2)?;
        Some(if le {
            u16::from_le_bytes([b[0], b[1]])
        } else {
            u16::from_be_bytes([b[0], b[1]])
        })
    };
    let u32_at = |i: usize| -> Option<u32> {
        let b = rest.get(i..i + 4)?;
        Some(if le {
            u32::from_le_bytes(b.try_into().ok()?)
        } else {
            u32::from_be_bytes(b.try_into().ok()?)
        })
    };
    if u16_at(2)? != 42 {
        return None;
    }
    let mut offset = u32_at(4)? as usize;
    for _ in 0..4 {
        let count = u16_at(offset)? as usize;
        offset += 2;
        for _ in 0..count {
            let tag = u16_at(offset)?;
            let kind = u16_at(offset + 2)?;
            let n = u32_at(offset + 4)?;
            if tag == 0x0112 && kind == 3 && n >= 1 {
                return Some(u16_at(offset + 8)? as u8);
            }
            offset += 12;
        }
        let next = u32_at(offset)? as usize;
        if next == 0 {
            break;
        }
        offset = next;
    }
    None
}

/// Join split APP2 `ICC_PROFILE` chunks. An incomplete set is dropped.
fn assemble_icc(app2: &[Vec<u8>]) -> Vec<u8> {
    let mut parts: Vec<(u8, u8, &[u8])> = Vec::new();
    for seg in app2 {
        let Some(rest) = seg.strip_prefix(b"ICC_PROFILE\0") else {
            continue;
        };
        if rest.len() < 2 {
            continue;
        }
        let (seq, total) = (rest[0], rest[1]);
        if seq >= 1 && total >= 1 && seq <= total {
            parts.push((seq, total, &rest[2..]));
        }
    }
    if parts.is_empty() {
        return Vec::new();
    }
    let total = parts[0].1;
    if parts.iter().any(|part| part.1 != total) {
        return Vec::new();
    }
    parts.sort_by_key(|part| part.0);
    let mut seen = vec![false; total as usize + 1];
    let mut out = Vec::new();
    for (seq, _, data) in parts {
        let idx = seq as usize;
        if idx >= seen.len() || seen[idx] {
            continue;
        }
        seen[idx] = true;
        out.extend_from_slice(data);
    }
    if seen.iter().skip(1).any(|present| !present) {
        Vec::new()
    } else {
        out
    }
}

unsafe fn write_icc(c: &mut jpeg_compress_struct, icc: &[u8]) {
    if icc.is_empty() {
        return;
    }
    // Marker payload limit is 65533 bytes; 14 of those are the ICC header.
    const CHUNK: usize = 60000;
    let count = icc.len().div_ceil(CHUNK).max(1);
    if count > 255 {
        return;
    }
    for (i, chunk) in icc.chunks(CHUNK).enumerate() {
        let mut payload = Vec::with_capacity(14 + chunk.len());
        payload.extend_from_slice(b"ICC_PROFILE\0");
        payload.push((i + 1) as u8);
        payload.push(count as u8);
        payload.extend_from_slice(chunk);
        unsafe {
            jpeg_write_marker(
                c,
                jpeg_marker::APP0 as c_int + 2,
                payload.as_ptr(),
                payload.len() as c_uint,
            );
        }
    }
}
