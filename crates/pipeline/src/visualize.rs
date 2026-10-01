//! Diagnostic views drawn over a finished render ([`Overlay`]), shared by the CPU and GPU renderers
//! (they run on the 8-bit result, after the histogram is taken).
//!
//! - **Point Color range:** the photo is rendered without that sample's own adjustment; pixels
//!   inside the sample's range keep their colour, the rest turn grey (weighted by the range).

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
}

impl Overlay {
    /// A plain (kind, value) pair, e.g. for sending a request to a web worker.
    pub fn to_parts(self) -> (u8, f32) {
        match self {
            Overlay::None => (0, 0.0),
            Overlay::PointColorRange(i) => (1, i as f32),
        }
    }

    /// Inverse of [`Overlay::to_parts`] (unknown kinds: no overlay).
    pub fn from_parts(kind: u8, v: f32) -> Overlay {
        match kind {
            1 => Overlay::PointColorRange(v as u8),
            _ => Overlay::None,
        }
    }

    /// A stable value for render-cache keys (0 = no overlay).
    pub fn key(self) -> u64 {
        match self {
            Overlay::None => 0,
            Overlay::PointColorRange(i) => 0x1000 + i as u64,
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
