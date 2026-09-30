//! Colour tools in OkLCh on display-linear Rec.2020 values: vibrance, saturation, the 8-band colour
//! mixer, B&W mix, and 3-way colour grading.

use std::f32::consts::{PI, TAU};
use std::sync::OnceLock;

use lightcraft_color::perceptual::{hsv_to_rgb, lab_to_lch, lch_to_lab, oklab_from_2020, oklab_to_2020};
use lightcraft_color::{REC2020, SRGB};
use lightcraft_develop::{DevelopSettings, MIXER_HUES};

/// OkLCh hue angle (radians) of a pure sRGB colour with HSV hue `deg`.
pub fn oklch_hue_of_srgb_hue(deg: f64) -> f32 {
    let c = hsv_to_rgb(deg as f32, 1.0, 1.0);
    let lin = c.map(lightcraft_color::transfer::srgb_to_linear);
    let m = SRGB.to_space(&REC2020).apply_f32(lin);
    lab_to_lch(oklab_from_2020(m))[2]
}

fn band_hues() -> &'static [f32; 8] {
    static H: OnceLock<[f32; 8]> = OnceLock::new();
    H.get_or_init(|| MIXER_HUES.map(oklch_hue_of_srgb_hue))
}

#[inline]
fn wrap(a: f32) -> f32 {
    (a + PI).rem_euclid(TAU) - PI
}

/// Partition-of-unity weights of hue `h` over the 8 bands (raised cosine between neighbours).
#[inline]
pub fn band_weights(h: f32) -> [f32; 8] {
    let hues = band_hues();
    let mut w = [0.0f32; 8];
    for i in 0..8 {
        let a = hues[i];
        let b = hues[(i + 1) % 8];
        let span = wrap(b - a).rem_euclid(TAU);
        let d = wrap(h - a).rem_euclid(TAU);
        if d <= span {
            let t = d / span;
            let s = 0.5 - 0.5 * (t * PI).cos();
            w[i] += 1.0 - s;
            w[(i + 1) % 8] += s;
            break;
        }
    }
    w
}

#[derive(Clone, Copy)]
struct WheelK {
    a: f32,
    b: f32,
    lum: f32,
}

pub struct ColorOps {
    vibrance: f32,
    saturation: f32,
    hue: [f32; 8],
    sat: [f32; 8],
    lum: [f32; 8],
    mixer: bool,
    bw: Option<[f32; 8]>,
    grading: Option<([WheelK; 4], f32, f32)>,
    skin: f32,
}

fn wheel(w: &lightcraft_develop::Wheel) -> WheelK {
    let h = oklch_hue_of_srgb_hue(w.hue);
    let s = (w.sat / 100.0) as f32 * 0.09;
    WheelK { a: s * h.cos(), b: s * h.sin(), lum: (w.lum / 100.0) as f32 * 0.12 }
}

impl ColorOps {
    pub fn new(s: &DevelopSettings) -> ColorOps {
        let bands = s.mixer.bands();
        let g = &s.grading;
        ColorOps {
            vibrance: (s.color.vibrance / 100.0) as f32,
            saturation: (s.color.saturation / 100.0) as f32,
            hue: bands.map(|b| (b.hue / 100.0) as f32 * 0.5),
            sat: bands.map(|b| (b.sat / 100.0) as f32),
            lum: bands.map(|b| (b.lum / 100.0) as f32 * 0.18),
            mixer: !s.mixer.is_neutral(),
            bw: crate::is_bw(s).then(|| s.bw_mix.bands().map(|v| (v / 100.0) as f32)),
            grading: (!g.is_neutral()).then(|| {
                (
                    [wheel(&g.shadows), wheel(&g.midtones), wheel(&g.highlights), wheel(&g.global)],
                    (g.blending / 100.0) as f32,
                    (g.balance / 100.0) as f32,
                )
            }),
            skin: oklch_hue_of_srgb_hue(25.0),
        }
    }

    pub fn is_identity(&self) -> bool {
        self.vibrance == 0.0 && self.saturation == 0.0 && !self.mixer && self.bw.is_none() && self.grading.is_none()
    }

