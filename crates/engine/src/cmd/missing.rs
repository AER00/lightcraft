//! Missing originals: photos whose file is no longer where the library expects it (moved,
//! renamed, on an unplugged drive), and relinking them — one at a time (`photo.relink`) or by
//! searching a folder for files with the same name and size, or the same content
//! (`library.findMissing`).

use std::collections::{HashMap, HashSet};
use std::path::Path;

use lightcraft_catalog::{Catalog, Op, Photo, PhotoId, Source};
use serde_json::{Value, json};

use super::{CommandSpec, always, bad, cmd, str_param};
use crate::{Result, Session};

/// The file Missing Photos checks for `p`, if it is in scope: a library photo (not in Recently
/// Deleted, not a Local-only record seen while browsing a folder) whose original is a file.
///
/// Local browse records are out of scope on purpose: browsing can leave tens of thousands of
/// them in the catalog, they never show in library views (Missing Photos included), and a
/// folder that is gone is simply not browsed again. The sidebar count, the Missing Photos view,
/// `library.missing` and Find Missing Photos all use this one rule.
pub fn checked_path(p: &Photo) -> Option<&str> {
    match &p.source {
        Source::File { path } if p.in_library() => Some(path),
        _ => None,
    }
}

/// Every in-scope photo's file ([`checked_path`]), without touching the disk.
pub fn candidates(cat: &Catalog) -> Vec<String> {
    cat.photos().filter_map(|p| checked_path(p).map(str::to_string)).collect()
}

/// [`missing`] with the existence check supplied (tests count the file-system calls).
pub fn missing_with(cat: &Catalog, mut exists: impl FnMut(&str) -> bool) -> Vec<(PhotoId, String)> {
    cat.photos().filter_map(|p| checked_path(p).filter(|f| !exists(f)).map(|f| (p.id, f.to_string()))).collect()
}

/// Library photos whose original file can't be found: (id, path).
pub fn missing(s: &Session) -> Vec<(PhotoId, String)> {
    if cfg!(target_arch = "wasm32") {
        return Vec::new();
    }
    missing_with(&s.catalog, |f| Path::new(f).exists())
}

/// Whether photo `id` is in scope and its file is gone (the Missing Photos view checks only
/// the photos its query already narrowed to).
pub fn is_missing(cat: &Catalog, id: PhotoId) -> bool {
    !cfg!(target_arch = "wasm32") && cat.photo(id).and_then(|p| checked_path(p)).is_some_and(|f| !Path::new(f).exists())
}

fn relink_op(id: PhotoId, path: &str) -> Op {
    let file_name = Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string());
    Op::Relink { id, file_name, source: Source::File { path: path.to_string() }, format: None }
}

fn relink(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "photo.relink";
    let id = p.get("id").and_then(Value::as_u64).map(PhotoId).or(s.active()).ok_or_else(|| bad(C, "no photo (give `id`)"))?;
    let path = str_param(p, "path").ok_or_else(|| bad(C, "missing `path`"))?;
    let abs = std::path::absolute(path).map_err(|e| bad(C, e.to_string()))?;
    if !abs.is_file() {
        return Err(bad(C, format!("{path}: no such file")));
    }
    if !matches!(s.catalog.photo(id).map(|p| &p.source), Some(Source::File { .. })) {
        return Err(bad(C, "only photos from files can be relinked"));
    }
    let abs = abs.to_string_lossy().to_string();
    // the file may differ from the one imported (another export, a raw that decodes now or not):
    // its size, dimensions and preview-only state follow it
    let mut ops = vec![relink_op(id, &abs)];
    if s.media.file_probe.is_some()
        && let (Some(Ok(info)), Some(ph)) = (crate::import::probe_paths(s, std::slice::from_ref(&abs)).pop(), s.catalog.photo(id))
    {
        ops.extend(crate::cmd::convert::content_op(id, ph, info));
    }
    let op = match <[Op; 1]>::try_from(ops) {
        Ok([op]) => op,
        Err(ops) => Op::Batch { ops },
    };
    s.commit("Relink Photo", op)?;
    s.media.forget(id);
    Ok(json!({"id": id.0, "path": abs}))
}

/// The stored content hash of a photo, without the `:dupN` suffix duplicates get.
fn stored_hash(p: &Photo) -> Option<&str> {
    p.content_hash.as_deref().map(|h| h.split(':').next().unwrap_or(h)).filter(|h| !h.is_empty())
}

/// The content hash of the file at `path` (as import computes it), if it can be read.
fn file_hash(path: &str) -> Option<String> {
    std::fs::read(path).ok().map(|b| lightcraft_preview::hash_bytes(&b).to_string())
}

