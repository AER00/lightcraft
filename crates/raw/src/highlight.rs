//! Clip-aware highlight handling on demosaiced camera RGB (before white balance, white level = 1.0).
//!
//! - [`clip_neutral`]: white-balance-aware clipping so that sensor-clipped areas render neutral instead of
//!   magenta/cyan (each channel limited to the smallest white-balanced clip level).
//! - [`reconstruct`]: where only some channels are clipped, rebuild them from the unclipped channels using the
//!   local chromaticity of nearby unclipped pixels (diffused into the clipped region with a coarse-to-fine
//!   push-pull fill). Fully clipped pixels become neutral at the brightest plausible level.
//!
//! Issue #548 (a bright cloud turning cyan once Highlights pulled it down, green speckles in binned
//! previews) set three rules for the colour of partly clipped pixels:
//! - an unclipped pixel's chromaticity counts with a confidence that grows with its brightness
//!   (`confidence`): the colour of a clipped highlight is told by the bright pixels around it, not by
//!   dark ones in front of it (wires and branches across a cloud, with their own colour and demosaicing
//!   fringes, were the nearest colour inside the cloud and tinted all of it). A dim pixel fills in only as
//!   much as its confidence allows, the rest comes from the coarser level, i.e. from brighter pixels
//!   further out; where there are none, dim pixels still decide, pooled over a larger area;
//! - pixels within [`DEMOSAIC_REACH`] of a clipped one were demosaiced partly from clipped samples and
//!   give no colour (unless nothing else does);
//! - the more channels of a pixel clip, the nearer to neutral its rebuilt colour (`rebuild`).
//!
//! A binned image ([`crate::RawImage::develop_binned_masked`]) says which channels are clipped with a
//! mask instead of by value ([`reconstruct_masked`]).

use lightcraft_raster::Rgb32f;
use rayon::prelude::*;

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

/// A chromaticity premultiplied by its confidence, and the confidence (0..=1): `[w·r, w·g, w·b, w]`.
/// `w = 0` is an empty entry (a clipped pixel, or nothing known yet).
type Weighted = [f32; 4];

/// Brightness (as a share of the clip level) from which an unclipped pixel's colour counts fully.
const FULL_CONFIDENCE: f32 = 0.8;

/// How far an unclipped pixel's chromaticity is trusted to stand for a clipped highlight next to it:
/// `(m / (0.8 · clip))⁴` for its brightest channel `m`, at most 1 (from 80 % of the clip level up;
/// half of the clip level 0.15, a third of it 0.03, a tenth of it 0.0002).
#[inline]
fn confidence(p: &[f32; 3], clip: f32) -> f32 {
    let m = (p[0].max(p[1]).max(p[2]) / (FULL_CONFIDENCE * clip)).clamp(0.0, 1.0);
    let m2 = m * m;
    m2 * m2
}

/// Fill `values` coarse to fine (push-pull): each entry keeps its own colour by its confidence `w`
/// and takes the rest, `1 − w`, from the next coarser level. With confidences of only 0 and 1 this is
/// a plain fill of the empty entries from the mean of the known ones nearby.
fn fill(w: usize, h: usize, values: &mut [Weighted]) {
    if w * h <= 1 || values.iter().all(|v| v[3] >= 1.0) || !values.iter().any(|v| v[3] > 0.0) {
        return;
    }
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    let mut cv = vec![[0f32; 4]; cw * ch];
    {
        let values = &*values;
        downsample(w, h, &mut cv, |i| values[i]);
    }
    fill(cw, ch, &mut cv);
    values.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for (x, v) in row.iter_mut().enumerate() {
            let rest = 1.0 - v[3];
            if rest > 0.0 {
                let u = upsample(&cv, cw, ch, x, y);
                for c in 0..4 {
                    v[c] += rest * u[c];
                }
            }
        }
    });
}

