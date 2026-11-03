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
mod cost;
mod crypto;
mod error;
mod layout;
mod stc;

use rand::{Rng, RngCore};

pub use carrier::{CarrierFormat, CarrierKind};
pub use container::{Entry, EntryKind};
pub use error::{Error, Result};

use crypto::{Keys, SALT_LEN, TAG_LEN};
use layout::{HEADER_LEN, HEADER_PLAIN_LEN, from_bits, to_bits};

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
    Ok(CarrierInfo {
        format: carrier.format(),
        kind: carrier.format().kind(),
        description: carrier.describe(),
        capacity,
        recommended,
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

pub fn embed(
    data: &[u8],
    entries: &[Entry],
    passphrase: &str,
    options: &EmbedOptions,
) -> Result<Embedded> {
    let mut carrier = carrier::open(data)?;
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

    let mut rng = rand::thread_rng();
    let mut salt = [0u8; SALT_LEN];
    rng.fill_bytes(&mut salt);
    let keys = Keys::derive(passphrase, &salt)?;
    let body = keys.seal_body(&container)?;
    let mut header = [0u8; HEADER_PLAIN_LEN];
    header[0] = FORMAT_VERSION;
    header[4..].copy_from_slice(&(body.len() as u32).to_be_bytes());
    let header = keys.seal_header(&header)?;
    debug_assert_eq!(header.len(), HEADER_LEN);

    let (mut order, salt_end) = layout::public_order(n, format.tag());
    layout::keyed_shuffle(&mut order[salt_end..], &keys.permutation);
    let header_end = layout::header_end(n);

    // Gather costs and parities in embedding order once; drop the rest early.
    let rho: Vec<f32> = {
        let costs = carrier.costs();
        order.iter().map(|&p| costs[p as usize]).collect()
    };
    let cover = carrier.cover_mut();
    let x: Vec<u8> = order
        .iter()
        .map(|&p| parity(cover.values[p as usize]))
        .collect();

    let mut changed = 0;
    for (range, bytes) in [
        (0..salt_end, &salt[..]),
        (salt_end..header_end, &header[..]),
        (header_end..n, &body[..]),
    ] {
        let y = stc::embed(&x[range.clone()], &rho[range.clone()], &to_bits(bytes)).ok_or_else(
            || {
                Error::UnsuitableCarrier(
                    "not enough noisy or textured area to hide this payload \
                 (large flat areas, transparency or silence can't be used)"
                        .into(),
                )
            },
        )?;
        for ((&p, &a), b) in order[range.clone()].iter().zip(&x[range]).zip(y) {
            if a != b {
                let v = &mut cover.values[p as usize];
                *v = if *v <= cover.lo {
                    *v + 1
                } else if *v >= cover.hi {
                    *v - 1
                } else if rng.r#gen() {
                    *v + 1
                } else {
                    *v - 1
                };
                changed += 1;
            }
        }
    }
    drop((order, rho, x));

    let out = carrier.encode()?;
    // Re-read the output to make sure the codec preserved every element.
    let expected = std::mem::take(&mut carrier.cover_mut().values);
    drop(carrier);
    if carrier::open(&out)?.cover().values != expected {
        return Err(Error::Internal(
            "re-encoded carrier did not preserve the payload".into(),
        ));
    }

    Ok(Embedded {
        data: out,
        format,
        report: EmbedReport {
            embedded_bytes: body.len(),
            elements: n,
            changed,
            capacity,
            recommended,
        },
    })
}

pub fn extract(data: &[u8], passphrase: &str) -> Result<Vec<Entry>> {
    let carrier = carrier::open(data)?;
    let n = carrier.cover().values.len();
    if n < layout::MIN_ELEMENTS {
        return Err(Error::NotFound);
    }
    let (mut order, salt_end) = layout::public_order(n, carrier.format().tag());
    let header_end = layout::header_end(n);
    let parities = |range: std::ops::Range<usize>, order: &[u32]| -> Vec<u8> {
        let values = &carrier.cover().values;
        order[range]
            .iter()
            .map(|&p| parity(values[p as usize]))
            .collect()
    };
    let read = |y: Vec<u8>, bytes: usize| from_bits(&stc::extract(&y, bytes * 8));

    let salt: [u8; SALT_LEN] = read(parities(0..salt_end, &order), SALT_LEN)
        .try_into()
        .unwrap();
    let keys = Keys::derive(passphrase, &salt)?;
    layout::keyed_shuffle(&mut order[salt_end..], &keys.permutation);

    let header = keys.open_header(&read(parities(salt_end..header_end, &order), HEADER_LEN))?;
    if header[0] != FORMAT_VERSION {
        return Err(Error::Corrupt(format!(
            "unsupported format version {}",
            header[0]
        )));
    }
    let body_len = u32::from_be_bytes(header[4..8].try_into().unwrap()) as usize;
    if body_len * 8 > n - header_end {
        return Err(Error::Corrupt(
            "declared payload is larger than the carrier".into(),
        ));
    }
    let body = read(parities(header_end..n, &order), body_len);
    let container = keys
        .open_body(&body)
        .map_err(|_| Error::Corrupt("the carrier was modified after the data was hidden".into()))?;
    container::decode(&container)
}

fn parity(v: i32) -> u8 {
    (v & 1) as u8
}

#[cfg(test)]
mod tests;
