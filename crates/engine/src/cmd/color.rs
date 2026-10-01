//! Commands that sample the developed image: Point Color samples and the targeted adjustment tool.
//!
//! Sampling renders a small proxy with every stage *after* the sampled one neutralized, so the
//! picked colour is the one the adjustment will see (e.g. Point Color samples after the colour
//! mixer, before vibrance, grading, vignette and curves).

use lightcraft_color::perceptual::{lab_to_lch, oklab_from_2020};
use lightcraft_color::transfer::srgb_to_linear;
use lightcraft_color::{REC2020, SRGB};
use lightcraft_develop::{DevelopSettings, MAX_POINT_COLORS, PointColor, Treatment};
use lightcraft_geom::Point;
use serde_json::{Value, json};

use super::{CommandSpec, bad, cmd, f64_req, has_active};
use crate::media::SourceLevel;
use crate::{Result, Session};

/// Long edge of the proxy rendered for sampling.
const PROBE_EDGE: usize = 384;

/// The sRGB-encoded colour (0..1) at normalized image point `(x, y)` of the active photo rendered
/// with its settings changed by `neutral`, averaged over 3 × 3 proxy pixels.
pub(crate) fn probe(s: &mut Session, c: &str, x: f64, y: f64, neutral: impl FnOnce(&mut DevelopSettings)) -> Result<[f32; 3]> {
    let id = s.active().ok_or_else(|| bad(c, "no active photo"))?;
    let src = s.source_now(id, SourceLevel::Thumb).map_err(|e| bad(c, e))?;
    let info = s.catalog.photo(id).map(|p| crate::media::source_info(p)).unwrap_or_default();
    let mut d = (*s.develop_of(id).unwrap_or_default()).clone();
    neutral(&mut d);
    let req = lightcraft_pipeline::RenderRequest::fit(PROBE_EDGE, PROBE_EDGE);
    let plan = lightcraft_pipeline::plan(&src, &info, &d, &req);
    let q = plan.frame.norm_to_out(plan.w, plan.h).apply(Point::new(x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)));
    let img = lightcraft_pipeline::render(&src, &info, &d, &req).image;
    let (cx, cy) = (q.x.floor() as isize, q.y.floor() as isize);
    let mut acc = [0.0f32; 3];
    let mut n = 0.0;
    for dy in -1..=1 {
        for dx in -1..=1 {
            let (px, py) = ((cx + dx).clamp(0, img.width as isize - 1) as usize, (cy + dy).clamp(0, img.height as isize - 1) as usize);
            let p = img.data[py * img.width + px];
            for k in 0..3 {
                acc[k] += p[k] as f32 / 255.0;
            }
            n += 1.0;
        }
    }
    Ok(acc.map(|v| v / n))
}

/// sRGB-encoded → OkLCh (lightness, chroma, hue in radians) via linear Rec.2020.
pub(crate) fn encoded_to_oklch(e: [f32; 3]) -> [f32; 3] {
    let lin = SRGB.to_space(&REC2020).apply_f32(e.map(srgb_to_linear));
    lab_to_lch(oklab_from_2020(lin))
}

/// Everything after the colour mixer, neutralized (the colour Point Color sees).
pub(crate) fn after_mixer_neutral(d: &mut DevelopSettings) {
    d.color.vibrance = 0.0;
    d.color.saturation = 0.0;
    d.point_colors.clear();
    d.treatment = Treatment::Color;
    d.grading = Default::default();
    d.vignette.amount = 0.0;
    d.grain.amount = 0.0;
    d.curve = Default::default();
}

fn index(p: &Value, c: &str) -> Result<usize> {
    Ok(f64_req(p, "index", c)? as usize)
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "pointColor.pick",
            "Add Point Color Sample",
            [],
            None,
            "{x, y} normalized image coords — samples the colour there (max 8); returns {index, lum, chroma, hue}",
            has_active,
            |s, p| {
                let c = "pointColor.pick";
                let (x, y) = (f64_req(p, "x", c)?, f64_req(p, "y", c)?);
                let id = s.active().ok_or_else(|| bad(c, "no active photo"))?;
                if s.develop_of(id).is_some_and(|d| d.point_colors.len() >= MAX_POINT_COLORS) {
                    return Err(bad(c, format!("at most {MAX_POINT_COLORS} point colours")));
                }
                let [l, ch, h] = encoded_to_oklch(probe(s, c, x, y, after_mixer_neutral)?);
                let sample = PointColor { lum: l as f64, chroma: ch as f64, hue: (h as f64).to_degrees().rem_euclid(360.0), ..Default::default() };
                let mut d = (*s.develop_of(id).unwrap_or_default()).clone();
                d.point_colors.push(sample);
                let i = d.point_colors.len() - 1;
                s.set_develop(id, d, "Point Color")?;
                Ok(json!({"index": i, "lum": sample.lum, "chroma": sample.chroma, "hue": sample.hue}))
            }
        ),
        cmd!("pointColor.delete", "Delete Point Color Sample", [], None, "{index}", has_active, |s, p| {
            let c = "pointColor.delete";
            let i = index(p, c)?;
            let id = s.active().ok_or_else(|| bad(c, "no active photo"))?;
            let mut d = (*s.develop_of(id).unwrap_or_default()).clone();
            if i >= d.point_colors.len() {
                return Err(bad(c, "no such sample"));
            }
            d.point_colors.remove(i);
            s.set_develop(id, d, "Delete Point Color")?;
            Ok(Value::Null)
        }),
    ]
}
