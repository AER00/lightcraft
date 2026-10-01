//! Masking and Remove (spot) commands on the active photo.

use lightcraft_develop::{BrushStroke, LocalAdjustments, Mask, MaskComponent, MaskOp, MaskShape, RedEye, Spot, SpotMode};
use lightcraft_geom::Point;
use serde_json::{Value, json};

use super::{CommandSpec, bad, bool_or, cmd, f64_or, has_active, ok, point, str_param};
use crate::{Result, Session};

fn shape_from(kind: &str, p: &Value, c: &str) -> Result<MaskShape> {
    Ok(match kind {
        "brush" => MaskShape::Brush { strokes: vec![] },
        "linear" => {
            MaskShape::Linear { start: point(p, "start").unwrap_or(Point::new(0.5, 0.25)), end: point(p, "end").unwrap_or(Point::new(0.5, 0.6)) }
        }
        "radial" => MaskShape::Radial {
            center: point(p, "center").unwrap_or(Point::new(0.5, 0.5)),
            rx: f64_or(p, "rx", 0.22),
            ry: f64_or(p, "ry", 0.16),
            angle: f64_or(p, "angle", 0.0),
            feather: f64_or(p, "feather", 50.0),
            invert: bool_or(p, "invert", false),
        },
        "sky" => MaskShape::Sky,
        "subject" => MaskShape::Subject,
        "background" => MaskShape::Background,
        "luminanceRange" => MaskShape::LuminanceRange {
            lo: f64_or(p, "lo", 0.6),
            hi: f64_or(p, "hi", 1.0),
            lo_feather: f64_or(p, "loFeather", 0.1),
            hi_feather: f64_or(p, "hiFeather", 0.1),
        },
        "colorRange" => {
            let samples: Vec<[f64; 3]> = p
                .get("samples")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|s| Some([s.get(0)?.as_f64()?, s.get(1)?.as_f64()?, s.get(2)?.as_f64()?])).collect())
                .unwrap_or_default();
            MaskShape::ColorRange { samples, refine: f64_or(p, "refine", 50.0) }
        }
        other => return Err(bad(c, format!("unknown mask kind `{other}` (brush|linear|radial|sky|subject|background|luminanceRange|colorRange)"))),
    })
}

fn masks_edit(s: &mut Session, c: &str, label: &str, f: impl FnOnce(&mut Vec<Mask>, &mut Option<u32>) -> Result<()>) -> Result<Value> {
    let id = s.active().ok_or_else(|| bad(c, "no active photo"))?;
    let mut d = (*s.develop_of(id).unwrap_or_default()).clone();
    let mut active = s.active_mask;
    f(&mut d.masks, &mut active)?;
    s.set_develop(id, d, label)?;
    s.active_mask = active;
    Ok(json!({"activeMask": active}))
}

fn mask_id(p: &Value, active: Option<u32>, c: &str) -> Result<u32> {
    p.get("id").and_then(Value::as_u64).map(|v| v as u32).or(active).ok_or_else(|| bad(c, "no mask selected (give `id`)"))
}

