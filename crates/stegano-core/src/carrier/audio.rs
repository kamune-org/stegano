//! PCM audio: WAV (16/24/32-bit integer, 32-bit float) and FLAC (16/24-bit).
//!
//! Every sample is a cover element. Float samples are changed by one unit in
//! the last place, which keeps them as close to the original as possible.

use std::io::Cursor;

use flacenc::component::BitRepr;
use flacenc::error::Verify;
use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use rayon::prelude::*;

use super::{Carrier, CarrierFormat, Cover};
use crate::cost;
use crate::error::{Error, Result};

pub(crate) struct Pcm {
    format: CarrierFormat,
    channels: usize,
    sample_rate: u32,
    bits: u16,
    float: bool,
    cover: Cover,
    /// Original bytes (FLAC metadata is carried over from here).
    source: Vec<u8>,
}

impl Pcm {
    pub fn wav(data: &[u8]) -> Result<Self> {
        let reader = WavReader::new(Cursor::new(data)).map_err(decode_err)?;
        let spec = reader.spec();
        let (float, values) = match (spec.sample_format, spec.bits_per_sample) {
            (SampleFormat::Int, 16 | 24 | 32) => (
                false,
                reader
                    .into_samples::<i32>()
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(decode_err)?,
            ),
            (SampleFormat::Float, 32) => (
                true,
                reader
                    .into_samples::<f32>()
                    .map(|s| s.map(|f| f.to_bits() as i32))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(decode_err)?,
            ),
            (SampleFormat::Int, 8) => {
                return Err(Error::UnsuitableCarrier(
                    "8-bit audio is too coarse: changes would be audible".into(),
                ));
            }
            (f, b) => {
                return Err(Error::UnsupportedFormat(format!("{b}-bit {f:?} WAV")));
            }
        };
        Self::new(
            CarrierFormat::Wav,
            spec.channels as usize,
            spec.sample_rate,
            spec.bits_per_sample,
            float,
            values,
            data,
        )
    }

    pub fn flac(data: &[u8]) -> Result<Self> {
        let mut reader = claxon::FlacReader::new(Cursor::new(data)).map_err(decode_err)?;
        let info = reader.streaminfo();
        if !matches!(info.bits_per_sample, 16 | 24) {
            return Err(Error::UnsupportedFormat(format!(
                "{}-bit FLAC (16 and 24-bit are supported)",
                info.bits_per_sample
            )));
        }
        let values = reader
            .samples()
            .collect::<Result<Vec<_>, _>>()
            .map_err(decode_err)?;
        Self::new(
            CarrierFormat::Flac,
            info.channels as usize,
            info.sample_rate,
            info.bits_per_sample as u16,
            false,
            values,
            data,
        )
    }

    fn new(
        format: CarrierFormat,
        channels: usize,
        sample_rate: u32,
        bits: u16,
        float: bool,
        values: Vec<i32>,
        source: &[u8],
    ) -> Result<Self> {
        if channels == 0 || !values.len().is_multiple_of(channels) {
            return Err(decode_err("sample count doesn't match channel count"));
        }
        if values.len() > u32::MAX as usize {
            return Err(Error::UnsuitableCarrier("audio is too long".into()));
        }
        // Audio padded up from a lower bit depth (16-bit in a 24-bit file, or
        // integers stored as floats) has a constant lowest bit. Any change
        // there would be trivially detectable.
        let nonzero = values.iter().filter(|&&v| v != 0).count();
        let odd = values.iter().filter(|&&v| v & 1 == 1).count();
        if nonzero > 0 && odd * 200 < nonzero {
            return Err(Error::UnsuitableCarrier(
                "the lowest bit of every sample is zero, so this audio was padded from a \
                 lower bit depth; use a file at its original bit depth"
                    .into(),
            ));
        }
        let (lo, hi) = if float {
            // Bit patterns; unsafe neighbours (±0, non-finite) are wet in `costs`.
            (i32::MIN + 1, i32::MAX - 1)
        } else {
            (-(1 << (bits - 1)), (1 << (bits - 1)) - 1)
        };
        let effective = values.len();
        Ok(Pcm {
            format,
            channels,
            sample_rate,
            bits,
            float,
            cover: Cover {
                values,
                lo,
                hi,
                effective,
            },
            source: source.to_vec(),
        })
    }

    fn sample(&self, v: i32) -> f64 {
        if self.float {
            f32::from_bits(v as u32) as f64
        } else {
            v as f64
        }
    }

    fn encode_wav(&self, values: &[i32]) -> Result<Vec<u8>> {
        let spec = WavSpec {
            channels: self.channels as u16,
            sample_rate: self.sample_rate,
            bits_per_sample: self.bits,
            sample_format: if self.float {
                SampleFormat::Float
            } else {
                SampleFormat::Int
            },
        };
        let mut out = Cursor::new(Vec::new());
        let mut writer = WavWriter::new(&mut out, spec).map_err(encode_err)?;
        for &v in values {
            match (self.float, self.bits) {
                (true, _) => writer.write_sample(f32::from_bits(v as u32)),
                (false, 16) => writer.write_sample(v as i16),
                (false, _) => writer.write_sample(v),
            }
            .map_err(encode_err)?;
        }
        writer.finalize().map_err(encode_err)?;
        Ok(out.into_inner())
    }

