//! Scene-linear preparation: white balance + exposure, and the spatial planes the per-pixel stage
//! needs (edge-aware base layer for highlights/shadows, clarity/texture bands, dehaze veil).

use lightcraft_color::cct::{temp_tint_to_xy, wb_matrix};
use lightcraft_color::{REC2020, luminance_2020};
use lightcraft_develop::DevelopSettings;
use lightcraft_raster::blur::gaussian;
use lightcraft_raster::{Plane, Rgb32f};

use crate::geometry::Frame;
use crate::{Prepared, Quality, SourceInfo, for_rows, masks};

/// White balance (relative to the source's as-shot white) and exposure, in place.
pub fn scene_linear_pre(img: &mut Rgb32f, info: &SourceInfo, s: &DevelopSettings) {
    let (t, tint) = effective_wb(info, s);
    let m = if (t - info.as_shot_temp).abs() < 1e-6 && (tint - info.as_shot_tint).abs() < 1e-6 {
        None
    } else {
        let set = wb_matrix(&REC2020, temp_tint_to_xy(t, tint));
        let shot = wb_matrix(&REC2020, temp_tint_to_xy(info.as_shot_temp, info.as_shot_tint));
        let m = set.mul(&shot.inverse().unwrap_or(lightcraft_color::Mat3::IDENTITY));
        // Normalize so neutral luminance is preserved (WB shouldn't change exposure).
        let g = m.apply([1.0, 1.0, 1.0]);
        let y = g[0] * 0.2627 + g[1] * 0.6780 + g[2] * 0.0593;
        Some(m.mul(&lightcraft_color::Mat3::diag(1.0 / y, 1.0 / y, 1.0 / y)).to_f32())
    };
    let gain = 2f32.powf(s.light.exposure as f32);
    let w = img.width;
    for_rows(&mut img.data, w, |_, row| {
        for p in row.iter_mut() {
            let mut c = *p;
            if let Some(m) = &m {
                c = [
                    m[0][0] * c[0] + m[0][1] * c[1] + m[0][2] * c[2],
                    m[1][0] * c[0] + m[1][1] * c[1] + m[1][2] * c[2],
                    m[2][0] * c[0] + m[2][1] * c[1] + m[2][2] * c[2],
                ];
            }
            *p = [(c[0] * gain).max(0.0), (c[1] * gain).max(0.0), (c[2] * gain).max(0.0)];
        }
    });
}

/// The white balance actually in effect (presets resolve to their Kelvin values for raw files).
pub fn effective_wb(info: &SourceInfo, s: &DevelopSettings) -> (f64, f64) {
    use lightcraft_develop::WbMode;
    match s.wb.mode {
        WbMode::AsShot => (info.as_shot_temp, info.as_shot_tint),
        m if info.raw => m.preset().unwrap_or((s.wb.temp, s.wb.tint)),
        _ => (s.wb.temp, s.wb.tint),
    }
}

/// Self-guided filter (He et al.) on a single plane with Gaussian windows of `sigma` px.
pub fn guided(p: &Plane, sigma: f32, eps: f32) -> Plane {
    let mean = gaussian(p, sigma);
    let sq = p.map(|v| v * v);
    let corr = gaussian(&sq, sigma);
    let mut a = Plane::new(p.width, p.height);
    let mut b = Plane::new(p.width, p.height);
    for i in 0..p.len() {
        let var = (corr.data[i] - mean.data[i] * mean.data[i]).max(0.0);
        let ai = var / (var + eps);
        a.data[i] = ai;
        b.data[i] = mean.data[i] - ai * mean.data[i];
    }
    let ma = gaussian(&a, sigma);
    let mb = gaussian(&b, sigma);
    let mut q = Plane::new(p.width, p.height);
    for i in 0..p.len() {
        q.data[i] = ma.data[i] * p.data[i] + mb.data[i];
    }
    q
}

