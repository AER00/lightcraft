//! Import: recursive folders, duplicate detection by content, add vs copy into the library.

use std::path::Path;

use serde_json::{Value, json};

use crate::Session;

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("lc-import-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A small procedural PNG (distinct per seed).
fn write_png(path: &Path, seed: u8) {
    let (w, h) = (48usize, 32usize);
    let data: Vec<[u8; 4]> = (0..w * h).map(|i| [(i % w * 5) as u8, (i / w * 7) as u8, seed, 255]).collect();
    let img = lightcraft_raster::Rgba8 { width: w, height: h, data };
    let bytes = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&img), &lightcraft_codecs::EncodeMeta::default()).unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn ids(v: &Value, key: &str) -> usize {
    v[key].as_array().map(Vec::len).unwrap_or(0)
}

#[test]
fn recursive_import_with_duplicates() {
    let src = temp_dir("src");
    write_png(&src.join("a.png"), 1);
    write_png(&src.join("trip/b.png"), 2);
    write_png(&src.join("trip/day2/c.PNG"), 3);
    std::fs::copy(src.join("a.png"), src.join("trip/a-copy.png")).unwrap(); // same bytes
    write_png(&src.join(".hidden/x.png"), 9);
    std::fs::write(src.join("notes.txt"), "not a photo").unwrap();
    std::fs::write(src.join("trip/broken.jpg"), "garbage").unwrap();

    let mut s = Session::new().with_fs();
    let r = s.execute("library.import", &json!({"paths": [src.to_string_lossy()]})).unwrap();
    assert_eq!(r["scanned"], 5, "{r}"); // a, b, c, a-copy, broken (hidden + txt skipped)
    assert_eq!(ids(&r, "imported"), 3, "{r}");
    assert_eq!(ids(&r, "duplicates"), 1, "{r}");
    assert_eq!(r["duplicates"][0]["reason"], "content");
    assert_eq!(ids(&r, "failed"), 1, "{r}");
    assert_eq!(s.catalog.len(), 3);
    assert!(s.catalog.photos().all(|p| p.content_hash.as_ref().is_some_and(|h| h.len() == 32) && p.width == 48));

    // importing again: everything is a duplicate (by path), one undo step for the first import
    let r2 = s.execute("library.import", &json!({"paths": [src.to_string_lossy()]})).unwrap();
    assert_eq!(ids(&r2, "imported"), 0);
    assert_eq!(r2["duplicates"].as_array().unwrap().iter().filter(|d| d["reason"] == "path").count(), 3);
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.catalog.len(), 0);
    let _ = std::fs::remove_dir_all(&src);
}

#[test]
fn copy_into_library_and_persist() {
    let src = temp_dir("copysrc");
    let lib = temp_dir("copylib");
    write_png(&src.join("one.png"), 4);
    write_png(&src.join("sub/one.png"), 5); // same name, different content
    let mut s = Session::new().with_fs();
    assert!(s.execute("library.import", &json!({"paths": [src.to_string_lossy()], "mode": "copy"})).is_err(), "copy needs a library");
    s.open_library(&lib, false).unwrap();
    let r = s.execute("library.import", &json!({"paths": [src.to_string_lossy()], "mode": "copy"})).unwrap();
    assert_eq!(ids(&r, "imported"), 2, "{r}");
    let paths: Vec<String> = s
        .catalog
        .photos()
        .map(|p| match &p.source {
            lightcraft_catalog::Source::File { path } => path.clone(),
            _ => panic!(),
        })
        .collect();
    for p in &paths {
        assert!(Path::new(p).starts_with(lib.join("Originals")), "{p}");
        assert!(Path::new(p).exists());
    }
    assert_ne!(paths[0], paths[1], "unique names");
    // originals deleted from the source: the library copies remain usable after a restart
    std::fs::remove_dir_all(&src).unwrap();
    drop(s);
    let mut s = Session::new().with_fs();
    s.open_library(&lib, false).unwrap();
    assert_eq!(s.catalog.len(), 2);
    let id = s.catalog.photos().next().unwrap().id;
    assert!(s.render_now(id, 32, 32).is_ok());
    // the library folder itself is never re-imported
    let r = s.execute("library.import", &json!({"paths": [lib.to_string_lossy()]})).unwrap();
    assert_eq!(r["scanned"], 0, "{r}");
    let _ = std::fs::remove_dir_all(&lib);
}

