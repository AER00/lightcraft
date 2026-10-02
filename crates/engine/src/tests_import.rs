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