/// Fast guided filter (He & Sun 2015): coefficients computed on a `s×` subsampled plane and
/// bilinearly upsampled; the output keeps full-resolution edges. Equivalent to [`guided`] for
/// large windows at a fraction of the cost.
pub fn guided_fast(p: &Plane, sigma: f32, eps: f32) -> Plane {
    use lightcraft_raster::resample::{Filter, resize};
    let s = (sigma / 3.0).floor().clamp(1.0, 16.0) as usize;
    if s <= 1 {
        return guided(p, sigma, eps);
    }
    let (lw, lh) = (p.width.div_ceil(s).max(1), p.height.div_ceil(s).max(1));
    let lo = resize(p, lw, lh, Filter::Box);
    let ls = sigma / s as f32;
    let mean = gaussian(&lo, ls);
    let corr = gaussian(&lo.map(|v| v * v), ls);
    let mut a = Plane::new(lw, lh);
    let mut b = Plane::new(lw, lh);
    for i in 0..lo.len() {
        let var = (corr.data[i] - mean.data[i] * mean.data[i]).max(0.0);
        let ai = var / (var + eps);
        a.data[i] = ai;
        b.data[i] = mean.data[i] - ai * mean.data[i];
    }
    let ma = resize(&gaussian(&a, ls), p.width, p.height, Filter::Bilinear);
    let mb = resize(&gaussian(&b, ls), p.width, p.height, Filter::Bilinear);
    let mut q = Plane::new(p.width, p.height);
    let w = p.width;
    for_rows(&mut q.data, w, |y, row| {
        for (x, v) in row.iter_mut().enumerate() {
            let i = y * w + x;
            *v = ma.data[i] * p.data[i] + mb.data[i];
        }
    });
    q
}

/// Noise reduction at output resolution: luminance via an edge-aware self-guided filter on
/// log-luminance, colour by blurring chromaticity (rgb / Y) and re-applying the original luminance.
/// Radii scale with how much the source was downsampled (preview noise is already averaged out).
pub fn denoise(img: &mut Rgb32f, s: &DevelopSettings, src_long: usize, out_long: usize) {
    let lum = (s.detail.nr_luminance / 100.0) as f32;
    let col = (s.detail.nr_color / 100.0) as f32;
    if lum <= 0.0 && col <= 0.0 {
        return;
    }
    let scale = (out_long as f32 / src_long.max(1) as f32).clamp(0.05, 1.0);
    let w = img.width;
    if lum > 0.0 {
        let l = img.map(log_lum);
        let detail = (s.detail.nr_detail / 100.0) as f32;
        let eps = 0.002 + lum * lum * 0.25 * (1.0 - 0.7 * detail);
        let f = guided(&l, (1.0 + 2.5 * lum) * scale.max(0.4), eps);
        let k = lum.sqrt();
        for_rows(&mut img.data, w, |y, row| {
            for (x, p) in row.iter_mut().enumerate() {
                let i = y * w + x;
                let d = (f.data[i] - l.data[i]) * k;
                let g = 2f32.powf(d);
                *p = p.map(|v| v * g);
            }
        });
    }
    if col > 0.0 {
        let chroma = img.map(|c| {
            let y = luminance_2020(c).max(1e-6);
            [c[0] / y, c[1] / y, c[2] / y]
        });
        let sigma = (1.5 + 6.0 * col) * scale.max(0.35) * (1.0 + (s.detail.nr_color_smoothness / 100.0) as f32);
        let b = gaussian(&chroma, sigma);
        let keep = (s.detail.nr_color_detail / 100.0) as f32 * 0.5;
        for_rows(&mut img.data, w, |y, row| {
            for (x, p) in row.iter_mut().enumerate() {
                let i = y * w + x;
                let yl = luminance_2020(*p);
                let c0 = chroma.data[i];
                let cb = b.data[i];
                let t = col * (1.0 - keep);
                *p = [0, 1, 2].map(|k| ((c0[k] + (cb[k] - c0[k]) * t) * yl).max(0.0));
            }
        });
    }
}

pub fn log_lum(c: [f32; 3]) -> f32 {
    (luminance_2020(c).max(1e-7) / crate::tone::GREY).log2()
}

pub fn prepare(img: Rgb32f, s: &DevelopSettings, frame: &Frame, px_per_long: f64, q: Quality) -> Prepared {
    let log_l = img.map(log_lum);
    let ppl = px_per_long as f32;
    let tone_active =
        s.light.highlights != 0.0 || s.light.shadows != 0.0 || s.masks.iter().any(|m| m.adjust.highlights != 0.0 || m.adjust.shadows != 0.0);
    let base = if tone_active {
        // Edge-aware base at ~1.5% of the long edge (EV² epsilon: edges of > ~0.6 EV are preserved).
        let sigma = (0.015 * ppl).max(1.0);
        let sigma = if q == Quality::Draft { sigma.min(24.0) } else { sigma };
        guided_fast(&log_l, sigma, 0.35)
    } else {
        log_l.clone()
    };
    let local_any = |f: fn(&lightcraft_develop::LocalAdjustments) -> f64| s.masks.iter().any(|m| f(&m.adjust) != 0.0);
    let clarity_blur = (s.effects.clarity != 0.0 || local_any(|a| a.clarity)).then(|| guided_fast(&log_l, (0.012 * ppl).max(1.0), 0.8));
    let texture_blur = (s.effects.texture != 0.0 || s.detail.sharpen_amount != 0.0 || local_any(|a| a.texture) || local_any(|a| a.sharpness))
        .then(|| gaussian(&log_l, (0.0018 * ppl).max(0.6)));
    let dark = (s.effects.dehaze != 0.0 || local_any(|a| a.dehaze)).then(|| {
        let min_c = img.map(|c| c[0].min(c[1]).min(c[2]));
        gaussian(&min_c, (0.02 * ppl).max(1.0))
    });
    let masks = masks::evaluate(&s.masks, frame, img.width, img.height, &img, &log_l);
    Prepared { img, log_l, base, clarity_blur, texture_blur, dark, masks, px_per_long }
}

