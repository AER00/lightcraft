//! Clip-aware highlight handling on demosaiced camera RGB (before white balance, white level = 1.0).
//!
//! - [`clip_neutral`]: white-balance-aware clipping so that sensor-clipped areas render neutral instead of
//!   magenta/cyan (each channel limited to the smallest white-balanced clip level).
//! - [`reconstruct`]: where only some channels are clipped, rebuild them from the unclipped channels using the
//!   local chromaticity of nearby unclipped pixels (diffused into the clipped region with a coarse-to-fine
//!   normalised-convolution fill). Fully clipped pixels become neutral at the brightest plausible level.

use lightcraft_raster::Rgb32f;

/// Clip every channel of `wb ⊙ img` at `min_c(wb_c) · clip` — i.e. at the lowest channel's clip level after WB —
/// then divide the multipliers back out. `clip` is the sensor clip in normalised units (≈ 1.0).
pub fn clip_neutral(img: &mut Rgb32f, wb: [f32; 3], clip: f32) {
    let limit = wb.iter().cloned().fold(f32::MAX, f32::min) * clip;
    for p in &mut img.data {
        for c in 0..3 {
            p[c] = (p[c] * wb[c]).min(limit) / wb[c];
        }
    }
}

/// Fill `values` (per-pixel vectors) where `valid` is false from valid neighbours, coarse to fine.
fn fill_invalid(w: usize, h: usize, values: &mut [[f32; 3]], valid: &mut [bool]) {
    if w == 0 || h == 0 || valid.iter().all(|&v| v) || !valid.iter().any(|&v| v) {
        return;
    }
    // Build a pyramid of weighted means, fill each level from the next coarser one.
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    let mut cv = vec![[0f32; 3]; cw * ch];
    let mut cval = vec![false; cw * ch];
    for y in 0..ch {
        for x in 0..cw {
            let (mut s, mut n) = ([0f32; 3], 0);
            for dy in 0..2 {
                for dx in 0..2 {
                    let (sx, sy) = (2 * x + dx, 2 * y + dy);
                    if sx < w && sy < h && valid[sy * w + sx] {
                        for c in 0..3 {
                            s[c] += values[sy * w + sx][c];
                        }
                        n += 1;
                    }
                }
            }
            if n > 0 {
                cv[y * cw + x] = s.map(|v| v / n as f32);
                cval[y * cw + x] = true;
            }
        }
    }
    if cw * ch < w * h {
        fill_invalid(cw, ch, &mut cv, &mut cval);
    } else {
        // cannot shrink further (1×1): nothing valid anywhere handled above
        return;
    }
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if !valid[i] {
                // bilinear upsample of the coarse level
                let fx = ((x as f32 + 0.5) / 2.0 - 0.5).clamp(0.0, (cw - 1) as f32);
                let fy = ((y as f32 + 0.5) / 2.0 - 0.5).clamp(0.0, (ch - 1) as f32);
                let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
                let (x1, y1) = ((x0 + 1).min(cw - 1), (y0 + 1).min(ch - 1));
                let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
                let g = |xx: usize, yy: usize| cv[yy * cw + xx];
                let mut v = [0f32; 3];
                for c in 0..3 {
                    let top = g(x0, y0)[c] + (g(x1, y0)[c] - g(x0, y0)[c]) * tx;
                    let bot = g(x0, y1)[c] + (g(x1, y1)[c] - g(x0, y1)[c]) * tx;
                    v[c] = top + (bot - top) * ty;
                }
                values[i] = v;
                valid[i] = true;
            }
        }
    }
}

