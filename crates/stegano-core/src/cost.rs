//! Content-adaptive distortion functions. Low cost means "a ±1 change here is
//! hard to detect" (noisy texture, busy audio); high cost means smooth areas,
//! flat colour, silence. STC uses these to put changes where they hide best.

/// Costs above this are treated as "practically never touch".
const MAX_COST: f32 = 1e6;

use rayon::prelude::*;

use crate::control::Control;
use crate::error::Result;

/// HILL (Li, Wang, Huang & Huang, 2014) for one image plane.
///
/// A 3×3 high-pass residual measures local texture, which is spread with a 3×3
/// box filter, inverted, and spread again with a 15×15 box filter so that
/// changes cluster in textured regions as a whole.
pub(crate) fn hill(plane: &[f32], width: usize, height: usize, ctl: &Control) -> Result<Vec<f32>> {
    const KB: [[f32; 3]; 3] = [[-1.0, 2.0, -1.0], [2.0, -4.0, 2.0], [-1.0, 2.0, -1.0]];
    ctl.check()?;
    let mut residual = vec![0f32; plane.len()];
    residual
        .par_chunks_mut(width)
        .enumerate()
        .try_for_each(|(y, row)| -> Result<()> {
            ctl.check()?;
            for (x, out) in row.iter_mut().enumerate() {
                let mut acc = 0.0;
                for (dy, k_row) in KB.iter().enumerate() {
                    let sy = reflect(y as isize + dy as isize - 1, height);
                    for (dx, k) in k_row.iter().enumerate() {
                        let sx = reflect(x as isize + dx as isize - 1, width);
                        acc += k * plane[sy * width + sx];
                    }
                }
                *out = acc.abs();
            }
            Ok(())
        })?;
    let mut texture = box_filter(&residual, width, height, 1, ctl)?;
    drop(residual);
    texture.par_iter_mut().try_for_each(|t| -> Result<()> {
        ctl.check()?;
        *t = 1.0 / (*t + 1e-6);
        Ok(())
    })?;
    Ok(clamp(box_filter(&texture, width, height, 7, ctl)?))
}

/// UERD (Guo, Ni, Su, Tang & Shi, 2015) for one JPEG component.
///
/// A coefficient's cost is its quantization step divided by the energy of its
/// own block plus a share of the energy of neighbouring blocks: busy blocks and
/// fine quantization are cheap, smooth blocks and coarse steps expensive.
/// Returns costs for the 63 AC coefficients of each block, in block order.
pub(crate) fn uerd(
    blocks: &[[i16; 64]],
    bw: usize,
    bh: usize,
    quant: &[u16; 64],
    ctl: &Control,
) -> Result<Vec<f32>> {
    ctl.check()?;
    let mut energy = Vec::with_capacity(blocks.len());
    for (i, b) in blocks.iter().enumerate() {
        if bw == 0 || i % bw == 0 {
            ctl.check()?;
        }
        energy.push(
            (1..64)
                .map(|k| (b[k] as f32 * quant[k] as f32).abs())
                .sum::<f32>(),
        );
    }

    let mut costs = Vec::with_capacity(blocks.len() * 63);
    for by in 0..bh {
        ctl.check()?;
        for bx in 0..bw {
            let mut d = energy[by * bw + bx];
            for (nx, ny) in [(-1isize, 0isize), (1, 0), (0, -1), (0, 1)] {
                let (x, y) = (bx as isize + nx, by as isize + ny);
                if (0..bw as isize).contains(&x) && (0..bh as isize).contains(&y) {
                    d += 0.25 * energy[y as usize * bw + x as usize];
                }
            }
            for &q in &quant[1..] {
                costs.push((q as f32 / (d + 1e-3)).min(MAX_COST));
            }
        }
    }
    Ok(costs)
}

/// Cost for one audio channel: the inverse of the local second-difference
/// residual, smoothed. Digital silence and constant runs are left untouched,
/// since any change there is trivially visible.
pub(crate) fn audio(samples: &[f64], ctl: &Control) -> Result<Vec<f32>> {
    ctl.check()?;
    let n = samples.len();
    let mut residual = Vec::with_capacity(n);
    for i in 0..n {
        if i.is_multiple_of(4096) {
            ctl.check()?;
        }
        let prev = samples[i.saturating_sub(1)];
        let next = samples[(i + 1).min(n - 1)];
        residual.push((2.0 * samples[i] - prev - next).abs() as f32);
    }
    let texture = box_filter_1d(&residual, 3, ctl)?;
    let mut inverse = Vec::with_capacity(texture.len());
    for (i, &t) in texture.iter().enumerate() {
        if i.is_multiple_of(4096) {
            ctl.check()?;
        }
        inverse.push(if t == 0.0 { f32::INFINITY } else { 1.0 / t });
    }
    let mut costs = box_filter_1d(&inverse, 7, ctl)?;
    // Anything whose own neighbourhood is silent stays wet even after smoothing.
    for (c, t) in costs.iter_mut().zip(&texture) {
        if *t == 0.0 {
            *c = f32::INFINITY;
        }
    }
    Ok(clamp(costs))
}

