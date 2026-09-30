//! Demosaicing for non-Bayer patterns (Fujifilm X-Trans and others): our own edge-weighted colour-difference
//! interpolation.
//!
//! 1. Green at non-green sites: robust weighted mean of the green samples in the 3×3 neighbourhood (5×5 if
//!    none), each weighted by `1 / (ε + |g − median|)` so samples across an edge contribute little.
//! 2. Red and blue everywhere: `G(p) + Σ w_q (C(q) − G(q)) / Σ w_q` over same-colour samples `q` in the 5×5
//!    neighbourhood, with `w_q = 1 / (d²(p, q) · (ε + |G(q) − G(p)|))` — spatially close samples on the same
//!    side of an edge dominate.

use super::Mosaic;
use crate::Rgb32f;
use lightcraft_raster::par_rows;

const EPS: f32 = 1e-3;

fn green(m: &Mosaic) -> Vec<f32> {
    let (w, h) = (m.w, m.h);
    let mut g = vec![0f32; w * h];
    par_rows(&mut g, w, |y, row| {
        let y = y as isize;
        let mut vals: Vec<f32> = Vec::with_capacity(24);
        for (x, o) in row.iter_mut().enumerate() {
            let x = x as isize;
            if m.color(x, y) == 1 {
                *o = m.get(x, y);
                continue;
            }
            vals.clear();
            for r in [1isize, 2] {
                for dy in -r..=r {
                    for dx in -r..=r {
                        if (dx != 0 || dy != 0) && m.color(x + dx, y + dy) == 1 {
                            vals.push(m.get(x + dx, y + dy));
                        }
                    }
                }
                if !vals.is_empty() {
                    break;
                }
            }
            if vals.is_empty() {
                *o = m.get(x, y);
                continue;
            }
            let mut sorted = vals.clone();
            sorted.sort_by(|a, b| a.total_cmp(b));
            let med = sorted[sorted.len() / 2];
            let (mut s, mut ws) = (0.0f32, 0.0f32);
            for &v in &vals {
                let wt = 1.0 / (EPS + (v - med).abs());
                s += wt * v;
                ws += wt;
            }
            *o = s / ws;
        }
    });
    g
}

pub(crate) fn directional(m: &Mosaic) -> Rgb32f {
    let (w, h) = (m.w, m.h);
    let g = green(m);
    let gg = |x: isize, y: isize| g[super::reflect(y, h) * w + super::reflect(x, w)];
    let mut out = Rgb32f::new(w, h);
    par_rows(&mut out.data, w, |y, row| {
        let y = y as isize;
        for (x, px) in row.iter_mut().enumerate() {
            let x = x as isize;
            let own = m.color(x, y) as usize;
            let gp = gg(x, y);
            px[1] = gp;
            for c in [0usize, 2] {
                if c == own {
                    px[c] = m.get(x, y);
                    continue;
                }
                let (mut s, mut ws) = (0.0f32, 0.0f32);
                for dy in -2isize..=2 {
                    for dx in -2isize..=2 {
                        if m.color(x + dx, y + dy) as usize != c {
                            continue;
                        }
                        let gq = gg(x + dx, y + dy);
                        let d2 = (dx * dx + dy * dy) as f32;
                        let wt = 1.0 / (d2 * (EPS + (gq - gp).abs()));
                        s += wt * (m.get(x + dx, y + dy) - gq);
                        ws += wt;
                    }
                }
                px[c] = if ws > 0.0 { gp + s / ws } else { gp };
            }
        }
    });
    out
}
