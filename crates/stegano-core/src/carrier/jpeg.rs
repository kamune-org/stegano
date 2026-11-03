//! JPEG, embedded in the quantized DCT coefficients so the file never goes
//! through a decode/re-compress cycle. Uses libjpeg's lossless transcoding
//! path (the one `jpegtran` uses): read coefficients, change some by ±1,
//! write them back with the original quantization tables and markers.
//!
//! The cover is every AC coefficient of every component; DC coefficients are
//! left alone because changing them shifts the brightness of a whole block.

use std::ffi::{c_int, c_uint, c_ulong};
use std::mem;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;

use mozjpeg_sys::*;

use super::{Carrier, CarrierFormat, Cover};
use crate::cost;
use crate::error::{Error, Result};

/// Coefficient magnitude limit for 8-bit baseline JPEG.
const COEF_LIMIT: i32 = 1023;

struct Component {
    width_in_blocks: usize,
    height_in_blocks: usize,
    quant: [u16; 64],
    blocks: Vec<[i16; 64]>,
}

pub(crate) struct Jpeg {
    source: Vec<u8>,
    width: u32,
    height: u32,
    progressive: bool,
    components: Vec<Component>,
    cover: Cover,
}

impl Jpeg {
    pub fn open(data: &[u8]) -> Result<Self> {
        let (width, height, progressive, components) =
            with_coefficients(data, |d, arrays| unsafe {
                let mut comps = Vec::new();
                for ci in 0..d.num_components as usize {
                    let info = &*d.comp_info.add(ci);
                    if info.quant_table.is_null() {
                        return Err(Error::Decode("missing quantization table".into()));
                    }
                    let mut c = Component {
                        width_in_blocks: info.width_in_blocks as usize,
                        height_in_blocks: info.height_in_blocks as usize,
                        quant: (*info.quant_table).quantval,
                        blocks: Vec::new(),
                    };
                    let (w, h) = (c.width_in_blocks, c.height_in_blocks);
                    for_each_block_row(d, *arrays.add(ci), w, h, false, |row| {
                        c.blocks.extend_from_slice(row);
                    });
                    comps.push(c);
                }
                Ok((d.image_width, d.image_height, !d.coef_bits.is_null(), comps))
            })??;

        let values: Vec<i32> = components
            .iter()
            .flat_map(|c| {
                c.blocks
                    .iter()
                    .flat_map(|b| b[1..].iter().map(|&v| v as i32))
            })
            .collect();
        let effective = values.iter().filter(|&&v| v != 0).count();
        Ok(Jpeg {
            source: data.to_vec(),
            width,
            height,
            progressive,
            components,
            cover: Cover {
                values,
                lo: -COEF_LIMIT,
                hi: COEF_LIMIT,
                effective,
            },
        })
    }
}

impl Carrier for Jpeg {
    fn format(&self) -> CarrierFormat {
        CarrierFormat::Jpeg
    }

    fn describe(&self) -> String {
        format!(
            "{} × {} JPEG, {} component{}{}",
            self.width,
            self.height,
            self.components.len(),
            if self.components.len() == 1 { "" } else { "s" },
            if self.progressive {
                ", progressive"
            } else {
                ""
            }
        )
    }

    fn cover(&self) -> &Cover {
        &self.cover
    }

    fn cover_mut(&mut self) -> &mut Cover {
        &mut self.cover
    }

    fn costs(&self) -> Vec<f32> {
        self.components
            .iter()
            .flat_map(|c| cost::uerd(&c.blocks, c.width_in_blocks, c.height_in_blocks, &c.quant))
            .collect()
    }

    fn encode(&mut self) -> Result<Vec<u8>> {
        // Same element order as in `open`.
        let mut values = self.cover.values.iter();
        let components: Vec<Vec<[i16; 64]>> = self
            .components
            .iter()
            .map(|c| {
                c.blocks
                    .iter()
                    .map(|b| {
                        let mut b = *b;
                        for coef in &mut b[1..] {
                            *coef = *values.next().unwrap() as i16;
                        }
                        b
                    })
                    .collect()
            })
            .collect();

        let progressive = self.progressive;
        with_coefficients(&self.source, |d, arrays| unsafe {
            for (ci, blocks) in components.iter().enumerate() {
                let info = &*d.comp_info.add(ci);
                let (w, h) = (
                    info.width_in_blocks as usize,
                    info.height_in_blocks as usize,
                );
                let mut rows = blocks.chunks(w);
                for_each_block_row(d, *arrays.add(ci), w, h, true, |row| {
                    row.copy_from_slice(rows.next().unwrap());
                });
            }
            write(d, arrays, progressive)
        })?
    }
}

unsafe extern "C-unwind" fn error_exit(cinfo: &mut jpeg_common_struct) {
    let mut msg = String::from("libjpeg error");
    unsafe {
        let err = &*cinfo.err;
        if let Some(format) = err.format_message {
            let buf = [0u8; 80];
            format(cinfo, &buf);
            let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
            msg = String::from_utf8_lossy(&buf[..end]).into_owned();
        }
    }
    std::panic::resume_unwind(Box::new(msg));
}