fn find(masks: &mut [Mask], id: u32, c: &str) -> Result<usize> {
    masks.iter().position(|m| m.id == id).ok_or_else(|| bad(c, format!("no mask {id}")))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "mask.add",
            "Create New Mask",
            [],
            None,
            "{kind: brush|linear|radial|sky|subject|background|luminanceRange|colorRange, ...shape params (start/end, center/rx/ry/angle/feather, lo/hi…), name?}",
            has_active,
            |s, p| {
                let kind = str_param(p, "kind").unwrap_or("radial").to_string();
                let shape = shape_from(&kind, p, "mask.add")?;
                let name = str_param(p, "name").map(str::to_string);
                let next = s.active().and_then(|id| s.develop_of(id)).map(|d| d.next_mask_id()).unwrap_or(1);
                masks_edit(s, "mask.add", "Add Mask", |masks, active| {
                    masks.push(Mask {
                        id: next,
                        name: name.unwrap_or_else(|| format!("Mask {next}")),
                        components: vec![MaskComponent { op: MaskOp::Add, invert: false, shape }],
                        ..Default::default()
                    });
                    *active = Some(next);
                    Ok(())
                })
            }
        ),
        cmd!(
            "mask.addComponent",
            "Add/Subtract/Intersect Mask",
            [],
            None,
            "{id?: maskId, op: add|subtract|intersect, kind, ...shape params}",
            has_active,
            |s, p| {
                let kind = str_param(p, "kind").unwrap_or("brush").to_string();
                let shape = shape_from(&kind, p, "mask.addComponent")?;
                let op: MaskOp =
                    serde_json::from_value(p.get("op").cloned().unwrap_or(json!("add"))).map_err(|e| bad("mask.addComponent", e.to_string()))?;
                let active = s.active_mask;
                let mid = mask_id(p, active, "mask.addComponent")?;
                masks_edit(s, "mask.addComponent", "Edit Mask", |masks, _| {
                    let i = find(masks, mid, "mask.addComponent")?;
                    masks[i].components.push(MaskComponent { op, invert: bool_or(p, "invert", false), shape });
                    Ok(())
                })
            }
        ),
        cmd!(
            "mask.brushStroke",
            "Brush Stroke",
            [],
            None,
            "{id?: maskId, points: [[x,y],…] normalized, size?: fraction of long edge (0.03), feather?, flow?, density?, erase?: bool, autoMask?: bool}",
            has_active,
            |s, p| {
                let pts: Vec<Point> = p
                    .get("points")
                    .and_then(Value::as_array)
                    .ok_or_else(|| bad("mask.brushStroke", "missing points"))?
                    .iter()
                    .filter_map(|q| Some(Point::new(q.get(0)?.as_f64()?, q.get(1)?.as_f64()?)))
                    .collect();
                let d = BrushStroke::default();
                let stroke = BrushStroke {
                    points: pts,
                    size: f64_or(p, "size", d.size),
                    feather: f64_or(p, "feather", d.feather),
                    flow: f64_or(p, "flow", d.flow),
                    density: f64_or(p, "density", d.density),
                    erase: bool_or(p, "erase", false),
                    auto_mask: bool_or(p, "autoMask", false),
                };
                let next = s.active().and_then(|id| s.develop_of(id)).map(|d| d.next_mask_id()).unwrap_or(1);
                let want = p.get("id").and_then(Value::as_u64).map(|v| v as u32).or(s.active_mask);
                masks_edit(s, "mask.brushStroke", "Brush", |masks, active| {
                    let i = match want.and_then(|m| masks.iter().position(|x| x.id == m)) {
                        Some(i) => i,
                        None => {
                            masks.push(Mask { id: next, name: format!("Mask {next}"), ..Default::default() });
                            *active = Some(next);
                            masks.len() - 1
                        }
                    };
                    let m = &mut masks[i];
                    if let Some(MaskComponent { shape: MaskShape::Brush { strokes }, .. }) =
                        m.components.iter_mut().rev().find(|c| matches!(c.shape, MaskShape::Brush { .. }))
                    {
                        strokes.push(stroke);
                    } else {
                        m.components.push(MaskComponent { op: MaskOp::Add, invert: false, shape: MaskShape::Brush { strokes: vec![stroke] } });
                    }
                    Ok(())
                })
            }
        ),
        cmd!("mask.update", "Update Mask Shape", [], None, "{id?, component?: index (0), shape: MaskShape JSON}", has_active, |s, p| {
            let shape: MaskShape =
                serde_json::from_value(p.get("shape").cloned().unwrap_or_default()).map_err(|e| bad("mask.update", e.to_string()))?;
            let comp = p.get("component").and_then(Value::as_u64).unwrap_or(0) as usize;
            let mid = mask_id(p, s.active_mask, "mask.update")?;
            masks_edit(s, "mask.update", "Edit Mask", |masks, _| {
                let i = find(masks, mid, "mask.update")?;
                let c = masks[i].components.get_mut(comp).ok_or_else(|| bad("mask.update", "no such component"))?;
                c.shape = shape;
                Ok(())
            })
        }),
        cmd!(
            "mask.adjust",
            "Set Mask Adjustments",
            [],
            None,
            "{id?, values: {exposure, contrast, highlights, shadows, whites, blacks, temp, tint, texture, clarity, dehaze, hue, saturation, sharpness, noise, moire, defringe, color_hue, color_sat, amount}}",
            has_active,
            |s, p| {
                let vals = p.get("values").cloned().ok_or_else(|| bad("mask.adjust", "missing values"))?;
                let mid = mask_id(p, s.active_mask, "mask.adjust")?;
                masks_edit(s, "mask.adjust", "Mask Adjustment", |masks, _| {
                    let i = find(masks, mid, "mask.adjust")?;
                    let mut v = serde_json::to_value(masks[i].adjust).unwrap_or_default();
                    lightcraft_develop::presets::deep_merge(&mut v, &vals);
                    let a: LocalAdjustments = serde_json::from_value(v).map_err(|e| bad("mask.adjust", e.to_string()))?;
                    masks[i].adjust = clamp_local(a);
                    Ok(())
                })
            }
        ),
        cmd!("mask.select", "Select Mask", [], None, "{id|null}", has_active, |s, p| {
            s.active_mask = p.get("id").and_then(Value::as_u64).map(|v| v as u32);
            ok()
        }),
        cmd!("mask.delete", "Delete Mask", [], None, "{id?}", has_active, |s, p| {
            let mid = mask_id(p, s.active_mask, "mask.delete")?;
            masks_edit(s, "mask.delete", "Delete Mask", |masks, active| {
                let i = find(masks, mid, "mask.delete")?;
                masks.remove(i);
                *active = masks.last().map(|m| m.id);
                Ok(())
            })
        }),
        cmd!("mask.deleteAll", "Delete All Masks", [], None, "{}", has_active, |s, _| {
            masks_edit(s, "mask.deleteAll", "Delete All Masks", |masks, active| {
                masks.clear();
                *active = None;
                Ok(())
            })
        }),
        cmd!("mask.rename", "Rename Mask", [], None, "{id?, name}", has_active, |s, p| {
            let name = str_param(p, "name").ok_or_else(|| bad("mask.rename", "missing name"))?.to_string();
            let mid = mask_id(p, s.active_mask, "mask.rename")?;
            masks_edit(s, "mask.rename", "Rename Mask", |masks, _| {
                let i = find(masks, mid, "mask.rename")?;
                masks[i].name = name;
                Ok(())
            })
        }),
        cmd!("mask.invert", "Invert Mask", [], None, "{id?}", has_active, |s, p| {
            let mid = mask_id(p, s.active_mask, "mask.invert")?;
            masks_edit(s, "mask.invert", "Invert Mask", |masks, _| {
                let i = find(masks, mid, "mask.invert")?;
                masks[i].invert = !masks[i].invert;
                Ok(())
            })
        }),
        cmd!("mask.visible", "Show/Hide Mask", [], None, "{id?, visible?: bool}", has_active, |s, p| {
            let mid = mask_id(p, s.active_mask, "mask.visible")?;
            masks_edit(s, "mask.visible", "Toggle Mask", |masks, _| {
                let i = find(masks, mid, "mask.visible")?;
                masks[i].visible = bool_or(p, "visible", !masks[i].visible);
                Ok(())
            })
        }),
        cmd!("mask.duplicate", "Duplicate Mask", [], None, "{id?}", has_active, |s, p| {
            let mid = mask_id(p, s.active_mask, "mask.duplicate")?;
            let next = s.active().and_then(|id| s.develop_of(id)).map(|d| d.next_mask_id()).unwrap_or(1);
            masks_edit(s, "mask.duplicate", "Duplicate Mask", |masks, active| {
                let i = find(masks, mid, "mask.duplicate")?;
                let mut m = masks[i].clone();
                m.id = next;
                m.name = format!("{} copy", m.name);
                masks.push(m);
                *active = Some(next);
                Ok(())
            })
        }),
        // ---- Remove tool (spots)
        cmd!(
            "spot.add",
            "Add Remove Spot",
            [],
            None,
            "{mode?: remove|heal|clone, points: [[x,y],…], size?: fraction of long edge, feather?, opacity?, source?: [dx,dy]}",
            has_active,
            |s, p| {
                let mode: SpotMode =
                    serde_json::from_value(p.get("mode").cloned().unwrap_or(json!("remove"))).map_err(|e| bad("spot.add", e.to_string()))?;
                let pts: Vec<Point> = p
                    .get("points")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(|q| Some(Point::new(q.get(0)?.as_f64()?, q.get(1)?.as_f64()?))).collect())
                    .unwrap_or_default();
                if pts.is_empty() {
                    return Err(bad("spot.add", "missing points"));
                }
                let d = Spot::default();
                let spot = Spot {
                    mode,
                    points: pts,
                    size: f64_or(p, "size", d.size),
                    feather: f64_or(p, "feather", d.feather),
                    opacity: f64_or(p, "opacity", d.opacity),
                    source_offset: point(p, "source"),
                };
                let id = s.active().ok_or_else(|| bad("spot.add", "no active photo"))?;
                let mut dd = (*s.develop_of(id).unwrap_or_default()).clone();
                dd.spots.push(spot);
                let n = dd.spots.len();
                s.set_develop(id, dd, "Remove")?;
                Ok(json!({"index": n - 1}))
            }
        ),
        cmd!("spot.delete", "Delete Spot", [], None, "{index}", has_active, |s, p| {
            let i = super::f64_req(p, "index", "spot.delete")? as usize;
            let id = s.active().ok_or_else(|| bad("spot.delete", "no active photo"))?;
            let mut dd = (*s.develop_of(id).unwrap_or_default()).clone();
            if i >= dd.spots.len() {
                return Err(bad("spot.delete", "no such spot"));
            }
            dd.spots.remove(i);
            s.set_develop(id, dd, "Delete Spot")?;
            ok()
        }),
        // ---- Red eye / pet eye
        cmd!(
            "redeye.add",
            "Add Red Eye Correction",
            [],
            None,
            "{center: [x,y] normalized, rx, ry: radii as fractions of the long edge, pet?: bool, pupilSize?: 0..100, darken?: 0..100} — the pupil inside is found automatically; returns {index}",
            has_active,
            |s, p| {
                let c = "redeye.add";
                let center = point(p, "center").ok_or_else(|| bad(c, "missing center"))?;
                let d = RedEye::default();
                let eye = RedEye {
                    center,
                    rx: f64_or(p, "rx", d.rx).clamp(1e-4, 0.5),
                    ry: f64_or(p, "ry", d.ry).clamp(1e-4, 0.5),
                    pupil_size: f64_or(p, "pupilSize", d.pupil_size).clamp(0.0, 100.0),
                    darken: f64_or(p, "darken", d.darken).clamp(0.0, 100.0),
                    pet: bool_or(p, "pet", false),
                    catchlight: None,
                };
                let id = s.active().ok_or_else(|| bad(c, "no active photo"))?;
                let mut dd = (*s.develop_of(id).unwrap_or_default()).clone();
                dd.red_eye.push(eye);
                let n = dd.red_eye.len();
                s.set_develop(id, dd, if eye.pet { "Pet Eye" } else { "Red Eye" })?;
                Ok(json!({"index": n - 1}))
            }
        ),
        cmd!("redeye.delete", "Delete Red Eye Correction", [], None, "{index}", has_active, |s, p| {
            let c = "redeye.delete";
            let i = super::f64_req(p, "index", c)? as usize;
            let id = s.active().ok_or_else(|| bad(c, "no active photo"))?;
            let mut dd = (*s.develop_of(id).unwrap_or_default()).clone();
            if i >= dd.red_eye.len() {
                return Err(bad(c, "no such eye"));
            }
            dd.red_eye.remove(i);
            s.set_develop(id, dd, "Delete Red Eye")?;
            ok()
        }),
    ]
}

fn clamp_local(mut a: LocalAdjustments) -> LocalAdjustments {
    let c = |v: &mut f64, lo: f64, hi: f64| *v = if v.is_finite() { v.clamp(lo, hi) } else { 0.0 };
    c(&mut a.exposure, -4.0, 4.0);
    for v in [
        &mut a.temp,
        &mut a.tint,
        &mut a.contrast,
        &mut a.highlights,
        &mut a.shadows,
        &mut a.whites,
        &mut a.blacks,
        &mut a.texture,
        &mut a.clarity,
        &mut a.dehaze,
        &mut a.hue,
        &mut a.saturation,
        &mut a.sharpness,
        &mut a.noise,
        &mut a.moire,
        &mut a.defringe,
    ] {
        c(v, -100.0, 100.0);
    }
    c(&mut a.color_hue, 0.0, 360.0);
    c(&mut a.color_sat, 0.0, 100.0);
    c(&mut a.amount, 0.0, 200.0);
    a
}
