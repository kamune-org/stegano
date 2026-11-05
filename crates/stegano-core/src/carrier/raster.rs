//! Lossless raster images: PNG, BMP, TIFF and lossless WebP.
//!
//! PNG is handled with the `png` crate directly so the output keeps every
//! original ancillary chunk (ICC profile, EXIF, text, physical size, …) in its
//! original position; only the pixel data differs. The other formats go
//! through the `image` crate.

use std::io::Cursor;

use image::{DynamicImage, ImageBuffer, ImageFormat};
use rayon::prelude::*;

use super::{Carrier, CarrierFormat, Cover};
use crate::control::Control;
use crate::cost;
use crate::error::{Error, Result};

pub(crate) struct Raster {
    format: CarrierFormat,
    width: usize,
    height: usize,
    /// Channels per pixel, including alpha.
    channels: usize,
    alpha: bool,
    depth: u8,
    /// Interleaved samples.
    samples: Vec<u16>,
    /// Cover element -> index into `samples`, only when transparent pixels
    /// are skipped. Otherwise the mapping is arithmetic (see `sample_index`).
    positions: Option<Vec<u32>>,
    cover: Cover,
    /// Original PNG bytes, for carrying ancillary chunks over.
    png_source: Option<Vec<u8>>,
}

impl Raster {
    pub fn png(data: &[u8]) -> Result<Self> {
        let mut decoder = png::Decoder::new(Cursor::new(data));
        decoder.set_transformations(png::Transformations::IDENTITY);
        let mut reader = decoder.read_info().map_err(decode_err)?;
        let info = reader.info();
        let (channels, alpha) = match info.color_type {
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
        let depth = match info.bit_depth {
            png::BitDepth::Eight => 8,
            png::BitDepth::Sixteen => 16,
            _ => {
                return Err(Error::UnsuitableCarrier(
                    "PNGs with fewer than 8 bits per channel are not supported".into(),
                ));
            }
        };
        if info.trns.is_some() {
            return Err(Error::UnsuitableCarrier(
                "PNGs with a colour-key transparency (tRNS) chunk are not supported".into(),
            ));
        }
        let (width, height) = (info.width as usize, info.height as usize);
        let mut buf = vec![
            0;
            reader
                .output_buffer_size()
                .ok_or_else(|| decode_err("image too large"))?
        ];
        let frame = reader.next_frame(&mut buf).map_err(decode_err)?;
        buf.truncate(frame.buffer_size());
        let samples = if depth == 16 {
            buf.as_chunks::<2>()
                .0
                .iter()
                .map(|&b| u16::from_be_bytes(b))
                .collect()
        } else {
            buf.into_iter().map(u16::from).collect()
        };
        Self::new(
            CarrierFormat::Png,
            width,
            height,
            channels,
            alpha,
            depth,
            samples,
            Some(data.to_vec()),
        )
    }

    pub fn generic(data: &[u8], format: CarrierFormat) -> Result<Self> {
        let image_format = match format {
            CarrierFormat::Bmp => ImageFormat::Bmp,
            CarrierFormat::Tiff => ImageFormat::Tiff,
            CarrierFormat::WebP => {
                check_webp_lossless(data)?;
                ImageFormat::WebP
            }
            _ => unreachable!(),
        };
        let img = image::load_from_memory_with_format(data, image_format).map_err(decode_err)?;
        let (width, height) = (img.width() as usize, img.height() as usize);
        let (channels, alpha, depth, samples): (usize, bool, u8, Vec<u16>) = match img {
            DynamicImage::ImageLuma8(b) => (1, false, 8, widen(b.into_raw())),
            DynamicImage::ImageLumaA8(b) => (2, true, 8, widen(b.into_raw())),
            DynamicImage::ImageRgb8(b) => (3, false, 8, widen(b.into_raw())),
            DynamicImage::ImageRgba8(b) => (4, true, 8, widen(b.into_raw())),
            DynamicImage::ImageLuma16(b) => (1, false, 16, b.into_raw()),
            DynamicImage::ImageLumaA16(b) => (2, true, 16, b.into_raw()),
            DynamicImage::ImageRgb16(b) => (3, false, 16, b.into_raw()),
            DynamicImage::ImageRgba16(b) => (4, true, 16, b.into_raw()),
            _ => {
                return Err(Error::UnsuitableCarrier(
                    "floating-point images are not supported".into(),
                ));
            }
        };
        Self::new(format, width, height, channels, alpha, depth, samples, None)
    }

    #[allow(clippy::too_many_arguments)]
    fn new(
        format: CarrierFormat,
        width: usize,
        height: usize,
        channels: usize,
        alpha: bool,
        depth: u8,
        samples: Vec<u16>,
        png_source: Option<Vec<u8>>,
    ) -> Result<Self> {
        if samples.len() != width * height * channels {
            return Err(decode_err("pixel data has unexpected size"));
        }
        if samples.len() > u32::MAX as usize {
            return Err(Error::UnsuitableCarrier("image is too large".into()));
        }
        let colour = if alpha { channels - 1 } else { channels };
        // Fully transparent pixels are skipped: their colour is invisible, so
        // tools treat any noise there as a giveaway. Alpha is never modified,
        // so the receiver skips exactly the same pixels.
        let visible = |p: usize| !alpha || samples[p * channels + channels - 1] != 0;
        let positions = (!(0..width * height).all(visible)).then(|| {
            (0..width * height)
                .filter(|&p| visible(p))
                .flat_map(|p| (0..colour).map(move |c| (p * channels + c) as u32))
                .collect::<Vec<u32>>()
        });
        let count = positions.as_ref().map_or(width * height * colour, Vec::len);
        let index = |e: usize| match &positions {
            Some(p) => p[e] as usize,
            None => e / colour * channels + e % colour,
        };
        let values: Vec<i32> = (0..count).map(|e| samples[index(e)] as i32).collect();
        let effective = values.len();
        Ok(Raster {
            format,
            width,
            height,
            channels,
            alpha,
            depth,
            samples,
            positions,
            cover: Cover {
                values,
                lo: 0,
                hi: if depth == 16 { 65535 } else { 255 },
                effective,
            },
            png_source,
        })
    }

    fn colour_channels(&self) -> usize {
        if self.alpha {
            self.channels - 1
        } else {
            self.channels
        }
    }

    fn sample_index(&self, element: usize) -> usize {
        match &self.positions {
            Some(p) => p[element] as usize,
            None => {
                let colour = self.colour_channels();
                element / colour * self.channels + element % colour
            }
        }
    }

    fn encode_png(&self, samples: &[u16]) -> Result<Vec<u8>> {
        let mut fresh = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut fresh, self.width as u32, self.height as u32);
            enc.set_color(match (self.channels, self.alpha) {
                (1, _) => png::ColorType::Grayscale,
                (2, _) => png::ColorType::GrayscaleAlpha,
                (3, _) => png::ColorType::Rgb,
                _ => png::ColorType::Rgba,
            });
            enc.set_depth(if self.depth == 16 {
                png::BitDepth::Sixteen
            } else {
                png::BitDepth::Eight
            });
            enc.set_compression(png::Compression::Fast);
            let mut writer = enc.write_header().map_err(encode_err)?;
            let bytes: Vec<u8> = if self.depth == 16 {
                samples.iter().flat_map(|s| s.to_be_bytes()).collect()
            } else {
                samples.iter().map(|&s| s as u8).collect()
            };
            writer.write_image_data(&bytes).map_err(encode_err)?;
            writer.finish().map_err(encode_err)?;
        }
        match &self.png_source {
            Some(original) => splice_png(original, &fresh),
            None => Ok(fresh),
        }
    }

    fn encode_generic(&self, samples: Vec<u16>) -> Result<Vec<u8>> {
        // Consumes a copy: the image crate wants owned buffers.
        let (w, h) = (self.width as u32, self.height as u32);
        let narrow = || samples.iter().map(|&s| s as u8).collect::<Vec<u8>>();
        let img = match (self.channels, self.depth) {
            (1, 8) => ImageBuffer::from_raw(w, h, narrow()).map(DynamicImage::ImageLuma8),
            (2, 8) => ImageBuffer::from_raw(w, h, narrow()).map(DynamicImage::ImageLumaA8),
            (3, 8) => ImageBuffer::from_raw(w, h, narrow()).map(DynamicImage::ImageRgb8),
            (4, 8) => ImageBuffer::from_raw(w, h, narrow()).map(DynamicImage::ImageRgba8),
            (1, _) => ImageBuffer::from_raw(w, h, samples).map(DynamicImage::ImageLuma16),
            (2, _) => ImageBuffer::from_raw(w, h, samples).map(DynamicImage::ImageLumaA16),
            (3, _) => ImageBuffer::from_raw(w, h, samples).map(DynamicImage::ImageRgb16),
            _ => ImageBuffer::from_raw(w, h, samples).map(DynamicImage::ImageRgba16),
        }
        .ok_or_else(|| encode_err("pixel buffer size mismatch"))?;

        let mut out = Cursor::new(Vec::new());
        match self.format {
            CarrierFormat::Bmp => {
                img.write_with_encoder(image::codecs::bmp::BmpEncoder::new(&mut out))
            }
            CarrierFormat::Tiff => {
                img.write_with_encoder(image::codecs::tiff::TiffEncoder::new(&mut out))
            }
            CarrierFormat::WebP => {
                img.write_with_encoder(image::codecs::webp::WebPEncoder::new_lossless(&mut out))
            }
            _ => unreachable!(),
        }
        .map_err(encode_err)?;
        Ok(out.into_inner())
    }
}