/// One pyramid step: each `2 × 2` block of a `w × h` level (`at(i)`: the entry at index `i`) becomes, in
/// `cv` (`⌈w/2⌉ × ⌈h/2⌉`), the confidence-weighted mean of its colours, with their summed confidence
/// (at most 1).
fn downsample(w: usize, h: usize, cv: &mut [Weighted], at: impl Fn(usize) -> Weighted + Sync) {
    let cw = w.div_ceil(2);
    cv.par_chunks_mut(cw).enumerate().for_each(|(y, row)| {
        for (x, out) in row.iter_mut().enumerate() {
            let mut s = [0f32; 4];
            for dy in 0..2 {
                for dx in 0..2 {
                    let (sx, sy) = (2 * x + dx, 2 * y + dy);
                    if sx < w && sy < h {
                        let v = at(sy * w + sx);
                        for c in 0..4 {
                            s[c] += v[c];
                        }
                    }
                }
            }
            if s[3] > 0.0 {
                let k = s[3].min(1.0) / s[3];
                *out = s.map(|v| v * k);
            }
        }
    });
}

/// Bilinear sample of the `cw × ch` coarse level at fine pixel `(x, y)` (twice the resolution).
fn upsample(cv: &[Weighted], cw: usize, ch: usize, x: usize, y: usize) -> Weighted {
    let fx = ((x as f32 + 0.5) / 2.0 - 0.5).clamp(0.0, (cw - 1) as f32);
    let fy = ((y as f32 + 0.5) / 2.0 - 0.5).clamp(0.0, (ch - 1) as f32);
    let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(cw - 1), (y0 + 1).min(ch - 1));
    let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
    let g = |xx: usize, yy: usize| cv[yy * cw + xx];
    let mut v = [0f32; 4];
    for c in 0..4 {
        let top = g(x0, y0)[c] + (g(x1, y0)[c] - g(x0, y0)[c]) * tx;
        let bot = g(x0, y1)[c] + (g(x1, y1)[c] - g(x0, y1)[c]) * tx;
        v[c] = top + (bot - top) * ty;
    }
    v
}

/// White-balanced chromaticity of an unclipped pixel with its confidence (empty: clipped or too dark to tell).
#[inline]
fn chroma_of(p: &[f32; 3], clipped: [bool; 3], wb: [f32; 3], clip: f32) -> Weighted {
    if clipped.iter().any(|&b| b) {
        return [0.0; 4];
    }
    let q = [p[0] * wb[0], p[1] * wb[1], p[2] * wb[2]];
    let s = q[0] + q[1] + q[2];
    let w = confidence(p, clip);
    if s > 1e-4 && w > 0.0 { [q[0].max(0.0) / s * w, q[1].max(0.0) / s * w, q[2].max(0.0) / s * w, w] } else { [0.0; 4] }
}

/// Bits of a [`reconstruct_masked`] mask entry: which channels of the pixel are clipped.
pub const CLIPPED_R: u8 = 1;
pub const CLIPPED_G: u8 = 2;
pub const CLIPPED_B: u8 = 4;

/// Reconstruct partially clipped channels. `clip` is the sensor clip level in normalised units (use slightly
/// below 1.0, e.g. 0.99), `wb` the white-balance multipliers that will be applied afterwards.
/// Returns the number of pixels that had at least one clipped channel.
///
/// The chromaticity field is diffused coarse to fine from the unclipped pixels. A clipped pixel is
/// never known at full resolution, so its chromaticity is always the bilinear sample of the
/// half-resolution level: we start the pyramid there, straight from the image, and never build
/// the full-resolution field (same result, a quarter of the memory and far less work).
///
/// Demosaicing spreads clipped samples into the pixels around them (AHD's green filter and colour
/// differences reach a few pixels), so their colour is not a measurement either: unclipped pixels
/// within [`DEMOSAIC_REACH`] pixels of a clipped one only give the colour when nothing else does.
pub fn reconstruct(img: &mut Rgb32f, wb: [f32; 3], clip: f32) -> usize {
    reconstruct_with(img, wb, clip, DEMOSAIC_REACH, |_, p| [p[0] >= clip, p[1] >= clip, p[2] >= clip])
}

