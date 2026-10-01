//! The LightCraft engine façade.
//!
//! Every user-visible action is a command with a stable id (`photo.rate`, `develop.set`,
//! `album.create`, `mask.add`…) and JSON parameters. The egui UI, the CLI, the control channel and
//! the MCP server all go through [`Session::execute`].
//!
//! State: a [`Catalog`] (mutated only by ops, so every change is undoable and journaled), the
//! library view (filter/sort/source), the selection, the develop clipboard, presets, and caches
//! of decoded source proxies. Rendering is done by [`RenderJob`]s that are `Send` so frontends can
//! run them off the UI thread.
#![forbid(unsafe_code)]

pub mod cmd;
pub mod demo;
pub mod files;
pub mod media;
pub mod presets;
mod view;

use std::sync::Arc;

pub use cmd::{CommandInfo, CommandSpec, command_specs, find_command};
use lightcraft_catalog::{Catalog, Filter, Op, PhotoId, Sort};
use lightcraft_develop::DevelopSettings;
pub use media::{RenderJob, SourceLevel};
use serde_json::Value;
pub use view::{LibrarySource, Selection};
pub use {lightcraft_catalog as catalog, lightcraft_develop as develop, lightcraft_pipeline as pipeline};

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("unknown command `{0}`")]
    UnknownCommand(String),
    #[error("command `{0}` is not available right now: {1}")]
    Disabled(String, String),
    #[error("invalid parameters for `{cmd}`: {msg}")]
    BadParams { cmd: String, msg: String },
    #[error("{0}")]
    Catalog(#[from] lightcraft_catalog::CatalogError),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, EngineError>;

/// One undo step: the inverse op and a label.
#[derive(Clone, Debug)]
pub struct UndoEntry {
    pub label: String,
    pub op: Op,
}

/// An in-progress slider drag / brush stroke: one undo step when it ends.
#[derive(Clone, Debug)]
pub struct Interaction {
    pub label: String,
    pub photo: PhotoId,
    pub original: Arc<DevelopSettings>,
}

pub struct Session {
    pub catalog: Catalog,
    pub source: LibrarySource,
    pub filter: Filter,
    pub sort: Sort,
    pub selection: Selection,
    /// The grid order for the current source/filter/sort (cached by catalog revision).
    visible: Vec<PhotoId>,
    visible_key: Option<(u64, String)>,
    pub undo: Vec<UndoEntry>,
    pub redo: Vec<UndoEntry>,
    pub interaction: Option<Interaction>,
    /// Copied develop settings (partial JSON) for Paste.
    pub clipboard: Option<Value>,
    /// Groups last used for Copy (Lightroom remembers them).
    pub copy_groups: Vec<lightcraft_develop::SettingsGroup>,
    pub presets: Vec<lightcraft_develop::Preset>,
    /// Executed commands (actions / debugging / replay).
    pub journal: Vec<(String, Value)>,
    /// Ops applied since the last `drain_log` (for persistence).
    pending_log: Vec<Op>,
    pub media: media::MediaCache,
    /// Current time provider (ISO 8601); injectable for tests and wasm.
    pub clock: Box<dyn Fn() -> String + Send>,
    depth: u32,
    /// Selected mask (Masking panel), by mask id.
    pub active_mask: Option<u32>,
}

impl Default for Session {
    fn default() -> Self {
        Session::new()
    }
}

impl Session {
    pub fn new() -> Session {
        Session {
            catalog: Catalog::new(),
            source: LibrarySource::All,
            filter: Filter::default(),
            sort: Sort::default(),
            selection: Selection::default(),
            visible: Vec::new(),
            visible_key: None,
            undo: Vec::new(),
            redo: Vec::new(),
            interaction: None,
            clipboard: None,
            copy_groups: lightcraft_develop::SettingsGroup::default_copy(),
            presets: presets::builtin(),
            journal: Vec::new(),
            pending_log: Vec::new(),
            media: media::MediaCache::default(),
            clock: Box::new(|| "2026-09-30T12:00:00".to_string()),
            depth: 0,
            active_mask: None,
        }
    }

    /// A session with the procedurally generated demo library loaded.
    pub fn with_demo() -> Session {
        let mut s = Session::new();
        demo::load(&mut s);
        s
    }

    /// Run a command by id. THE entry point for every frontend.
    pub fn execute(&mut self, id: &str, params: &Value) -> Result<Value> {
        let spec = find_command(id).ok_or_else(|| EngineError::UnknownCommand(id.to_string()))?;
        (spec.enabled)(self).map_err(|why| EngineError::Disabled(id.to_string(), why))?;
        let empty = Value::Object(Default::default());
        let params = if params.is_null() { &empty } else { params };
        self.depth += 1;
        let r = (spec.run)(self, params);
        self.depth -= 1;
        if r.is_ok() && spec.journal && self.depth == 0 {
            self.journal.push((id.to_string(), params.clone()));
            if self.journal.len() > 10_000 {
                self.journal.drain(..1000);
            }
        }
        r
    }

    pub fn commands(&self) -> Vec<CommandInfo> {
        command_specs().iter().map(|c| c.info(self)).collect()
    }

    // ---------------------------------------------------------------- ops, undo

    /// Apply an op as one undoable step.
    pub fn commit(&mut self, label: &str, op: Op) -> Result<()> {
        let fwd = op.clone();
        let inv = self.catalog.apply(op)?;
        self.pending_log.push(fwd);
        self.undo.push(UndoEntry { label: label.to_string(), op: inv });
        if self.undo.len() > 1000 {
            self.undo.remove(0);
        }
        self.redo.clear();
        Ok(())
    }

    /// Apply without recording undo (interactive previews).
    fn apply_silent(&mut self, op: Op) -> Result<()> {
        self.catalog.apply(op)?;
        Ok(())
    }

    pub fn undo_step(&mut self) -> Result<String> {
        let e = self.undo.pop().ok_or_else(|| EngineError::Other("nothing to undo".into()))?;
        let redo = self.catalog.apply(e.op.clone())?;
        self.pending_log.push(e.op);
        self.redo.push(UndoEntry { label: e.label.clone(), op: redo });
        Ok(e.label)
    }

    pub fn redo_step(&mut self) -> Result<String> {
        let e = self.redo.pop().ok_or_else(|| EngineError::Other("nothing to redo".into()))?;
        let undo = self.catalog.apply(e.op.clone())?;
        self.pending_log.push(e.op);
        self.undo.push(UndoEntry { label: e.label.clone(), op: undo });
        Ok(e.label)
    }

    /// Ops applied since the last call, for the op-log store.
    pub fn drain_log(&mut self) -> Vec<Op> {
        std::mem::take(&mut self.pending_log)
    }

    // ---------------------------------------------------------------- develop edits

    /// The photo being edited (the active photo).
    pub fn active(&self) -> Option<PhotoId> {
        self.selection.active.filter(|id| self.catalog.photo(*id).is_some())
    }

    pub fn develop_of(&self, id: PhotoId) -> Option<Arc<DevelopSettings>> {
        self.catalog.photo(id).map(|p| p.develop.clone())
    }

    /// Change a photo's develop settings. During an interaction the change is previewed without an
    /// undo step; otherwise it is committed with a history entry.
    pub fn set_develop(&mut self, id: PhotoId, settings: DevelopSettings, label: &str) -> Result<()> {
        let now = (self.clock)();
        let settings = Arc::new(settings);
        if let Some(i) = &self.interaction
            && i.photo == id
        {
            return self.apply_silent(Op::SetDevelop { id, settings, label: label.into(), edited: Some(now) });
        }
        let op = self.develop_op(id, (*settings).clone(), label).ok_or(lightcraft_catalog::CatalogError::NoPhoto(id))?;
        self.commit(label, op)
    }

    /// The op that sets a photo's develop settings and appends a History entry (for batches).
    pub fn develop_op(&self, id: PhotoId, settings: DevelopSettings, label: &str) -> Option<Op> {
        let p = self.catalog.photo(id)?;
        let settings = Arc::new(settings);
        let mut history = p.history.clone();
        history.push(lightcraft_catalog::HistoryStep { label: label.into(), settings: settings.clone() });
        if history.len() > 200 {
            history.remove(0);
        }
        Some(Op::Batch {
            ops: vec![Op::SetDevelop { id, settings, label: label.into(), edited: Some((self.clock)()) }, Op::SetHistory { id, history }],
        })
    }

    pub fn begin_interaction(&mut self, label: &str) -> Result<()> {
        if self.interaction.is_some() {
            self.end_interaction()?;
        }
        let id = self.active().ok_or_else(|| EngineError::Other("no active photo".into()))?;
        let original = self.develop_of(id).unwrap_or_default();
        self.interaction = Some(Interaction { label: label.into(), photo: id, original });
        Ok(())
    }

    /// Commit the interaction as one undo step (no-op if nothing changed).
    pub fn end_interaction(&mut self) -> Result<()> {
        let Some(i) = self.interaction.take() else { return Ok(()) };
        let Some(cur) = self.develop_of(i.photo) else { return Ok(()) };
        if *cur == *i.original {
            return Ok(());
        }
        // Restore the original silently, then commit the final value as one step.
        self.apply_silent(Op::SetDevelop { id: i.photo, settings: i.original.clone(), label: i.label.clone(), edited: None })?;
        self.set_develop(i.photo, (*cur).clone(), &i.label)
    }

    pub fn cancel_interaction(&mut self) -> Result<()> {
        if let Some(i) = self.interaction.take() {
            self.apply_silent(Op::SetDevelop { id: i.photo, settings: i.original, label: i.label, edited: None })?;
        }
        Ok(())
    }

    // ---------------------------------------------------------------- library view

    /// Photos shown in the grid/filmstrip for the current source, filter and sort.
    pub fn visible(&mut self) -> &[PhotoId] {
        let key = (self.catalog.revision, format!("{:?}|{:?}|{:?}", self.source, self.filter, self.sort));
        if self.visible_key.as_ref() != Some(&key) {
            let f = self.source.to_filter(&self.filter, &self.catalog);
            self.visible = self.catalog.query(&f, &self.sort);
            if matches!(self.source, LibrarySource::Album(_))
                && self.sort.key == lightcraft_catalog::SortKey::CaptureDate
                && let LibrarySource::Album(a) = self.source
                && let Some(al) = self.catalog.album(a)
                && self.filter == Filter::default()
            {
                let order = al.photos.clone();
                self.visible.sort_by_key(|id| order.iter().position(|x| x == id).unwrap_or(usize::MAX));
                if !self.sort.ascending {
                    self.visible.reverse();
                }
            }
            self.visible_key = Some(key);
        }
        &self.visible
    }

    pub fn visible_cloned(&mut self) -> Vec<PhotoId> {
        self.visible().to_vec()
    }

    /// Targets of photo commands: explicit `ids`/`id` param, else the selection.
    pub fn targets(&self, p: &Value) -> Vec<PhotoId> {
        if let Some(a) = p.get("ids").and_then(Value::as_array) {
            return a.iter().filter_map(Value::as_u64).map(PhotoId).collect();
        }
        if let Some(id) = p.get("id").and_then(Value::as_u64) {
            return vec![PhotoId(id)];
        }
        if self.selection.ids.is_empty() { self.selection.active.into_iter().collect() } else { self.selection.ids.clone() }
    }
}

#[cfg(test)]
mod tests;