impl Carrier for Raster {
    fn format(&self) -> CarrierFormat {
        self.format
    }

    fn describe(&self) -> String {
        let layout = match (self.channels, self.alpha) {
            (1, _) => "greyscale",
            (2, _) => "greyscale + alpha",
            (3, _) => "RGB",
            _ => "RGBA",
        };
        format!(
            "{} × {} {}, {}-bit",
            self.width, self.height, layout, self.depth
        )
    }

    fn cover(&self) -> &Cover {
        &self.cover
    }

    fn cover_mut(&mut self) -> &mut Cover {
        &mut self.cover
    }

    fn costs(&self, ctl: &Control) -> Result<Vec<f32>> {
        // One plane at a time (each parallelized internally) to bound memory.
        let colour = self.colour_channels().max(1);
        let mut costs = vec![0f32; self.cover.values.len()];
        for c in 0..self.colour_channels() {
            ctl.check()?;
            let mut plane = vec![0f32; self.width * self.height];
            let channels = self.channels;
            let samples = &self.samples;
            plane.par_chunks_mut(self.width).enumerate().try_for_each(
                |(y, row)| -> Result<()> {
                    ctl.check()?;
                    for (x, out) in row.iter_mut().enumerate() {
                        *out = samples[(y * self.width + x) * channels + c] as f32;
                    }
                    Ok(())
                },
            )?;
            let plane = cost::hill(&plane, self.width, self.height, ctl)?;
            match &self.positions {
                None => {
                    ctl.check()?;
                    costs
                        .par_iter_mut()
                        .skip(c)
                        .step_by(colour)
                        .zip(plane.par_iter())
                        .for_each(|(dst, &src)| *dst = src);
                }
                Some(positions) => {
                    for (n, (dst, &i)) in costs.iter_mut().zip(positions).enumerate() {
                        if n.is_multiple_of(4096) {
                            ctl.check()?;
                        }
                        let i = i as usize;
                        if i % self.channels == c {
                            *dst = plane[i / self.channels];
                        }
                    }
                }
            }
            ctl.report(crate::Stage::Analyzing, (c + 1) as f32 / colour as f32);
        }
        Ok(costs)
    }

