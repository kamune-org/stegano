//! Syndrome-trellis codes (Filler, Judas & Fridrich, 2011).
//!
//! The receiver reads the message as the syndrome `H·y` of the stego parity
//! bits `y`, where `H` is a banded parity-check matrix built from a small
//! random `h × w` submatrix. The sender runs the Viterbi algorithm over the
//! trellis of `H` to find the `y` with that syndrome that has the lowest total
//! distortion. Only the sender needs the per-element costs, so it can use any
//! content-adaptive cost function while the receiver needs nothing but the
//! passphrase.
//!
//! Long covers are cut into independent chunks (processed in parallel) to
//! bound the Viterbi path memory to a few MiB per chunk.

use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};

use rand_chacha::ChaCha20Rng;
use rand_chacha::rand_core::{Rng, SeedableRng};
use rayon::prelude::*;
use sha2::{Digest, Sha256};

/// Cancellation and batched progress for one STC call.
///
/// `on_blocks` receives how many message bits finished since the previous
/// call: every [`BIT_BATCH`] bits, then the remainder. The cancel flag is
/// loaded on that same cadence. When both fields are `None` the trellis loop
/// does neither, which is the CLI path.
#[derive(Copy, Clone)]
pub(crate) struct StcOpts<'a> {
    pub cancel: Option<&'a AtomicBool>,
    /// Called with the number of message bits just finished.
    pub on_blocks: Option<&'a (dyn Fn(usize) + Sync)>,
}

impl StcOpts<'static> {
    #[allow(dead_code)] // used by the crate tests; embed builds its own opts
    pub(crate) fn none() -> Self {
        Self {
            cancel: None,
            on_blocks: None,
        }
    }
}

/// Message bits per progress callback and cancel poll.
const BIT_BATCH: usize = 1024;

#[inline(always)]
fn report_blocks(opts: &StcOpts<'_>, n: usize) {
    if let Some(on_blocks) = opts.on_blocks {
        on_blocks(n);
    }
}

#[derive(Debug)]
pub(crate) enum StcError {
    /// The message only fits by changing an infinite-cost element.
    Wet,
    Cancelled,
}

fn cancelled(flag: Option<&AtomicBool>) -> bool {
    flag.is_some_and(|f| f.load(Ordering::Relaxed))
}

/// Viterbi path memory per chunk, in bits (8 MiB).
const PATH_BITS_PER_CHUNK: usize = 1 << 26;

struct Chunk {
    cols: Range<usize>,
    bits: Range<usize>,
}

struct Plan {
    h: u32,
    chunks: Vec<Chunk>,
}

/// Constraint height trades embedding efficiency for speed (cost is O(n·2^h)).
/// It depends only on `n` and `m`, so the receiver derives the same plan.
fn plan(n: usize, m: usize) -> Plan {
    let h = match n {
        _ if n <= 1 << 21 => 10,
        _ if n <= 1 << 23 => 9,
        _ if n <= 1 << 25 => 8,
        _ => 7,
    };
    let per_chunk = PATH_BITS_PER_CHUNK >> h;
    let count = n.div_ceil(per_chunk).min(m / 64).max(1);
    let split = |total: usize, k: usize| (total as u128 * k as u128 / count as u128) as usize;
    let chunks = (0..count)
        .map(|k| Chunk {
            cols: split(n, k)..split(n, k + 1),
            bits: split(m, k)..split(m, k + 1),
        })
        .collect();
    Plan { h, chunks }
}

/// Columns of the submatrix. Top and bottom rows are always set, which keeps
/// every block able to reach every syndrome bit.
fn submatrix(h: u32, w: usize) -> Vec<u32> {
    let mut seed = Sha256::new();
    seed.update(b"stegano/v2/stc");
    seed.update(h.to_be_bytes());
    seed.update((w as u64).to_be_bytes());
    let mut rng = ChaCha20Rng::from_seed(seed.finalize().into());
    let mask = (1u32 << h) - 1;
    (0..w)
        .map(|_| (rng.next_u32() & mask) | 1 | (1 << (h - 1)))
        .collect()
}

