//! Keyed, content-adaptive steganography for images and audio.
//!
//! # How a payload is hidden
//!
//! 1. The entries (text and files) are packed into a container, optionally
//!    compressed, and encrypted with ChaCha20-Poly1305 under a key derived with
//!    Argon2id from the passphrase and a fresh random salt.
//! 2. The carrier is reduced to a list of integer *cover elements*: pixel
//!    channels, JPEG AC coefficients or audio samples.
//! 3. The salt goes into elements chosen by a public permutation. Everything
//!    else (an encrypted header with the body length, then the body) goes into
//!    elements chosen by a passphrase-keyed permutation. Nothing is plaintext,
//!    nothing sits at a fixed position, and nothing can be read without the
//!    passphrase.
//! 4. Bits are embedded with syndrome-trellis codes, which find the fewest,
//!    least detectable ±1 changes according to a content-adaptive cost
//!    function (HILL for pixels, UERD for JPEG, a residual model for audio).
//!    ±1 changes (rather than overwriting the low bit) avoid the statistical
//!    traces classic LSB tools leave.

mod carrier;
mod container;
mod control;
mod cost;
mod crypto;
mod error;
mod layout;
mod pixels;
mod preview;
mod resize;
mod stc;

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use rand::{Rng, RngExt};
use rayon::prelude::*;

pub use carrier::{CarrierFormat, CarrierKind};
pub use container::{Entry, EntryKind};
pub use control::{Control, Stage};
pub use error::{Error, Result};
pub use preview::{thumbnail, waveform};
pub use resize::resize_image;

use crypto::{Keys, SALT_LEN, TAG_LEN};
use layout::{HEADER_LEN, HEADER_PLAIN_LEN, Layout, from_bits, to_bits};

const FORMAT_VERSION: u8 = 2;

/// Payload size, in bits per effective cover element, below which modern
/// steganalysis has a hard time telling stego from clean carriers.
const RECOMMENDED_RATE: f64 = 0.1;