    /// `local_sat` (−1..1) and `local_hue` (radians) come from masks.
    #[inline]
    pub fn apply(&self, rgb: [f32; 3], local_sat: f32, local_hue: f32) -> [f32; 3] {
        if self.is_identity() && local_sat == 0.0 && local_hue == 0.0 {
            return rgb;
        }
        let lab = oklab_from_2020(rgb);
        let [mut l, mut c, mut h] = lab_to_lch(lab);
        if self.mixer {
            let w = band_weights(h);
            let (mut dh, mut ds, mut dl) = (0.0, 0.0, 0.0);
            for i in 0..8 {
                dh += w[i] * self.hue[i];
                ds += w[i] * self.sat[i];
                dl += w[i] * self.lum[i];
            }
            let chroma_w = (c / 0.12).min(1.0);
            h += dh * chroma_w;
            c *= (1.0 + ds).max(0.0);
            l += dl * chroma_w * l.max(0.05).sqrt();
        }
        if self.vibrance != 0.0 {
            let low = 1.0 - (c / 0.22).clamp(0.0, 1.0);
            let skin = if self.vibrance > 0.0 { 1.0 - 0.6 * (-(wrap(h - self.skin) / 0.35).powi(2)).exp() } else { 1.0 };
            c *= (1.0 + self.vibrance * low * low * skin * 1.2).max(0.0);
        }
        if self.saturation != 0.0 || local_sat != 0.0 {
            c *= (1.0 + self.saturation + local_sat).max(0.0);
        }
        h += local_hue;
        if let Some(bw) = &self.bw {
            let w = band_weights(h);
            let mix: f32 = (0..8).map(|i| w[i] * bw[i]).sum();
            l = (l + mix * (c / 0.2).min(1.0) * 0.25).max(0.0);
            c = 0.0;
        }
        let mut lab = lch_to_lab([l, c, h]);
        if let Some((wheels, blending, balance)) = &self.grading {
            let m = 0.5 - balance * 0.25;
            let width = 0.15 + blending * 0.5;
            let ws = 1.0 - smooth(m - width, m + width * 0.25, lab[0]);
            let wh = smooth(m - width * 0.25, m + width, lab[0]);
            let wm = (1.0 - ws - wh).max(0.0);
            for (k, wt) in wheels.iter().zip([ws, wm, wh, 1.0]) {
                lab[1] += k.a * wt;
                lab[2] += k.b * wt;
                lab[0] += k.lum * wt;
            }
        }
        oklab_to_2020(lab)
    }
}

#[inline]
fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightcraft_color::perceptual::lab_to_lch;

    #[test]
    fn weights_partition_unity() {
        for i in 0..360 {
            let h = (i as f32).to_radians() - PI;
            let w = band_weights(h);
            let s: f32 = w.iter().sum();
            assert!((s - 1.0).abs() < 1e-4, "{i}: {s}");
        }
        // at a band centre, that band dominates
        let w = band_weights(band_hues()[5]);
        assert!(w[5] > 0.99);
    }

    #[test]
    fn neutral_is_identity() {
        let ops = ColorOps::new(&DevelopSettings::default());
        assert!(ops.is_identity());
        assert_eq!(ops.apply([0.2, 0.3, 0.4], 0.0, 0.0), [0.2, 0.3, 0.4]);
    }

    #[test]
    fn saturation_and_bw() {
        let mut s = DevelopSettings::default();
        s.color.saturation = -100.0;
        let g = ColorOps::new(&s).apply([0.4, 0.1, 0.05], 0.0, 0.0);
        assert!((g[0] - g[1]).abs() < 1e-3 && (g[1] - g[2]).abs() < 1e-3, "{g:?}");
        let mut s = DevelopSettings { treatment: lightcraft_develop::Treatment::Bw, ..Default::default() };
        let ops = ColorOps::new(&s);
        let b = ops.apply([0.05, 0.1, 0.5], 0.0, 0.0);
        assert!((b[0] - b[2]).abs() < 1e-3);
        // raising blue in the B&W mix brightens blue things
        s.bw_mix.blue = 80.0;
        let b2 = ColorOps::new(&s).apply([0.05, 0.1, 0.5], 0.0, 0.0);
        assert!(b2[1] > b[1]);
    }

    #[test]
    fn mixer_targets_its_band() {
        let mut s = DevelopSettings::default();
        s.mixer.blue.sat = -100.0;
        let ops = ColorOps::new(&s);
        let blue = [0.02, 0.05, 0.4];
        let red = [0.4, 0.03, 0.02];
        let cb = lab_to_lch(oklab_from_2020(ops.apply(blue, 0.0, 0.0)))[1];
        let cr0 = lab_to_lch(oklab_from_2020(red))[1];
        let cr = lab_to_lch(oklab_from_2020(ops.apply(red, 0.0, 0.0)))[1];
        assert!(cb < 0.02, "{cb}");
        assert!((cr - cr0).abs() < 0.01);
    }

    #[test]
    fn grading_tints_shadows_only() {
        let mut s = DevelopSettings::default();
        s.grading.shadows = lightcraft_develop::Wheel { hue: 220.0, sat: 60.0, lum: 0.0 };
        let ops = ColorOps::new(&s);
        let dark = ops.apply([0.01, 0.01, 0.01], 0.0, 0.0);
        let bright = ops.apply([0.8, 0.8, 0.8], 0.0, 0.0);
        assert!(dark[2] > dark[0]);
        assert!((bright[2] - bright[0]).abs() < 0.02);
    }
}
