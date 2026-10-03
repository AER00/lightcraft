//! Local: browse a folder on disk without adding it to the library (Lightroom's Local tab).
//! The folder's photos are probed in place as `local` photos — only folder views list them —
//! and edits go to the files' XMP sidecars like any photo added in place. "Add to My Photos"
//! makes them ordinary library photos.

use std::path::Path;

use lightcraft_catalog::{Op, PhotoId};
use serde_json::{Value, json};

use super::{CommandSpec, always, bad, bool_or, cmd, str_param};
use crate::import::{ImportMode, ImportOptions, import_with, is_supported};
use crate::{Browse, LibrarySource, Result, Session};

fn browse(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "library.browse";
    let path = str_param(p, "path").ok_or_else(|| bad(C, "missing `path`"))?;
    let dir = std::path::absolute(Path::new(path)).map_err(|e| bad(C, e.to_string()))?;
    if !dir.is_dir() {
        return Err(bad(C, format!("{path}: not a folder")));
    }
    let dir_s = dir.to_string_lossy().trim_end_matches(['/', '\\']).to_string();
    let subfolders = bool_or(p, "subfolders", s.browse.as_ref().is_some_and(|b| b.subfolders));
    let files: Vec<String> = if subfolders {
        crate::import::expand(std::slice::from_ref(&dir_s), None)
    } else {
        let mut v: Vec<String> = std::fs::read_dir(&dir)
            .map_err(|e| bad(C, format!("{path}: {e}")))?
            .flatten()
            .map(|e| e.path())
            .filter(|f| f.is_file() && is_supported(f) && !f.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')))
            .map(|f| f.to_string_lossy().to_string())
            .collect();
        v.sort();
        v
    };
    let report = import_with(s, &files, &ImportOptions { mode: ImportMode::Add, local: true, ..Default::default() })?;
    s.browse = Some(Browse { path: dir_s.clone(), subfolders });
    s.source = LibrarySource::Folder;
    let shown = s.visible().len();
    Ok(json!({"path": dir_s, "subfolders": subfolders, "photos": shown, "new": report.imported.len(), "failed": report.failed.len()}))
}

/// The browsed photos among `ids` (default: the selection) become library photos.
fn add_to_library(s: &mut Session, p: &Value) -> Result<Value> {
    let ids: Vec<PhotoId> = s.targets(p).into_iter().filter(|id| s.catalog.photo(*id).is_some_and(|ph| ph.local)).collect();
    if ids.is_empty() {
        return Ok(json!({"added": 0}));
    }
    let n = ids.len();
    let ops = ids.into_iter().map(|id| Op::SetLocal { id, local: false }).collect();
    s.commit("Add to My Photos", Op::Batch { ops })?;
    Ok(json!({"added": n}))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "folder.rename",
            "Rename Folder",
            [],
            None,
            "{path, name} — rename a folder on disk (its files and sidecars go along) and relink the photos in it; not an undo step → {path, relinked}",
            always,
            |s, p| {
                let from = str_param(p, "path").ok_or_else(|| bad("folder.rename", "missing path"))?.trim_end_matches(['/', '\\']).to_string();
                let name = str_param(p, "name")
                    .map(str::trim)
                    .filter(|n| !n.is_empty() && !n.contains(['/', '\\']) && *n != "." && *n != "..")
                    .ok_or_else(|| bad("folder.rename", "give a folder name (no slashes)"))?;
                let to = Path::new(&from).with_file_name(name).to_string_lossy().to_string();
                let n = move_folder(s, &from, &to).map_err(|e| bad("folder.rename", e))?;
                Ok(json!({"path": to, "relinked": n}))
            }
        ),
        cmd!(
            "folder.move",
            "Move Folder",
            [],
            None,
            "{path, into: destination folder} — move a folder on disk into another and relink the photos in it; not an undo step → {path, relinked}",
            always,
            |s, p| {
                let from = str_param(p, "path").ok_or_else(|| bad("folder.move", "missing path"))?.trim_end_matches(['/', '\\']).to_string();
                let into = str_param(p, "into").ok_or_else(|| bad("folder.move", "missing into"))?;
                let name = Path::new(&from).file_name().ok_or_else(|| bad("folder.move", "bad path"))?;
                let to = Path::new(into).join(name).to_string_lossy().to_string();
                let n = move_folder(s, &from, &to).map_err(|e| bad("folder.move", e))?;
                Ok(json!({"path": to, "relinked": n}))
            }
        ),
        cmd!(
            "library.browse",
            "Browse Folder",
            [],
            None,
            "{path, subfolders?: bool} — show a folder's photos without adding them to the library (they're read in place; edits go to XMP sidecars) → {path, photos, new}",
            always,
            browse
        ),
        cmd!("photo.addToLibrary", "Add to My Photos", ["Photo"], None, "{ids?} — browsed (Local) photos join the library", always, add_to_library),
    ]
}

/// Rename or move a folder on disk (everything in it goes along, sidecars included) and relink
/// the photos inside. Like a rename in the file manager it is not an undo step: undoing would
/// point the photos back at a folder that no longer exists.
pub(crate) fn move_folder(s: &mut crate::Session, from: &str, to: &str) -> std::result::Result<usize, String> {
    use std::path::Path;
    let (src, dst) = (Path::new(from), Path::new(to));
    if !src.is_dir() {
        return Err(format!("{from} is not a folder"));
    }
    if dst.exists() {
        return Err(format!("{to} already exists"));
    }
    if dst.starts_with(src) {
        return Err("a folder can't move into itself".into());
    }
    if s.library.as_ref().is_some_and(|l| l.dir.starts_with(src) || src.starts_with(&l.dir)) {
        return Err("the library's own folders can't be moved here".into());
    }
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::rename(src, dst).map_err(|e| format!("{from} → {to}: {e}"))?;
    let mut n = 0;
    let ops: Vec<lightcraft_catalog::Op> = s
        .catalog
        .photos()
        .filter_map(|p| match &p.source {
            lightcraft_catalog::Source::File { path } => Path::new(path).strip_prefix(src).ok().map(|rel| (p.id, p.file_name.clone(), dst.join(rel))),
            _ => None,
        })
        .map(|(id, file_name, np)| {
            n += 1;
            lightcraft_catalog::Op::Relink {
                id,
                file_name,
                source: lightcraft_catalog::Source::File { path: np.to_string_lossy().to_string() },
                format: None,
            }
        })
        .collect();
    for op in ops {
        if s.catalog.apply(op.clone()).is_ok() {
            s.pending_log.push(op);
        }
    }
    // a browsed folder at or below it follows
    if let Some(b) = &mut s.browse
        && let Ok(rel) = Path::new(&b.path).strip_prefix(src)
    {
        b.path = dst.join(rel).to_string_lossy().to_string();
        let f = b.path.clone();
        s.filter.folder = Some(f);
    }
    Ok(n)
}
