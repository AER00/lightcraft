//! Import: files and folders (recursive) → photos.
//!
//! 1. Expand folders recursively into supported files (hidden entries and the library's own
//!    folder are skipped), sorted for a stable order.
//! 2. Skip paths already in the catalog.
//! 3. Probe the rest — dimensions, metadata, and the **content hash** of the bytes — in parallel
//!    on native targets.
//! 4. Skip **duplicates by content** (same bytes already in the library, or twice in this batch).
//! 5. *Add* in place (the photo points at the original file) or *copy* into the library's
//!    `Originals/YYYY/YYYY-MM-DD/` folder (names made unique) and point at the copy.
//! 6. Commit all new photos as one undoable op.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use lightcraft_catalog::{Op, Photo, PhotoId, Source};
use serde::Serialize;

use crate::Session;
use crate::media::ProbeInfo;

/// File extensions LightCraft imports (lower case).
pub const EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "tif", "tiff", "webp", "dng", "cr2", "cr3", "nef", "arw", "raf", "orf", "rw2", "pef", "psd", "jxl", "gif", "bmp", "heic",
    "avif",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImportMode {
    /// Reference the files where they are.
    #[default]
    Add,
    /// Copy the files into the library's `Originals/` folder.
    Copy,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Duplicate {
    pub path: String,
    /// The photo that already has these bytes (or this path).
    pub existing: Option<u64>,
    /// `"path"` (already imported from there) or `"content"` (same bytes elsewhere).
    pub reason: &'static str,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ImportReport {
    pub imported: Vec<u64>,
    pub duplicates: Vec<Duplicate>,
    /// (path, error)
    pub failed: Vec<(String, String)>,
    /// Files found after expanding folders.
    pub scanned: usize,
}

pub fn is_supported(path: &Path) -> bool {
    path.extension().is_some_and(|e| EXTENSIONS.contains(&e.to_string_lossy().to_lowercase().as_str()))
}

/// Expand files and folders (recursively) into supported files. `skip` (e.g. the library folder)
/// is never descended into.
pub fn expand(paths: &[String], skip: Option<&Path>) -> Vec<String> {
    fn walk(p: &Path, skip: Option<&Path>, out: &mut Vec<String>, top: bool) {
        let hidden = p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.'));
        if (hidden && !top) || skip.is_some_and(|s| p == s) {
            return;
        }
        if p.is_dir() {
            let Ok(rd) = std::fs::read_dir(p) else { return };
            let mut v: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
            v.sort();
            for c in v {
                walk(&c, skip, out, false);
            }
        } else if top || is_supported(p) {
            // explicitly named files are attempted even with an unknown extension (sniffed)
            out.push(p.to_string_lossy().to_string());
        }
    }
    let mut out = Vec::new();
    for p in paths {
        walk(Path::new(p), skip, &mut out, true);
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|p| seen.insert(p.clone()));
    out
}

fn probe_all(s: &Session, paths: &[String]) -> Vec<Result<ProbeInfo, String>> {
    let Some(probe) = s.media.file_probe.clone() else {
        return paths
            .iter()
            .map(|p| {
                Ok(ProbeInfo {
                    format: Path::new(p).extension().map(|e| e.to_string_lossy().to_uppercase()).unwrap_or_default(),
                    ..Default::default()
                })
            })
            .collect();
    };
    #[cfg(not(target_arch = "wasm32"))]
    {
        let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 8).min(paths.len().max(1));
        if n > 1 {
            let mut results: Vec<Option<Result<ProbeInfo, String>>> = vec![None; paths.len()];
            let next = std::sync::atomic::AtomicUsize::new(0);
            let out = std::sync::Mutex::new(&mut results);
            std::thread::scope(|sc| {
                for _ in 0..n {
                    sc.spawn(|| {
                        loop {
                            let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            if i >= paths.len() {
                                break;
                            }
                            let r = probe(&paths[i]);
                            out.lock().unwrap_or_else(|e| e.into_inner())[i] = Some(r);
                        }
                    });
                }
            });
            return results.into_iter().map(|r| r.unwrap_or_else(|| Err("not probed".into()))).collect();
        }
    }
    paths.iter().map(|p| probe(p)).collect()
}

