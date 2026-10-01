//! The per-pixel stage: everything after the spatial planes are ready, in one parallel pass.

use lightcraft_color::spline::{Lut1, MonotoneCurve};
use lightcraft_color::transfer::linear_to_srgb;
use lightcraft_color::{REC2020, SRGB, luminance_2020};
use lightcraft_develop::{DevelopSettings, ToneCurve, VignetteStyle};
use lightcraft_geom::Point;
use lightcraft_raster::Rgba8;

use crate::colorops::ColorOps;
use crate::geometry::Frame;
use crate::local::log_lum;
use crate::tone::ToneMap;
use crate::{Prepared, SourceInfo, for_rows};

#[inline]
fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Parametric region curve (encoded domain) composed with the master point curve.
fn curve_luts(c: &ToneCurve) -> Option<[Lut1; 3]> {
    let parametric = c.highlights != 0.0 || c.lights != 0.0 || c.darks != 0.0 || c.shadows != 0.0;
    let master = !ToneCurve::point_curve_is_identity(&c.master);
    let chans = [&c.red, &c.green, &c.blue].map(|p| !ToneCurve::point_curve_is_identity(p));
    if !parametric && !master && !chans.iter().any(|b| *b) {
        return None;
    }
    const N: usize = 1024;
    let (s1, s2, s3) = ((c.split_shadows / 100.0) as f32, (c.split_mid / 100.0) as f32, (c.split_highlights / 100.0) as f32);
    let regions = [(0.0, s1, c.shadows), (s1, s2, c.darks), (s2, s3, c.lights), (s3, 1.0, c.highlights)];
    let mut base = Lut1::from_fn(N, |x| {
        let mut d = 0.0;
        for (a, b, amt) in regions {
            if amt == 0.0 {
                continue;
            }
            let (ctr, half) = ((a + b) / 2.0, (b - a) * 0.75 + 0.05);
            let t = ((x - ctr) / half).clamp(-1.0, 1.0);
            let win = 0.5 + 0.5 * (t * std::f32::consts::PI).cos();
            d += (amt / 100.0) as f32 * 0.22 * win;
        }
        (x + d * 4.0 * x * (1.0 - x)).clamp(0.0, 1.0)
    });
    // keep monotone
    for i in 1..N {
        base.v[i] = base.v[i].max(base.v[i - 1]);
    }
    let to_pts = |p: &[Point]| p.iter().map(|q| (q.x, q.y)).collect::<Vec<_>>();
    if master {
        base = MonotoneCurve::new(&to_pts(&c.master)).to_lut(N).compose(&base);
    }
    let per = [&c.red, &c.green, &c.blue];
    Some(std::array::from_fn(|i| if chans[i] { MonotoneCurve::new(&to_pts(per[i])).to_lut(N).compose(&base) } else { base.clone() }))
}

struct Vig {
    amount: f32,
    start: f32,
    width: f32,
    aspect_mix: f32,
    power: f32,
    highlights: f32,
    style: VignetteStyle,
}

fn vignette(s: &DevelopSettings) -> Option<Vig> {
    let v = &s.vignette;
    (v.amount != 0.0).then(|| {
        let r = (v.roundness / 100.0) as f32;
        Vig {
            amount: (v.amount / 100.0) as f32,
            start: 0.15 + (v.midpoint / 100.0) as f32 * 0.95,
            width: 0.05 + (v.feather / 100.0) as f32 * 1.1,
            aspect_mix: ((r + 1.0) / 2.0).clamp(0.0, 1.0),
            power: if r >= 0.0 { 2.0 } else { 2.0 + (-r) * 6.0 },
            highlights: (v.highlights / 100.0) as f32,
            style: v.style,
        }
    })
}

#[inline]
fn grain_noise(x: f32, y: f32, seed: u32) -> f32 {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let h = |i: i32, j: i32| {
        let mut v = (i as u32).wrapping_mul(0x8da6_b343) ^ (j as u32).wrapping_mul(0xd816_3841) ^ seed.wrapping_mul(0xcb1a_b31f);
        v ^= v >> 13;
        v = v.wrapping_mul(0x5bd1_e995);
        v ^= v >> 15;
        (v & 0xffff) as f32 / 32768.0 - 1.0
    };
    let (i, j) = (x0 as i32, y0 as i32);
    let (u, v) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let a = h(i, j) + (h(i + 1, j) - h(i, j)) * u;
    let b = h(i, j + 1) + (h(i + 1, j + 1) - h(i, j + 1)) * u;
    a + (b - a) * v
}

