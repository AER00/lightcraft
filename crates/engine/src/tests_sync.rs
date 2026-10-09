//! Synchronize Folder: bring a library folder up to date with what is on disk (see
//! [`crate::sync`]).
//!
//! Scenarios, in the words of someone whose folder changed outside LightCraft:
//!
//! * Given a folder of the library, when files were added to it (or to a folder inside it), the
//!   scan lists them as new; files the library already has, by path or by content, are not new.
//! * When a photo's file was deleted or moved away, the scan lists the photo as missing.
//! * When another app saved a photo's XMP sidecar after the photo came into the library, and the
//!   sidecar says something the library doesn't, the scan lists a metadata update; a sidecar older
//!   than that, or one that agrees with the library, is not an update.
//! * Scanning changes nothing.
//! * Synchronizing imports the new files; it removes missing photos (to Recently Deleted) and
//!   reads metadata updates only when asked. Whatever it does is one undo step.
//! * Only a folder of the library can be synchronized, never the whole startup disk.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde_json::{Value, json};

use crate::Session;

/// A scratch folder that goes away with the test, however it ends.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("lc-sync-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }
    fn path(&self, rel: &str) -> String {
        self.0.join(rel).to_string_lossy().to_string()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A small procedural PNG (distinct per seed).
fn write_png(path: &str, seed: u8) {
    let (w, h) = (24usize, 16usize);
    let data: Vec<[u8; 4]> = (0..w * h).map(|i| [(i % w * 9) as u8, (i / w * 13) as u8, seed, 255]).collect();
    let img = lightcraft_raster::Rgba8 { width: w, height: h, data };
    let bytes = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&img), &lightcraft_codecs::EncodeMeta::default()).unwrap();
    std::fs::create_dir_all(Path::new(path).parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn sidecar(rating: u8) -> String {
    format!(
        r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about="" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmp:Rating="{rating}"/>
</rdf:RDF></x:xmpmeta>"#
    )
}

/// Write `a.xmp` beside `a.png`, dated `age` before now.
fn write_sidecar(png: &str, rating: u8, age: Duration) {
    let xmp = Path::new(png).with_extension("xmp");
    std::fs::write(&xmp, sidecar(rating)).unwrap();
    let f = std::fs::File::options().write(true).open(&xmp).unwrap();
    f.set_modified(SystemTime::now() - age).unwrap();
}

/// A library holding `trip/a.png` and `trip/day1/b.png`, the session clock at a fixed past time.
fn library(dir: &Scratch) -> Session {
    write_png(&dir.path("trip/a.png"), 1);
    write_png(&dir.path("trip/day1/b.png"), 2);
    let mut s = Session::new().with_fs();
    s.execute("library.import", &json!({"paths": [dir.path("trip")]})).unwrap();
    assert_eq!(s.catalog.len(), 2);
    s
}

fn paths(v: &Value, key: &str) -> Vec<String> {
    let mut out: Vec<String> = v[key].as_array().unwrap().iter().map(|c| c["path"].as_str().unwrap().to_string()).collect();
    out.sort();
    out
}

/// What the library holds: each photo's file, whether it is in the library, and its rating.
fn holdings(s: &Session) -> Vec<(String, bool, u8)> {
    let mut v: Vec<(String, bool, u8)> = s.catalog.photos().map(|p| (p.file_name.clone(), p.in_library(), p.rating)).collect();
    v.sort();
    v
}

fn scan(s: &mut Session, path: &str) -> Value {
    s.execute("folder.scanChanges", &json!({"path": path})).unwrap()
}

#[test]
fn an_unchanged_folder_has_no_changes() {
    let dir = Scratch::new("none");
    let mut s = library(&dir);
    let r = scan(&mut s, &dir.path("trip"));
    assert!(paths(&r, "new").is_empty() && paths(&r, "missing").is_empty() && paths(&r, "metadata").is_empty(), "{r}");
}

#[test]
fn files_added_to_the_folder_or_a_folder_inside_it_are_new() {
    let dir = Scratch::new("new");
    let mut s = library(&dir);
    write_png(&dir.path("trip/c.png"), 3);
    write_png(&dir.path("trip/day2/d.png"), 4);
    // the same bytes as a photo the library has: not new
    std::fs::copy(dir.path("trip/a.png"), dir.path("trip/a-copy.png")).unwrap();
    let before = s.catalog.to_snapshot();
    let r = scan(&mut s, &dir.path("trip"));
    assert_eq!(paths(&r, "new"), vec![dir.path("trip/c.png"), dir.path("trip/day2/d.png")], "{r}");
    assert_eq!(r["duplicates"], 1, "{r}");
    assert_eq!(s.catalog.to_snapshot(), before, "scanning changes nothing");
}

#[test]
fn a_photo_whose_file_is_gone_is_missing() {
    let dir = Scratch::new("missing");
    let mut s = library(&dir);
    std::fs::remove_file(dir.path("trip/day1/b.png")).unwrap();
    let r = scan(&mut s, &dir.path("trip"));
    assert_eq!(paths(&r, "missing"), vec![dir.path("trip/day1/b.png")], "{r}");
    assert!(r["missing"][0]["id"].is_u64());
}

#[test]
fn a_sidecar_saved_by_another_app_is_a_metadata_update() {
    let dir = Scratch::new("meta");
    let mut s = library(&dir);
    // saved now: after the photo came in (the session clock says 2026-09-30)
    write_sidecar(&dir.path("trip/a.png"), 4, Duration::ZERO);
    let r = scan(&mut s, &dir.path("trip"));
    assert_eq!(paths(&r, "metadata"), vec![dir.path("trip/a.png")], "{r}");
}

#[test]
fn an_old_sidecar_or_one_that_agrees_is_no_update() {
    let dir = Scratch::new("meta-old");
    let mut s = library(&dir);
    // older than the import: whatever it says, the library has had its say since
    write_sidecar(&dir.path("trip/a.png"), 4, Duration::from_secs(400 * 24 * 3600));
    // new, but says what the library already knows
    write_sidecar(&dir.path("trip/day1/b.png"), 0, Duration::ZERO);
    let r = scan(&mut s, &dir.path("trip"));
    assert!(paths(&r, "metadata").is_empty(), "{r}");
}

#[test]
fn synchronizing_imports_new_files_and_leaves_the_rest_unless_asked() {
    let dir = Scratch::new("sync");
    let mut s = library(&dir);
    write_png(&dir.path("trip/c.png"), 3);
    std::fs::remove_file(dir.path("trip/day1/b.png")).unwrap();
    write_sidecar(&dir.path("trip/a.png"), 5, Duration::ZERO);
    let r = s.execute("folder.synchronize", &json!({"path": dir.path("trip")})).unwrap();
    assert_eq!((r["imported"].as_u64(), r["removed"].as_u64(), r["read"].as_u64()), (Some(1), Some(0), Some(0)), "{r}");
    assert_eq!(s.catalog.photos().filter(|p| p.in_library()).count(), 3, "the missing photo stays");
    let a = s.catalog.photos().find(|p| p.file_name == "a.png").unwrap();
    assert_eq!(a.rating, 0, "metadata is read only when asked");
}

#[test]
fn synchronizing_everything_is_one_undo_step() {
    let dir = Scratch::new("sync-all");
    let mut s = library(&dir);
    let before = holdings(&s);
    write_png(&dir.path("trip/c.png"), 3);
    std::fs::remove_file(dir.path("trip/day1/b.png")).unwrap();
    write_sidecar(&dir.path("trip/a.png"), 5, Duration::ZERO);
    let r =
        s.execute("folder.synchronize", &json!({"path": dir.path("trip"), "importNew": true, "removeMissing": true, "readMetadata": true})).unwrap();
    assert_eq!((r["imported"].as_u64(), r["removed"].as_u64(), r["read"].as_u64()), (Some(1), Some(1), Some(1)), "{r}");
    let library: Vec<String> = s.catalog.photos().filter(|p| p.in_library()).map(|p| p.file_name.clone()).collect();
    assert_eq!(library.len(), 2, "b went to Recently Deleted: {library:?}");
    assert_eq!(s.catalog.photos().find(|p| p.file_name == "a.png").unwrap().rating, 5, "the sidecar was read");
    let undo = s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(undo["undone"], "Synchronize Folder");
    assert_eq!(holdings(&s), before, "one undo step puts everything back");
}

#[test]
fn a_folder_with_no_changes_synchronizes_without_an_undo_step() {
    let dir = Scratch::new("sync-none");
    let mut s = library(&dir);
    let steps = s.undo.len();
    let r = s.execute("folder.synchronize", &json!({"path": dir.path("trip"), "removeMissing": true, "readMetadata": true})).unwrap();
    assert_eq!((r["imported"].as_u64(), r["removed"].as_u64(), r["read"].as_u64()), (Some(0), Some(0), Some(0)), "{r}");
    assert_eq!(s.undo.len(), steps);
}

#[test]
fn only_a_folder_of_the_library_is_synchronized() {
    let dir = Scratch::new("refused");
    let mut s = library(&dir);
    std::fs::create_dir_all(dir.0.join("elsewhere")).unwrap();
    for (cmd, p, why) in [
        ("folder.scanChanges", json!({}), "missing path"),
        ("folder.scanChanges", json!({"path": "  "}), "blank path"),
        ("folder.scanChanges", json!({"path": dir.path("elsewhere")}), "no photo was imported from it"),
        ("folder.scanChanges", json!({"path": "/"}), "the startup disk"),
        ("folder.synchronize", json!({"path": dir.path("elsewhere")}), "no photo was imported from it"),
        ("folder.synchronize", json!({"path": "/"}), "the startup disk"),
    ] {
        assert!(s.execute(cmd, &p).is_err(), "{cmd}: {why}");
    }
}

#[test]
fn synchronizing_acts_on_the_changes_just_scanned() {
    let dir = Scratch::new("cached");
    let mut s = library(&dir);
    write_png(&dir.path("trip/c.png"), 3);
    let r = scan(&mut s, &dir.path("trip"));
    assert_eq!(paths(&r, "new").len(), 1);
    // arrived after the scan the person was shown: not part of what they agreed to
    write_png(&dir.path("trip/d.png"), 4);
    let r = s.execute("folder.synchronize", &json!({"path": dir.path("trip")})).unwrap();
    assert_eq!(r["imported"], 1, "{r}");
    // the scan is used once; the next synchronize looks again
    let r = s.execute("folder.synchronize", &json!({"path": dir.path("trip")})).unwrap();
    assert_eq!(r["imported"], 1, "d.png now: {r}");
}

#[test]
fn a_scan_is_stale_once_the_library_changed() {
    let dir = Scratch::new("stale");
    let mut s = library(&dir);
    write_png(&dir.path("trip/c.png"), 3);
    scan(&mut s, &dir.path("trip"));
    // c.png comes in some other way between the scan and the synchronize
    s.execute("library.import", &json!({"paths": [dir.path("trip/c.png")]})).unwrap();
    let r = s.execute("folder.synchronize", &json!({"path": dir.path("trip")})).unwrap();
    assert_eq!(r["imported"], 0, "the library changed: it looked again: {r}");
}
