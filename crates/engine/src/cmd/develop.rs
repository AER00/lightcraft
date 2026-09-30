//! Develop commands. All operate on the active photo unless `ids` are given (sync).

use lightcraft_catalog::{Op, PhotoId, Version};
use lightcraft_develop::{DevelopSettings, Preset, Section, SettingsGroup, Treatment, WbMode, controls};
use lightcraft_geom::{CropGeometry, Point, Rect, crop_fit_angle};
use serde_json::{Value, json};

use super::{CommandSpec, always, bad, bool_or, cmd, f64_or, f64_req, has_active, has_clipboard, has_selection, ok, str_param};
use crate::{Result, Session, media::SourceLevel};

fn active(s: &Session, c: &str) -> Result<PhotoId> {
    s.active().ok_or_else(|| bad(c, "no active photo"))
}

/// Modify the active photo's settings with `f` and commit (or preview during an interaction).
fn edit(s: &mut Session, c: &str, label: &str, f: impl FnOnce(&mut DevelopSettings) -> Result<()>) -> Result<Value> {
    let id = active(s, c)?;
    let mut d = (*s.develop_of(id).unwrap_or_default()).clone();
    f(&mut d)?;
    s.set_develop(id, d, label)?;
    ok()
}

fn section_param(p: &Value, c: &str) -> Result<Section> {
    let v = p.get("section").cloned().ok_or_else(|| bad(c, "missing `section`"))?;
    serde_json::from_value(v).map_err(|e| bad(c, e.to_string()))
}

fn groups_param(p: &Value) -> Option<Vec<SettingsGroup>> {
    p.get("groups").and_then(|g| serde_json::from_value(g.clone()).ok())
}

fn crop_dims(s: &Session, id: PhotoId) -> (f64, f64) {
    let p = s.catalog.photo(id);
    let (w, h) = p.map(|p| (p.width.max(1) as f64, p.height.max(1) as f64)).unwrap_or((3.0, 2.0));
    let o = p.map(|p| p.develop.orientation).unwrap_or_default();
    if o.swaps_axes() { (h, w) } else { (w, h) }
}

