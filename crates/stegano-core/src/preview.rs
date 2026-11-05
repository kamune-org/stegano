//! Small previews: a JPEG or PNG thumbnail, and an audio peak waveform.

use std::io::Cursor;

use image::codecs::png::PngEncoder;
use image::{DynamicImage, ImageFormat};

use crate::carrier::{CarrierFormat, jpeg_pixels};
use crate::error::{Error, Result};
use crate::pixels::{apply_orientation, fit_max_side, resample_u8};

/// Thumbnail whose long side is at most `max_side`.
///
/// Returns encoded bytes and a MIME type: `image/png` when the image has an
/// alpha channel, otherwise `image/jpeg`. JPEG inputs are DCT-scaled before
/// the pixels are touched. 16-bit samples are reduced to 8-bit.
pub fn thumbnail(data: &[u8], max_side: u32) -> Result<(Vec<u8>, &'static str)> {
    if max_side == 0 {
        return Err(Error::InvalidPayload(
            "thumbnail size must be at least 1".into(),
        ));
    }
    match CarrierFormat::detect(data)? {
        CarrierFormat::Wav | CarrierFormat::Flac => Err(Error::UnsupportedFormat(
            "audio has no image thumbnail".into(),
        )),
        CarrierFormat::Jpeg => thumbnail_jpeg(data, max_side),
        format => thumbnail_raster(data, format, max_side),
    }
}

fn thumbnail_jpeg(data: &[u8], max_side: u32) -> Result<(Vec<u8>, &'static str)> {
    let decoded = jpeg_pixels::decode_jpeg(data, Some(max_side), None)?;
    let (pixels, w, h) = apply_orientation(
        decoded.pixels,
        decoded.width,
        decoded.height,
        decoded.components,
        decoded.orientation,
        None,
    )?;
    let (nw, nh) = fit_max_side(w, h, max_side)?;
    let pixels = if nw == w && nh == h {
        pixels
    } else {
        resample_u8(&pixels, w, h, decoded.components, nw, nh)?
    };
    let bytes = jpeg_pixels::encode_jpeg(nw, nh, decoded.components, &pixels, &[], 80, true)?;
    Ok((bytes, "image/jpeg"))
}

fn thumbnail_raster(
    data: &[u8],
    format: CarrierFormat,
    max_side: u32,
) -> Result<(Vec<u8>, &'static str)> {
    if format == CarrierFormat::WebP {
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
    }
    let image_format = match format {
        CarrierFormat::Png => ImageFormat::Png,
        CarrierFormat::Bmp => ImageFormat::Bmp,
        CarrierFormat::Tiff => ImageFormat::Tiff,
        CarrierFormat::WebP => ImageFormat::WebP,
        _ => {
            return Err(Error::UnsupportedFormat("expected an image carrier".into()));
        }
    };
    let img = image::load_from_memory_with_format(data, image_format)
        .map_err(|e| Error::Decode(e.to_string()))?;
    let (nw, nh) = fit_max_side(img.width(), img.height(), max_side)?;
    let img = if nw == img.width() && nh == img.height() {
        img
    } else {
        img.resize_exact(nw, nh, image::imageops::FilterType::Lanczos3)
    };
    if has_alpha(&img) {
        let rgba = img.to_rgba8();
        let mut out = Cursor::new(Vec::new());
        rgba.write_with_encoder(PngEncoder::new(&mut out))
            .map_err(|e| Error::Encode(e.to_string()))?;
        Ok((out.into_inner(), "image/png"))
    } else if matches!(
        img,
        DynamicImage::ImageLuma8(_) | DynamicImage::ImageLuma16(_)
    ) {
        let grey = img.to_luma8();
        let bytes = jpeg_pixels::encode_jpeg(
            grey.width(),
            grey.height(),
            1,
            grey.as_raw(),
            &[],
            80,
            false,
        )?;
        Ok((bytes, "image/jpeg"))
    } else {
        let rgb = img.to_rgb8();
        let bytes =
            jpeg_pixels::encode_jpeg(rgb.width(), rgb.height(), 3, rgb.as_raw(), &[], 80, true)?;
        Ok((bytes, "image/jpeg"))
    }
}