/// Column range of block `i` when `n` columns carry `m` bits.
fn block(i: usize, n: usize, m: usize) -> Range<usize> {
    let at = |k: usize| (k as u128 * n as u128 / m as u128) as usize;
    at(i)..at(i + 1)
}

/// Embed `msg` into `n` cover elements, choosing the cheapest changes.
/// `element(j)` returns the parity bit and change cost of element `j`; it is
/// called from worker threads, chunk by chunk. Returns the (sorted) indices of
/// the elements whose parity must flip, or [`StcError::Wet`] if the message can
/// only be embedded by touching elements with infinite cost.
pub(crate) fn embed(
    n: usize,
    msg: &[u8],
    element: impl Fn(usize) -> (u8, f32) + Sync,
    opts: &StcOpts<'_>,
) -> Result<Vec<usize>, StcError> {
    assert!(msg.len() <= n);
    let plan = plan(n, msg.len());
    let parts = plan
        .chunks
        .par_iter()
        .map(|c| {
            if cancelled(opts.cancel) {
                return Err(StcError::Cancelled);
            }
            let (x, rho): (Vec<u8>, Vec<f32>) = c.cols.clone().map(&element).unzip();
            let y = viterbi(&x, &rho, &msg[c.bits.clone()], plan.h, opts)?;
            Ok((0..x.len())
                .filter(|&j| x[j] != y[j])
                .map(|j| c.cols.start + j)
                .collect::<Vec<_>>())
        })
        .collect::<Result<Vec<_>, StcError>>()?;
    Ok(parts.concat())
}

/// Read `m` message bits back out of `n` elements, given each one's parity.
pub(crate) fn extract(
    n: usize,
    m: usize,
    parity: impl Fn(usize) -> u8 + Sync,
    opts: &StcOpts<'_>,
) -> Result<Vec<u8>, StcError> {
    let plan = plan(n, m);
    let parts = plan
        .chunks
        .par_iter()
        .map(|c| {
            if cancelled(opts.cancel) {
                return Err(StcError::Cancelled);
            }
            let y: Vec<u8> = c.cols.clone().map(&parity).collect();
            syndrome(&y, c.bits.len(), plan.h, opts)
        })
        .collect::<Result<Vec<_>, StcError>>()?;
    Ok(parts.concat())
}

fn syndrome(y: &[u8], m: usize, h: u32, opts: &StcOpts<'_>) -> Result<Vec<u8>, StcError> {
    if m == 0 {
        return Ok(Vec::new());
    }
    let n = y.len();
    let hhat = submatrix(h, n.div_ceil(m));
    let mut window = 0u32;
    let mut out = Vec::with_capacity(m);
    // No flag and no callback: the original per-bit loop, with nothing extra.
    if opts.cancel.is_none() && opts.on_blocks.is_none() {
        for i in 0..m {
            window = syndrome_bit(y, &hhat, h, n, m, i, window, &mut out);
        }
        return Ok(out);
    }
    let mut pending = 0usize;
    for i in 0..m {
        if pending == 0 && cancelled(opts.cancel) {
            return Err(StcError::Cancelled);
        }
        window = syndrome_bit(y, &hhat, h, n, m, i, window, &mut out);
        pending += 1;
        if pending == BIT_BATCH {
            report_blocks(opts, pending);
            pending = 0;
        }
    }
    if pending > 0 {
        report_blocks(opts, pending);
    }
    Ok(out)
}

#[inline(always)]
#[allow(clippy::too_many_arguments)]
fn syndrome_bit(
    y: &[u8],
    hhat: &[u32],
    h: u32,
    n: usize,
    m: usize,
    i: usize,
    mut window: u32,
    out: &mut Vec<u8>,
) -> u32 {
    let mask = row_mask(h, m - i);
    let cols = block(i, n, m);
    for (j, &bit) in y[cols].iter().enumerate() {
        if bit == 1 {
            window ^= hhat[j] & mask;
        }
    }
    out.push((window & 1) as u8);
    window >> 1
}

/// Near the end of the message the band runs off the bottom of `H`.
fn row_mask(h: u32, rows_left: usize) -> u32 {
    let rows = (rows_left as u32).min(h);
    if rows == 32 {
        u32::MAX
    } else {
        (1 << rows) - 1
    }
}