/// How far (pixels) demosaicing carries a clipped sample into its neighbours' colour. On the issue #548
/// iPhone frame (AHD), the cloud-edge pixels next to clipped green came out 25 % greener relative to red
/// than those 6 px out, which match the sky (bilinear: 1–2 px).
pub const DEMOSAIC_REACH: usize = 6;

/// [`reconstruct`] with the clipped channels given by `mask` (one entry per pixel, bits
/// [`CLIPPED_R`], [`CLIPPED_G`], [`CLIPPED_B`]) instead of by the values: for a binned image
/// ([`crate::RawImage::develop_binned_masked`]), whose values are block means (a lower bound for
/// a channel with clipped samples) and whose mask says which blocks held clipped samples of each
/// colour. A mask of the wrong length leaves the image as it is (returns 0).
pub fn reconstruct_masked(img: &mut Rgb32f, wb: [f32; 3], clip: f32, mask: &[u8]) -> usize {
    if mask.len() != img.data.len() {
        return 0;
    }
    // (a block mean is no interpolation: its neighbours are measurements)
    reconstruct_with(img, wb, clip, 0, |i, _| {
        let m = mask.get(i).copied().unwrap_or(0);
        [m & CLIPPED_R != 0, m & CLIPPED_G != 0, m & CLIPPED_B != 0]
    })
}

/// Every pixel within `r` pixels (a square) of a `true` one of the `w × h` `marks`.
fn dilate(w: usize, h: usize, marks: &[bool], r: usize) -> Vec<bool> {
    // a running count along rows, then along columns
    let run = |line: &[bool], out: &mut [bool]| {
        let n = line.len();
        let mut count = line.iter().take(r.min(n)).filter(|&&b| b).count();
        for (i, o) in out.iter_mut().enumerate() {
            if i + r < n && line[i + r] {
                count += 1;
            }
            if i > r && line[i - r - 1] {
                count -= 1;
            }
            *o = count > 0;
        }
    };
    let mut rows = vec![false; w * h];
    rows.par_chunks_mut(w).zip(marks.par_chunks(w)).for_each(|(out, line)| run(line, out));
    // columns: per-column counts of marked rows in the window, swept down the image in strips
    let mut cols = vec![false; w * h];
    let strip = 256usize;
    let rows = &rows;
    let parts: Vec<(usize, Vec<bool>)> = (0..w.div_ceil(strip))
        .into_par_iter()
        .map(|sx| {
            let (x0, x1) = (sx * strip, ((sx + 1) * strip).min(w));
            let n = x1 - x0;
            let mut count = vec![0u32; n];
            for y in 0..r.min(h) {
                for (c, &b) in count.iter_mut().zip(&rows[y * w + x0..y * w + x1]) {
                    *c += u32::from(b);
                }
            }
            let mut out = vec![false; n * h];
            for y in 0..h {
                if y + r < h {
                    let yy = y + r;
                    for (c, &b) in count.iter_mut().zip(&rows[yy * w + x0..yy * w + x1]) {
                        *c += u32::from(b);
                    }
                }
                if y > r {
                    let yy = y - r - 1;
                    for (c, &b) in count.iter_mut().zip(&rows[yy * w + x0..yy * w + x1]) {
                        *c -= u32::from(b);
                    }
                }
                for (o, &c) in out[y * n..(y + 1) * n].iter_mut().zip(&count) {
                    *o = c > 0;
                }
            }
            (x0, out)
        })
        .collect();
    for (x0, out) in parts {
        let n = out.len() / h.max(1);
        for y in 0..h {
            cols[y * w + x0..y * w + x0 + n].copy_from_slice(&out[y * n..(y + 1) * n]);
        }
    }
    cols
}