/// Reconstruct partially clipped channels. `clip` is the sensor clip level in normalised units (use slightly
/// below 1.0, e.g. 0.99), `wb` the white-balance multipliers that will be applied afterwards.
/// Returns the number of pixels that had at least one clipped channel.
pub fn reconstruct(img: &mut Rgb32f, wb: [f32; 3], clip: f32) -> usize {
    let (w, h) = (img.width, img.height);
    let n = w * h;
    let clipped: Vec<[bool; 3]> = img.data.iter().map(|p| [p[0] >= clip, p[1] >= clip, p[2] >= clip]).collect();
    let count = clipped.iter().filter(|c| c.iter().any(|&b| b)).count();
    if count == 0 {
        return 0;
    }
    // chromaticity (white-balanced ratios to the channel mean) of unclipped pixels
    let mut chroma = vec![[1f32 / 3.0; 3]; n];
    let mut valid = vec![false; n];
    for i in 0..n {
        if clipped[i].iter().any(|&b| b) {
            continue;
        }
        let p = img.data[i];
        let q = [p[0] * wb[0], p[1] * wb[1], p[2] * wb[2]];
        let s = q[0] + q[1] + q[2];
        if s > 1e-4 {
            chroma[i] = q.map(|v| v.max(0.0) / s);
            valid[i] = true;
        }
    }
    fill_invalid(w, h, &mut chroma, &mut valid);
    let max_level = wb.iter().cloned().fold(0.0f32, f32::max) * clip;
    for i in 0..n {
        let cl = clipped[i];
        if !cl.iter().any(|&b| b) {
            continue;
        }
        let p = img.data[i];
        let q = [p[0] * wb[0], p[1] * wb[1], p[2] * wb[2]];
        let r = chroma[i];
        // estimate the white-balanced channel sum from the unclipped channels
        let ests: Vec<f32> = (0..3).filter(|&c| !cl[c] && r[c] > 1e-3).map(|c| q[c] / r[c]).collect();
        let mut out = q;
        if ests.is_empty() {
            let v = q.iter().cloned().fold(0.0f32, f32::max).max(max_level);
            out = [v; 3];
        } else {
            let sum = ests.iter().sum::<f32>() / ests.len() as f32;
            for c in 0..3 {
                if cl[c] {
                    out[c] = q[c].max(sum * r[c]);
                }
            }
        }
        img.data[i] = [out[0] / wb[0], out[1] / wb[1], out[2] / wb[2]];
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_neutral_makes_clipped_white_neutral() {
        let wb = [2.0, 1.0, 1.5];
        let mut img = Rgb32f::filled(2, 1, [1.0, 1.0, 1.0]);
        img.data[1] = [0.2, 0.3, 0.4];
        clip_neutral(&mut img, wb, 1.0);
        let p = img.data[0];
        let q = [p[0] * wb[0], p[1] * wb[1], p[2] * wb[2]];
        assert!((q[0] - q[1]).abs() < 1e-6 && (q[1] - q[2]).abs() < 1e-6);
        assert_eq!(img.data[1], [0.2, 0.3, 0.4]);
    }

    #[test]
    fn reconstruct_restores_clipped_channel() {
        // a warm gradient whose red channel clips in the bright half
        let truth = Rgb32f::from_fn(64, 16, |x, _| {
            let v = 0.3 + 1.2 * x as f32 / 63.0;
            [v * 1.0, v * 0.6, v * 0.35]
        });
        let mut img = truth.map(|p| p.map(|v| v.min(1.0)));
        let before: f32 = img.data.iter().zip(&truth.data).map(|(a, b)| (a[0] - b[0]).abs()).sum();
        let n = reconstruct(&mut img, [1.0, 1.0, 1.0], 0.999);
        assert!(n > 0);
        let after: f32 = img.data.iter().zip(&truth.data).map(|(a, b)| (a[0] - b[0]).abs()).sum();
        assert!(after < before * 0.2, "before {before} after {after}");
        // unclipped pixels untouched
        assert_eq!(img.get(0, 0), truth.get(0, 0));
    }

    #[test]
    fn fully_clipped_and_no_clipping() {
        let mut img = Rgb32f::filled(4, 4, [1.0; 3]);
        assert_eq!(reconstruct(&mut img, [2.0, 1.0, 1.5], 0.99), 16);
        let p = img.data[0];
        assert!((p[0] * 2.0 - p[1]).abs() < 1e-5 && (p[2] * 1.5 - p[1]).abs() < 1e-5);
        let mut img = Rgb32f::filled(4, 4, [0.5; 3]);
        assert_eq!(reconstruct(&mut img, [2.0, 1.0, 1.5], 0.99), 0);
        let mut empty = Rgb32f::new(0, 0);
        assert_eq!(reconstruct(&mut empty, [1.0; 3], 0.99), 0);
    }
}