fn viterbi(
    x: &[u8],
    rho: &[f32],
    msg: &[u8],
    h: u32,
    opts: &StcOpts<'_>,
) -> Result<Vec<u8>, StcError> {
    let (n, m) = (x.len(), msg.len());
    if m == 0 {
        return Ok(x.to_vec());
    }
    let hhat = submatrix(h, n.div_ceil(m));
    let states = 1usize << h;
    // One survivor bit per state per column, packed 8 states to a byte.
    let groups = states / LANES;
    let mut path = vec![0u8; n * groups];
    let mut cost = vec![f32::INFINITY; states];
    let mut next = vec![0f32; states];
    cost[0] = 0.0;

    // No flag and no callback: same column loop as before, no per-bit poll.
    if opts.cancel.is_none() && opts.on_blocks.is_none() {
        for (i, &bit) in msg.iter().enumerate() {
            advance_bit(
                i, bit, n, m, h, &hhat, x, rho, groups, states, &mut path, &mut cost, &mut next,
            )?;
        }
    } else {
        let mut pending = 0usize;
        for (i, &bit) in msg.iter().enumerate() {
            if pending == 0 && cancelled(opts.cancel) {
                return Err(StcError::Cancelled);
            }
            advance_bit(
                i, bit, n, m, h, &hhat, x, rho, groups, states, &mut path, &mut cost, &mut next,
            )?;
            pending += 1;
            if pending == BIT_BATCH {
                report_blocks(opts, pending);
                pending = 0;
            }
        }
        if pending > 0 {
            report_blocks(opts, pending);
        }
    }

    let (mut state, best) = cost
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.total_cmp(b.1))
        .map(|(s, &c)| (s, c))
        .ok_or(StcError::Wet)?;
    if !best.is_finite() {
        return Err(StcError::Wet);
    }

    let mut y = vec![0u8; n];
    for i in (0..m).rev() {
        state = (state << 1) | msg[i] as usize;
        let mask = row_mask(h, m - i);
        let cols = block(i, n, m);
        for j in cols.clone().rev() {
            let take = (path[j * groups + state / LANES] >> (state % LANES)) & 1;
            y[j] = take as u8;
            if take == 1 {
                state ^= (hhat[j - cols.start] & mask) as usize;
            }
        }
    }
    debug_assert_eq!(state, 0);
    Ok(y)
}

/// One message bit: ACS across its columns, then shift that bit out of the window.
#[inline(always)]
#[allow(clippy::too_many_arguments)]
fn advance_bit(
    i: usize,
    bit: u8,
    n: usize,
    m: usize,
    h: u32,
    hhat: &[u32],
    x: &[u8],
    rho: &[f32],
    groups: usize,
    states: usize,
    path: &mut [u8],
    cost: &mut Vec<f32>,
    next: &mut Vec<f32>,
) -> Result<(), StcError> {
    let mask = row_mask(h, m - i);
    let cols = block(i, n, m);
    for j in cols.clone() {
        let col = (hhat[j - cols.start] & mask) as usize;
        // Cost of making the stego bit 0 or 1.
        let (c0, c1) = if x[j] == 0 {
            (0.0, rho[j])
        } else {
            (rho[j], 0.0)
        };
        let p = &mut path[j * groups..(j + 1) * groups];
        let hi = col & !(LANES - 1);
        // Monomorphize on the low bits of `col` so the in-group shuffle is
        // a constant and the loop vectorizes.
        match col & (LANES - 1) {
            0 => acs::<0>(cost, next, p, hi, c0, c1),
            1 => acs::<1>(cost, next, p, hi, c0, c1),
            2 => acs::<2>(cost, next, p, hi, c0, c1),
            3 => acs::<3>(cost, next, p, hi, c0, c1),
            4 => acs::<4>(cost, next, p, hi, c0, c1),
            5 => acs::<5>(cost, next, p, hi, c0, c1),
            6 => acs::<6>(cost, next, p, hi, c0, c1),
            _ => acs::<7>(cost, next, p, hi, c0, c1),
        }
        std::mem::swap(cost, next);
    }

    // Row i leaves the window; only states whose low bit matches msg[i] survive.
    let bit = bit as usize;
    let mut floor = f32::INFINITY;
    for s in 0..states / 2 {
        next[s] = cost[(s << 1) | bit];
        floor = floor.min(next[s]);
    }
    next[states / 2..].fill(f32::INFINITY);
    if !floor.is_finite() {
        return Err(StcError::Wet);
    }
    // Renormalize so accumulated costs stay within f32 precision.
    for c in &mut next[..states / 2] {
        *c -= floor;
    }
    std::mem::swap(cost, next);
    Ok(())
}

