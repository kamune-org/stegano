//! Downscale an image carrier without changing its format.
//!
//! Scale `1.0` returns the input bytes unchanged. JPEG is decoded and encoded
//! with mozjpeg (quality 92, 4:2:0); the EXIF orientation is applied and then
//! dropped, and the ICC profile is kept. PNG keeps bit depth, colour type, and
//! the `iCCP` / `sRGB` / `gAMA` / `cHRM` chunks. BMP, TIFF, and lossless WebP
//! are resampled with Lanczos3 and written back in the same format.

use std::io::Cursor;

use image::ImageFormat;
use image::codecs::bmp::BmpEncoder;
use image::codecs::tiff::TiffEncoder;
use image::codecs::webp::WebPEncoder;

use crate::carrier::{CarrierFormat, jpeg_pixels};
use crate::control::Control;
use crate::error::{Error, Result};
use crate::pixels::{apply_orientation, resample_u8, resample_u16, scaled_dim};

/// Resize `data` by `scale` (`0 < scale ≤ 1`). Audio is refused.
pub fn resize_image(data: &[u8], scale: f32, ctl: &Control) -> Result<Vec<u8>> {
    if !(scale > 0.0 && scale <= 1.0) {
        return Err(Error::InvalidPayload(
            "scale must be in the range (0, 1]".into(),
        ));
    }
    if scale == 1.0 {
        return Ok(data.to_vec());
    }
    ctl.check()?;
    match CarrierFormat::detect(data)? {
        CarrierFormat::Wav | CarrierFormat::Flac => Err(Error::UnsupportedFormat(
            "resizing is only supported for images".into(),
        )),
        CarrierFormat::Jpeg => resize_jpeg(data, scale, ctl),
        CarrierFormat::Png => resize_png(data, scale, ctl),
        format @ (CarrierFormat::Bmp | CarrierFormat::Tiff | CarrierFormat::WebP) => {
            resize_dynamic(data, format, scale, ctl)
        }
    }
}

fn resize_jpeg(data: &[u8], scale: f32, ctl: &Control) -> Result<Vec<u8>> {
    ctl.report(crate::Stage::Resizing, 0.0);
    let decoded = jpeg_pixels::decode_jpeg(data, None, ctl.cancel)?;
    ctl.check()?;
    let (pixels, w, h) = apply_orientation(
        decoded.pixels,
        decoded.width,
        decoded.height,
        decoded.components,
        decoded.orientation,
        ctl.cancel,
    )?;
    ctl.report(crate::Stage::Resizing, 0.34);
    let (nw, nh) = (scaled_dim(w, scale), scaled_dim(h, scale));
    let pixels = if nw == w && nh == h {
        pixels
    } else {
        resample_u8(&pixels, w, h, decoded.components, nw, nh)?
    };
    ctl.check()?;
    ctl.report(crate::Stage::Resizing, 0.7);
    let out =
        jpeg_pixels::encode_jpeg(nw, nh, decoded.components, &pixels, &decoded.icc, 92, true)?;
    ctl.report(crate::Stage::Resizing, 1.0);
    Ok(out)
}

fn resize_png(data: &[u8], scale: f32, ctl: &Control) -> Result<Vec<u8>> {
    ctl.report(crate::Stage::Resizing, 0.0);
    let mut decoder = png::Decoder::new(Cursor::new(data));
    decoder.set_transformations(png::Transformations::IDENTITY);
    let mut reader = decoder.read_info().map_err(decode_err)?;
    let (color_type, bit_depth, has_trns, w, h) = {
        let info = reader.info();
        (
            info.color_type,
            info.bit_depth,
            info.trns.is_some(),
            info.width,
            info.height,
        )
    };
    let (channels, alpha) = match color_type {
        png::ColorType::Grayscale => (1, false),
        png::ColorType::GrayscaleAlpha => (2, true),
        png::ColorType::Rgb => (3, false),
        png::ColorType::Rgba => (4, true),
        png::ColorType::Indexed => {
            return Err(Error::UnsuitableCarrier(
                "palette (indexed-colour) PNGs can't hide data without looking altered; \
                 convert it to a true-colour PNG first"
                    .into(),
            ));
        }
    };
    let depth = match bit_depth {
        png::BitDepth::Eight => 8u8,
        png::BitDepth::Sixteen => 16,
        _ => {
            return Err(Error::UnsuitableCarrier(
                "PNGs with fewer than 8 bits per channel are not supported".into(),
            ));
        }
    };
    if has_trns {
        return Err(Error::UnsuitableCarrier(
            "PNGs with a colour-key transparency (tRNS) chunk are not supported".into(),
        ));
    }
    if w == 0 || h == 0 {
        return Err(Error::Decode("image has no pixels".into()));
    }
    let mut buf = vec![
        0;
        reader
            .output_buffer_size()
            .ok_or_else(|| decode_err("image too large"))?
    ];
    let frame = reader.next_frame(&mut buf).map_err(decode_err)?;
    buf.truncate(frame.buffer_size());
    let kept = color_chunks(data);
    ctl.check()?;
    ctl.report(crate::Stage::Resizing, 0.34);
    let (nw, nh) = (scaled_dim(w, scale), scaled_dim(h, scale));
    let encoded = if depth == 16 {
        let samples: Vec<u16> = buf
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_be_bytes(*b))
            .collect();
        let samples = if nw == w && nh == h {
            samples
        } else {
            resample_u16(&samples, w, h, channels, nw, nh)?
        };
        ctl.check()?;
        ctl.report(crate::Stage::Resizing, 0.7);
        let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_be_bytes()).collect();
        encode_png(nw, nh, channels, alpha, 16, &bytes, &kept)?
    } else {
        let samples = if nw == w && nh == h {
            buf
        } else {
            resample_u8(&buf, w, h, channels, nw, nh)?
        };
        ctl.check()?;
        ctl.report(crate::Stage::Resizing, 0.7);
        encode_png(nw, nh, channels, alpha, 8, &samples, &kept)?
    };
    ctl.report(crate::Stage::Resizing, 1.0);
    Ok(encoded)
}