pub fn finish(p: &Prepared, s: &DevelopSettings, frame: &Frame, info: &SourceInfo) -> Rgba8 {
    let (w, h) = (p.img.width, p.img.height);
    let tone = if info.raw {
        ToneMap::new(s.light.contrast, s.light.whites, s.light.blacks)
    } else {
        ToneMap::display(s.light.contrast, s.light.whites, s.light.blacks)
    };
    let ops = ColorOps::new(s);
    let curves = curve_luts(&s.curve);
    let vig = if s.section_enabled("effects") { vignette(s) } else { None };
    let to_srgb: [[f32; 3]; 3] = REC2020.to_space(&SRGB).to_f32();
    let hl = (s.light.highlights / 100.0) as f32;
    let sh = (s.light.shadows / 100.0) as f32;
    let (clar, tex, dehaze) = if s.section_enabled("effects") {
        ((s.effects.clarity / 100.0) as f32, (s.effects.texture / 100.0) as f32, (s.effects.dehaze / 100.0) as f32)
    } else {
        (0.0, 0.0, 0.0)
    };
    let sharpen = (s.detail.sharpen_amount / 150.0) as f32;
    let sharpen_mask = (s.detail.sharpen_masking / 100.0) as f32;
    // Planes are pre-exposure: scale the airlight, shift log luminance (see `Prepared`).
    let (air, air_pre, gain, ev) = (p.air * p.gain, p.air, p.gain, p.ev);
    let grain = (s.grain.amount > 0.0 && s.section_enabled("effects")).then(|| {
        let cell = (0.0006 + (s.grain.size / 100.0) as f32 * 0.0024) * p.px_per_long as f32;
        ((s.grain.amount / 100.0) as f32 * 0.13, cell.max(0.6), (s.grain.roughness / 100.0) as f32, s.grain.seed)
    });
    let out_to_norm = frame.out_to_norm(w, h);
    let aspect = w as f32 / h as f32;

    let mut out = Rgba8::new(w, h);
    for_rows(&mut out.data, w, |y, row| {
        for (x, px) in row.iter_mut().enumerate() {
            let i = y * w + x;
            let raw = p.img.data[i];
            let mut c = if gain == 1.0 { raw } else { raw.map(|v| v * gain) };
            let l_pre = p.log_l.data[i];
            let l0 = l_pre + ev;

            // --- local (mask) contributions
            let (mut l_exp, mut l_temp, mut l_tint, mut l_con, mut l_hl, mut l_sh, mut l_wh, mut l_bl) = (0.0f32, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
            let (mut l_tex, mut l_clar, mut l_dehaze, mut l_sat, mut l_hue, mut l_sharp) = (0.0f32, 0.0, 0.0, 0.0, 0.0, 0.0);
            let mut tint_col: Option<([f32; 3], f32)> = None;
            for m in &p.masks {
                let a = m.alpha.data[i];
                if a <= 0.0 {
                    continue;
                }
                let j = &m.adjust;
                l_exp += a * j.exposure as f32;
                l_temp += a * (j.temp / 100.0) as f32;
                l_tint += a * (j.tint / 100.0) as f32;
                l_con += a * (j.contrast / 100.0) as f32;
                l_hl += a * (j.highlights / 100.0) as f32;
                l_sh += a * (j.shadows / 100.0) as f32;
                l_wh += a * (j.whites / 100.0) as f32;
                l_bl += a * (j.blacks / 100.0) as f32;
                l_tex += a * (j.texture / 100.0) as f32;
                l_clar += a * (j.clarity / 100.0) as f32;
                l_dehaze += a * (j.dehaze / 100.0) as f32;
                l_sat += a * (j.saturation / 100.0) as f32;
                l_hue += a * (j.hue / 100.0) as f32 * 0.6;
                l_sharp += a * (j.sharpness / 100.0) as f32;
                if j.color_sat > 0.0 {
                    let hue = crate::colorops::oklch_hue_of_srgb_hue(j.color_hue);
                    tint_col = Some(([hue.cos(), hue.sin(), 0.0], a * (j.color_sat / 100.0) as f32));
                }
            }

            // --- dehaze (scene linear)
            let dz = dehaze + l_dehaze;
            if dz != 0.0
                && let Some(dark) = &p.dark
            {
                let d = (dark.data[i] / air_pre).clamp(0.0, 1.0);
                if dz > 0.0 {
                    let t = (1.0 - 0.95 * dz.min(1.0) * d).max(0.12);
                    c = c.map(|v| ((v - air * (1.0 - t)) / t).max(0.0));
                } else {
                    let k = (-dz).min(1.0) * 0.7 * (0.35 + 0.65 * d);
                    c = c.map(|v| v + (air * 0.9 - v) * k);
                }
            }

            // --- local exposure / temp / tint
            if l_exp != 0.0 {
                let g = 2f32.powf(l_exp);
                c = c.map(|v| v * g);
            }
            if l_temp != 0.0 || l_tint != 0.0 {
                let y0 = luminance_2020(c);
                c = [c[0] * (1.0 + 0.3 * l_temp), c[1] * (1.0 - 0.22 * l_tint), c[2] * (1.0 - 0.3 * l_temp).max(0.0)];
                let y1 = luminance_2020(c).max(1e-9);
                c = c.map(|v| v * y0 / y1);
            }

            // --- local tone in log luminance
            let l1 = if dz != 0.0 || l_exp != 0.0 { log_lum(c) } else { l0 };
            let shift = l1 - l0;
            let base = p.base.data[i] + ev + shift;
            let mut delta = 0.0f32;
            let (hh, ss) = (hl + l_hl, sh + l_sh);
            if hh != 0.0 || ss != 0.0 {
                let ws = 1.0 - smooth(-4.8, 0.3, base);
                let wh = smooth(-1.0, 2.8, base);
                delta += ss * 1.7 * ws * ws.sqrt() + hh * 1.7 * wh;
            }
            if l_wh != 0.0 {
                delta += l_wh * 0.8 * smooth(0.5, 3.0, l1);
            }
            if l_bl != 0.0 {
                delta += l_bl * 0.8 * (1.0 - smooth(-6.0, -1.5, l1));
            }
            if l_con != 0.0 {
                delta += l_con * 0.14 * l1.clamp(-6.0, 4.0);
            }
            let cl = clar + l_clar;
            if cl != 0.0
                && let Some(b) = &p.clarity_blur
            {
                let det = (l_pre - b.data[i]).clamp(-2.5, 2.5);
                let mid = (-(base / 3.2).powi(2)).exp();
                delta += cl * 0.85 * det * (0.35 + 0.65 * mid);
            }
            let tx = tex + l_tex;
            let sp = l_sharp * 0.6 + sharpen;
            if (tx != 0.0 || sp != 0.0)
                && let Some(b) = &p.texture_blur
            {
                let det = l_pre - b.data[i];
                let tame = 1.0 - 0.6 * smooth(0.4, 1.6, det.abs());
                delta += tx * 1.1 * det.clamp(-1.0, 1.0) * tame;
                if sp != 0.0 {
                    let m = if sharpen_mask > 0.0 { smooth(sharpen_mask * 0.25, sharpen_mask * 0.25 + 0.15, det.abs()) } else { 1.0 };
                    delta += sp * 1.3 * det.clamp(-0.8, 0.8) * m;
                }
            }
            if delta != 0.0 {
                let g = 2f32.powf(delta);
                c = c.map(|v| v * g);
            }

            // --- tone map on luminance, highlight desaturation
            let yl = luminance_2020(c);
            let o = tone.apply(yl);
            let mut d = if yl > 1e-9 { c.map(|v| v * o / yl) } else { [0.0; 3] };
            let mx = d[0].max(d[1]).max(d[2]);
            if mx > 1.0 {
                let t = ((mx - 1.0) / (mx - o).max(1e-6)).clamp(0.0, 1.0);
                d = d.map(|v| v + (o - v) * t);
            }

            // --- colour
            d = ops.apply(d, l_sat, l_hue);
            if let Some((dir, amt)) = tint_col {
                let lab = lightcraft_color::perceptual::oklab_from_2020(d);
                d = lightcraft_color::perceptual::oklab_to_2020([lab[0], lab[1] + dir[0] * 0.08 * amt, lab[2] + dir[1] * 0.08 * amt]);
            }

            // --- vignette (display linear, post-crop)
            if let Some(v) = &vig {
                let u = (x as f32 + 0.5) / w as f32 * 2.0 - 1.0;
                let vv = (y as f32 + 0.5) / h as f32 * 2.0 - 1.0;
                let sx = 1.0 + (aspect - 1.0) * v.aspect_mix;
                let sy = 1.0 + (1.0 / aspect - 1.0) * v.aspect_mix;
                let (ax, ay) = ((u * sx.max(1.0) / sx.max(sy)).abs(), (vv * sy.max(1.0) / sx.max(sy)).abs());
                let dist = (ax.powf(v.power) + ay.powf(v.power)).powf(1.0 / v.power);
                let t = smooth(v.start, v.start + v.width, dist);
                if t > 0.0 {
                    let lum = luminance_2020(d).clamp(0.0, 1.0);
                    if v.amount < 0.0 {
                        let mut f = 1.0 + v.amount * t;
                        if v.style == VignetteStyle::HighlightPriority {
                            f += (1.0 - f) * v.highlights * smooth(0.4, 1.0, lum);
                        }
                        if v.style == VignetteStyle::PaintOverlay {
                            d = d.map(|c| c * (1.0 - (-v.amount) * t) + 0.0);
                        } else {
                            d = d.map(|c| c * f);
                        }
                    } else {
                        d = d.map(|c| c + (1.0 - c) * v.amount * t * 0.85);
                    }
                }
            }

            // --- gamut map to sRGB (desaturate towards luminance until in range)
            let m = &to_srgb;
            let mut r = [
                m[0][0] * d[0] + m[0][1] * d[1] + m[0][2] * d[2],
                m[1][0] * d[0] + m[1][1] * d[1] + m[1][2] * d[2],
                m[2][0] * d[0] + m[2][1] * d[1] + m[2][2] * d[2],
            ];
            let yy = (0.2126 * r[0] + 0.7152 * r[1] + 0.0722 * r[2]).clamp(0.0, 1.0);
            let mut t = 1.0f32;
            for c in r {
                if c < 0.0 {
                    t = t.min(yy / (yy - c).max(1e-9));
                } else if c > 1.0 {
                    t = t.min((1.0 - yy) / (c - yy).max(1e-9));
                }
            }
            if t < 1.0 {
                r = r.map(|c| yy + (c - yy) * t);
            }

            // --- encode, curves, grain
            let mut e = r.map(|v| linear_to_srgb(v.clamp(0.0, 1.0)));
            if let Some(l) = &curves {
                e = [l[0].eval(e[0]), l[1].eval(e[1]), l[2].eval(e[2])];
            }
            if let Some((amt, cell, rough, seed)) = grain {
                let n = out_to_norm.apply(Point::new(x as f64 + 0.5, y as f64 + 0.5));
                let (gx, gy) = ((n.x * frame.ow) as f32 / frame.ow.max(frame.oh) as f32, (n.y * frame.oh) as f32 / frame.ow.max(frame.oh) as f32);
                let sc = p.px_per_long as f32 / cell;
                let mut g = grain_noise(gx * sc, gy * sc, seed);
                g = g * (1.0 - rough * 0.5) + grain_noise(gx * sc * 2.3, gy * sc * 2.3, seed ^ 0x55) * rough * 0.7;
                let lum = 0.2126 * e[0] + 0.7152 * e[1] + 0.0722 * e[2];
                let k = amt * g * (0.35 + 2.6 * lum * (1.0 - lum));
                e = e.map(|v| v + k);
            }
            *px = [enc(e[0]), enc(e[1]), enc(e[2]), 255];
        }
    });
    out
}

#[inline]
fn enc(v: f32) -> u8 {
    // `v` is already sRGB-encoded; round to 8 bits.
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}