fn has_alpha(img: &DynamicImage) -> bool {
    matches!(
        img,
        DynamicImage::ImageLumaA8(_)
            | DynamicImage::ImageLumaA16(_)
            | DynamicImage::ImageRgba8(_)
            | DynamicImage::ImageRgba16(_)
    )
}

/// Peak absolute amplitude per bucket, in `0.0..=1.0` of full scale.
///
/// Channels are mixed by taking the loudest sample in each frame. `buckets`
/// must be at least 1.
pub fn waveform(data: &[u8], buckets: usize) -> Result<Vec<f32>> {
    if buckets == 0 {
        return Err(Error::InvalidPayload(
            "waveform buckets must be at least 1".into(),
        ));
    }
    let peaks = frame_peaks(data)?;
    let mut out = vec![0f32; buckets];
    if peaks.is_empty() {
        return Ok(out);
    }
    for (i, peak) in peaks.iter().enumerate() {
        let bucket = i * buckets / peaks.len();
        if *peak > out[bucket] {
            out[bucket] = *peak;
        }
    }
    Ok(out)
}

fn frame_peaks(data: &[u8]) -> Result<Vec<f32>> {
    match CarrierFormat::detect(data)? {
        CarrierFormat::Wav => wav_peaks(data),
        CarrierFormat::Flac => flac_peaks(data),
        _ => Err(Error::UnsupportedFormat(
            "waveform is only available for audio".into(),
        )),
    }
}

fn wav_peaks(data: &[u8]) -> Result<Vec<f32>> {
    let reader =
        hound::WavReader::new(Cursor::new(data)).map_err(|e| Error::Decode(e.to_string()))?;
    let spec = reader.spec();
    let channels = spec.channels as usize;
    if channels == 0 {
        return Err(Error::Decode("audio has no channels".into()));
    }
    match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Float, _) => {
            let samples = reader
                .into_samples::<f32>()
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| Error::Decode(e.to_string()))?;
            Ok(mix_float(&samples, channels))
        }
        (hound::SampleFormat::Int, bits) if (8..=32).contains(&bits) => {
            let samples = reader
                .into_samples::<i32>()
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| Error::Decode(e.to_string()))?;
            let full = (1u64 << (bits - 1)) as f32;
            Ok(mix_int(&samples, channels, full))
        }
        (format, bits) => Err(Error::UnsupportedFormat(format!(
            "{bits}-bit {format:?} WAV"
        ))),
    }
}

fn flac_peaks(data: &[u8]) -> Result<Vec<f32>> {
    let mut reader =
        claxon::FlacReader::new(Cursor::new(data)).map_err(|e| Error::Decode(e.to_string()))?;
    let info = reader.streaminfo();
    let channels = info.channels as usize;
    let bits = info.bits_per_sample;
    if channels == 0 || bits == 0 || bits > 32 {
        return Err(Error::Decode("unsupported FLAC layout".into()));
    }
    let samples = reader
        .samples()
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| Error::Decode(e.to_string()))?;
    let full = (1u64 << (bits - 1)) as f32;
    Ok(mix_int(&samples, channels, full))
}

fn mix_int(samples: &[i32], channels: usize, full: f32) -> Vec<f32> {
    if full == 0.0 {
        return Vec::new();
    }
    samples
        .chunks(channels)
        .map(|frame| {
            frame
                .iter()
                .map(|s| (*s as f32).abs() / full)
                .fold(0.0, f32::max)
                .clamp(0.0, 1.0)
        })
        .collect()
}

fn mix_float(samples: &[f32], channels: usize) -> Vec<f32> {
    samples
        .chunks(channels)
        .map(|frame| {
            frame
                .iter()
                .map(|s| s.abs())
                .fold(0.0, f32::max)
                .clamp(0.0, 1.0)
        })
        .collect()
}
