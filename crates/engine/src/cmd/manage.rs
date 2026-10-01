//! Photo management commands: batch rename, capture time, colour label names.

use serde_json::{Value, json};

use lightcraft_catalog::{ColorLabel, Op};

use super::{CommandSpec, always, bad, bool_or, cmd, f64_or, has_selection, str_param};
use crate::Result;

fn rename_args(s: &crate::Session, p: &Value, c: &str) -> Result<(Vec<lightcraft_catalog::PhotoId>, String, usize)> {
    let template = str_param(p, "template").ok_or_else(|| bad(c, "missing `template`"))?.to_string();
    let start = p.get("start").and_then(Value::as_u64).unwrap_or(1) as usize;
    Ok((s.targets(p), template, start))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(query "photo.renamePreview", "Rename Preview", [], None, "{template: e.g. `{date}_{name}`, `Trip-{seq:3}` (tokens: {name} {seq} {seq:N} {date} {date:%Y%m%d} {camera} {title} {ext}), start?: first sequence number (1), ids?} — renames the files on disk (sidecars too, never overwriting: collisions get -1, -2…); undoable", has_selection, |s, p| {
            let (ids, template, start) = rename_args(s, p, "photo.renamePreview")?;
            Ok(serde_json::to_value(s.plan_rename(&ids, &template, start)).unwrap_or_default())
        }),
        cmd!(
            "photo.rename",
            "Rename Photos",
            [],
            None,
            "{template: e.g. `{date}_{name}`, `Trip-{seq:3}` (tokens: {name} {seq} {seq:N} {date} {date:%Y%m%d} {camera} {title} {ext}), start?: first sequence number (1), ids?} — renames the files on disk (sidecars too, never overwriting: collisions get -1, -2…); undoable",
            has_selection,
            |s, p| {
                let (ids, template, start) = rename_args(s, p, "photo.rename")?;
                let plans = s.plan_rename(&ids, &template, start);
                let n = s.apply_rename(&plans)?;
                Ok(json!({"renamed": n, "plans": plans}))
            }
        ),
        cmd!(
            "photo.setCaptureTime",
            "Edit Capture Time",
            [],
            None,
            "{ids?, time?: `2026-09-30T14:05:00` (the active photo gets it, the others shift by the same amount), each?: bool (every photo gets `time`), shift?: seconds, hours?: time-zone shift in hours} → {changed, captured: [..]}",
            has_selection,
            |s, p| {
                use lightcraft_catalog::dates::{iso_seconds, normalize_iso, shift_iso};
                let c = "photo.setCaptureTime";
                let targets: Vec<_> = s.targets(p).into_iter().filter(|id| s.catalog.photo(*id).is_some()).collect();
                if targets.is_empty() {
                    return Err(bad(c, "no photos"));
                }
                // photos without a capture time start from their import time
                let base = |s: &crate::Session, id| s.catalog.photo(id).map(|p| p.date().to_string()).unwrap_or_default();
                let mut delta = (f64_or(p, "shift", 0.0) + f64_or(p, "hours", 0.0) * 3600.0).round() as i64;
                let mut each: Option<String> = None;
                if let Some(t) = str_param(p, "time") {
                    let t = normalize_iso(t).ok_or_else(|| bad(c, format!("`{t}` is not a date (YYYY-MM-DDTHH:MM:SS)")))?;
                    if bool_or(p, "each", false) {
                        each = Some(t);
                    } else {
                        let anchor = s.active().filter(|a| targets.contains(a)).unwrap_or(targets[0]);
                        let from = iso_seconds(&base(s, anchor)).ok_or_else(|| bad(c, "the photo's date doesn't parse"))?;
                        delta += iso_seconds(&t).unwrap_or(from) - from;
                    }
                }
                let mut ops = Vec::new();
                let mut out = Vec::new();
                for id in &targets {
                    let new = match &each {
                        Some(t) => shift_iso(t, delta),
                        None => shift_iso(&base(s, *id), delta),
                    };
                    let Some(new) = new else { continue };
                    out.push(json!(new));
                    if s.catalog.photo(*id).and_then(|p| p.captured.as_deref()) != Some(new.as_str()) {
                        ops.push(Op::SetCaptured { id: *id, captured: Some(new) });
                    }
                }
                let n = ops.len();
                if n > 0 {
                    s.commit("Edit Capture Time", Op::Batch { ops })?;
                }
                Ok(json!({"changed": n, "captured": out}))
            }
        ),
        cmd!(query "label.names", "Color Label Names", [], None, "{} → [{label, name, custom}]", always, |s, _| {
            Ok(json!(ColorLabel::ALL
                .iter()
                .map(|l| json!({"label": format!("{l:?}").to_lowercase(), "name": s.catalog.label_name(*l), "custom": s.catalog.custom_label_name(*l)}))
                .collect::<Vec<_>>()))
        }),
        cmd!(
            "label.setNames",
            "Edit Color Label Names",
            [],
            None,
            "{names: {red?: name|null, yellow?, green?, blue?, purple?}} — null or empty restores the colour's name",
            always,
            |s, p| {
                let names = p.get("names").and_then(Value::as_object).ok_or_else(|| bad("label.setNames", "missing `names`"))?;
                let mut ops = Vec::new();
                for (k, v) in names {
                    let label = ColorLabel::parse(k).ok_or_else(|| bad("label.setNames", format!("unknown label `{k}`")))?;
                    let name =
                        v.as_str().map(str::trim).filter(|n| !n.is_empty() && !n.eq_ignore_ascii_case(&format!("{label:?}"))).map(str::to_string);
                    if s.catalog.custom_label_name(label) != name.as_deref() {
                        ops.push(Op::SetLabelName { label, name });
                    }
                }
                let n = ops.len();
                if n > 0 {
                    s.commit("Edit Label Names", Op::Batch { ops })?;
                }
                Ok(json!({"changed": n}))
            }
        ),
    ]
}