fn clamp(mut costs: Vec<f32>) -> Vec<f32> {
    for c in &mut costs {
        if c.is_finite() {
            *c = c.min(MAX_COST);
        } else if c.is_nan() {
            *c = f32::INFINITY;
        }
    }
    costs
}

fn reflect(i: isize, len: usize) -> usize {
    let len = len as isize;
    let mut i = i;
    if i < 0 {
        i = -i - 1;
    }
    if i >= len {
        i = 2 * len - i - 1;
    }
    i.clamp(0, len - 1) as usize
}

/// Mean over a (2r+1)×(2r+1) window with mirrored edges (separable).
/// Inputs must be finite.
fn box_filter(
    src: &[f32],
    width: usize,
    height: usize,
    r: usize,
    ctl: &Control,
) -> Result<Vec<f32>> {
    ctl.check()?;
    let mut tmp = vec![0f32; src.len()];
    tmp.par_chunks_mut(width)
        .zip(src.par_chunks(width))
        .try_for_each(|(out, row)| -> Result<()> {
            ctl.check()?;
            out.copy_from_slice(&box_filter_1d(row, r, ctl)?);
            Ok(())
        })?;

    // Vertical pass as a running sum of whole rows: cache-friendly, no transpose.
    let row = |y: isize| &tmp[reflect(y, height) * width..][..width];
    let k = (2 * r + 1) as f64;
    let mut acc = vec![0f64; width];
    for y in -(r as isize)..=r as isize {
        for (a, &v) in acc.iter_mut().zip(row(y)) {
            *a += v as f64;
        }
    }
    let mut out = vec![0f32; src.len()];
    for y in 0..height as isize {
        ctl.check()?;
        let dst = &mut out[y as usize * width..][..width];
        for (d, a) in dst.iter_mut().zip(&acc) {
            *d = (a / k) as f32;
        }
        let (leaving, entering) = (row(y - r as isize), row(y + r as isize + 1));
        for ((a, &l), &e) in acc.iter_mut().zip(leaving).zip(entering) {
            *a += e as f64 - l as f64;
        }
    }
    Ok(out)
}

fn box_filter_1d(src: &[f32], r: usize, ctl: &Control) -> Result<Vec<f32>> {
    ctl.check()?;
    let n = src.len();
    let k = (2 * r + 1) as f64;
    // f64 running sum: infinities propagate correctly and finite values don't drift.
    let at = |i: isize| src[reflect(i, n)] as f64;
    let mut sum: f64 = (-(r as isize)..=r as isize).map(at).sum();
    let mut out = Vec::with_capacity(n);
    for i in 0..n as isize {
        if (i as usize).is_multiple_of(4096) {
            ctl.check()?;
        }
        out.push((sum / k) as f32);
        let leaving = at(i - r as isize);
        let entering = at(i + r as isize + 1);
        sum = if leaving.is_infinite() || entering.is_infinite() || sum.is_infinite() {
            ((i + 1 - r as isize)..=(i + 1 + r as isize)).map(at).sum()
        } else {
            sum - leaving + entering
        };
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texture_is_cheaper_than_flat() {
        let (w, h) = (64, 64);
        let plane: Vec<f32> = (0..w * h)
            .map(|i| {
                if i % w < w / 2 {
                    128.0
                } else {
                    ((i * 7919) % 251) as f32
                }
            })
            .collect();
        let costs = hill(&plane, w, h, &Control::none()).unwrap();
        let flat = costs[32 * w + 5];
        let busy = costs[32 * w + 58];
        assert!(flat > 10.0 * busy, "flat={flat} busy={busy}");
    }

    #[test]
    fn silence_is_wet() {
        let mut s = vec![0.0; 100];
        s.extend((0..100).map(|i| ((i * 37) % 17) as f64 * 100.0));
        let c = audio(&s, &Control::none()).unwrap();
        assert!(c[10].is_infinite());
        assert!(c[150].is_finite());
    }

    #[test]
    fn box_filter_handles_infinity() {
        let v = [1.0, f32::INFINITY, 1.0, 1.0, 1.0, 1.0, 1.0];
        let out = box_filter_1d(&v, 1, &Control::none()).unwrap();
        assert!(out[0].is_infinite() && out[2].is_infinite());
        assert_eq!(out[4], 1.0);
    }
}