fn set_aspect(d: &mut DevelopSettings, w: f64, h: f64, aspect: Option<(u32, u32)>) {
    d.crop.aspect = aspect;
    let a = aspect.map(|(aw, ah)| {
        // orientation of the crop follows the image (landscape image → landscape crop)
        let (aw, ah) = (aw as f64, ah as f64);
        if (w >= h) == (aw >= ah) { aw / ah } else { ah / aw }
    });
    let angle = d.crop.geometry.angle;
    d.crop.geometry = crop_fit_angle(w, h, angle, a);
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "develop.set",
            "Set Develop Values",
            [],
            None,
            "{control?: controlId, value?: number, values?: {controlId: number}, ids?: [photo]} — see develop.controls",
            has_active,
            |s, p| {
                let mut vals: Vec<(String, f64)> = Vec::new();
                if let (Some(c), Some(v)) = (str_param(p, "control"), p.get("value").and_then(Value::as_f64)) {
                    vals.push((c.to_string(), v));
                }
                if let Some(o) = p.get("values").and_then(Value::as_object) {
                    for (k, v) in o {
                        vals.push((k.clone(), v.as_f64().ok_or_else(|| bad("develop.set", format!("`{k}` must be a number")))?));
                    }
                }
                if vals.is_empty() {
                    return Err(bad("develop.set", "give `control`+`value` or `values`"));
                }
                for (k, _) in &vals {
                    if controls::find(k).is_none() {
                        return Err(bad("develop.set", format!("unknown control `{k}`")));
                    }
                }
                let label = if vals.len() == 1 { controls::find(&vals[0].0).map(|c| c.label).unwrap_or("Edit").to_string() } else { "Edit".into() };
                let apply = |d: &mut DevelopSettings| {
                    for (k, v) in &vals {
                        controls::set(d, k, *v);
                        if k == "wb.temp" || k == "wb.tint" {
                            d.wb.mode = WbMode::Custom;
                        }
                    }
                };
                if let Some(ids) = p.get("ids").and_then(Value::as_array) {
                    let ids: Vec<PhotoId> = ids.iter().filter_map(Value::as_u64).map(PhotoId).collect();
                    let ops = ids
                        .iter()
                        .filter_map(|id| s.develop_of(*id).map(|d| (*id, d)))
                        .filter_map(|(id, d)| {
                            let mut d = (*d).clone();
                            apply(&mut d);
                            s.develop_op(id, d, &label)
                        })
                        .collect();
                    s.commit(&label, Op::Batch { ops })?;
                    return ok();
                }
                edit(s, "develop.set", &label, |d| {
                    apply(d);
                    Ok(())
                })
            }
        ),
        cmd!("develop.adjust", "Nudge Develop Value", [], None, "{control, delta}", has_active, |s, p| {
            let c = str_param(p, "control").ok_or_else(|| bad("develop.adjust", "missing control"))?.to_string();
            let spec = controls::find(&c).ok_or_else(|| bad("develop.adjust", "unknown control"))?;
            let delta = f64_req(p, "delta", "develop.adjust")?;
            edit(s, "develop.adjust", spec.label, |d| {
                let v = controls::get(d, &c).unwrap_or(spec.default);
                controls::set(d, &c, v + delta);
                Ok(())
            })
        }),
        cmd!("develop.merge", "Apply Settings JSON", [], None, "{settings: partial DevelopSettings JSON, label?}", has_active, |s, p| {
            let partial = p.get("settings").cloned().ok_or_else(|| bad("develop.merge", "missing settings"))?;
            let label = str_param(p, "label").unwrap_or("Edit").to_string();
            edit(s, "develop.merge", &label, |d| {
                *d = d.merged(&partial).map_err(|e| bad("develop.merge", e.to_string()))?;
                Ok(())
            })
        }),
        cmd!("develop.reset", "Reset Edits", ["Photo"], Some("Cmd+Shift+R"), "{ids?}", has_selection, |s, p| {
            let ids = s.targets(p);
            let ops = ids
                .iter()
                .filter_map(|id| s.develop_of(*id).map(|d| (*id, d)))
                .filter_map(|(id, d)| {
                    let fresh = DevelopSettings { wb: lightcraft_develop::WhiteBalance { mode: WbMode::AsShot, ..d.wb }, ..Default::default() };
                    s.develop_op(id, fresh, "Reset")
                })
                .collect();
            s.commit("Reset", Op::Batch { ops })?;
            ok()
        }),
        cmd!(
            "develop.resetSection",
            "Reset Section",
            [],
            None,
            "{section: light|curve|color|mixer|grading|effects|vignette|grain|detail|optics|geometry}",
            has_active,
            |s, p| {
                let sec = section_param(p, "develop.resetSection")?;
                edit(s, "develop.resetSection", &format!("Reset {}", sec.label()), |d| {
                    d.reset_section(sec);
                    Ok(())
                })
            }
        ),
        cmd!("develop.resetControl", "Reset Control", [], None, "{control}", has_active, |s, p| {
            let c = str_param(p, "control").ok_or_else(|| bad("develop.resetControl", "missing control"))?.to_string();
            let spec = controls::find(&c).ok_or_else(|| bad("develop.resetControl", "unknown control"))?;
            edit(s, "develop.resetControl", spec.label, |d| {
                controls::set(d, &c, spec.default);
                Ok(())
            })
        }),
        cmd!("develop.sectionEnabled", "Toggle Section", [], None, "{section: string, enabled: bool}", has_active, |s, p| {
            let sec = str_param(p, "section").ok_or_else(|| bad("develop.sectionEnabled", "missing section"))?.to_string();
            let on = bool_or(p, "enabled", true);
            edit(s, "develop.sectionEnabled", "Toggle Section", |d| {
                d.set_section_enabled(&sec, on);
                Ok(())
            })
        }),
        cmd!("develop.auto", "Auto Settings", ["Photo"], Some("Shift+A"), "{}", has_active, |s, _| {
            let id = active(s, "develop.auto")?;
            let src = s.source_now(id, SourceLevel::Thumb).map_err(|e| bad("develop.auto", e))?;
            let info = s.catalog.photo(id).map(|p| crate::media::source_info(p)).unwrap_or_default();
            let d = s.develop_of(id).unwrap_or_default();
            let a = lightcraft_pipeline::auto::auto_tone(&src, &info, &d);
            edit(s, "develop.auto", "Auto", |d| {
                d.light.exposure = a.exposure;
                d.light.contrast = a.contrast;
                d.light.highlights = a.highlights;
                d.light.shadows = a.shadows;
                d.light.whites = a.whites;
                d.light.blacks = a.blacks;
                d.color.vibrance = a.vibrance;
                d.color.saturation = a.saturation;
                Ok(())
            })?;
            Ok(serde_json::to_value(a).unwrap_or_default())
        }),
        cmd!(
            "develop.wb",
            "White Balance",
            [],
            None,
            "{mode: asShot|auto|daylight|cloudy|shade|tungsten|fluorescent|flash|custom, temp?, tint?}",
            has_active,
            |s, p| {
                let mode: WbMode =
                    serde_json::from_value(p.get("mode").cloned().unwrap_or(json!("custom"))).map_err(|e| bad("develop.wb", e.to_string()))?;
                let id = active(s, "develop.wb")?;
                let info = s.catalog.photo(id).map(|p| crate::media::source_info(p)).unwrap_or_default();
                let (mut t, mut tint) = match mode {
                    WbMode::AsShot => (info.as_shot_temp, info.as_shot_tint),
                    WbMode::Auto => {
                        let src = s.source_now(id, SourceLevel::Thumb).map_err(|e| bad("develop.wb", e))?;
                        lightcraft_pipeline::auto::auto_wb(&src, &info)
                    }
                    m => m
                        .preset()
                        .map(|(t, ti)| {
                            // presets are relative to daylight for rendered files
                            if info.raw { (t, ti) } else { (6500.0 * t / 5500.0, ti) }
                        })
                        .unwrap_or((info.as_shot_temp, info.as_shot_tint)),
                };
                t = f64_or(p, "temp", t);
                tint = f64_or(p, "tint", tint);
                // The resolved temperature/tint is stored, so rendering never depends on the mode; the
                // mode is kept for the UI's WB dropdown.
                edit(s, "develop.wb", "White Balance", |d| {
                    d.wb.mode = mode;
                    d.wb.temp = t.clamp(2000.0, 50000.0);
                    d.wb.tint = tint.clamp(-150.0, 150.0);
                    Ok(())
                })?;
                Ok(json!({"temp": t, "tint": tint}))
            }
        ),
        cmd!("develop.wbPick", "White Balance Selector", [], Some("W"), "{x, y} normalized image coords of a neutral point", has_active, |s, p| {
            let (x, y) = (f64_req(p, "x", "develop.wbPick")?, f64_req(p, "y", "develop.wbPick")?);
            let id = active(s, "develop.wbPick")?;
            let src = s.source_now(id, SourceLevel::Thumb).map_err(|e| bad("develop.wbPick", e))?;
            let info = s.catalog.photo(id).map(|p| crate::media::source_info(p)).unwrap_or_default();
            let d = s.develop_of(id).unwrap_or_default();
            // Sample a small neighbourhood in the oriented source.
            let o = src.oriented(d.orientation);
            let (cx, cy) = ((x.clamp(0.0, 1.0) * o.width as f64) as isize, (y.clamp(0.0, 1.0) * o.height as f64) as isize);
            let mut acc = [0.0f32; 3];
            for dy in -2..=2 {
                for dx in -2..=2 {
                    let c = o.get_clamped(cx + dx, cy + dy);
                    for i in 0..3 {
                        acc[i] += c[i] / 25.0;
                    }
                }
            }
            let patch = lightcraft_raster::Rgb32f::filled(4, 4, acc);
            let (t, tint) = lightcraft_pipeline::auto::auto_wb(&patch, &info);
            edit(s, "develop.wbPick", "White Balance", |d| {
                d.wb.mode = WbMode::Custom;
                d.wb.temp = t;
                d.wb.tint = tint;
                Ok(())
            })?;
            Ok(json!({"temp": t, "tint": tint}))
        }),
        cmd!("develop.treatment", "Black & White", ["Photo"], Some("V"), "{bw?: bool} (toggles when omitted)", has_active, |s, p| {
            edit(s, "develop.treatment", "Treatment", |d| {
                let bw = p.get("bw").and_then(Value::as_bool).unwrap_or(d.treatment != Treatment::Bw);
                d.treatment = if bw { Treatment::Bw } else { Treatment::Color };
                Ok(())
            })
        }),
        cmd!("develop.profile", "Set Profile", [], None, "{id: profile id (see profiles.list), amount?: 0..200}", has_active, |s, p| {
            let id = str_param(p, "id").ok_or_else(|| bad("develop.profile", "missing id"))?.to_string();
            if !crate::presets::PROFILES.iter().any(|x| x.id == id) {
                return Err(bad("develop.profile", format!("unknown profile `{id}`")));
            }
            edit(s, "develop.profile", "Profile", |d| {
                d.profile.id = id;
                d.profile.amount = f64_or(p, "amount", d.profile.amount).clamp(0.0, 200.0);
                Ok(())
            })
        }),
        cmd!("develop.curve", "Set Point Curve", [], None, "{channel: master|red|green|blue, points: [[x,y],...] in 0..1}", has_active, |s, p| {
            let ch = str_param(p, "channel").unwrap_or("master").to_string();
            let pts: Vec<Point> = p
                .get("points")
                .and_then(Value::as_array)
                .ok_or_else(|| bad("develop.curve", "missing points"))?
                .iter()
                .filter_map(|q| Some(Point::new(q.get(0)?.as_f64()?.clamp(0.0, 1.0), q.get(1)?.as_f64()?.clamp(0.0, 1.0))))
                .collect();
            edit(s, "develop.curve", "Tone Curve", |d| {
                match ch.as_str() {
                    "master" | "rgb" => d.curve.master = pts,
                    "red" => d.curve.red = pts,
                    "green" => d.curve.green = pts,
                    "blue" => d.curve.blue = pts,
                    other => return Err(bad("develop.curve", format!("unknown channel `{other}`"))),
                }
                Ok(())
            })
        }),
        // ---- crop & rotate
        cmd!("crop.set", "Set Crop", [], None, "{rect?: [x0,y0,x1,y1] normalized in the straightened frame, angle?: degrees}", has_active, |s, p| {
            let id = active(s, "crop.set")?;
            let (w, h) = crop_dims(s, id);
            edit(s, "crop.set", "Crop", |d| {
                if let Some(a) = p.get("rect").and_then(Value::as_array) {
                    let v: Vec<f64> = a.iter().filter_map(Value::as_f64).collect();
                    if v.len() != 4 {
                        return Err(bad("crop.set", "rect needs 4 numbers"));
                    }
                    d.crop.geometry.rect = Rect::new(v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3]));
                }
                if let Some(a) = p.get("angle").and_then(Value::as_f64) {
                    d.crop.geometry.angle = a.clamp(-45.0, 45.0);
                }
                d.crop.geometry = d.crop.geometry.constrained(w, h);
                Ok(())
            })
        }),
        cmd!(
            "crop.straighten",
            "Straighten",
            [],
            None,
            "{angle: degrees -45..45} — keeps the largest crop of the current aspect",
            has_active,
            |s, p| {
                let id = active(s, "crop.straighten")?;
                let (w, h) = crop_dims(s, id);
                let angle = f64_req(p, "angle", "crop.straighten")?.clamp(-45.0, 45.0);
                edit(s, "crop.straighten", "Straighten", |d| {
                    let r = d.crop.geometry.rect_px(w, h);
                    let fit = crop_fit_angle(w, h, angle, Some(r.width() / r.height()));
                    // keep the current crop size if it still fits, else shrink
                    let cand = CropGeometry { rect: d.crop.geometry.rect, angle };
                    d.crop.geometry = if cand.is_within_image(w, h) { cand } else { fit };
                    Ok(())
                })
            }
        ),
        cmd!(
            "crop.aspect",
            "Crop Aspect",
            [],
            None,
            "{aspect: \"original\"|\"free\"|\"1x1\"|\"4x5\"|\"8.5x11\"|\"5x7\"|\"2x3\"|\"4x3\"|\"16x9\"|\"16x10\"|[w,h]}",
            has_active,
            |s, p| {
                let id = active(s, "crop.aspect")?;
                let (w, h) = crop_dims(s, id);
                let aspect = match p.get("aspect") {
                    Some(Value::String(a)) if a == "free" => None,
                    Some(Value::String(a)) if a == "original" => Some(((w * 100.0) as u32, (h * 100.0) as u32)),
                    Some(Value::String(a)) => {
                        let (x, y) = a.split_once('x').ok_or_else(|| bad("crop.aspect", "use WxH"))?;
                        let (x, y): (f64, f64) =
                            (x.parse().map_err(|_| bad("crop.aspect", "bad number"))?, y.parse().map_err(|_| bad("crop.aspect", "bad number"))?);
                        Some(((x * 100.0) as u32, (y * 100.0) as u32))
                    }
                    Some(Value::Array(a)) if a.len() == 2 => {
                        Some(((a[0].as_f64().unwrap_or(1.0) * 100.0) as u32, (a[1].as_f64().unwrap_or(1.0) * 100.0) as u32))
                    }
                    _ => return Err(bad("crop.aspect", "missing aspect")),
                };
                edit(s, "crop.aspect", "Crop Aspect", |d| {
                    if aspect.is_none() {
                        d.crop.aspect = None;
                    } else {
                        set_aspect(d, w, h, aspect);
                    }
                    Ok(())
                })
            }
        ),
        cmd!("crop.rotateAspect", "Rotate Crop Aspect", [], Some("X"), "{}", has_active, |s, _| {
            let id = active(s, "crop.rotateAspect")?;
            let (w, h) = crop_dims(s, id);
            edit(s, "crop.rotateAspect", "Rotate Crop", |d| {
                let r = d.crop.geometry.rect_px(w, h);
                let a = r.height() / r.width();
                d.crop.aspect = d.crop.aspect.map(|(x, y)| (y, x));
                let angle = d.crop.geometry.angle;
                d.crop.geometry = crop_fit_angle(w, h, angle, Some(a));
                Ok(())
            })
        }),
        cmd!("crop.reset", "Reset Crop", [], Some("Cmd+Alt+R"), "{}", has_active, |s, _| {
            edit(s, "crop.reset", "Reset Crop", |d| {
                d.crop = Default::default();
                Ok(())
            })
        }),
        // ---- copy / paste / sync
        cmd!(
            "develop.copy",
            "Copy Edit Settings",
            ["Edit"],
            Some("Cmd+C"),
            "{groups?: [settingsGroup]} (default: all but crop, masks, remove)",
            has_active,
            |s, p| {
                let id = active(s, "develop.copy")?;
                if let Some(g) = groups_param(p) {
                    s.copy_groups = g;
                }
                let d = s.develop_of(id).unwrap_or_default();
                s.clipboard = Some(lightcraft_develop::extract_groups(&d, &s.copy_groups));
                Ok(json!({"groups": s.copy_groups}))
            }
        ),
        cmd!("develop.paste", "Paste Edit Settings", ["Edit"], Some("Cmd+V"), "{ids?}", has_clipboard, |s, p| {
            let clip = s.clipboard.clone().unwrap_or_default();
            let ids = s.targets(p);
            let ops = ids
                .iter()
                .filter_map(|id| s.develop_of(*id).map(|d| (*id, d)))
                .filter_map(|(id, d)| s.develop_op(id, lightcraft_develop::apply_partial(&d, &clip, 1.0), "Paste Settings"))
                .collect::<Vec<_>>();
            let n = ops.len();
            s.commit("Paste Settings", Op::Batch { ops })?;
            Ok(json!({"changed": n}))
        }),
        cmd!(
            "develop.sync",
            "Sync Settings",
            [],
            None,
            "{groups?} — copy the active photo's settings to all selected photos",
            has_selection,
            |s, p| {
                let id = active(s, "develop.sync")?;
                let groups = groups_param(p).unwrap_or_else(SettingsGroup::default_copy);
                let d = s.develop_of(id).unwrap_or_default();
                let partial = lightcraft_develop::extract_groups(&d, &groups);
                let ops = s
                    .selection
                    .ids
                    .clone()
                    .into_iter()
                    .filter(|x| *x != id)
                    .filter_map(|x| s.develop_of(x).map(|d| (x, d)))
                    .filter_map(|(x, dd)| s.develop_op(x, lightcraft_develop::apply_partial(&dd, &partial, 1.0), "Sync Settings"))
                    .collect::<Vec<_>>();
                let n = ops.len();
                s.commit("Sync Settings", Op::Batch { ops })?;
                Ok(json!({"changed": n}))
            }
        ),
        // ---- presets
        cmd!("preset.apply", "Apply Preset", [], None, "{id: presetId, amount?: 0..200 (percent), ids?}", has_selection, |s, p| {
            let pid = str_param(p, "id").ok_or_else(|| bad("preset.apply", "missing id"))?;
            let preset = s.presets.iter().find(|x| x.id == pid).cloned().ok_or_else(|| bad("preset.apply", format!("unknown preset `{pid}`")))?;
            let amount = f64_or(p, "amount", 100.0).clamp(0.0, 200.0) / 100.0;
            let ids = s.targets(p);
            let ops = ids
                .iter()
                .filter_map(|id| s.develop_of(*id).map(|d| (*id, d)))
                .filter_map(|(id, d)| s.develop_op(id, preset.apply(&d, amount), &format!("Preset: {}", preset.name)))
                .collect::<Vec<_>>();
            s.commit(&format!("Preset: {}", preset.name), Op::Batch { ops })?;
            ok()
        }),
        cmd!("preset.create", "Create Preset", ["Photo"], None, "{name, group?, groups?: [settingsGroup]}", has_active, |s, p| {
            let id = active(s, "preset.create")?;
            let name = str_param(p, "name").ok_or_else(|| bad("preset.create", "missing name"))?;
            let group = str_param(p, "group").unwrap_or("User Presets");
            let groups = groups_param(p).unwrap_or_else(SettingsGroup::default_copy);
            let d = s.develop_of(id).unwrap_or_default();
            let pid = format!("user.{}.{}", s.presets.len(), name.to_lowercase().replace(' ', "-"));
            s.presets.push(Preset::from_settings(&pid, name, group, &d, &groups));
            Ok(json!({"id": pid}))
        }),
        cmd!("preset.delete", "Delete Preset", [], None, "{id}", always, |s, p| {
            let pid = str_param(p, "id").ok_or_else(|| bad("preset.delete", "missing id"))?;
            let i = s.presets.iter().position(|x| x.id == pid).ok_or_else(|| bad("preset.delete", "unknown preset"))?;
            if s.presets[i].builtin {
                return Err(bad("preset.delete", "built-in presets can't be deleted"));
            }
            s.presets.remove(i);
            ok()
        }),
        cmd!("preset.favorite", "Favorite Preset", [], None, "{id, favorite?: bool}", always, |s, p| {
            let pid = str_param(p, "id").ok_or_else(|| bad("preset.favorite", "missing id"))?;
            let pr = s.presets.iter_mut().find(|x| x.id == pid).ok_or_else(|| bad("preset.favorite", "unknown preset"))?;
            pr.favorite = bool_or(p, "favorite", !pr.favorite);
            ok()
        }),
        // ---- versions & history
        cmd!("version.create", "Create Version", ["Photo"], Some("Cmd+Shift+S"), "{name?}", has_active, |s, p| {
            let id = active(s, "version.create")?;
            let ph = s.catalog.photo(id).cloned().ok_or_else(|| bad("version.create", "no photo"))?;
            let mut versions = ph.versions.clone();
            let name = str_param(p, "name").map(str::to_string).unwrap_or_else(|| format!("Version {}", versions.len() + 1));
            versions.push(Version { name, created: (s.clock)(), settings: ph.develop.clone(), auto: false });
            s.commit("Create Version", Op::SetVersions { id, versions })?;
            ok()
        }),
        cmd!("version.restore", "Restore Version", [], None, "{index}", has_active, |s, p| {
            let id = active(s, "version.restore")?;
            let i = f64_req(p, "index", "version.restore")? as usize;
            let v = s.catalog.photo(id).and_then(|ph| ph.versions.get(i).cloned()).ok_or_else(|| bad("version.restore", "no such version"))?;
            s.set_develop(id, (*v.settings).clone(), &format!("Restore {}", v.name))?;
            ok()
        }),
        cmd!("version.delete", "Delete Version", [], None, "{index}", has_active, |s, p| {
            let id = active(s, "version.delete")?;
            let i = f64_req(p, "index", "version.delete")? as usize;
            let mut versions = s.catalog.photo(id).map(|ph| ph.versions.clone()).unwrap_or_default();
            if i >= versions.len() {
                return Err(bad("version.delete", "no such version"));
            }
            versions.remove(i);
            s.commit("Delete Version", Op::SetVersions { id, versions })?;
            ok()
        }),
        cmd!("history.restore", "Go to History Step", [], None, "{index}", has_active, |s, p| {
            let id = active(s, "history.restore")?;
            let i = f64_req(p, "index", "history.restore")? as usize;
            let st = s.catalog.photo(id).and_then(|ph| ph.history.get(i).cloned()).ok_or_else(|| bad("history.restore", "no such step"))?;
            s.set_develop(id, (*st.settings).clone(), &format!("History: {}", st.label))?;
            ok()
        }),
        // ---- interactions (slider drags, brush strokes)
        cmd!("develop.beginInteraction", "Begin Interaction", [], None, "{label}", has_active, |s, p| {
            s.begin_interaction(str_param(p, "label").unwrap_or("Edit"))?;
            ok()
        }),
        cmd!("develop.endInteraction", "End Interaction", [], None, "{}", always, |s, _| {
            s.end_interaction()?;
            ok()
        }),
        cmd!("develop.cancelInteraction", "Cancel Interaction", [], None, "{}", always, |s, _| {
            s.cancel_interaction()?;
            ok()
        }),
    ]
}