/// [`reconstruct`] with `clipped(index, pixel)` telling the clipped channels; unclipped pixels within
/// `reach` of a clipped one give its colour only when no other pixel does.
fn reconstruct_with(img: &mut Rgb32f, wb: [f32; 3], clip: f32, reach: usize, clipped: impl Fn(usize, &[f32; 3]) -> [bool; 3] + Sync) -> usize {
    let (w, h) = (img.width, img.height);
    let marks: Vec<bool> = img.data.par_iter().enumerate().map(|(i, p)| clipped(i, p).iter().any(|&b| b)).collect();
    let count = marks.iter().filter(|&&b| b).count();
    if count == 0 {
        return 0;
    }
    // chromaticity (white-balanced ratios to the channel sum) of unclipped pixels, at half resolution
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    // (a 1 × 1 image has no coarser level: its one pixel is clipped and nothing is known)
    let known = cw * ch < w * h;
    let field = |near: Option<&[bool]>| {
        let mut cv = vec![[0f32; 4]; cw * ch];
        let data = &img.data;
        downsample(w, h, &mut cv, |i| {
            if near.is_some_and(|n| n.get(i).copied().unwrap_or(false)) { [0.0; 4] } else { chroma_of(&data[i], clipped(i, &data[i]), wb, clip) }
        });
        cv
    };
    let near = (reach > 0).then(|| dilate(w, h, &marks, reach));
    let mut cv = field(near.as_deref());
    if near.is_some() && !cv.iter().any(|v| v[3] > 0.0) {
        cv = field(None);
    }
    if known {
        fill(cw, ch, &mut cv);
    }
    img.data.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for (x, px) in row.iter_mut().enumerate() {
            let p = *px;
            let cl = clipped(y * w + x, &p);
            if !cl.iter().any(|&b| b) {
                continue;
            }
            let v = if known { upsample(&cv, cw, ch, x, y) } else { [0.0; 4] };
            let r = if v[3] > 0.0 { [v[0] / v[3], v[1] / v[3], v[2] / v[3]] } else { [1f32 / 3.0; 3] };
            let out = rebuild([p[0] * wb[0], p[1] * wb[1], p[2] * wb[2]], cl, r, p, clip);
            *px = [out[0] / wb[0], out[1] / wb[1], out[2] / wb[2]];
        }
    });
    count
}

