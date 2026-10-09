//! Synchronize Folder: bring a folder of the library up to date with what is on disk, as
//! Lightroom Classic's Synchronize Folder does.
//!
//! A [scan](scan_with) compares the folder (and the folders inside it) with the library and finds
//! three kinds of change, touching nothing:
//!
//! - **new** files: supported files the library doesn't have, by path or by content (the import
//!   scan's duplicate rules, [`crate::import::scan_with`]);
//! - **missing** photos: library photos in the folder whose file is gone (the Missing Photos rule,
//!   [`crate::cmd::missing::checked_path`]);
//! - **metadata** updates: photos whose XMP sidecar file was saved after the photo came into the
//!   library or was last edited here, and says something the library doesn't (rating, flag,
//!   label, metadata, capture time or develop settings). Reading one makes the sidecar win, as
//!   Read Metadata from File does. Embedded XMP in a raw is not a sidecar and is not looked at.
//!
//! [`synchronize`] then imports the new files (in place), moves the missing photos to Recently
//! Deleted and reads the metadata updates — each only when chosen — as one undo step.
//!
//! The scan runs without the session ([`SyncInput`] + [`scan_with`]), so the app can run it on a
//! worker thread: a folder on a sleeping network share must never stall a frame.
//!
//! Limits: a sidecar the library itself wrote (auto-write on) agrees with it and is never an
//! update; one written here before auto-write was turned off, followed by changes made only in
//! the library, is listed as an update even though the library is newer (there is no record of
//! when a photo's metadata last changed here). Reading updates is therefore never the default.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use lightcraft_catalog::{Catalog, Filter, Op, Photo, PhotoId, Sort, Source};
use serde::Serialize;

use crate::import::{ImportCandidate, ImportMode, ImportOptions, ScanInput, ScanProgress};
use crate::media::ProbeInfo;
use crate::sidecar::SidecarNaming;
use crate::{EngineError, Result, Session};

/// The largest XMP sidecar read when looking for metadata updates; a bigger one is skipped.
const MAX_SIDECAR_BYTES: u64 = 16 * 1024 * 1024;

/// What a scan of a folder needs, taken from the session so it can run on a worker thread.
pub struct SyncInput {
    folder: String,
    revision: u64,
    scan: ScanInput,
    /// The library's photos in the folder, each with how its sidecar is named.
    photos: Vec<(Arc<Photo>, SidecarNaming)>,
    /// For the colour label names a sidecar may use.
    labels: Catalog,
}

/// What changed in a folder since it came into the library.
#[derive(Clone, Debug, Default, Serialize)]
pub struct FolderChanges {
    /// The folder, as asked for.
    pub path: String,
    /// Files to import.
    pub new: Vec<ImportCandidate>,
    /// Files whose content the library already has (under another path).
    pub duplicates: usize,
    /// Supported files that couldn't be read.
    pub unreadable: Vec<ImportCandidate>,
    /// Photos whose file is gone.
    pub missing: Vec<FolderPhoto>,
    /// Photos whose sidecar has news.
    pub metadata: Vec<FolderPhoto>,
    /// The probes of the new files, kept for the import that follows.
    #[serde(skip)]
    probes: HashMap<String, ProbeInfo>,
    /// The catalog revision the scan was made against.
    #[serde(skip)]
    revision: u64,
}

/// A photo of the folder and its file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FolderPhoto {
    pub id: u64,
    pub path: String,
}

/// What [`synchronize`] does with the changes found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyncChoice {
    pub import_new: bool,
    pub remove_missing: bool,
    pub read_metadata: bool,
}

impl Default for SyncChoice {
    /// Lightroom Classic's defaults: import the new files, leave the rest to be looked at.
    fn default() -> Self {
        SyncChoice { import_new: true, remove_missing: false, read_metadata: false }
    }
}

/// What [`synchronize`] did.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SyncReport {
    pub imported: usize,
    pub removed: usize,
    pub read: usize,
    /// (path or photo, why) for what couldn't be done.
    pub failed: Vec<(String, String)>,
}

fn invalid(msg: impl Into<String>) -> EngineError {
    EngineError::Other(msg.into())
}

impl SyncInput {
    /// The scan of folder `path`: a folder the library holds photos in, not the startup disk.
    pub fn new(s: &mut Session, path: &str) -> Result<SyncInput> {
        let path = path.trim();
        if path.is_empty() {
            return Err(invalid("missing `path`"));
        }
        if lightcraft_catalog::folders::is_startup_disk(path) {
            return Err(invalid("choose a folder, not the whole startup disk"));
        }
        let f = Filter { library_folder: Some(path.to_string()), ..Default::default() };
        let ids = s.catalog.query(&f, &Sort::default());
        if ids.is_empty() {
            return Err(invalid(format!("{path}: no photo in the library was imported from it")));
        }
        let photos = ids.iter().filter_map(|id| s.catalog.photo(*id).map(|p| (Arc::clone(p), s.sidecar_naming(*id)))).collect();
        let (scan, _) = ScanInput::new(s, &[path.to_string()]);
        Ok(SyncInput { folder: path.to_string(), revision: s.catalog.revision, scan, photos, labels: s.catalog.clone() })
    }
}

