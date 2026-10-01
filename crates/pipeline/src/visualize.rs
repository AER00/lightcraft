//! Diagnostic views drawn over a finished render ([`Overlay`]), shared by the CPU and GPU renderers
//! (they run on the 8-bit result, after the histogram is taken).
//!
//! - **Point Color range:** the photo is rendered without that sample's own adjustment; pixels
//!   inside the sample's range keep their colour, the rest turn grey (weighted by the range).
//! - **Visualize Spots** (Remove tool): a high-pass of the luminance thresholded to black/white,
//!   so dust spots and specks stand out as white dots. The scale is relative to the image (the
//!   same spots show at any preview size); the threshold slider (0..100) raises sensitivity.

use std::borrow::Cow;

use lightcraft_color::perceptual::{lab_to_lch, oklab_from_2020};
use lightcraft_color::transfer::srgb_to_linear;
use lightcraft_color::{REC2020, SRGB};
use lightcraft_develop::DevelopSettings;
use lightcraft_raster::Rgba8;

use crate::colorops::PointK;
use crate::{Plan, for_rows};

/// A diagnostic view of a render.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Overlay {
    #[default]
    None,
    /// Point Color sample `i`: its range in colour, everything else grey.
    PointColorRange(u8),
    /// Visualize Spots with a threshold 0..100 (higher = more sensitive).
    Spots(u8),
}

impl Overlay {
    /// A plain (kind, value) pair, e.g. for sending a request to a web worker.
    pub fn to_parts(self) -> (u8, f32) {
        match self {
            Overlay::None => (0, 0.0),
            Overlay::PointColorRange(i) => (1, i as f32),
            Overlay::Spots(t) => (2, t as f32),
        }
    }

    /// Inverse of [`Overlay::to_parts`] (unknown kinds: no overlay).
    pub fn from_parts(kind: u8, v: f32) -> Overlay {
        match kind {
            1 => Overlay::PointColorRange(v as u8),
            2 => Overlay::Spots(v.clamp(0.0, 100.0) as u8),
            _ => Overlay::None,
        }
    }

    /// A stable value for render-cache keys (0 = no overlay).
    pub fn key(self) -> u64 {
        match self {
            Overlay::None => 0,
            Overlay::PointColorRange(i) => 0x1000 + i as u64,
            Overlay::Spots(t) => 0x2000 + t as u64,
        }
    }
}

/// Settings changes an overlay needs before rendering (e.g. the visualized sample's adjustment
/// is left out, so the range is shown on the colours it selects).
pub fn adjust_settings(o: Overlay, s: &mut Cow<'_, DevelopSettings>) {
    if let Overlay::PointColorRange(i) = o
        && let Some(p) = s.point_colors.get(i as usize)
        && !p.is_neutral()
    {
        let p = &mut s.to_mut().point_colors[i as usize];
        (p.hue_shift, p.sat_shift, p.lum_shift, p.variance) = (0.0, 0.0, 0.0, 0.0);
    }
}

/// Draw overlay `o` over `img` (rendered with `plan`).
pub fn apply(img: &mut Rgba8, o: Overlay, plan: &Plan<'_>) {
    match o {
        Overlay::None => {}
        Overlay::PointColorRange(i) => {
            if let Some(p) = plan.settings.point_colors.get(i as usize) {
                point_color_range(img, &PointK::new(p));
            }
        }
        Overlay::Spots(t) => spots(img, t, plan.px_per_long),
    }
}

/// Blur radius (Gaussian sigma) of the spot view, as a fraction of the long edge.
const SPOT_SIGMA: f64 = 0.006;

/// High-pass threshold (encoded luminance) for slider value `t` (0..100): 100 shows the faintest
/// specks, 0 only strong ones.
pub fn spot_threshold(t: u8) -> f32 {
    let k = 1.0 - t.min(100) as f32 / 100.0;
    0.01 + 0.2 * k * k
}

fn spots(img: &mut Rgba8, t: u8, ppl: f64) {
    let (w, h) = (img.width, img.height);
    if w == 0 || h == 0 {
        return;
    }
    let l = lightcraft_raster::Plane::from_fn(w, h, |x, y| {
        let p = img.data[y * w + x];
        (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32) / 255.0
    });
    let b = lightcraft_raster::blur::gaussian(&l, (SPOT_SIGMA * ppl).max(0.8) as f32);
    let thr = spot_threshold(t);
    for (i, px) in img.data.iter_mut().enumerate() {
        let v = if (l.data[i] - b.data[i]).abs() > thr { 255 } else { 0 };
        *px = [v, v, v, 255];
    }
}