#[test]
fn civil_dates() {
    assert_eq!(crate::import::civil(0), "1970-01-01T00:00:00");
    assert_eq!(crate::import::civil(951_782_400), "2000-02-29T00:00:00");
    assert_eq!(crate::import::civil(1_790_000_000), "2026-09-21T14:13:20");
}

#[test]
fn browsing_a_folder_lists_its_photos_without_adding_them() {
    use crate::LibrarySource;
    let dir = std::env::temp_dir().join(format!("lc-browse-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    let png = |p: &std::path::Path, seed: u8| {
        let img = lightcraft_raster::Rgba8::from_fn(16, 12, |x, y| [(x * 9) as u8, (y * 11) as u8, seed, 255]);
        let b = crate::export::encode_image(&img, &crate::export::ExportOptions { format: crate::export::ExportFormat::Png, ..Default::default() })
            .unwrap();
        std::fs::write(p, b).unwrap();
    };
    png(&dir.join("a.png"), 1);
    png(&dir.join("b.png"), 2);
    png(&dir.join("sub/c.png"), 3);
    std::fs::write(dir.join("notes.txt"), "x").unwrap();
    let mut s = Session::new().with_fs();
    let r = s.execute("library.browse", &serde_json::json!({"path": dir.to_string_lossy()})).unwrap();
    assert_eq!(r["photos"], 2, "{r}");
    assert_eq!(s.source, LibrarySource::Folder);
    assert_eq!(s.visible_cloned().len(), 2);
    // not in the library
    s.execute("library.source", &serde_json::json!({"kind": "all"})).unwrap();
    assert!(s.visible_cloned().is_empty(), "browsed photos stay out of All Photos");
    assert_eq!(s.execute("catalog.stats", &serde_json::json!({})).unwrap()["photos"], 0, "nor in the counts");
    assert!(s.catalog.date_groups().is_empty());
    // subfolders; browsing again reuses the photos
    let r = s.execute("library.browse", &serde_json::json!({"path": dir.to_string_lossy(), "subfolders": true})).unwrap();
    assert_eq!((r["photos"].as_u64(), r["new"].as_u64()), (Some(3), Some(1)));
    // add one to the library
    let first = s.visible_cloned()[0];
    s.execute("photo.addToLibrary", &serde_json::json!({"ids": [first.0]})).unwrap();
    // importing the folder for real brings in the rest (no duplicates of the browsed ones)
    let r = s.execute("library.import", &serde_json::json!({"paths": [dir.to_string_lossy()]})).unwrap();
    assert_eq!(r["imported"].as_array().map(Vec::len), Some(2), "{r}");
    s.execute("library.source", &serde_json::json!({"kind": "all"})).unwrap();
    assert_eq!(s.visible_cloned().len(), 3);
    assert_eq!(s.catalog.photos().count(), 3);
    assert!(s.execute("library.browse", &serde_json::json!({"path": dir.join("a.png").to_string_lossy()})).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn missing_files_are_found_and_relinked() {
    let dir = std::env::temp_dir().join(format!("lc-missing-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("old")).unwrap();
    std::fs::create_dir_all(dir.join("moved/deeper")).unwrap();
    let png = |p: &std::path::Path, seed: u8| {
        let img = lightcraft_raster::Rgba8::from_fn(16, 12, |x, y| [(x * 9) as u8, (y * 11) as u8, seed, 255]);
        let b = crate::export::encode_image(&img, &crate::export::ExportOptions { format: crate::export::ExportFormat::Png, ..Default::default() })
            .unwrap();
        std::fs::write(p, b).unwrap();
    };
    png(&dir.join("old/a.png"), 1);
    png(&dir.join("old/b.png"), 2);
    let mut s = Session::new().with_fs();
    s.execute("library.import", &serde_json::json!({"paths": [dir.join("old").to_string_lossy()]})).unwrap();
    assert_eq!(s.execute("library.missing", &serde_json::json!({})).unwrap().as_array().map(Vec::len), Some(0));
    // the files move away
    std::fs::rename(dir.join("old/a.png"), dir.join("moved/a.png")).unwrap();
    std::fs::rename(dir.join("old/b.png"), dir.join("moved/deeper/b.png")).unwrap();
    let m = s.execute("library.missing", &serde_json::json!({})).unwrap();
    assert_eq!(m.as_array().map(Vec::len), Some(2));
    s.execute("library.source", &serde_json::json!({"kind": "missing"})).unwrap();
    assert_eq!(s.visible_cloned().len(), 2, "the Missing Photos source lists them");
    s.execute("library.source", &serde_json::json!({"kind": "all"})).unwrap();
    // one by hand
    let id = m[0]["id"].as_u64().unwrap();
    let name = std::path::Path::new(m[0]["path"].as_str().unwrap()).file_name().unwrap().to_string_lossy().to_string();
    let new = if name == "a.png" { dir.join("moved/a.png") } else { dir.join("moved/deeper/b.png") };
    s.execute("photo.relink", &serde_json::json!({"id": id, "path": new.to_string_lossy()})).unwrap();
    assert_eq!(s.execute("library.missing", &serde_json::json!({})).unwrap().as_array().map(Vec::len), Some(1));
    assert!(s.execute("photo.relink", &serde_json::json!({"id": id, "path": dir.join("nope.png").to_string_lossy()})).is_err());
    // the rest by searching a folder
    let r = s.execute("library.findMissing", &serde_json::json!({"folder": dir.join("moved").to_string_lossy()})).unwrap();
    assert_eq!((r["found"].as_array().map(Vec::len), r["missing"].as_u64()), (Some(1), Some(0)), "{r}");
    // undo points the photo back at the old path but doesn't move any file
    s.execute("edit.undo", &serde_json::json!({})).unwrap();
    assert_eq!(s.execute("library.missing", &serde_json::json!({})).unwrap().as_array().map(Vec::len), Some(1));
    assert!(dir.join("moved/a.png").exists() && dir.join("moved/deeper/b.png").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn recently_added_covers_recent_imports_newest_first() {
    let dir = std::env::temp_dir().join(format!("lc-recent-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let png = |name: &str, seed: u8| {
        let img = lightcraft_raster::Rgba8::from_fn(16, 12, |x, y| [(x * 9) as u8, (y * 11) as u8, seed, 255]);
        let b = crate::export::encode_image(&img, &crate::export::ExportOptions { format: crate::export::ExportFormat::Png, ..Default::default() })
            .unwrap();
        std::fs::write(dir.join(name), b).unwrap();
        dir.join(name).to_string_lossy().to_string()
    };
    let mut s = Session::new().with_fs();
    // three imports: 60 days ago, 10 days ago, today
    for (when, name, seed) in [("2026-08-01T09:00:00", "old.png", 1), ("2026-09-20T09:00:00", "mid.png", 2), ("2026-09-30T09:00:00", "new.png", 3)] {
        let path = png(name, seed);
        let w = when.to_string();
        s.clock = Box::new(move || w.clone());
        s.execute("library.import", &serde_json::json!({"paths": [path]})).unwrap();
    }
    s.execute("library.source", &serde_json::json!({"kind": "recentlyAdded"})).unwrap();
    let names: Vec<String> = s.visible_cloned().iter().map(|id| s.catalog.photo(*id).unwrap().file_name.clone()).collect();
    assert_eq!(names, ["new.png", "mid.png"], "the last 30 days, newest import first");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Copy imports: a destination folder, flat / by-month folders, renamed copies (numbered in
/// import order), and a metadata preset on every photo.
#[test]
fn copy_with_destination_organize_rename_and_metadata_preset() {
    let src = temp_dir("orgsrc");
    let dest = temp_dir("orgdest");
    write_png(&src.join("a.png"), 1);
    write_png(&src.join("b.png"), 2);
    let mut s = Session::new().with_fs();
    s.execute("metadata.savePreset", &json!({"name": "Studio", "fields": {"copyright": "© Studio", "creator": "Sam"}})).unwrap();
    assert!(s.execute("library.import", &json!({"paths": [src.to_string_lossy()], "metadataPreset": "Nope"})).is_err());
    assert!(s.execute("library.import", &json!({"paths": [src.to_string_lossy()], "mode": "copy", "organize": "weekly"})).is_err());
    let r = s
        .execute(
            "library.import",
            &json!({"paths": [src.to_string_lossy()], "mode": "copy", "destination": dest.to_string_lossy(), "organize": "flat",
                    "rename": "Shoot-{seq:3}", "renameStart": 7, "metadataPreset": "Studio"}),
        )
        .unwrap();
    assert_eq!(ids(&r, "imported"), 2, "{r}");
    let mut names: Vec<String> = s.catalog.photos().map(|p| p.file_name.clone()).collect();
    names.sort();
    assert_eq!(names, ["Shoot-007.png", "Shoot-008.png"], "catalogued under the new names");
    assert!(dest.join("Shoot-007.png").exists() && dest.join("Shoot-008.png").exists(), "flat: straight into the destination");
    assert!(s.catalog.photos().all(|p| p.meta.copyright == "© Studio" && p.meta.creator == "Sam"));
    // by month, no library needed when a destination is given
    let dest2 = temp_dir("orgdest2");
    write_png(&src.join("c.png"), 3);
    let r = s
        .execute(
            "library.import",
            &json!({"paths": [src.join("c.png").to_string_lossy()], "mode": "copy", "destination": dest2.to_string_lossy(), "organize": "month"}),
        )
        .unwrap();
    assert_eq!(ids(&r, "imported"), 1, "{r}");
    let p = s.catalog.photos().find(|p| p.file_name == "c.png").unwrap();
    let lightcraft_catalog::Source::File { path } = &p.source else { panic!() };
    let rel = Path::new(path).strip_prefix(&dest2).unwrap();
    assert_eq!(rel.components().count(), 3, "YYYY/YYYY-MM/c.png: {rel:?}");
    assert_eq!(rel.parent().unwrap().file_name().unwrap().len(), 7);
    for d in [&src, &dest, &dest2] {
        let _ = std::fs::remove_dir_all(d);
    }
}

/// Auto import: files in the watched folder are added once their size held between two scans,
/// into the named album; non-photos are tried once; the selection stays put.
#[test]
fn auto_import_watched_folder() {
    let dir = temp_dir("watch");
    let mut s = Session::new().with_fs();
    assert!(s.execute("library.autoImport", &json!({"folder": dir.join("nope").to_string_lossy()})).is_err());
    s.execute("library.autoImport", &json!({"folder": dir.to_string_lossy(), "album": "Tethered"})).unwrap();
    assert_eq!(s.execute("library.autoImportScan", &json!({})).unwrap()["imported"], json!([]));
    write_png(&dir.join("one.png"), 1);
    std::fs::write(dir.join("notes.txt"), "not a photo").unwrap();
    // first sight: wait (it may still be copying)
    assert_eq!(s.execute("library.autoImportScan", &json!({})).unwrap()["imported"], json!([]));
    let r = s.execute("library.autoImportScan", &json!({})).unwrap();
    assert_eq!(r["imported"].as_array().unwrap().len(), 1, "{r}");
    let album = s.catalog.albums().find(|a| a.name == "Tethered").expect("album").id;
    assert_eq!(s.catalog.album_count(album), 1);
    assert!(s.selection.ids.is_empty(), "arrivals don't take the selection");
    // nothing new: nothing happens, and the text file isn't retried
    for _ in 0..2 {
        assert_eq!(s.execute("library.autoImportScan", &json!({})).unwrap()["imported"], json!([]));
    }
    write_png(&dir.join("two.png"), 2);
    s.execute("library.autoImportScan", &json!({})).unwrap();
    s.execute("library.autoImportScan", &json!({})).unwrap();
    assert_eq!(s.catalog.album_count(album), 2);
    s.execute("library.autoImport", &json!({"folder": null})).unwrap();
    assert_eq!(s.execute("library.autoImportScan", &json!({})).unwrap()["folder"], json!(null));
    let _ = std::fs::remove_dir_all(&dir);
}
