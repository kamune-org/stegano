//! How cover elements are split between the salt, the header and the body.
//!
//! 1. A *public* permutation (seeded only by the carrier format and size) picks
//!    the salt segment. The salt is random, so reading it from any carrier,
//!    stego or not, just yields noise.
//! 2. The remaining elements are shuffled with a key derived from the
//!    passphrase and salt, and split into the header and body segments.
//!
//! No segment has a fixed location, and nothing in the cover is plaintext.

use rand_chacha::ChaCha20Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};
use sha2::{Digest, Sha256};

use crate::crypto::{SALT_LEN, TAG_LEN};

/// Header plaintext: u8 version, 3 reserved bytes, u32 body length.
pub(crate) const HEADER_PLAIN_LEN: usize = 8;
pub(crate) const HEADER_LEN: usize = HEADER_PLAIN_LEN + TAG_LEN;

/// The body never uses more than half the remaining elements. Past that the
/// change rate becomes obvious to any steganalysis tool.
pub(crate) const MAX_BODY_RATE: f64 = 0.5;

/// Smallest cover we accept: room for the fixed segments plus a small body.
pub(crate) const MIN_ELEMENTS: usize = 16 * 1024;

fn salt_segment_len(n: usize) -> usize {
    (n / 128).max(SALT_LEN * 8 * 8)
}

fn header_segment_len(n: usize) -> usize {
    (3 * n / 256).max(HEADER_LEN * 8 * 8)
}

/// Body capacity in bits for a cover of `n` elements.
pub(crate) fn body_capacity_bits(n: usize) -> usize {
    let body = n.saturating_sub(salt_segment_len(n) + header_segment_len(n));
    (body as f64 * MAX_BODY_RATE) as usize
}

/// All element positions in public-permutation order, plus where the salt
/// segment ends. The caller shuffles `order[salt_end..]` with
/// [`keyed_shuffle`] once it has the key; the header segment then follows the
/// salt and the body takes the rest.
pub(crate) fn public_order(n: usize, format_tag: &str) -> (Vec<u32>, usize) {
    let mut h = Sha256::new();
    h.update(b"stegano/v2/public");
    h.update(format_tag.as_bytes());
    h.update((n as u64).to_be_bytes());
    let mut all: Vec<u32> = (0..n as u32).collect();
    shuffle(&mut all, h.finalize().into());
    (all, salt_segment_len(n))
}

pub(crate) fn keyed_shuffle(tail: &mut [u32], key: &[u8; 32]) {
    shuffle(tail, *key);
}

/// End of the header segment within the order returned by [`public_order`].
pub(crate) fn header_end(n: usize) -> usize {
    salt_segment_len(n) + header_segment_len(n)
}

/// Fisher-Yates driven only by ChaCha20 output words, so the permutation is
/// identical on every platform and pointer width.
pub(crate) fn shuffle(v: &mut [u32], seed: [u8; 32]) {
    let mut rng = ChaCha20Rng::from_seed(seed);
    for i in (1..v.len()).rev() {
        let j = bounded(&mut rng, i as u32 + 1);
        v.swap(i, j as usize);
    }
}

/// Unbiased integer in `0..bound` (Lemire's method).
fn bounded(rng: &mut ChaCha20Rng, bound: u32) -> u32 {
    let threshold = bound.wrapping_neg() % bound;
    loop {
        let m = u64::from(rng.next_u32()) * u64::from(bound);
        if (m as u32) >= threshold {
            return (m >> 32) as u32;
        }
    }
}

pub(crate) fn to_bits(bytes: &[u8]) -> Vec<u8> {
    bytes
        .iter()
        .flat_map(|b| (0..8).rev().map(move |i| (b >> i) & 1))
        .collect()
}

pub(crate) fn from_bits(bits: &[u8]) -> Vec<u8> {
    bits.chunks(8)
        .map(|c| c.iter().fold(0u8, |acc, &b| (acc << 1) | b))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_partition_the_cover() {
        let n = 100_000;
        let (mut all, salt_end) = public_order(n, "png");
        keyed_shuffle(&mut all[salt_end..], &[3; 32]);
        assert!(salt_end < header_end(n) && header_end(n) < n);
        all.sort_unstable();
        assert_eq!(all, (0..n as u32).collect::<Vec<_>>());
    }

    #[test]
    fn shuffle_is_keyed_and_stable() {
        let mut a: Vec<u32> = (0..1000).collect();
        let mut b = a.clone();
        let mut c = a.clone();
        shuffle(&mut a, [1; 32]);
        shuffle(&mut b, [1; 32]);
        shuffle(&mut c, [2; 32]);
        assert_eq!(a, b);
        assert_ne!(a, c);
        // Pin the output so an accidental algorithm change (which would break
        // every existing stego file) fails loudly.
        assert_eq!(&a[..6], &[385, 812, 291, 926, 200, 101]);
    }

    #[test]
    fn bits_roundtrip() {
        let bytes = b"\x00\xffStegano";
        assert_eq!(from_bits(&to_bits(bytes)), bytes);
    }
}