#[derive(Clone, Debug)]
pub struct CarrierInfo {
    pub format: CarrierFormat,
    pub kind: CarrierKind,
    /// Human-readable summary, e.g. "1920 × 1080 RGB, 8-bit".
    pub description: String,
    /// Largest container (see [`payload_size`]) that fits, in bytes.
    pub capacity: usize,
    /// Largest container that keeps detection risk low, in bytes.
    pub recommended: usize,
    /// Pixel width and height. `None` for audio.
    pub dimensions: Option<(u32, u32)>,
    /// Sample rate in hertz. `None` for images.
    pub sample_rate: Option<u32>,
    /// Channel count. `None` for images.
    pub channels: Option<u16>,
    /// Audio frame count. `None` for images.
    pub frames: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct EmbedOptions {
    /// DEFLATE the payload when that makes it smaller.
    pub compress: bool,
}

impl Default for EmbedOptions {
    fn default() -> Self {
        EmbedOptions { compress: true }
    }
}

#[derive(Clone, Debug)]
pub struct EmbedReport {
    /// Bytes actually embedded (after compression and encryption).
    pub embedded_bytes: usize,
    /// Cover elements in the carrier.
    pub elements: usize,
    /// Cover elements changed by ±1.
    pub changed: usize,
    pub capacity: usize,
    pub recommended: usize,
}

pub struct Embedded {
    pub data: Vec<u8>,
    pub format: CarrierFormat,
    pub report: EmbedReport,
}

/// Size of the uncompressed container for these entries, to compare against
/// [`CarrierInfo::capacity`] before embedding.
pub fn payload_size(entries: &[Entry]) -> usize {
    container::encoded_size(entries)
}

/// Same as [`payload_size`] but from (file name length, data length) pairs, so
/// callers don't need to load files to check capacity.
pub fn estimate_payload_size(items: impl IntoIterator<Item = (usize, usize)>) -> usize {
    6 + items
        .into_iter()
        .map(|(name, data)| 19 + name + data)
        .sum::<usize>()
}

pub fn inspect(data: &[u8]) -> Result<CarrierInfo> {
    let carrier = carrier::open(data)?;
    let (capacity, recommended) = capacities(carrier.cover());
    let dimensions = carrier.dimensions();
    let (sample_rate, channels, frames) = match carrier.audio_details() {
        Some((rate, channels, frames)) => (Some(rate), Some(channels), Some(frames)),
        None => (None, None, None),
    };
    Ok(CarrierInfo {
        format: carrier.format(),
        kind: carrier.format().kind(),
        description: carrier.describe(),
        capacity,
        recommended,
        dimensions,
        sample_rate,
        channels,
        frames,
    })
}

fn capacities(cover: &carrier::Cover) -> (usize, usize) {
    let n = cover.values.len();
    if n < layout::MIN_ELEMENTS {
        return (0, 0);
    }
    let max_bits = layout::body_capacity_bits(n).min(cover.effective);
    let safe_bits = ((cover.effective as f64 * RECOMMENDED_RATE) as usize).min(max_bits);
    let bytes = |bits: usize| (bits / 8).saturating_sub(TAG_LEN);
    (bytes(max_bits), bytes(safe_bits))
}

/// Hide `entries` in `data`.
///
/// Same output as [`embed_with`] with [`Control::none`].
pub fn embed(
    data: &[u8],
    entries: &[Entry],
    passphrase: &str,
    options: &EmbedOptions,
) -> Result<Embedded> {
    embed_with(data, entries, passphrase, options, &Control::none())
}

/// Hide `entries` in `data`, reporting each stage and stopping when `ctl` is cancelled.
///
/// A cancelled run returns [`Error::Cancelled`] and does not produce a file.
/// The same inputs and the same RNG draws still produce the same stego bytes:
/// progress callbacks must not draw from the embed RNG.
pub fn embed_with(
    data: &[u8],
    entries: &[Entry],
    passphrase: &str,
    options: &EmbedOptions,
    ctl: &Control,
) -> Result<Embedded> {
    ctl.check()?;
    let mut carrier = carrier::open(data)?;
    ctl.check()?;
    let format = carrier.format();
    let n = carrier.cover().values.len();
    if n < layout::MIN_ELEMENTS {
        return Err(Error::UnsuitableCarrier("carrier is too small".into()));
    }

    let (capacity, recommended) = capacities(carrier.cover());
    let container = container::encode(entries, options.compress)?;
    if container.len() > capacity {
        return Err(Error::PayloadTooLarge {
            needed: container.len(),
            available: capacity,
        });
    }
    ctl.check()?;

    let mut rng = rand::rng();
    let mut salt = [0u8; SALT_LEN];
    rng.fill_bytes(&mut salt);
    // Key derivation and cost computation are independent; overlap them.
    // The salt is already drawn, and neither side touches `rng`.
    ctl.report(Stage::Analyzing, 0.0);
    let (keys, costs) = rayon::join(
        || {
            ctl.check()?;
            ctl.report(Stage::Keying, 0.0);
            let keys = Keys::derive(passphrase, &salt);
            ctl.check()?;
            if keys.is_ok() {
                ctl.report(Stage::Keying, 1.0);
            }
            keys
        },
        || carrier.costs(ctl),
    );
    let keys = keys?;
    let costs = costs?;
    ctl.report(Stage::Analyzing, 1.0);
    ctl.check()?;
    let body = keys.seal_body(&container)?;
    let mut header = [0u8; HEADER_PLAIN_LEN];
    header[0] = FORMAT_VERSION;
    header[4..].copy_from_slice(&(body.len() as u32).to_be_bytes());
    let header = keys.seal_header(&header)?;
    debug_assert_eq!(header.len(), HEADER_LEN);

    let mut layout = Layout::new(n, format.tag());
    layout.set_key(&keys.permutation);

    // The segments use disjoint elements, so they embed independently. The
    // salt and header are tiny but run the slowest trellis; overlap them with
    // the body instead of leaving cores idle.
    let cover = carrier.cover_mut();
    let values = &cover.values;
    let segments = [
        (0..layout.salt_end, &salt[..]),
        (layout.salt_end..layout.header_end, &header[..]),
        (layout.header_end..n, &body[..]),
    ];
    let total_bits = segments.iter().map(|(_, bytes)| bytes.len() * 8).sum();
    let meter = BlockMeter::new(ctl, Stage::Embedding, 0.0, 1.0, total_bits);
    let on_blocks = |n| meter.add(n);
    let opts = stc::StcOpts {
        cancel: ctl.cancel,
        on_blocks: ctl
            .progress
            .is_some()
            .then_some(&on_blocks as &(dyn Fn(usize) + Sync)),
    };
    ctl.check()?;
    ctl.report(Stage::Embedding, 0.0);
    let flips = segments
        .into_par_iter()
        .map(|(segment, bytes)| {
            let changes = stc::embed(
                segment.len(),
                &to_bits(bytes),
                |j| {
                    let p = layout.position(segment.start + j);
                    (parity(values[p]), costs[p])
                },
                &opts,
            )
            .map_err(map_embed_stc)?;
            Ok(changes
                .into_iter()
                .map(|j| layout.position(segment.start + j))
                .collect::<Vec<_>>())
        })
        .collect::<Result<Vec<_>, Error>>()?
        .concat();
    drop(costs);
    meter.finish();
    ctl.check()?;

    for &p in &flips {
        let v = &mut cover.values[p];
        *v = if *v <= cover.lo {
            *v + 1
        } else if *v >= cover.hi {
            *v - 1
        } else if rng.random() {
            *v + 1
        } else {
            *v - 1
        };
    }

    ctl.check()?;
    ctl.report(Stage::Encoding, 0.0);
    let out = carrier.encode()?;
    ctl.report(Stage::Encoding, 1.0);
    ctl.check()?;
    ctl.report(Stage::Verifying, 0.0);
    // Re-read the output to make sure the codec preserved every element.
    let expected = std::mem::take(&mut carrier.cover_mut().values);
    drop(carrier);
    if carrier::open(&out)?.cover().values != expected {
        return Err(Error::Internal(
            "re-encoded carrier did not preserve the payload".into(),
        ));
    }
    ctl.report(Stage::Verifying, 1.0);

    Ok(Embedded {
        data: out,
        format,
        report: EmbedReport {
            embedded_bytes: body.len(),
            elements: n,
            changed: flips.len(),
            capacity,
            recommended,
        },
    })
}

/// Read hidden entries from `data`.
///
/// Same result as [`extract_with`] with [`Control::none`].
pub fn extract(data: &[u8], passphrase: &str) -> Result<Vec<Entry>> {
    extract_with(data, passphrase, &Control::none())
}

/// Read hidden entries from `data`, reporting each stage and stopping when `ctl` is cancelled.
pub fn extract_with(data: &[u8], passphrase: &str, ctl: &Control) -> Result<Vec<Entry>> {
    ctl.check()?;
    let carrier = carrier::open(data)?;
    ctl.check()?;
    let values = &carrier.cover().values;
    let n = values.len();
    if n < layout::MIN_ELEMENTS {
        return Err(Error::NotFound);
    }
    let mut layout = Layout::new(n, carrier.format().tag());
    ctl.report(Stage::Extracting, 0.0);

    let salt = read_bits(
        ctl,
        values,
        &layout,
        0..layout.salt_end,
        SALT_LEN,
        0.0,
        0.05,
    )?;
    let salt: [u8; SALT_LEN] = salt.try_into().unwrap();
    ctl.check()?;
    ctl.report(Stage::Keying, 0.0);
    let keys = Keys::derive(passphrase, &salt)?;
    ctl.check()?;
    ctl.report(Stage::Keying, 1.0);
    layout.set_key(&keys.permutation);

    let header_at = layout.salt_end..layout.header_end;
    let header = read_bits(ctl, values, &layout, header_at, HEADER_LEN, 0.05, 0.07)?;
    ctl.check()?;
    ctl.report(Stage::Decrypting, 0.0);
    let header = keys.open_header(&header)?;
    if header[0] != FORMAT_VERSION {
        return Err(Error::Corrupt(format!(
            "unsupported format version {}",
            header[0]
        )));
    }
    let body_len = u32::from_be_bytes(header[4..8].try_into().unwrap()) as usize;
    if body_len * 8 > n - layout.header_end {
        return Err(Error::Corrupt(
            "declared payload is larger than the carrier".into(),
        ));
    }
    ctl.check()?;
    let body = read_bits(
        ctl,
        values,
        &layout,
        layout.header_end..n,
        body_len,
        0.12,
        0.88,
    )?;
    let container = keys
        .open_body(&body)
        .map_err(|_| Error::Corrupt("the carrier was modified after the data was hidden".into()))?;
    let entries = container::decode(&container)?;
    ctl.report(Stage::Decrypting, 1.0);
    Ok(entries)
}

/// Counts finished STC message bits and reports a monotonic stage fraction.
///
/// Idle when the caller passed no progress callback: no atomics and no lock.
/// Otherwise one `fetch_add` per batch, and the lock is taken only to publish
/// a fraction strictly ahead of the last one this meter emitted.
struct BlockMeter<'a> {
    ctl: &'a Control<'a>,
    stage: Stage,
    start: f32,
    span: f32,
    total: usize,
    active: bool,
    done: AtomicUsize,
    reported: AtomicUsize,
    lock: Mutex<()>,
}