unsafe extern "C-unwind" fn silence(_: &mut jpeg_common_struct, _: c_int) {}

fn new_error_mgr() -> Box<jpeg_error_mgr> {
    unsafe {
        let mut err: Box<jpeg_error_mgr> = Box::new(mem::zeroed());
        jpeg_std_error(&mut err);
        err.error_exit = Some(error_exit);
        err.emit_message = Some(silence);
        err
    }
}

/// Open `data` for coefficient access and run `f` on it, turning libjpeg
/// errors (which unwind out of the C code) into `Error`s.
fn with_coefficients<T>(
    data: &[u8],
    f: impl FnOnce(&mut jpeg_decompress_struct, *mut *mut jvirt_barray_control) -> T,
) -> Result<T> {
    let mut err = new_error_mgr();
    let mut d: Box<jpeg_decompress_struct> = Box::new(unsafe { mem::zeroed() });
    d.common.err = &mut *err;
    let result = catch_unwind(AssertUnwindSafe(|| unsafe {
        jpeg_create_decompress(&mut *d);
        jpeg_mem_src(&mut d, data.as_ptr(), data.len() as c_ulong);
        for m in 0..16 {
            jpeg_save_markers(&mut d, jpeg_marker::APP0 as c_int + m, 0xFFFF);
        }
        jpeg_save_markers(&mut d, jpeg_marker::COM as c_int, 0xFFFF);
        jpeg_read_header(&mut d, 1);
        let arrays = jpeg_read_coefficients(&mut d);
        if arrays.is_null() {
            std::panic::resume_unwind(Box::new(String::from("could not read DCT coefficients")));
        }
        f(&mut d, arrays)
    }));
    unsafe { jpeg_destroy_decompress(&mut d) };
    result.map_err(|e| Error::Decode(panic_message(e)))
}

/// Visit each row of `width` blocks in one component's coefficient array.
unsafe fn for_each_block_row(
    d: &mut jpeg_decompress_struct,
    array: *mut jvirt_barray_control,
    width: usize,
    rows: usize,
    writable: bool,
    mut f: impl FnMut(&mut [[i16; 64]]),
) {
    unsafe {
        let access = (*d.common.mem).access_virt_barray.unwrap();
        for r in 0..rows {
            let ptrs = access(
                &mut d.common,
                array,
                r as JDIMENSION,
                1,
                writable as boolean,
            );
            f(std::slice::from_raw_parts_mut(*ptrs, width));
        }
    }
}

unsafe fn write(
    d: &mut jpeg_decompress_struct,
    arrays: *mut *mut jvirt_barray_control,
    progressive: bool,
) -> Result<Vec<u8>> {
    let mut err = new_error_mgr();
    let mut c: Box<jpeg_compress_struct> = Box::new(unsafe { mem::zeroed() });
    c.common.err = &mut *err;
    let mut out_ptr: *mut u8 = ptr::null_mut();
    let mut out_len: c_ulong = 0;
    let result = catch_unwind(AssertUnwindSafe(|| unsafe {
        jpeg_create_compress(&mut *c);
        // Plain libjpeg behaviour: no mozjpeg-specific scan optimisations that
        // would make the file look unlike the camera/editor that produced it.
        jpeg_c_set_int_param(
            &mut c,
            J_INT_PARAM::JINT_COMPRESS_PROFILE,
            JINT_COMPRESS_PROFILE_VALUE::JCP_FASTEST as c_int,
        );
        jpeg_mem_dest(&mut c, &mut out_ptr, &mut out_len);
        jpeg_copy_critical_parameters(d, &mut c);
        if progressive {
            jpeg_simple_progression(&mut c);
        }
        jpeg_write_coefficients(&mut c, arrays);

        let mut marker = d.marker_list;
        while !marker.is_null() {
            let m = &*marker;
            let data = std::slice::from_raw_parts(m.data, m.data_length as usize);
            let duplicate = (c.write_JFIF_header != 0
                && m.marker == jpeg_marker::APP0 as u8
                && data.starts_with(b"JFIF\0"))
                || (c.write_Adobe_marker != 0
                    && m.marker == jpeg_marker::APP0 as u8 + 14
                    && data.starts_with(b"Adobe"));
            if !duplicate {
                jpeg_write_marker(&mut c, m.marker as c_int, m.data, m.data_length as c_uint);
            }
            marker = m.next;
        }
        jpeg_finish_compress(&mut c);
    }));
    unsafe { jpeg_destroy_compress(&mut c) };
    let out = if out_ptr.is_null() {
        Vec::new()
    } else {
        let v = unsafe { std::slice::from_raw_parts(out_ptr, out_len as usize).to_vec() };
        unsafe { libc::free(out_ptr.cast()) };
        v
    };
    result.map_err(|e| Error::Encode(panic_message(e)))?;
    Ok(out)
}

fn panic_message(e: Box<dyn std::any::Any + Send>) -> String {
    e.downcast_ref::<String>()
        .cloned()
        .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_else(|| "libjpeg error".into())
}