/// Airlight estimate: bright end of the dark channel.
pub fn airlight(dark: &Plane) -> f32 {
    let mut v: Vec<f32> = dark.data.iter().step_by(7).copied().collect();
    if v.is_empty() {
        return 1.0;
    }
    let k = ((v.len() as f32) * 0.995) as usize;
    let k = k.min(v.len() - 1);
    v.select_nth_unstable_by(k, |a, b| a.total_cmp(b));
    v[k].max(0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guided_preserves_step_edges_and_flattens_texture() {
        let p = Plane::from_fn(80, 20, |x, y| if x < 40 { -2.0 } else { 2.0 } + if (x + y) % 2 == 0 { 0.05 } else { -0.05 });
        let q = guided(&p, 4.0, 0.3);
        // edge kept
        assert!(q.get(35, 10) < -1.5 && q.get(45, 10) > 1.5);
        // checkerboard texture removed away from the edge
        assert!((q.get(10, 10) - q.get(11, 10)).abs() < 0.02);
    }

    #[test]
    fn wb_as_shot_is_identity_and_warming_adds_red() {
        let s = DevelopSettings::default();
        let info = SourceInfo::default();
        let mut img = Rgb32f::filled(4, 4, [0.3, 0.3, 0.3]);
        scene_linear_pre(&mut img, &info, &s);
        assert!((img.get(0, 0)[0] - 0.3).abs() < 1e-6);
        let mut warm = s.clone();
        warm.wb.mode = lightcraft_develop::WbMode::Custom;
        warm.wb.temp = 9000.0;
        let mut img2 = Rgb32f::filled(4, 4, [0.3, 0.3, 0.3]);
        scene_linear_pre(&mut img2, &info, &warm);
        let c = img2.get(0, 0);
        assert!(c[0] > c[2], "{c:?}");
        // luminance preserved
        assert!((luminance_2020(c) - 0.3).abs() < 0.01);
    }

    #[test]
    fn exposure_doubles() {
        let mut s = DevelopSettings::default();
        s.light.exposure = 1.0;
        let mut img = Rgb32f::filled(2, 2, [0.1, 0.2, 0.3]);
        scene_linear_pre(&mut img, &SourceInfo::default(), &s);
        assert!((img.get(1, 1)[2] - 0.6).abs() < 1e-5);
    }
}

#[cfg(test)]
mod fast_tests {
    use super::*;

    #[test]
    fn fast_guided_close_to_reference() {
        let p = Plane::from_fn(160, 120, |x, y| ((x as f32 * 0.13).sin() + (y as f32 * 0.07).cos()) * 2.0 + if x > 80 { 1.5 } else { 0.0 });
        let a = guided(&p, 12.0, 0.3);
        let b = guided_fast(&p, 12.0, 0.3);
        let err: f32 = a.data.iter().zip(&b.data).map(|(u, v)| (u - v).abs()).sum::<f32>() / a.len() as f32;
        assert!(err < 0.08, "{err}");
    }
}

#[cfg(test)]
mod nr_tests {
    use super::*;

    #[test]
    fn luminance_nr_reduces_noise_keeps_mean() {
        let mut img = Rgb32f::from_fn(64, 64, |x, y| {
            let n = (((x * 7919 + y * 104729) % 97) as f32 / 97.0 - 0.5) * 0.06;
            [0.2 + n; 3]
        });
        let var = |im: &Rgb32f| {
            let m = im.data.iter().map(|p| p[1]).sum::<f32>() / im.len() as f32;
            (im.data.iter().map(|p| (p[1] - m).powi(2)).sum::<f32>() / im.len() as f32, m)
        };
        let (v0, m0) = var(&img);
        let mut s = DevelopSettings::default();
        s.detail.nr_luminance = 80.0;
        denoise(&mut img, &s, 64, 64);
        let (v1, m1) = var(&img);
        assert!(v1 < v0 * 0.5, "{v0} -> {v1}");
        assert!((m1 - m0).abs() < 0.01);
    }
}