/// `Originals/YYYY/YYYY-MM-DD/name`, made unique.
fn copy_into_library(lib: &Path, src: &str, date: &str) -> Result<String, String> {
    let day = date.get(..10).filter(|d| d.len() == 10).unwrap_or("undated");
    let year = day.get(..4).unwrap_or("undated");
    let dir = lib.join("Originals").join(year).join(day);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let name = Path::new(src).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "photo".into());
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) => (s.to_string(), format!(".{e}")),
        None => (name.clone(), String::new()),
    };
    let mut dst = dir.join(&name);
    let mut i = 1;
    while dst.exists() {
        dst = dir.join(format!("{stem}-{i}{ext}"));
        i += 1;
    }
    std::fs::copy(src, &dst).map_err(|e| format!("copy {src}: {e}"))?;
    Ok(dst.to_string_lossy().to_string())
}

/// Import files/folders. See the module docs.
pub fn import(s: &mut Session, paths: &[String], mode: ImportMode) -> crate::Result<ImportReport> {
    let lib_dir = s.library.as_ref().map(|l| l.dir.clone());
    if mode == ImportMode::Copy && lib_dir.is_none() {
        return Err(crate::EngineError::Other("copying into the library needs an open library".into()));
    }
    let files = expand(paths, lib_dir.as_deref());
    let mut report = ImportReport { scanned: files.len(), ..Default::default() };

    // existing paths and content hashes
    let mut by_path: HashMap<&str, PhotoId> = HashMap::new();
    let mut by_hash: HashMap<String, PhotoId> = HashMap::new();
    for p in s.catalog.photos() {
        if let Source::File { path } = &p.source {
            by_path.insert(path.as_str(), p.id);
        }
        if let Some(h) = &p.content_hash {
            by_hash.insert(h.clone(), p.id);
        }
    }
    let mut todo = Vec::new();
    for f in files {
        match by_path.get(f.as_str()) {
            Some(id) => report.duplicates.push(Duplicate { path: f, existing: Some(id.0), reason: "path" }),
            None => todo.push(f),
        }
    }

    let probed = probe_all(s, &todo);
    let now = (s.clock)();
    let mut ops = Vec::new();
    for (path, info) in todo.into_iter().zip(probed) {
        let info = match info {
            Ok(i) => i,
            Err(e) => {
                log::warn!("import {path}: {e}");
                report.failed.push((path, e));
                continue;
            }
        };
        if let Some(h) = &info.content_hash
            && let Some(id) = by_hash.get(h)
        {
            report.duplicates.push(Duplicate { path, existing: Some(id.0), reason: "content" });
            continue;
        }
        let stored = match (mode, &lib_dir) {
            (ImportMode::Copy, Some(lib)) if !Path::new(&path).starts_with(lib) => {
                match copy_into_library(lib, &path, info.captured.as_deref().unwrap_or(&now)) {
                    Ok(p) => p,
                    Err(e) => {
                        report.failed.push((path, e));
                        continue;
                    }
                }
            }
            _ => path.clone(),
        };
        let id = s.catalog.alloc_photo_id();
        if let Some(h) = &info.content_hash {
            by_hash.insert(h.clone(), id);
        }
        let name = Path::new(&path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.clone());
        let mut p = Photo::new(id, Source::File { path: stored }, &name, &info.format, info.width, info.height, &now);
        p.kind = info.kind;
        p.file_size = info.file_size;
        p.captured = info.captured;
        p.meta = info.meta;
        p.as_shot_wb = info.as_shot_wb;
        p.content_hash = info.content_hash;
        if let Some((t, tint)) = info.as_shot_wb {
            p.develop = std::sync::Arc::new(lightcraft_develop::DevelopSettings::for_raw(t, tint));
        }
        report.imported.push(id.0);
        ops.push(Op::AddPhoto { photo: Box::new(p) });
    }
    if !ops.is_empty() {
        s.commit(&format!("Add {} Photo{}", ops.len(), if ops.len() == 1 { "" } else { "s" }), Op::Batch { ops })?;
    }
    Ok(report)
}

/// The current local time as ISO 8601 (`YYYY-MM-DDTHH:MM:SS`, UTC on targets without a clock
/// offset); for [`Session::clock`] on native hosts.
pub fn system_clock() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
        civil(secs)
    }
    #[cfg(target_arch = "wasm32")]
    {
        "2026-01-01T00:00:00".to_string()
    }
}

/// Unix seconds → `YYYY-MM-DDTHH:MM:SS` (UTC), proleptic Gregorian (H. Hinnant's algorithm).
pub fn civil(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

impl Session {
    /// Use the system clock for import/edit times (native hosts).
    pub fn with_system_clock(mut self) -> Self {
        self.clock = Box::new(system_clock);
        self
    }
}
