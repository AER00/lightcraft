//! A failed save is an error, not a silent success: a command whose journal append fails returns
//! [`EngineError::NotSaved`], its change stays applied and queued, and the next save writes it.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use lightcraft_catalog::{Flag, MemStore, SnapshotPolicy, Store};
use serde_json::json;

use crate::library::LibraryStores;
use crate::{EngineError, Session};

/// A store whose writes fail while `fail` is set (a full or unplugged disk).
#[derive(Clone, Default)]
struct Flaky {
    files: MemStore,
    fail: Arc<AtomicBool>,
    /// Only snapshots fail (appends work).
    snapshots_only: bool,
}

impl Flaky {
    fn failing(&self) -> bool {
        self.fail.load(Ordering::SeqCst)
    }
    fn set_failing(&self, on: bool) {
        self.fail.store(on, Ordering::SeqCst);
    }
    /// The files as a crash at this moment would leave them.
    fn crash_copy(&self) -> Flaky {
        let m = MemStore::new();
        *m.files.lock().unwrap() = self.files.files.lock().unwrap().clone();
        Flaky { files: m, ..Default::default() }
    }
}

impl Store for Flaky {
    fn read(&mut self, name: &str) -> std::io::Result<Option<Vec<u8>>> {
        self.files.read(name)
    }
    fn write_atomic(&mut self, name: &str, data: &[u8]) -> std::io::Result<()> {
        if self.failing() {
            return Err(std::io::Error::other("disk full"));
        }
        self.files.write_atomic(name, data)
    }
    fn append(&mut self, name: &str, data: &[u8]) -> std::io::Result<()> {
        if self.failing() && !self.snapshots_only {
            return Err(std::io::Error::other("disk full"));
        }
        self.files.append(name, data)
    }
    fn truncate(&mut self, name: &str, len: u64) -> std::io::Result<()> {
        self.files.truncate(name, len)
    }
    fn describe(&self) -> String {
        "flaky".into()
    }
}

fn open(store: &Flaky, seed: bool) -> Session {
    let mut s = Session::new();
    let stores = LibraryStores { dir: "flaky".into(), catalog: Box::new(store.clone()), files: Box::new(MemStore::new()), on_disk: false };
    s.open_library_in(stores, seed).unwrap();
    s
}

#[test]
fn failed_append_fails_the_command_and_is_retried() {
    let store = Flaky::default();
    let mut s = open(&store, true);
    let id = s.selection.active.unwrap();
    s.execute("photo.rate", &json!({"rating": 1})).unwrap();
    store.set_failing(true);

    let e = s.execute("photo.rate", &json!({"rating": 4})).unwrap_err();
    assert!(matches!(e, EngineError::NotSaved(_)), "{e:?}");
    let msg = e.to_string();
    assert!(
        msg.starts_with("saved in memory but not written to disk: ") && msg.contains("disk full") && msg.ends_with("LightCraft will retry"),
        "{msg}"
    );
    // applied in memory, undoable, queued
    assert_eq!(s.catalog.photo(id).unwrap().rating, 4);
    assert!(!s.undo.is_empty());
    assert_eq!(s.unsaved().map(|u| u.0), Some(1));
    // queries and commands that change nothing are unaffected (they retry the queue, but report
    // their own result)
    let info = s.execute("library.info", &json!({})).unwrap();
    assert_eq!(info["unsavedOps"], 1, "{info}");
    assert!(info["unsavedError"].as_str().unwrap().contains("disk full"), "{info}");
    s.execute("library.select", &json!({"ids": [id.0]})).unwrap();
    // a second change fails too; both stay queued, in order
    let e = s.execute("photo.flag", &json!({"flag": "pick"})).unwrap_err();
    assert!(matches!(e, EngineError::NotSaved(_)));
    assert_eq!(s.unsaved().map(|u| u.0), Some(2));
    // the frame loop backs off instead of retrying every frame
    s.persist_if_dirty();
    assert_eq!(s.unsaved().map(|u| u.0), Some(2));
    // a crash now loses only the unsaved changes; the earlier ones are on disk
    assert_eq!(open(&store.crash_copy(), true).catalog.photo(id).unwrap().rating, 1);

    // the disk comes back: the next save writes the whole queue
    store.set_failing(false);
    s.execute("library.info", &json!({})).unwrap();
    assert_eq!(s.unsaved(), None);
    assert!(s.library.as_ref().unwrap().last_error.is_none());
    let expect = s.catalog.to_snapshot();
    drop(s); // no close: like a crash
    let s2 = open(&store, true);
    assert_eq!(s2.catalog.to_snapshot(), expect);
    let p = s2.catalog.photo(id).unwrap();
    assert_eq!((p.rating, p.flag), (4, Flag::Pick));
}

/// Undo is a command like any other: a failed write of its op is reported, and retried.
#[test]
fn failed_append_of_undo_is_reported_and_retried() {
    let store = Flaky::default();
    let mut s = open(&store, true);
    let id = s.selection.active.unwrap();
    let before = s.catalog.photo(id).unwrap().rating;
    s.execute("photo.rate", &json!({"rating": 3})).unwrap();
    store.set_failing(true);
    assert!(matches!(s.execute("edit.undo", &json!({})), Err(EngineError::NotSaved(_))));
    assert_eq!(s.catalog.photo(id).unwrap().rating, before);
    store.set_failing(false);
    s.persist().unwrap();
    drop(s);
    assert_eq!(open(&store, true).catalog.photo(id).unwrap().rating, before);
}

/// A failed compaction is not a failed command: the command's own records were appended.
#[test]
fn failed_snapshot_does_not_fail_the_command() {
    let store = Flaky { snapshots_only: true, ..Default::default() };
    let mut s = open(&store, false);
    s.library.as_mut().unwrap().journal_mut().policy = SnapshotPolicy { max_records: 1, max_bytes: u64::MAX };
    store.set_failing(true);
    s.execute("album.create", &json!({"name": "A"})).unwrap();
    s.execute("album.create", &json!({"name": "B"})).unwrap();
    assert_eq!(s.unsaved(), None);
    assert!(s.library.as_ref().unwrap().last_error.is_some(), "the compaction failure is reported");
    drop(s);
    store.set_failing(false);
    assert_eq!(open(&store, false).catalog.albums().count(), 2);
}
