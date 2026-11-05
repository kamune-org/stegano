//! How cover elements are split between the salt, the header and the body.
//!
//! Elements are visited in a *segment order*: indices `0..salt_end` hold the
//! salt, `salt_end..header_end` the header and the rest the body. Index `i`
//! maps to a cover position as follows:
//!
//! 1. The salt segment uses a public affine scatter (golden-ratio stride), so
//!    its positions are spread evenly over the cover. The salt is random, so
//!    reading it from any carrier, stego or not, just yields noise.
//! 2. Header and body indices first go through a keyed pseudorandom
//!    permutation (indices sorted by ChaCha20 keystream tags, keyed from the
//!    passphrase and salt), then through the same affine map.
//!
//! No segment has a fixed location, and nothing in the cover is plaintext.

use rand_chacha::ChaCha20Rng;
use rand_chacha::rand_core::{Rng, SeedableRng};
use rayon::prelude::*;
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

pub(crate) struct Layout {
    n: u64,
    pub salt_end: usize,
    pub header_end: usize,
    stride: u64,
    offset: u64,
    /// Keyed order of the non-salt indices (empty until a key is set).
    keyed: Vec<u32>,
}

impl Layout {
    /// Layout for `n` elements; only the salt segment is usable until
    /// [`Layout::set_key`] is called.
    pub fn new(n: usize, format_tag: &str) -> Self {
        let mut h = Sha256::new();
        h.update(b"stegano/v2/public");
        h.update(format_tag.as_bytes());
        h.update((n as u64).to_be_bytes());
        let digest = h.finalize();
        let n64 = n as u64;
        // Golden-ratio stride, nudged to be coprime with n so the map is a bijection.
        let mut stride = ((n64 as f64 * 0.618_033_988_749_895) as u64).max(1) | 1;
        while gcd(stride, n64) != 1 {
            stride += 2;
        }
        Layout {
            n: n64,
            salt_end: salt_segment_len(n),
            header_end: salt_segment_len(n) + header_segment_len(n),
            stride,
            offset: u64::from_be_bytes(digest[..8].try_into().unwrap()) % n64,
            keyed: Vec::new(),
        }
    }

    pub fn set_key(&mut self, key: &[u8; 32]) {
        self.keyed = keyed_order(key, self.n as usize - self.salt_end);
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.n as usize
    }

    /// Cover position of segment-order index `i`.
    pub fn position(&self, i: usize) -> usize {
        let i = i as u64;
        let j = if i < self.salt_end as u64 {
            i
        } else {
            self.salt_end as u64 + self.keyed[(i - self.salt_end as u64) as usize] as u64
        };
        ((j as u128 * self.stride as u128 + self.offset as u128) % self.n as u128) as usize
    }
}

fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// A keyed pseudorandom permutation of `0..len`: every index gets a 32-bit
/// tag from the ChaCha20 keystream (index `j` uses keystream word `j`), and
/// indices are sorted by tag, ties broken by index. Sorting by pseudorandom
/// tags gives a uniformly random permutation without the key; both steps run
/// in parallel, since the keystream can be entered at any word.
fn keyed_order(key: &[u8; 32], len: usize) -> Vec<u32> {
    const CHUNK: usize = 1 << 16;
    let mut tagged = vec![0u64; len];
    tagged
        .par_chunks_mut(CHUNK)
        .enumerate()
        .for_each(|(c, out)| {
            let mut rng = ChaCha20Rng::from_seed(*key);
            rng.set_word_pos((c * CHUNK) as u128);
            for (k, slot) in out.iter_mut().enumerate() {
                *slot = (u64::from(rng.next_u32()) << 32) | (c * CHUNK + k) as u64;
            }
        });
    tagged.par_sort_unstable();
    tagged.into_par_iter().map(|t| t as u32).collect()
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

    fn positions(layout: &Layout) -> Vec<usize> {
        (0..layout.len()).map(|i| layout.position(i)).collect()
    }

    #[test]
    fn layout_is_a_permutation() {
        for n in [16_384, 100_000, 131_072, 1_000_003] {
            let mut layout = Layout::new(n, "raster");
            layout.set_key(&[3; 32]);
            let mut all = positions(&layout);
            all.sort_unstable();
            assert!(all.iter().copied().eq(0..n), "n = {n}");
            assert!(layout.salt_end < layout.header_end && layout.header_end < n);
        }
    }

    #[test]
    fn keyed_order_depends_on_key_only_past_the_salt() {
        let n = 50_000;
        let mut a = Layout::new(n, "pcm");
        let mut b = Layout::new(n, "pcm");
        a.set_key(&[1; 32]);
        b.set_key(&[2; 32]);
        let (pa, pb) = (positions(&a), positions(&b));
        assert_eq!(pa[..a.salt_end], pb[..b.salt_end]);
        assert_ne!(pa[a.salt_end..], pb[b.salt_end..]);
        // Pin the output so an accidental algorithm change (which would break
        // every existing stego file) fails loudly.
        assert_eq!(&pa[..4], &[37_635, 18_536, 49_437, 30_338]);
        assert_eq!(
            &pa[a.salt_end..a.salt_end + 4],
            &[17_585, 28_486, 4_071, 48_484]
        );
    }

    #[test]
    fn bits_roundtrip() {
        let bytes = b"\x00\xffStegano";
        assert_eq!(from_bits(&to_bits(bytes)), bytes);
    }
}
