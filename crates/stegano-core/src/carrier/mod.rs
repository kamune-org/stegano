//! Carrier formats. Each one exposes its cover as a flat list of integer
//! elements whose parity carries one bit, a cost per element, and a way to
//! write modified elements back in the original format.

mod audio;
mod jpeg;
pub(crate) mod jpeg_pixels;
mod raster;

use crate::control::Control;
use crate::error::{Error, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarrierFormat {
    Png,
    Bmp,
    Tiff,
    WebP,
    Jpeg,
    Wav,
    Flac,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarrierKind {
    Image,
    Audio,
}

impl CarrierFormat {
    pub fn detect(data: &[u8]) -> Result<Self> {
        Ok(match data {
            [0x89, b'P', b'N', b'G', ..] => Self::Png,
            [0xFF, 0xD8, 0xFF, ..] => Self::Jpeg,
            [b'B', b'M', ..] => Self::Bmp,
            [b'I', b'I', 42, 0, ..] | [b'M', b'M', 0, 42, ..] => Self::Tiff,
            [
                b'R',
                b'I',
                b'F',
                b'F',
                _,
                _,
                _,
                _,
                b'W',
                b'E',
                b'B',
                b'P',
                ..,
            ] => Self::WebP,
            [
                b'R',
                b'I',
                b'F',
                b'F',
                _,
                _,
                _,
                _,
                b'W',
                b'A',
                b'V',
                b'E',
                ..,
            ] => Self::Wav,
            [b'f', b'L', b'a', b'C', ..] => Self::Flac,
            _ => {
                return Err(Error::UnsupportedFormat(
                    "expected PNG, JPEG, BMP, TIFF, WebP, WAV or FLAC".into(),
                ));
            }
        })
    }

    pub fn kind(self) -> CarrierKind {
        match self {
            Self::Wav | Self::Flac => CarrierKind::Audio,
            _ => CarrierKind::Image,
        }
    }

    /// Conventional file extension, without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Bmp => "bmp",
            Self::Tiff => "tiff",
            Self::WebP => "webp",
            Self::Jpeg => "jpg",
            Self::Wav => "wav",
            Self::Flac => "flac",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Bmp => "BMP",
            Self::Tiff => "TIFF",
            Self::WebP => "WebP",
            Self::Jpeg => "JPEG",
            Self::Wav => "WAV",
            Self::Flac => "FLAC",
        }
    }

    /// Seeds the public permutation. Formats that share a cover model share a tag.
    pub(crate) fn tag(self) -> &'static str {
        match self {
            Self::Png | Self::Bmp | Self::Tiff | Self::WebP => "raster",
            Self::Jpeg => "jpeg",
            Self::Wav | Self::Flac => "pcm",
        }
    }
}

/// The embeddable part of a carrier.
pub(crate) struct Cover {
    pub values: Vec<i32>,
    /// Inclusive range every element must stay within.
    pub lo: i32,
    pub hi: i32,
    /// Elements that count toward the "safe" capacity estimate (e.g. non-zero
    /// AC coefficients for JPEG, all elements elsewhere).
    pub effective: usize,
}

pub(crate) trait Carrier: Send + Sync {
    fn format(&self) -> CarrierFormat;
    fn describe(&self) -> String;
    fn cover(&self) -> &Cover;
    fn cover_mut(&mut self) -> &mut Cover;
    /// Cost of a ±1 change per element; `INFINITY` means never change.
    fn costs(&self, ctl: &Control) -> Result<Vec<f32>>;
    /// Pixel width and height. Images only.
    fn dimensions(&self) -> Option<(u32, u32)> {
        None
    }
    /// Sample rate, channel count, and frame count. Audio only.
    fn audio_details(&self) -> Option<(u32, u16, u64)> {
        None
    }
    /// Re-encode the carrier with the current values in its cover.
    fn encode(&mut self) -> Result<Vec<u8>>;
}

pub(crate) fn open(data: &[u8]) -> Result<Box<dyn Carrier>> {
    let format = CarrierFormat::detect(data)?;
    Ok(match format {
        CarrierFormat::Png => Box::new(raster::Raster::png(data)?),
        CarrierFormat::Bmp | CarrierFormat::Tiff | CarrierFormat::WebP => {
            Box::new(raster::Raster::generic(data, format)?)
        }
        CarrierFormat::Jpeg => Box::new(jpeg::Jpeg::open(data)?),
        CarrierFormat::Wav => Box::new(audio::Pcm::wav(data)?),
        CarrierFormat::Flac => Box::new(audio::Pcm::flac(data)?),
    })
}
