//! Synchronize Folder commands (see [`crate::sync`]).

use serde_json::{Value, json};

use super::{CommandSpec, always, bad, bool_or, cmd, str_param};
use crate::sync::{SyncChoice, scan, synchronize};
use crate::{Result, Session};

fn path(p: &Value, c: &str) -> Result<String> {
    str_param(p, "path").filter(|d| !d.trim().is_empty()).map(str::to_string).ok_or_else(|| bad(c, "missing `path`"))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            query "folder.scanChanges",
            "Scan Folder for Changes",
            [],
            None,
            "{path} — compare a folder of the library (and the folders inside it) with the disk; changes nothing → {path, new: [{path, name, format, …}] files the library doesn't have, duplicates: files whose content it already has, unreadable: [{path, error}], missing: [{id, path}] photos whose file is gone, metadata: [{id, path}] photos whose XMP sidecar was saved after they came in or were last edited here and says something the library doesn't}",
            always,
            |s, p| {
                const C: &str = "folder.scanChanges";
                let path = path(p, C)?;
                let changes = scan(s, &path).map_err(|e| bad(C, e.to_string()))?;
                Ok(serde_json::to_value(changes).unwrap_or_default())
            }
        ),
        cmd!(
            "folder.synchronize",
            "Synchronize Folder",
            [],
            None,
            "{path, importNew?: true, removeMissing?: false, readMetadata?: false} — bring a folder of the library up to date with the disk (see folder.scanChanges): import its new files in place, move photos whose file is gone to Recently Deleted, read XMP sidecars saved by other apps (the sidecar wins); one undo step, no file is touched → {imported, removed, read, failed: [[path, error]]}",
            always,
            sync
        ),
    ]
}

fn sync(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "folder.synchronize";
    let path = path(p, C)?;
    let d = SyncChoice::default();
    let choice = SyncChoice {
        import_new: bool_or(p, "importNew", d.import_new),
        remove_missing: bool_or(p, "removeMissing", d.remove_missing),
        read_metadata: bool_or(p, "readMetadata", d.read_metadata),
    };
    let changes = scan(s, &path).map_err(|e| bad(C, e.to_string()))?;
    let r = synchronize(s, changes, choice)?;
    Ok(json!({"imported": r.imported, "removed": r.removed, "read": r.read, "failed": r.failed}))
}