/// Search `folder` (recursively) for each missing photo's file and relink every match in one
/// undoable step.
///
/// Matching, per photo:
/// 1. a file with the same name (any letter case), the same size and the same content hash;
/// 2. renamed: a file of the same size with the same content hash (only same-size files are
///    read, so this stays cheap);
/// 3. without a hash to confirm (or the file was edited in place): the one file with the same
///    name and size. Several such files are ambiguous: the photo is skipped and reported in
///    `ambiguous`, never guessed (two cameras' `IMG_0001.CR3` of one size are different photos).
///
/// Files that are already some photo's original, or that another photo was just matched with,
/// are not used.
fn find_missing(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "library.findMissing";
    let folder = str_param(p, "folder").ok_or_else(|| bad(C, "missing `folder`"))?;
    if !Path::new(folder).is_dir() {
        return Err(bad(C, format!("{folder}: not a folder")));
    }
    let lost = missing(s);
    if lost.is_empty() {
        return Ok(json!({"found": [], "missing": 0, "ambiguous": []}));
    }
    let in_use: HashSet<String> = s.catalog.photos().filter_map(|p| checked_path(p).map(str::to_string)).collect();
    // file name (lower case) → candidate paths; size → paths (filled lazily)
    let mut by_name: HashMap<String, Vec<String>> = HashMap::new();
    let files: Vec<String> = crate::import::expand(&[folder.to_string()], None).into_iter().filter(|f| !in_use.contains(f)).collect();
    for f in &files {
        if let Some(n) = Path::new(f).file_name() {
            by_name.entry(n.to_string_lossy().to_lowercase()).or_default().push(f.clone());
        }
    }
    let mut by_size: Option<HashMap<u64, Vec<String>>> = None;
    let mut hashes: HashMap<String, Option<String>> = HashMap::new();
    let mut hash_of = |f: &str| hashes.entry(f.to_string()).or_insert_with(|| file_hash(f)).clone();
    let size_of = |f: &str| std::fs::metadata(f).map(|m| m.len()).ok();
    let mut used: HashSet<String> = HashSet::new();
    let mut ops = Vec::new();
    let mut found = Vec::new();
    let mut ambiguous = Vec::new();
    for (id, old) in &lost {
        let Some(ph) = s.catalog.photo(*id) else { continue };
        let name = Path::new(old).file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        let size = ph.file_size;
        let hash = stored_hash(ph).map(str::to_string);
        let named: Vec<&String> = by_name
            .get(&name)
            .map(|c| c.iter().filter(|f| !used.contains(*f) && (size == 0 || size_of(f) == Some(size))).collect())
            .unwrap_or_default();
        // 1. same name and size, confirmed by the content hash
        let mut hit: Option<(String, &str)> =
            hash.as_deref().and_then(|h| named.iter().find(|f| hash_of(f).as_deref() == Some(h))).map(|f| (f.to_string(), "name"));
        // 2. renamed: same size and content
        if hit.is_none()
            && let Some(h) = &hash
            && size > 0
        {
            let sizes = by_size.get_or_insert_with(|| {
                let mut m: HashMap<u64, Vec<String>> = HashMap::new();
                for f in &files {
                    if let Some(n) = size_of(f) {
                        m.entry(n).or_default().push(f.clone());
                    }
                }
                m
            });
            hit = sizes
                .get(&size)
                .and_then(|c| c.iter().find(|f| !used.contains(*f) && hash_of(f).as_deref() == Some(h.as_str())))
                .map(|f| (f.clone(), "content"));
        }
        // 3. same name and size only (no hash, or the file was edited in place): one candidate
        //    is taken, several are ambiguous and skipped rather than guessed
        if hit.is_none() {
            match named.len() {
                1 => hit = Some((named[0].clone(), "name")),
                0 => {}
                _ => {
                    ambiguous.push(json!({"id": id.0, "from": old, "candidates": named}));
                    continue;
                }
            }
        }
        let Some((path, by)) = hit else { continue };
        ops.push(relink_op(*id, &path));
        found.push(json!({"id": id.0, "from": old, "to": path, "by": by}));
        used.insert(path);
    }
    let still = lost.len() - found.len();
    if !ops.is_empty() {
        s.commit("Find Missing Photos", Op::Batch { ops })?;
        for f in &found {
            if let Some(id) = f["id"].as_u64() {
                s.media.forget(PhotoId(id));
            }
        }
    }
    Ok(json!({"found": found, "missing": still, "ambiguous": ambiguous}))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(query "library.missing", "Missing Photos", [], None, "{} → [{id, path}] — photos whose original file isn't where the library expects it", always, |s, _| {
            Ok(Value::Array(missing(s).into_iter().map(|(id, path)| json!({"id": id.0, "path": path})).collect()))
        }),
        cmd!(
            "photo.relink",
            "Locate Photo",
            [],
            None,
            "{id?, path} — point a photo at its file's new location (undo never moves files)",
            always,
            relink
        ),
        cmd!(
            "library.findMissing",
            "Find Missing Photos",
            [],
            None,
            "{folder} — relink every missing photo whose file is somewhere in the folder: same name and size (and content, when the library knows its hash), or — renamed — same size and content → {found: [{id, from, to, by: name|content}], missing, ambiguous: [{id, from, candidates}] (several same-name candidates and no hash to tell them apart: skipped)}",
            always,
            find_missing
        ),
    ]
}