    fn encode_flac(&self, values: &[i32]) -> Result<Vec<u8>> {
        let config = flacenc::config::Encoder::default()
            .into_verified()
            .map_err(|(_, e)| encode_err(e))?;
        let source = flacenc::source::MemSource::from_samples(
            values,
            self.channels,
            self.bits as usize,
            self.sample_rate as usize,
        );
        let stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)
            .map_err(|e| encode_err(format!("{e:?}")))?;
        let mut sink = flacenc::bitsink::ByteSink::new();
        stream
            .write(&mut sink)
            .map_err(|e| encode_err(format!("{e:?}")))?;
        splice_flac(&self.source, sink.as_slice())
    }
}

impl Carrier for Pcm {
    fn format(&self) -> CarrierFormat {
        self.format
    }

    fn describe(&self) -> String {
        let frames = self.cover.values.len() / self.channels;
        let secs = frames as f64 / self.sample_rate as f64;
        format!(
            "{}:{:04.1}, {} ch, {} Hz, {}-bit{}",
            (secs / 60.0) as u64,
            secs % 60.0,
            self.channels,
            self.sample_rate,
            self.bits,
            if self.float { " float" } else { "" }
        )
    }

    fn cover(&self) -> &Cover {
        &self.cover
    }

    fn cover_mut(&mut self) -> &mut Cover {
        &mut self.cover
    }

    fn costs(&self) -> Vec<f32> {
        let values = &self.cover.values;
        let per_channel: Vec<Vec<f32>> = (0..self.channels)
            .into_par_iter()
            .map(|c| {
                let signal: Vec<f64> = values
                    .iter()
                    .skip(c)
                    .step_by(self.channels)
                    .map(|&v| self.sample(v))
                    .collect();
                cost::audio(&signal)
            })
            .collect();
        let mut costs: Vec<f32> = (0..values.len())
            .map(|i| per_channel[i % self.channels][i / self.channels])
            .collect();
        if self.float {
            for (c, &v) in costs.iter_mut().zip(values) {
                let f = f32::from_bits(v as u32);
                if f == 0.0 || !f.is_finite() || f.abs() == f32::MAX {
                    *c = f32::INFINITY;
                }
            }
        }
        costs
    }

    fn encode(&mut self) -> Result<Vec<u8>> {
        let values = &self.cover.values;
        match self.format {
            CarrierFormat::Flac => self.encode_flac(values),
            _ => self.encode_wav(values),
        }
    }
}

fn decode_err(e: impl std::fmt::Display) -> Error {
    Error::Decode(e.to_string())
}

fn encode_err(e: impl std::fmt::Display) -> Error {
    Error::Encode(e.to_string())
}

/// A metadata block: (type, raw bytes including its 4-byte header).
type FlacBlock<'a> = (u8, &'a [u8]);

/// Metadata blocks and the offset of the first audio frame.
fn flac_blocks(data: &[u8]) -> Result<(Vec<FlacBlock<'_>>, usize)> {
    let mut pos = 4;
    let mut blocks = Vec::new();
    loop {
        if pos + 4 > data.len() {
            return Err(decode_err("truncated FLAC metadata"));
        }
        let header = data[pos];
        let len = u32::from_be_bytes([0, data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        let end = pos + 4 + len;
        if end > data.len() {
            return Err(decode_err("truncated FLAC metadata"));
        }
        blocks.push((header & 0x7F, &data[pos..end]));
        pos = end;
        if header & 0x80 != 0 {
            return Ok((blocks, pos));
        }
    }
}

/// Fresh STREAMINFO and audio frames, with the original's tags, pictures and
/// other metadata. The seek table is dropped since frame offsets changed.
fn splice_flac(original: &[u8], fresh: &[u8]) -> Result<Vec<u8>> {
    const STREAMINFO: u8 = 0;
    const SEEKTABLE: u8 = 3;
    let (old, _) = flac_blocks(original)?;
    let (new, frames_at) = flac_blocks(fresh)?;
    let streaminfo = new
        .iter()
        .find(|(t, _)| *t == STREAMINFO)
        .ok_or_else(|| encode_err("encoder produced no STREAMINFO"))?;
    let blocks: Vec<&[u8]> = std::iter::once(streaminfo.1)
        .chain(
            old.iter()
                .filter(|(t, _)| *t != STREAMINFO && *t != SEEKTABLE)
                .map(|(_, raw)| *raw),
        )
        .collect();

    let mut out = b"fLaC".to_vec();
    for (i, raw) in blocks.iter().enumerate() {
        let start = out.len();
        out.extend_from_slice(raw);
        out[start] &= 0x7F;
        if i == blocks.len() - 1 {
            out[start] |= 0x80;
        }
    }
    out.extend_from_slice(&fresh[frames_at..]);
    Ok(out)
}
