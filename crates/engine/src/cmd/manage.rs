//! Photo management commands: batch rename, capture time, colour label names.

use serde_json::{Value, json};

use super::{CommandSpec, bad, cmd, has_selection, str_param};
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
    ]
}
