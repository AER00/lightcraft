//! A persistent library: a directory holding the catalog (op log + snapshot), user presets, the
//! last view state and the preview cache.
//!
//! ```text
//! LightCraft Library/
//!   catalog.snap   catalog.log      (lightcraft-catalog journal)
//!   presets.json   view.json        (user presets + favourites; last source/filter/sort/selection)
//!   prefs.json     (library preferences: XMP sidecars)
//!   thumbs/        (rendered thumbnail cache, safe to delete)
//!   Originals/     (photos imported with "copy into library")
//! ```
//!
//! Every top-level [`Session::execute`] persists the ops it produced (fsynced) before returning,
//! so a crash loses at most the command in flight. The log is compacted into a snapshot when it
//! grows (see [`lightcraft_catalog::SnapshotPolicy`]) and on [`Session::close_library`].

use std::path::{Path, PathBuf};

use lightcraft_catalog::{FsStore, Journal, LoadReport, Store};
use lightcraft_develop::Preset;
use serde::{Deserialize, Serialize};

use crate::{EngineError, LibrarySource, Result, Selection, Session};

/// Library directory name inside the user's Pictures folder.
pub const DEFAULT_NAME: &str = "LightCraft Library";

/// The default library location: `$LIGHTCRAFT_LIBRARY` if set, else `~/Pictures/LightCraft Library`
/// (`%USERPROFILE%\Pictures\LightCraft Library` on Windows).
pub fn default_dir() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("LIGHTCRAFT_LIBRARY").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    let home = if cfg!(windows) { std::env::var_os("USERPROFILE") } else { std::env::var_os("HOME") }?;
    Some(PathBuf::from(home).join("Pictures").join(DEFAULT_NAME))
}