fn encode_png(
    w: u32,
    h: u32,
    channels: usize,
    alpha: bool,
    depth: u8,
    bytes: &[u8],
    kept: &[Vec<u8>],
) -> Result<Vec<u8>> {
    let mut fresh = Vec::new();
    let mut enc = png::Encoder::new(&mut fresh, w, h);
    enc.set_color(match (channels, alpha) {
        (1, _) => png::ColorType::Grayscale,
        (2, _) => png::ColorType::GrayscaleAlpha,
        (3, _) => png::ColorType::Rgb,
        _ => png::ColorType::Rgba,
    });
    enc.set_depth(if depth == 16 {
        png::BitDepth::Sixteen
    } else {
        png::BitDepth::Eight
    });
    enc.set_compression(png::Compression::Fast);
    let mut writer = enc.write_header().map_err(encode_err)?;
    writer.write_image_data(bytes).map_err(encode_err)?;
    writer.finish().map_err(encode_err)?;
    Ok(insert_chunks(&fresh, kept))
}

/// Raw `iCCP`, `sRGB`, `gAMA`, and `cHRM` chunks, in file order.
fn color_chunks(data: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut pos = 8;
    while pos + 12 <= data.len() {
        let len = u32::from_be_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
        let end = pos + 12 + len;
        if end > data.len() {
            break;
        }
        let kind = &data[pos + 4..pos + 8];
        if matches!(kind, b"iCCP" | b"sRGB" | b"gAMA" | b"cHRM") {
            out.push(data[pos..end].to_vec());
        }
        pos = end;
    }
    out
}

fn insert_chunks(png: &[u8], chunks: &[Vec<u8>]) -> Vec<u8> {
    if chunks.is_empty() || png.len() < 8 {
        return png.to_vec();
    }
    let len = u32::from_be_bytes(png[8..12].try_into().unwrap()) as usize;
    let ihdr_end = 8 + 12 + len;
    if ihdr_end > png.len() {
        return png.to_vec();
    }
    let mut out = png[..ihdr_end].to_vec();
    for chunk in chunks {
        out.extend_from_slice(chunk);
    }
    out.extend_from_slice(&png[ihdr_end..]);
    out
}

fn resize_dynamic(
    data: &[u8],
    format: CarrierFormat,
    scale: f32,
    ctl: &Control,
) -> Result<Vec<u8>> {
    if format == CarrierFormat::WebP {
        reject_animated_webp(data)?;
    }
    ctl.report(crate::Stage::Resizing, 0.0);
    let image_format = match format {
        CarrierFormat::Bmp => ImageFormat::Bmp,
        CarrierFormat::Tiff => ImageFormat::Tiff,
        CarrierFormat::WebP => ImageFormat::WebP,
        _ => unreachable!(),
    };
    let img = image::load_from_memory_with_format(data, image_format).map_err(decode_err)?;
    ctl.check()?;
    let (w, h) = (img.width(), img.height());
    if w == 0 || h == 0 {
        return Err(Error::Decode("image has no pixels".into()));
    }
    ctl.report(crate::Stage::Resizing, 0.34);
    let (nw, nh) = (scaled_dim(w, scale), scaled_dim(h, scale));
    let img = if nw == w && nh == h {
        img
    } else {
        img.resize_exact(nw, nh, image::imageops::FilterType::Lanczos3)
    };
    ctl.check()?;
    ctl.report(crate::Stage::Resizing, 0.7);
    let mut out = Cursor::new(Vec::new());
    match format {
        CarrierFormat::Bmp => img.write_with_encoder(BmpEncoder::new(&mut out)),
        CarrierFormat::Tiff => img.write_with_encoder(TiffEncoder::new(&mut out)),
        CarrierFormat::WebP => img.write_with_encoder(WebPEncoder::new_lossless(&mut out)),
        _ => unreachable!(),
    }
    .map_err(encode_err)?;
    ctl.report(crate::Stage::Resizing, 1.0);
    Ok(out.into_inner())
}

fn reject_animated_webp(data: &[u8]) -> Result<()> {
    let mut pos = 12;
    while pos + 8 <= data.len() {
        let fourcc = &data[pos..pos + 4];
        let size = u32::from_le_bytes(data[pos + 4..pos + 8].try_into().unwrap()) as usize;
        if fourcc == b"ANIM" {
            return Err(Error::UnsuitableCarrier(
                "animated WebP is not supported".into(),
            ));
        }
        pos += 8 + size + (size & 1);
    }
    Ok(())
}

fn decode_err(e: impl std::fmt::Display) -> Error {
    Error::Decode(e.to_string())
}

fn encode_err(e: impl std::fmt::Display) -> Error {
    Error::Encode(e.to_string())
}