const LANES: usize = 8;

/// One trellis column: for every state `s`, keep the cheaper of staying (stego
/// bit 0) or arriving from `s ^ col` (stego bit 1), recording the choice.
/// States are processed in groups of `LANES`; the partner of group `g` is
/// group `g ^ hi`, with lanes permuted by `LO`.
#[inline(always)]
fn acs<const LO: usize>(
    cost: &[f32],
    next: &mut [f32],
    path: &mut [u8],
    hi: usize,
    c0: f32,
    c1: f32,
) {
    let groups = cost.as_chunks::<LANES>().0;
    let hi = hi / LANES;
    for (g, (out, bits)) in next
        .as_chunks_mut::<LANES>()
        .0
        .iter_mut()
        .zip(path.iter_mut())
        .enumerate()
    {
        let (own, other) = (&groups[g], &groups[g ^ hi]);
        let mut mask = 0u8;
        for l in 0..LANES {
            let keep = own[l] + c0;
            let flip = other[l ^ LO] + c1;
            let take = flip < keep;
            out[l] = if take { flip } else { keep };
            mask |= (take as u8) << l;
        }
        *bits = mask;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{RngExt, SeedableRng, rngs::StdRng};

    fn stego_bits(cover: &[u8], costs: &[f32], msg: &[u8]) -> Option<Vec<u8>> {
        let flips = embed(cover.len(), msg, |j| (cover[j], costs[j]), &StcOpts::none()).ok()?;
        let mut y = cover.to_vec();
        for j in flips {
            y[j] ^= 1;
        }
        Some(y)
    }

    fn read(y: &[u8], m: usize) -> Vec<u8> {
        extract(y.len(), m, |j| y[j], &StcOpts::none()).unwrap()
    }

    fn random_case(n: usize, m: usize, seed: u64) -> (Vec<u8>, Vec<f32>, Vec<u8>) {
        let mut rng = StdRng::seed_from_u64(seed);
        let cover = (0..n).map(|_| rng.random::<u8>() & 1).collect();
        let costs = (0..n).map(|_| rng.random_range(0.1f32..10.0)).collect();
        let msg = (0..m).map(|_| rng.random::<u8>() & 1).collect();
        (cover, costs, msg)
    }

    #[test]
    fn roundtrip_various_rates() {
        for (n, m) in [
            (1000, 1),
            (1000, 100),
            (1000, 500),
            (1000, 1000),
            (12_345, 777),
            (300_000, 20_000),
        ] {
            let (cover, costs, msg) = random_case(n, m, n as u64 ^ m as u64);
            let stego = stego_bits(&cover, &costs, &msg).unwrap();
            assert_eq!(read(&stego, m), msg, "n={n} m={m}");
        }
    }

    #[test]
    fn beats_naive_embedding() {
        // At rate 1/10, plain LSB replacement changes ~50% of 1 in 10 elements
        // (5%). STC with h=10 needs well under half that.
        let (cover, costs, msg) = random_case(100_000, 10_000, 9);
        let stego = stego_bits(&cover, &vec![1.0; costs.len()], &msg).unwrap();
        let changes = cover.iter().zip(&stego).filter(|(a, b)| a != b).count();
        assert!(changes < 2_200, "changes = {changes}");
    }

    #[test]
    fn avoids_wet_elements() {
        let (cover, mut costs, msg) = random_case(20_000, 1_000, 4);
        for c in costs.iter_mut().step_by(2) {
            *c = f32::INFINITY;
        }
        let stego = stego_bits(&cover, &costs, &msg).unwrap();
        for (i, (a, b)) in cover.iter().zip(&stego).enumerate() {
            if a != b {
                assert!(costs[i].is_finite());
            }
        }
        assert_eq!(read(&stego, msg.len()), msg);
    }
}
