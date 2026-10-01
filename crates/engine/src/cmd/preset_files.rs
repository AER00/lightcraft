//! Preset files: import `.lcpreset` / XMP presets, export `.lcpreset`.

use serde_json::{Value, json};

use super::{CommandSpec, always, bad, cmd, str_param};
use crate::presets::{LCPRESET_EXT, expand_preset_paths, parse_preset_file, to_lcpreset};
use crate::{Result, Session};

fn strings(p: &Value, key: &str) -> Option<Vec<String>> {
    p.get(key).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
}

fn import(s: &mut Session, p: &Value) -> Result<Value> {
    let paths = strings(p, "paths").filter(|v| !v.is_empty()).ok_or_else(|| bad("preset.import", "no paths"))?;
    let mut imported = Vec::new();
    let mut failed = Vec::new();
    let mut skipped = 0usize;
    for f in expand_preset_paths(&paths) {
        let parsed = std::fs::read(&f).map_err(|e| e.to_string()).and_then(|b| parse_preset_file(&f, &b));
        match parsed {
            Ok(list) => {
                let n = list.len();
                let ids = s.add_presets(list);
                skipped += n - ids.len();
                for id in ids {
                    if let Some(pr) = s.presets.iter().find(|x| x.id == id) {
                        imported.push(json!({"id": pr.id, "name": pr.name, "group": pr.group}));
                    }
                }
            }
            Err(e) => failed.push(json!([f, e])),
        }
    }
    Ok(json!({"imported": imported, "skipped": skipped, "failed": failed}))
}

fn export(s: &mut Session, p: &Value) -> Result<Value> {
    let path = str_param(p, "path").filter(|x| !x.trim().is_empty()).ok_or_else(|| bad("preset.export", "missing path"))?;
    let mut path = std::path::PathBuf::from(path);
    if path.extension().is_none() {
        path.set_extension(LCPRESET_EXT);
    }
    let ids = strings(p, "ids");
    let group = str_param(p, "group");
    let chosen: Vec<_> = s
        .presets
        .iter()
        .filter(|x| match (&ids, group) {
            (Some(ids), _) => ids.contains(&x.id),
            (None, Some(g)) => x.group == g,
            (None, None) => !x.builtin,
        })
        .cloned()
        .collect();
    if chosen.is_empty() {
        return Err(bad("preset.export", "no presets to export (create one first, or pass ids/group)"));
    }
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| crate::EngineError::Other(format!("{}: {e}", dir.display())))?;
    }
    std::fs::write(&path, to_lcpreset(&chosen)).map_err(|e| crate::EngineError::Other(format!("{}: {e}", path.display())))?;
    Ok(json!({"path": path.display().to_string(), "count": chosen.len()}))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "preset.import",
            "Import Presets",
            [],
            None,
            "{paths: [.lcpreset / .xmp file or folder]} — groups preserved; XMP presets map crs: fields (docs/xmp-interop.md) → {imported: [{id,name,group}], skipped, failed}",
            always,
            import
        ),
        cmd!(
            "preset.export",
            "Export Presets",
            [],
            None,
            "{path, ids?: [presetId], group?: name} (default: all user presets) → {path, count}; writes a .lcpreset JSON file",
            always,
            export
        ),
    ]
}