fn point_color_range(img: &mut Rgba8, k: &PointK) {
    let m = SRGB.to_space(&REC2020).to_f32();
    let lut: Vec<f32> = (0..256).map(|v| srgb_to_linear(v as f32 / 255.0)).collect();
    let w = img.width;
    for_rows(&mut img.data, w, |_, row| {
        for px in row.iter_mut() {
            let lin = [lut[px[0] as usize], lut[px[1] as usize], lut[px[2] as usize]];
            let c = std::array::from_fn(|r| m[r][0] * lin[0] + m[r][1] * lin[1] + m[r][2] * lin[2]);
            let [l, ch, h] = lab_to_lch(oklab_from_2020(c));
            let a = k.weight(l, ch, h);
            let grey = 0.2126 * px[0] as f32 + 0.7152 * px[1] as f32 + 0.0722 * px[2] as f32;
            for c in 0..3 {
                px[c] = (grey + (px[c] as f32 - grey) * a).round().clamp(0.0, 255.0) as u8;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RenderRequest, SourceInfo, render};
    use lightcraft_develop::PointColor;
    use lightcraft_raster::Rgb32f;

    #[test]
    fn spots_view_marks_specks_at_any_size() {
        // a smooth gradient with two small dark specks
        let src = Rgb32f::from_fn(400, 300, |x, y| {
            let d = |cx: f32, cy: f32| ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
            let base = 0.1 + 0.3 * x as f32 / 400.0;
            if d(100.0, 100.0) < 2.5 || d(300.0, 200.0) < 2.5 { [base * 0.5; 3] } else { [base; 3] }
        });
        let info = SourceInfo::default();
        let s = DevelopSettings::default();
        for size in [400, 200] {
            let req = RenderRequest { overlay: Overlay::Spots(50), ..RenderRequest::fit(size, size) };
            let v = render(&src, &info, &s, &req).image;
            let at = |x: f64, y: f64| v.data[(y * v.height as f64) as usize * v.width + (x * v.width as f64) as usize];
            assert_eq!(at(0.25, 1.0 / 3.0), [255, 255, 255, 255], "speck at size {size}");
            assert_eq!(at(0.75, 2.0 / 3.0), [255, 255, 255, 255]);
            assert_eq!(at(0.5, 0.5), [0, 0, 0, 255], "smooth gradient is black");
            let white = v.data.iter().filter(|p| p[0] == 255).count() as f64 / v.data.len() as f64;
            assert!(white < 0.01, "{white}");
        }
        assert!(spot_threshold(0) > spot_threshold(50) && spot_threshold(50) > spot_threshold(100));
        let (k, v) = Overlay::Spots(37).to_parts();
        assert_eq!(Overlay::from_parts(k, v), Overlay::Spots(37));
    }

    #[test]
    fn point_color_range_keeps_the_selected_colour_only() {
        // left half orange, right half blue
        let src = Rgb32f::from_fn(32, 16, |x, _| if x < 16 { [0.4, 0.15, 0.04] } else { [0.03, 0.06, 0.35] });
        let info = SourceInfo::default();
        let plain = render(&src, &info, &DevelopSettings::default(), &RenderRequest::fit(32, 16)).image;
        // sample the orange as it renders
        let px = plain.data[8 * 32 + 4];
        let lin = [px[0], px[1], px[2]].map(|v| srgb_to_linear(v as f32 / 255.0));
        let c = SRGB.to_space(&REC2020).apply_f32(lin);
        let [l, ch, h] = lab_to_lch(oklab_from_2020(c));
        let mut s = DevelopSettings::default();
        s.point_colors.push(PointColor { lum: l as f64, chroma: ch as f64, hue: (h as f64).to_degrees(), hue_shift: 80.0, ..Default::default() });
        let req = RenderRequest { overlay: Overlay::PointColorRange(0), ..RenderRequest::fit(32, 16) };
        let v = render(&src, &info, &s, &req).image;
        let (o, b) = (v.data[8 * 32 + 4], v.data[8 * 32 + 28]);
        // the orange stays orange (and unshifted: the overlay shows the selection, not the edit)
        assert!(o[0].abs_diff(px[0]) <= 2 && o[2].abs_diff(px[2]) <= 2, "{o:?} vs {px:?}");
        // the blue turns grey
        assert!(b[0].abs_diff(b[2]) <= 1, "{b:?}");
        // without the overlay the edit shows
        let e = render(&src, &info, &s, &RenderRequest::fit(32, 16)).image.data[8 * 32 + 4];
        assert!(e[1].abs_diff(px[1]) > 10, "{e:?} vs {px:?}");
    }
}