/// Find what changed in the folder (see the module docs). Reads the disk, never the session.
pub fn scan_with(input: SyncInput, progress: &ScanProgress) -> FolderChanges {
    let SyncInput { folder, revision, scan, photos, labels } = input;
    let out = crate::import::scan_with(scan, std::slice::from_ref(&folder), progress);
    let mut changes = FolderChanges { path: folder, probes: out.probes, revision, ..Default::default() };
    for c in out.candidates {
        match (&c.duplicate, &c.error) {
            (Some(d), _) if d == "path" => {}
            (Some(_), _) => changes.duplicates += 1,
            (None, Some(_)) => changes.unreadable.push(c),
            (None, None) => changes.new.push(c),
        }
    }
    for (p, naming) in &photos {
        let Some(file) = crate::cmd::missing::checked_path(p) else { continue };
        if cfg!(target_arch = "wasm32") {
            continue;
        }
        if !Path::new(file).exists() {
            changes.missing.push(FolderPhoto { id: p.id.0, path: file.to_string() });
        } else if p.copy_of.is_none() && sidecar_has_news(&labels, p, *naming) {
            changes.metadata.push(FolderPhoto { id: p.id.0, path: file.to_string() });
        }
    }
    changes
}

/// [`scan_with`] on the calling thread.
pub fn scan(s: &mut Session, path: &str) -> Result<FolderChanges> {
    let input = SyncInput::new(s, path)?;
    Ok(scan_with(input, &ScanProgress::default()))
}

impl Session {
    /// The last scan ([`Session::folder_changes`]) if it is of folder `path` and nothing changed
    /// in the library since: what the person was shown is what gets done. Used once.
    pub fn take_folder_changes(&mut self, path: &str) -> Option<FolderChanges> {
        let c = self.folder_changes.take()?;
        let same = lightcraft_catalog::query::folder_key(&c.path) == lightcraft_catalog::query::folder_key(path);
        (same && c.revision == self.catalog.revision).then_some(c)
    }
}

/// Whether `p`'s XMP sidecar file was saved after the library last had its say about the photo
/// (imported, or edited here) and says something the library doesn't.
fn sidecar_has_news(labels: &Catalog, p: &Photo, naming: SidecarNaming) -> bool {
    let Source::File { path } = &p.source else { return false };
    let Some(file) = crate::sidecar::find_sidecar(path, naming) else { return false };
    let Ok(meta) = std::fs::metadata(&file) else { return false };
    if meta.len() > MAX_SIDECAR_BYTES {
        return false;
    }
    let saved = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs());
    let Some(saved) = saved.and_then(|s| i64::try_from(s).ok()) else { return false };
    let seen = [Some(p.imported.as_str()), p.edited.as_deref()].into_iter().flatten().filter_map(lightcraft_catalog::stacks::iso_seconds).max();
    if seen.is_some_and(|t| saved <= t) {
        return false;
    }
    let Ok(text) = std::fs::read_to_string(&file) else { return false };
    let Ok(sc) = crate::sidecar::parse_sidecar(&text, p.kind == lightcraft_catalog::MediaKind::Raw) else { return false };
    let sc = sc.resolve_label(labels);
    let mut q = p.clone();
    crate::sidecar::merge_into(&mut q, &sc, "");
    (q.rating, q.flag, q.label, &q.meta, &q.captured, &q.develop) != (p.rating, p.flag, p.label, &p.meta, &p.captured, &p.develop)
}

/// Act on `changes` as `choice` says, as one undo step ("Synchronize Folder"). A missing photo
/// whose file is back by now is left alone.
pub fn synchronize(s: &mut Session, changes: FolderChanges, choice: SyncChoice) -> Result<SyncReport> {
    let mark = s.undo.len();
    let mut report = SyncReport::default();
    let FolderChanges { new, missing, metadata, probes, .. } = changes;
    if choice.import_new && !new.is_empty() {
        s.import_probes = probes;
        let files: Vec<String> = new.into_iter().map(|c| c.path).collect();
        let r = crate::import::import_with(s, &files, &ImportOptions { mode: ImportMode::Add, ..Default::default() })?;
        report.imported = r.imported.len();
        report.failed.extend(r.failed);
    }
    if choice.remove_missing {
        let gone: Vec<PhotoId> = missing.iter().filter(|m| !Path::new(&m.path).exists()).map(|m| PhotoId(m.id)).collect();
        let ops: Vec<Op> = gone
            .iter()
            .filter(|id| s.catalog.photo(**id).is_some_and(|p| p.in_library()))
            .map(|id| Op::SetDeleted { id: *id, deleted: true })
            .collect();
        if !ops.is_empty() {
            report.removed = ops.len();
            s.commit("Remove Missing Photos", Op::Batch { ops })?;
        }
    }
    if choice.read_metadata {
        let mut ops = Vec::new();
        for m in &metadata {
            match s.read_sidecar_op(PhotoId(m.id)) {
                Ok(Some((op, _))) => {
                    ops.push(op);
                    report.read += 1;
                }
                Ok(None) => report.failed.push((m.path.clone(), "no XMP sidecar".into())),
                Err(e) => report.failed.push((m.path.clone(), e.to_string())),
            }
        }
        if !ops.is_empty() {
            s.commit("Read Metadata from File", Op::Batch { ops })?;
        }
    }
    let steps = s.undo.len().saturating_sub(mark);
    s.merge_undo(steps, "Synchronize Folder");
    if let Some(last) = s.undo.last_mut().filter(|_| steps == 1) {
        last.label = "Synchronize Folder".into();
    }
    Ok(report)
}
