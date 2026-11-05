//! Pixel orientation and Lanczos resampling shared by resize and previews.

use std::sync::atomic::{AtomicBool, Ordering};

use image::{ImageBuffer, Luma, LumaA, Pixel, Rgb, Rgba};

use crate::error::{Error, Result};

/// Rotate or mirror `pixels` by an EXIF orientation tag (1–8).
///
/// Tag 1 returns the buffer unchanged. Tags 5–8 swap width and height.
pub(crate) fn apply_orientation(
    pixels: Vec<u8>,
    w: u32,
    h: u32,
    comps: usize,
    orientation: u8,
    cancel: Option<&AtomicBool>,
) -> Result<(Vec<u8>, u32, u32)> {
    if w == 0 || h == 0 || comps == 0 {
        return Err(Error::Decode("image has no pixels".into()));
    }
    if orientation <= 1 || orientation > 8 {
        return Ok((pixels, w, h));
    }
    let (dw, dh) = if matches!(orientation, 5..=8) {
        (h, w)
    } else {
        (w, h)
    };
    let need = (w as usize)
        .checked_mul(h as usize)
        .and_then(|n| n.checked_mul(comps))
        .ok_or_else(|| Error::Decode("image is too large".into()))?;
    if pixels.len() != need {
        return Err(Error::Decode("pixel buffer size mismatch".into()));
    }
    let mut out = vec![0u8; need];
    for y in 0..dh {
        if y.is_multiple_of(64) && cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
            return Err(Error::Cancelled);
        }
        for x in 0..dw {
            let (sx, sy) = source_xy(orientation, x, y, w, h);
            let si = (sy as usize * w as usize + sx as usize) * comps;
            let di = (y as usize * dw as usize + x as usize) * comps;
            out[di..di + comps].copy_from_slice(&pixels[si..si + comps]);
        }
    }
    Ok((out, dw, dh))
}

/// `(sx, sy)` in the source for destination pixel `(x, y)`.
fn source_xy(orientation: u8, x: u32, y: u32, w: u32, h: u32) -> (u32, u32) {
    match orientation {
        2 => (w - 1 - x, y),
        3 => (w - 1 - x, h - 1 - y),
        4 => (x, h - 1 - y),
        5 => (y, x),
        6 => (y, h - 1 - x),
        7 => (w - 1 - y, h - 1 - x),
        8 => (w - 1 - y, x),
        _ => (x, y),
    }
}

pub(crate) fn resample_u8(
    pixels: &[u8],
    w: u32,
    h: u32,
    comps: usize,
    nw: u32,
    nh: u32,
) -> Result<Vec<u8>> {
    match comps {
        1 => resample::<Luma<u8>>(pixels, w, h, nw, nh),
        2 => resample::<LumaA<u8>>(pixels, w, h, nw, nh),
        3 => resample::<Rgb<u8>>(pixels, w, h, nw, nh),
        4 => resample::<Rgba<u8>>(pixels, w, h, nw, nh),
        _ => Err(Error::Decode(format!("{comps} channels are not supported"))),
    }
}

pub(crate) fn resample_u16(
    pixels: &[u16],
    w: u32,
    h: u32,
    comps: usize,
    nw: u32,
    nh: u32,
) -> Result<Vec<u16>> {
    match comps {
        1 => resample::<Luma<u16>>(pixels, w, h, nw, nh),
        2 => resample::<LumaA<u16>>(pixels, w, h, nw, nh),
        3 => resample::<Rgb<u16>>(pixels, w, h, nw, nh),
        4 => resample::<Rgba<u16>>(pixels, w, h, nw, nh),
        _ => Err(Error::Decode(format!("{comps} channels are not supported"))),
    }
}

fn resample<P>(pixels: &[P::Subpixel], w: u32, h: u32, nw: u32, nh: u32) -> Result<Vec<P::Subpixel>>
where
    P: Pixel + 'static,
    P::Subpixel: Copy + 'static,
{
    let img: ImageBuffer<P, Vec<P::Subpixel>> = ImageBuffer::from_raw(w, h, pixels.to_vec())
        .ok_or_else(|| Error::Decode("pixel buffer size mismatch".into()))?;
    let out = image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Lanczos3);
    Ok(out.into_raw())
}

/// Round `n * scale` to the nearest pixel, and never below 1.
pub(crate) fn scaled_dim(n: u32, scale: f32) -> u32 {
    ((n as f32) * scale).round().max(1.0) as u32
}

/// Largest size that fits in a `max_side` square, keeping the aspect ratio.
pub(crate) fn fit_max_side(w: u32, h: u32, max_side: u32) -> Result<(u32, u32)> {
    if max_side == 0 {
        return Err(Error::InvalidPayload(
            "thumbnail size must be at least 1".into(),
        ));
    }
    let long = w.max(h);
    if long <= max_side {
        return Ok((w.max(1), h.max(1)));
    }
    let scale = max_side as f32 / long as f32;
    Ok((scaled_dim(w, scale), scaled_dim(h, scale)))
}