pub struct Library {
    pub dir: PathBuf,
    journal: Journal,
    /// What happened when the catalog was loaded.
    pub report: LoadReport,
    /// Last persistence error (shown by the UI; ops stay pending and are retried).
    pub last_error: Option<String>,
    presets_written: String,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct PresetsFile {
    user: Vec<Preset>,
    favorites: Vec<String>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct ViewFile {
    source: LibrarySource,
    filter: lightcraft_catalog::Filter,
    sort: lightcraft_catalog::Sort,
    selection: Selection,
}

impl Library {
    pub fn journal(&self) -> &Journal {
        &self.journal
    }
    pub fn thumbs_dir(&self) -> PathBuf {
        self.dir.join("thumbs")
    }
    pub fn originals_dir(&self) -> PathBuf {
        self.dir.join("Originals")
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct PrefsFile {
    xmp: crate::sidecar::XmpPrefs,
}

fn presets_json(presets: &[Preset]) -> String {
    let f = PresetsFile {
        user: presets.iter().filter(|p| !p.builtin).cloned().collect(),
        favorites: presets.iter().filter(|p| p.builtin && p.favorite).map(|p| p.id.clone()).collect(),
    };
    serde_json::to_string_pretty(&f).unwrap_or_default()
}

fn write_atomic(dir: &Path, name: &str, data: &[u8]) -> std::io::Result<()> {
    FsStore::open(dir)?.write_atomic(name, data)
}

impl Session {
    /// Open (or create) the library at `dir` into this session, replacing its catalog. With
    /// `seed_demo`, a newly created library starts with the procedural demo photos.
    pub fn open_library(&mut self, dir: impl AsRef<Path>, seed_demo: bool) -> Result<&LoadReport> {
        let dir = dir.as_ref().to_path_buf();
        let store = FsStore::open(&dir).map_err(|e| EngineError::Other(format!("can't open library {}: {e}", dir.display())))?;
        let (mut journal, catalog, report) = Journal::open(Box::new(store))?;
        self.catalog = catalog;
        self.undo.clear();
        self.redo.clear();
        self.interaction = None;
        self.pending_log.clear();
        self.selection = Selection::default();
        self.source = LibrarySource::All;
        if report.created && seed_demo {
            crate::demo::load(self);
            journal.snapshot(&self.catalog)?;
        }
        // presets
        if let Ok(bytes) = std::fs::read(dir.join("presets.json"))
            && let Ok(f) = serde_json::from_slice::<PresetsFile>(&bytes)
        {
            for p in &mut self.presets {
                p.favorite = p.builtin && f.favorites.contains(&p.id) || (!p.builtin && p.favorite);
            }
            for u in f.user {
                if !self.presets.iter().any(|p| p.id == u.id) {
                    self.presets.push(u);
                }
            }
        }
        // preferences
        self.xmp =
            std::fs::read(dir.join("prefs.json")).ok().and_then(|b| serde_json::from_slice::<PrefsFile>(&b).ok()).map(|p| p.xmp).unwrap_or_default();
        // view state
        if let Ok(bytes) = std::fs::read(dir.join("view.json"))
            && let Ok(v) = serde_json::from_slice::<ViewFile>(&bytes)
        {
            self.source = v.source;
            self.filter = v.filter;
            self.sort = v.sort;
            self.selection = v.selection;
            self.selection.ids.retain(|id| self.catalog.photo(*id).is_some());
            self.selection.active = self.selection.active.filter(|id| self.catalog.photo(*id).is_some());
        }
        if self.selection.active.is_none()
            && let Some(first) = self.visible_cloned().first()
        {
            self.selection = Selection::single(*first);
        }
        let presets_written = presets_json(&self.presets);
        self.media.attach_disk_cache(&dir.join("thumbs"));
        self.library = Some(Library { dir, journal, report, last_error: None, presets_written });
        Ok(&self.library.as_ref().expect("just set").report)
    }

    /// Write pending ops to the log (fsynced), compact when due, and save changed presets.
    /// Called after every top-level command; cheap when nothing changed.
    pub fn persist(&mut self) -> Result<()> {
        let Some(lib) = self.library.as_mut() else { return Ok(()) };
        if !self.pending_log.is_empty() {
            if let Err(e) = lib.journal.append(&self.pending_log) {
                lib.last_error = Some(e.to_string());
                log::error!("library: {e}");
                return Err(e.into());
            }
            self.pending_log.clear();
            lib.last_error = None;
        }
        // Never snapshot mid-interaction: the catalog then holds an uncommitted preview value.
        if lib.journal.wants_snapshot() && self.interaction.is_none() {
            lib.journal.snapshot(&self.catalog)?;
        }
        let presets = presets_json(&self.presets);
        if presets != lib.presets_written {
            if let Err(e) = write_atomic(&lib.dir, "presets.json", presets.as_bytes()) {
                log::error!("library: presets: {e}");
            } else {
                lib.presets_written = presets;
            }
        }
        Ok(())
    }

    /// Persist if commands left ops pending (cheap; frontends call it once per frame for state
    /// changed outside [`Session::execute`]).
    pub fn persist_if_dirty(&mut self) {
        if self.library.is_some() && !self.pending_log.is_empty() {
            let _ = self.persist();
        }
    }

    /// Flush everything and write a snapshot (on quit). Ends an open interaction first.
    pub fn close_library(&mut self) -> Result<()> {
        if self.library.is_none() {
            return Ok(());
        }
        let _ = self.end_interaction();
        self.persist()?;
        let view = ViewFile { source: self.source, filter: self.filter.clone(), sort: self.sort, selection: self.selection.clone() };
        let Some(lib) = self.library.as_mut() else { return Ok(()) };
        lib.journal.snapshot(&self.catalog)?;
        if let Ok(v) = serde_json::to_vec_pretty(&view) {
            let _ = write_atomic(&lib.dir, "view.json", &v);
        }
        Ok(())
    }

    /// Save the library preferences (no-op for in-memory sessions).
    pub fn save_prefs(&self) -> Result<()> {
        let Some(lib) = &self.library else { return Ok(()) };
        let v = serde_json::to_vec_pretty(&PrefsFile { xmp: self.xmp }).unwrap_or_default();
        write_atomic(&lib.dir, "prefs.json", &v).map_err(|e| EngineError::Other(format!("prefs: {e}")))
    }

    /// Compact the log into a snapshot now.
    pub fn compact_library(&mut self) -> Result<()> {
        self.persist()?;
        if self.interaction.is_some() {
            return Err(EngineError::Other("can't compact during an interaction".into()));
        }
        if let Some(lib) = self.library.as_mut() {
            lib.journal.snapshot(&self.catalog)?;
        }
        Ok(())
    }
}