    fn dimensions(&self) -> Option<(u32, u32)> {
        Some((self.width as u32, self.height as u32))
    }

    fn encode(&mut self) -> Result<Vec<u8>> {
        for e in 0..self.cover.values.len() {
            let i = self.sample_index(e);
            self.samples[i] = self.cover.values[e] as u16;
        }
        match self.format {
            CarrierFormat::Png => self.encode_png(&self.samples),
            _ => self.encode_generic(self.samples.clone()),
        }
    }
}

fn widen(v: Vec<u8>) -> Vec<u16> {
    v.into_iter().map(u16::from).collect()
}

fn decode_err(e: impl std::fmt::Display) -> Error {
    Error::Decode(e.to_string())
}

fn encode_err(e: impl std::fmt::Display) -> Error {
    Error::Encode(e.to_string())
}

/// Lossy WebP re-encoded losslessly would balloon in size and stand out, and
/// lossy re-encoding would destroy the payload. Refuse both, and animations.
fn check_webp_lossless(data: &[u8]) -> Result<()> {
    let mut pos = 12;
    while pos + 8 <= data.len() {
        let fourcc = &data[pos..pos + 4];
        let size = u32::from_le_bytes(data[pos + 4..pos + 8].try_into().unwrap()) as usize;
        match fourcc {
            b"VP8 " => {
                return Err(Error::UnsuitableCarrier(
                    "lossy WebP can't carry hidden data; use a PNG, JPEG or lossless WebP".into(),
                ));
            }
            b"ANIM" => {
                return Err(Error::UnsuitableCarrier(
                    "animated WebP is not supported".into(),
                ));
            }
            _ => {}
        }
        pos += 8 + size + (size & 1);
    }
    Ok(())
}

struct PngChunk<'a> {
    kind: [u8; 4],
    raw: &'a [u8],
}

fn png_chunks(data: &[u8]) -> Result<Vec<PngChunk<'_>>> {
    let mut chunks = Vec::new();
    let mut pos = 8;
    while pos + 12 <= data.len() {
        let len = u32::from_be_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
        let end = pos + 12 + len;
        if end > data.len() {
            return Err(decode_err("truncated PNG chunk"));
        }
        chunks.push(PngChunk {
            kind: data[pos + 4..pos + 8].try_into().unwrap(),
            raw: &data[pos..end],
        });
        pos = end;
    }
    Ok(chunks)
}

/// Fresh IHDR + IDAT, with every other chunk of the original kept in place.
fn splice_png(original: &[u8], fresh: &[u8]) -> Result<Vec<u8>> {
    let old = png_chunks(original)?;
    let new = png_chunks(fresh)?;
    let mut out = original[..8].to_vec();
    let find = |kind: [u8; 4]| new.iter().filter(move |c| c.kind == kind);
    let mut idat_written = false;
    for chunk in &old {
        match &chunk.kind {
            b"IHDR" => find(*b"IHDR").for_each(|c| out.extend_from_slice(c.raw)),
            b"IDAT" => {
                if !idat_written {
                    find(*b"IDAT").for_each(|c| out.extend_from_slice(c.raw));
                    idat_written = true;
                }
            }
            _ => out.extend_from_slice(chunk.raw),
        }
    }
    if !idat_written {
        return Err(decode_err("PNG has no image data"));
    }
    Ok(out)
}
