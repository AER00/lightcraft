//! Synchronize Folder in the app: the folder is scanned on a worker thread (a network share can
//! take minutes; no frame waits for it) while its dialog is open, and the dialog then says what
//! changed and what to do about it. Synchronize runs `folder.synchronize`, which acts on exactly
//! the scan the dialog showed (see `lightcraft_engine::sync`).

use std::sync::Arc;
use std::sync::atomic::Ordering;

use lightcraft_engine::import::ScanProgress;
use lightcraft_engine::sync::{FolderChanges, SyncChoice, SyncInput, scan_with};
use serde_json::{Value, json};

use crate::LightcraftApp;
use crate::state::{Dialog, SyncCounts};
use crate::theme::Tokens;

/// A running scan of the dialog's folder.
pub struct SyncTask {
    path: String,
    progress: Arc<ScanProgress>,
    rx: std::sync::mpsc::Receiver<FolderChanges>,
}

/// Open the dialog for folder `path` (called `name` in it) and start scanning.
pub fn open(app: &mut LightcraftApp, path: &str, name: &str, disk: bool) -> Result<(), String> {
    cancel(app);
    let input = SyncInput::new(&mut app.session, path, disk).map_err(|e| e.to_string())?;
    let progress = Arc::new(ScanProgress::default());
    let (tx, rx) = std::sync::mpsc::channel();
    let p = progress.clone();
    let job = move || {
        let _ = tx.send(scan_with(input, &p));
    };
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::spawn(job);
    #[cfg(target_arch = "wasm32")]
    job();
    app.sync = Some(SyncTask { path: path.to_string(), progress, rx });
    let d = SyncChoice::default();
    app.ui.dialog = Some(Dialog::SynchronizeFolder {
        path: path.to_string(),
        name: name.to_string(),
        disk,
        counts: None,
        import_new: d.import_new,
        relink_moved: d.relink_moved,
        remove_missing: d.remove_missing,
        read_metadata: d.read_metadata,
    });
    Ok(())
}

/// Stop a running scan and let go of a finished one the dialog made (one an agent made with
/// `folder.scanChanges` is the agent's).
fn cancel(app: &mut LightcraftApp) {
    if let Some(t) = app.sync.take() {
        t.progress.cancel.store(true, Ordering::Relaxed);
    }
    if std::mem::take(&mut app.sync_owns_changes) {
        app.session.folder_changes = None;
    }
}

/// Collect a finished scan into the dialog (called every frame). A scan whose dialog was closed
/// is cancelled.
pub fn poll(app: &mut LightcraftApp, ctx: &egui::Context) {
    let open_for = match &app.ui.dialog {
        Some(Dialog::SynchronizeFolder { path, .. }) => Some(path.clone()),
        _ => None,
    };
    let Some(open_for) = open_for else {
        if app.sync.is_some() || app.sync_owns_changes {
            cancel(app);
        }
        return;
    };
    let Some(task) = app.sync.as_ref() else { return };
    if task.path != open_for {
        cancel(app);
        return;
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(100));
    let changes = match task.rx.try_recv() {
        Ok(c) => c,
        Err(std::sync::mpsc::TryRecvError::Empty) => return,
        Err(_) => {
            app.sync = None;
            app.ui.dialog = None;
            app.toast(ctx, crate::i18n::tr("Scan failed"));
            return;
        }
    };
    app.sync = None;
    let c = SyncCounts {
        offline: changes.offline,
        new: changes.new.len(),
        duplicates: changes.duplicates,
        unreadable: changes.unreadable.len(),
        missing: changes.missing.len(),
        metadata: changes.metadata.len(),
        moved: changes.moved.len(),
    };
    app.session.folder_changes = Some(changes);
    app.sync_owns_changes = true;
    if let Some(Dialog::SynchronizeFolder { counts, .. }) = &mut app.ui.dialog {
        *counts = Some(c);
    }
}

/// How far the scan is (files probed, of how many).
fn progress(app: &LightcraftApp) -> Option<(usize, usize)> {
    app.sync.as_ref().map(|t| (t.progress.done.load(Ordering::Relaxed), t.progress.total.load(Ordering::Relaxed)))
}