impl<'a> BlockMeter<'a> {
    fn new(ctl: &'a Control<'a>, stage: Stage, start: f32, span: f32, total: usize) -> Self {
        Self {
            ctl,
            stage,
            start,
            span,
            total,
            active: ctl.progress.is_some(),
            done: AtomicUsize::new(0),
            reported: AtomicUsize::new(0),
            lock: Mutex::new(()),
        }
    }

    fn add(&self, n: usize) {
        if !self.active || n == 0 {
            return;
        }
        let done = self.done.fetch_add(n, Ordering::Relaxed) + n;
        if done <= self.reported.load(Ordering::Relaxed) {
            return;
        }
        let _guard = self
            .lock
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if done <= self.reported.load(Ordering::Relaxed) {
            return;
        }
        self.reported.store(done, Ordering::Relaxed);
        let frac = if self.total == 0 {
            self.start + self.span
        } else {
            self.start + self.span * (done as f32 / self.total as f32)
        };
        self.ctl.report(self.stage, frac);
    }

    fn finish(&self) {
        if self.active {
            self.ctl.report(self.stage, self.start + self.span);
        }
    }
}

fn read_bits(
    ctl: &Control,
    values: &[i32],
    layout: &Layout,
    segment: std::ops::Range<usize>,
    bytes: usize,
    start: f32,
    span: f32,
) -> Result<Vec<u8>> {
    let bits = bytes.saturating_mul(8);
    let meter = BlockMeter::new(ctl, Stage::Extracting, start, span, bits);
    let on_blocks = |n| meter.add(n);
    let msg = stc::extract(
        segment.len(),
        bits,
        |j| parity(values[layout.position(segment.start + j)]),
        &stc::StcOpts {
            cancel: ctl.cancel,
            on_blocks: ctl
                .progress
                .is_some()
                .then_some(&on_blocks as &(dyn Fn(usize) + Sync)),
        },
    )
    .map_err(map_extract_stc)?;
    meter.finish();
    Ok(from_bits(&msg))
}

fn map_embed_stc(err: stc::StcError) -> Error {
    match err {
        stc::StcError::Cancelled => Error::Cancelled,
        stc::StcError::Wet => Error::UnsuitableCarrier(
            "not enough noisy or textured area to hide this payload \
             (large flat areas, transparency or silence can't be used)"
                .into(),
        ),
    }
}

fn map_extract_stc(err: stc::StcError) -> Error {
    match err {
        stc::StcError::Cancelled => Error::Cancelled,
        stc::StcError::Wet => Error::Internal("extract hit a wet element".into()),
    }
}

fn parity(v: i32) -> u8 {
    (v & 1) as u8
}

#[cfg(test)]
mod tests;
