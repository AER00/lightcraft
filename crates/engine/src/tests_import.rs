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