/// The dialog's body.
pub fn body(app: &LightcraftApp, ui: &mut egui::Ui, dlg: &mut Dialog) {
    let Dialog::SynchronizeFolder { path, name, counts, import_new, relink_moved, remove_missing, read_metadata, .. } = dlg else { return };
    let t = Tokens::get(ui.ctx());
    ui.set_min_width(420.0);
    ui.label(crate::i18n::tr("Bring the library up to date with this folder and the folders inside it:"));
    ui.label(egui::RichText::new(name.as_str()).strong());
    ui.label(egui::RichText::new(path.as_str()).color(t.text_dim));
    ui.add_space(6.0);
    let Some(c) = counts.as_ref() else {
        ui.horizontal(|ui| {
            ui.spinner();
            match progress(app) {
                Some((done, total)) if total > 0 => ui.label(format!("{} {done} / {total}", crate::i18n::tr("Scanning…"))),
                _ => ui.label(crate::i18n::tr("Scanning…")),
            }
        });
        return;
    };
    if c.offline {
        ui.label(crate::i18n::tr("This folder isn't there: it was moved or renamed, or its disk isn't connected."));
        return;
    }
    let choice = |ui: &mut egui::Ui, on: &mut bool, n: usize, text: String, widget: &str| {
        let r = ui.add_enabled(n > 0, egui::Checkbox::new(on, text));
        crate::widgets::register(ui.ctx(), widget, r.rect);
    };
    let count = |text: &str, n: usize| format!("{} ({n})", crate::i18n::tr(text));
    choice(ui, import_new, c.new, count("Import new photos", c.new), "syncImportNew");
    choice(ui, relink_moved, c.moved, count("Relink photos whose file was renamed or moved", c.moved), "syncRelinkMoved");
    choice(ui, remove_missing, c.missing, count("Remove missing photos from the library", c.missing), "syncRemoveMissing");
    choice(ui, read_metadata, c.metadata, count("Read metadata updates from XMP sidecars", c.metadata), "syncReadMetadata");
    ui.add_space(6.0);
    let dim = |ui: &mut egui::Ui, s: &str| ui.label(egui::RichText::new(s).color(t.text_dim));
    if c.new + c.moved + c.missing + c.metadata == 0 {
        dim(ui, crate::i18n::tr("The library is up to date with this folder."));
    }
    if c.missing > 0 {
        dim(ui, crate::i18n::tr("Missing photos move to Recently Deleted and can be restored; no file on disk is touched."));
    }
    if c.metadata > 0 {
        dim(ui, crate::i18n::tr("Reading a sidecar replaces the photo's rating, labels, metadata and edits with what it says."));
    }
    if c.duplicates > 0 {
        dim(ui, &count("Skipped: files already in the library elsewhere", c.duplicates));
    }
    if c.unreadable > 0 {
        dim(ui, &count("Skipped: files that can't be read", c.unreadable));
    }
}

/// Synchronize: `folder.synchronize` with the dialog's choices.
pub fn confirm(app: &mut LightcraftApp, dlg: &Dialog) -> Result<Value, String> {
    let Dialog::SynchronizeFolder { path, disk, counts: Some(_), import_new, relink_moved, remove_missing, read_metadata, .. } = dlg else {
        return Err("the folder is still being scanned".into());
    };
    let r = app.run(
        "folder.synchronize",
        json!({"path": path, "disk": disk, "scanned": true, "importNew": import_new, "relinkMoved": relink_moved, "removeMissing": remove_missing, "readMetadata": read_metadata}),
    );
    app.sync_owns_changes = false;
    if r.is_err() {
        // (the folder changed since the scan, or went offline): look again, in the background
        let (path, disk) = (path.clone(), *disk);
        let name = match dlg {
            Dialog::SynchronizeFolder { name, .. } => name.clone(),
            _ => String::new(),
        };
        open(app, &path, &name, disk)?;
    }
    r
}