/// The white-balanced pixel `q` with its clipped channels `cl` rebuilt for the chromaticity `r`;
/// `p` is the pixel before white balance (sensor units, clip level `clip`).
///
/// The channel sum is estimated from the unclipped channels; each clipped one becomes its share of it,
/// and never less than it is (its clip level, or for a binned block the mean, a lower bound).
///
/// The more of a pixel is clipped, the less its colour is known, and the rebuilt colour is then drawn
/// toward neutral at its brightest channel (no channel goes down, so none drops below its clip level):
/// by `(n − 1 + s) / 2` for `n` clipped channels, where `s` rises smoothly from 0 to 1 as the brightest
/// unclipped channel goes from 85 % of the clip level to the clip level. One clipped channel well
/// below the others' clip levels keeps its rebuilt colour; two clipped channels are drawn half-way
/// and more as the third nears its clip; three are neutral; and the weight never jumps where one
/// more channel clips. Issue #548: in a cloud whose green and blue clip, the measured red puts blue's
/// clip level 27 % above red, bluer than the sky around it, and whichever value green gets the cloud
/// comes out tinted (cyan with green high, lavender with green low) unless it is desaturated.
#[inline]
fn rebuild(q: [f32; 3], cl: [bool; 3], r: [f32; 3], p: [f32; 3], clip: f32) -> [f32; 3] {
    let (mut sum, mut k) = (0f32, 0usize);
    for c in 0..3 {
        if !cl[c] && r[c] > 1e-3 {
            sum += q[c] / r[c];
            k += 1;
        }
    }
    if k == 0 {
        // fully clipped: neutral at its brightest channel (when the values are the clipped samples
        // themselves, that is at least every channel's white-balanced clip level)
        return [q.iter().cloned().fold(0.0f32, f32::max); 3];
    }
    let sum = sum / k as f32;
    let out: [f32; 3] = std::array::from_fn(|c| if cl[c] { q[c].max(sum * r[c]) } else { q[c] });
    let n = cl.iter().filter(|&&b| b).count();
    let brightest = (0..3).filter(|&c| !cl[c]).map(|c| p[c] / clip).fold(0.0f32, f32::max);
    let s = ((brightest - 0.85) / 0.15).clamp(0.0, 1.0);
    let t = ((n as f32 - 1.0 + s * s * (3.0 - 2.0 * s)) / 2.0).clamp(0.0, 1.0);
    if t <= 0.0 {
        return out;
    }
    let v = out.iter().cloned().fold(0.0f32, f32::max);
    out.map(|o| o + (v - o) * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same algorithm started at full resolution: the reference for `reconstruct`'s half-resolution start.
    fn reconstruct_ref(img: &mut Rgb32f, wb: [f32; 3], clip: f32) -> usize {
        let (w, h) = (img.width, img.height);
        let is_clipped = |p: &[f32; 3]| [p[0] >= clip, p[1] >= clip, p[2] >= clip];
        let count = img.data.iter().filter(|p| is_clipped(p).iter().any(|&b| b)).count();
        if count == 0 {
            return 0;
        }
        // the pixels near clipping, by brute force
        let near: Vec<bool> = (0..w * h)
            .map(|i| {
                let (x, y) = ((i % w) as i64, (i / w) as i64);
                let r = DEMOSAIC_REACH as i64;
                (y - r..=y + r).any(|yy| {
                    (x - r..=x + r).any(|xx| {
                        xx >= 0
                            && yy >= 0
                            && xx < w as i64
                            && yy < h as i64
                            && is_clipped(&img.data[yy as usize * w + xx as usize]).iter().any(|&b| b)
                    })
                })
            })
            .collect();
        let mut chroma: Vec<Weighted> =
            img.data.iter().zip(&near).map(|(p, &n)| if n { [0.0; 4] } else { chroma_of(p, is_clipped(p), wb, clip) }).collect();
        if !chroma.iter().any(|v| v[3] > 0.0) {
            chroma = img.data.iter().map(|p| chroma_of(p, is_clipped(p), wb, clip)).collect();
        }
        fill(w, h, &mut chroma);
        for (px, v) in img.data.iter_mut().zip(&chroma) {
            let p = *px;
            let cl = is_clipped(&p);
            if !cl.iter().any(|&b| b) {
                continue;
            }
            let r = if v[3] > 0.0 { [v[0] / v[3], v[1] / v[3], v[2] / v[3]] } else { [1f32 / 3.0; 3] };
            let out = rebuild([p[0] * wb[0], p[1] * wb[1], p[2] * wb[2]], cl, r, p, clip);
            *px = [out[0] / wb[0], out[1] / wb[1], out[2] / wb[2]];
        }
        count
    }

    /// `cargo test --release -p lightcraft-raw --lib -- --ignored bench_reconstruct --nocapture`
    #[test]
    #[ignore]
    fn bench_reconstruct() {
        let img = Rgb32f::from_fn(6000, 4000, |x, y| {
            let v = 0.4 + 0.8 * ((x as f32 * 0.003).sin() * (y as f32 * 0.002).cos()).abs();
            [v.min(1.0), (v * 0.7).min(1.0), (v * 0.5).min(1.0)]
        });
        let wb = [2.0, 1.0, 1.5];
        let time = |f: &dyn Fn(&mut Rgb32f) -> usize| {
            (0..3)
                .map(|_| {
                    let mut i = img.clone();
                    let t = std::time::Instant::now();
                    f(&mut i);
                    t.elapsed().as_secs_f64() * 1e3
                })
                .fold(f64::MAX, f64::min)
        };
        let new = time(&|i| reconstruct(i, wb, 0.99));
        let old = time(&|i| reconstruct_ref(i, wb, 0.99));
        eprintln!("reconstruct 24 MP: {new:.0} ms (full-resolution reference: {old:.0} ms)");
    }

    /// The half-resolution start gives exactly the full-resolution algorithm's result.
    #[test]
    fn matches_the_full_resolution_reference() {
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 40) as f32 / (1u64 << 24) as f32
        };
        for (w, h) in [(1, 1), (1, 2), (3, 1), (7, 5), (64, 48), (129, 77)] {
            for density in [0.02f32, 0.3, 0.97] {
                // smooth colour with clipped blobs, some dark pixels, some fully clipped ones
                let noise: Vec<f32> = (0..w * h).map(|_| rnd()).collect();
                let img = Rgb32f::from_fn(w, h, |x, y| {
                    let t = ((x as f32 * 0.21).sin() + (y as f32 * 0.17).cos()) * 0.5;
                    let base = [0.5 + 0.4 * t, 0.45 - 0.2 * t, 0.3 + 0.1 * t];
                    let r = noise[y * w + x];
                    if r < density * 0.3 {
                        [1.0, 1.0, 1.0]
                    } else if r < density {
                        [1.0, base[1] * 1.6, base[2]]
                    } else if r > 0.995 {
                        [0.0, 0.0, 0.00001]
                    } else {
                        base
                    }
                });
                let wb = [2.1, 1.0, 1.6];
                let (mut a, mut b) = (img.clone(), img.clone());
                assert_eq!(reconstruct(&mut a, wb, 0.99), reconstruct_ref(&mut b, wb, 0.99));
                assert!(a.data == b.data, "{w}×{h} at {density}");
            }
        }
    }

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

    /// Issue #548: a bright cloud whose green clips, crossed by a dark wire with a cyan fringe. The cloud
    /// takes the colour of the bright sky around it, not the wire's: before, the wire was the nearest
    /// unclipped colour for most of the cloud and turned it cyan.
    #[test]
    fn dark_detail_inside_a_highlight_does_not_tint_it() {
        let wb = [2.0f32, 1.0, 1.5];
        // neutral after white balance at luminance `l`
        let grey = |l: f32| [l / wb[0], l / wb[1], l / wb[2]];
        let (w, h) = (128usize, 96usize);
        let wire = |x: usize, y: usize| (y as i32 - 20 - x as i32 / 2).abs() <= 1;
        let disc = |x: usize, y: usize| (x as f32 - 64.0).powi(2) + (y as f32 - 48.0).powi(2) < 40.0f32.powi(2);
        let img = Rgb32f::from_fn(w, h, |x, y| {
            if wire(x, y) {
                [0.05 / wb[0], 0.12 / wb[1], 0.12 / wb[2]]
            } else if disc(x, y) {
                // brighter than green's clip level: only red and blue are left
                grey(1.4).map(|v| v.min(1.0))
            } else {
                grey(0.85)
            }
        });
        let mut out = img.clone();
        assert!(reconstruct(&mut out, wb, 0.99) > 1000);
        let mut worst = 0f32;
        for y in 0..h {
            for x in 0..w {
                let p = out.get(x, y);
                if disc(x, y) && !wire(x, y) {
                    let q = [p[0] * wb[0], p[1] * wb[1], p[2] * wb[2]];
                    worst = worst.max((q[1] / q[0] - 1.0).abs()).max((q[2] / q[0] - 1.0).abs());
                }
            }
        }
        assert!(worst < 0.03, "the cloud is tinted by up to {:.0} %", worst * 100.0);
    }

    /// The more channels clip, the nearer to neutral (at the brightest channel) the rebuilt colour, with no
    /// step where one more channel clips.
    #[test]
    fn desaturates_as_more_channels_clip() {
        let wb = [1.0f32; 3];
        let sky = [0.3 * 2.4, 0.33 * 2.4, 0.37 * 2.4];
        let rebuilt = |centre: [f32; 3]| {
            let mut img = Rgb32f::from_fn(64, 64, |x, y| if (x as i32 - 32).abs() < 16 && (y as i32 - 32).abs() < 16 { centre } else { sky });
            reconstruct(&mut img, wb, 0.99);
            img.get(32, 32)
        };
        // green clipped, red and blue well below their clip level: the plain rebuild (green from the sky's ratios)
        let p = rebuilt([0.5, 1.0, 0.6]);
        assert_eq!((p[0], p[2]), (0.5, 0.6));
        assert!(p[1] == 1.0, "{p:?}");
        let p = rebuilt([0.33, 1.0, 0.4]);
        assert!((p[1] - 0.33 / 0.3 * 0.33).abs() < 0.01 || p[1] == 1.0, "{p:?}");
        // green and blue clipped, red at half: half-way to neutral at the brightest channel (1.0)
        let p = rebuilt([0.5, 1.0, 1.0]);
        assert!((p[0] - 0.75).abs() < 1e-3 && p[1] >= 1.0 && p[2] >= 1.0, "{p:?}");
        // blue just below its clip level, then just above: (nearly) the same colour
        let below = rebuilt([0.5, 1.0, 0.989]);
        let above = rebuilt([0.5, 1.0, 0.99]);
        assert!(below.iter().zip(&above).all(|(a, b)| (a - b).abs() < 0.01), "{below:?} {above:?}");
        // all three clipped: neutral
        let p = rebuilt([1.0, 1.0, 1.0]);
        assert!(p[0] == p[1] && p[1] == p[2], "{p:?}");
    }

    /// The dilation matches a brute-force one, across several column strips.
    #[test]
    fn dilate_matches_brute_force() {
        let (w, h) = (600usize, 41usize);
        let marks: Vec<bool> = (0..w * h).map(|i| (i * 2654435761usize) % 997 < 3).collect();
        for r in [0usize, 1, 6, 50] {
            let d = dilate(w, h, &marks, r);
            for y in 0..h {
                for x in 0..w {
                    let want =
                        (y.saturating_sub(r)..=(y + r).min(h - 1)).any(|yy| (x.saturating_sub(r)..=(x + r).min(w - 1)).any(|xx| marks[yy * w + xx]));
                    assert_eq!(d[y * w + x], want, "r {r} at ({x}, {y})");
                }
            }
        }
    }

    /// With nothing bright nearby, dim pixels still give a clipped area their colour.
    #[test]
    fn a_highlight_among_dim_pixels_takes_their_colour() {
        let wb = [1.0f32; 3];
        let mut img =
            Rgb32f::from_fn(32, 32, |x, y| if (12..20).contains(&x) && (12..20).contains(&y) { [1.0, 0.5, 0.25] } else { [0.1, 0.05, 0.025] });
        reconstruct(&mut img, wb, 0.99);
        let p = img.get(16, 16);
        assert!((p[0] / p[1] - 2.0).abs() < 0.01, "{p:?}");
    }

    /// A mask decides which channels are clipped, whatever their values; a mask of another size changes nothing.
    #[test]
    fn a_mask_marks_the_clipped_channels() {
        let wb = [1.0f32; 3];
        let base = Rgb32f::from_fn(16, 16, |x, _| if x >= 8 { [0.2, 0.5, 0.25] } else { [0.4, 0.4, 0.2] });
        // the right half's red is a lower bound: its blocks held clipped samples
        let mask: Vec<u8> = (0..256).map(|i| if i % 16 >= 8 { CLIPPED_R } else { 0 }).collect();
        let mut img = base.clone();
        assert_eq!(reconstruct_masked(&mut img, wb, 0.99, &mask), 128);
        // red rebuilt from the left half's colour (red = green), the other channels kept
        for x in 8..16 {
            let p = img.get(x, 8);
            assert!((p[0] - 0.5).abs() < 1e-4 && p[1] == 0.5 && p[2] == 0.25, "{p:?}");
        }
        assert_eq!(img.get(0, 0), base.get(0, 0));
        let mut same = base.clone();
        assert_eq!(reconstruct_masked(&mut same, wb, 0.99, &mask[1..]), 0);
        assert!(same.data == base.data);
        // the thresholds alone see nothing clipped here
        let mut plain = base.clone();
        assert_eq!(reconstruct(&mut plain, wb, 0.99), 0);
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
