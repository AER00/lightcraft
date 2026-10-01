//! Organizing commands: smart albums, stacks, virtual copies — through `Session::execute`, with
//! undo and a persistent library (journal replay must reproduce the live state).

use lightcraft_catalog::Flag;
use serde_json::json;

use crate::{LibrarySource, Session};

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("lc-organize-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn open(dir: &std::path::Path) -> Session {
    let mut s = Session::new();
    s.open_library(dir, true).unwrap();
    s
}

#[test]
fn smart_album_from_view_updates_live_and_persists() {
    let dir = temp_dir("smart");
    let mut s = open(&dir);
    s.execute("library.filter", &json!({"rating": 4})).unwrap();
    let want = s.visible_cloned();
    assert!(!want.is_empty());
    let r = s.execute("album.createSmart", &json!({"name": "Four plus"})).unwrap();
    let id = r["id"].as_u64().unwrap();
    assert_eq!(r["count"].as_u64().unwrap() as usize, want.len());
    s.execute("library.clearFilter", &json!({})).unwrap();
    s.execute("library.source", &json!({"kind": "album", "id": id})).unwrap();
    let mut got = s.visible_cloned();
    got.sort();
    let mut w = want.clone();
    w.sort();
    assert_eq!(got, w);
    // live: rating another photo 5 adds it; rating a member 1 removes it
    let other = s.catalog.photos().find(|p| p.rating < 4 && !p.deleted).unwrap().id;
    s.execute("photo.rate", &json!({"ids": [other.0], "rating": 5})).unwrap();
    assert!(s.visible_cloned().contains(&other));
    s.execute("photo.rate", &json!({"ids": [want[0].0], "rating": 1})).unwrap();
    assert!(!s.visible_cloned().contains(&want[0]));
    // manual membership changes are refused
    assert!(s.execute("album.addPhotos", &json!({"id": id, "ids": [want[0].0]})).is_err());
    // edit rules (merge), rename; undo the rename
    s.execute("album.setRules", &json!({"id": id, "rules": {"flag": "pick"}})).unwrap();
    let rules = s.catalog.album(lightcraft_catalog::AlbumId(id)).unwrap().smart.clone().unwrap();
    assert_eq!((rules.rating, rules.flag), (4, Some(Flag::Pick)));
    s.execute("album.rename", &json!({"id": id, "name": "Best picks"})).unwrap();
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.catalog.album(lightcraft_catalog::AlbumId(id)).unwrap().name, "Four plus");
    // rules via params: camera + lens + date range
    let r2 = s
        .execute("album.createSmart", &json!({"name": "Range", "rules": {"dateFrom": "2000-01-01", "dateTo": "2100", "lens": "", "edited": true}}))
        .unwrap();
    let edited = s.catalog.photos().filter(|p| !p.deleted && p.is_edited()).count();
    assert_eq!(r2["count"].as_u64().unwrap() as usize, edited);
    let listed = s.execute("albums.list", &json!({})).unwrap();
    assert!(listed.to_string().contains("\"rulesText\""), "{listed}");
    let expect = s.catalog.to_snapshot();
    drop(s); // crash: replay the log
    let mut s2 = open(&dir);
    assert_eq!(s2.catalog.to_snapshot(), expect);
    // deleting the smart album
    s2.execute("album.delete", &json!({"id": id})).unwrap();
    assert!(s2.catalog.album(lightcraft_catalog::AlbumId(id)).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn smart_album_from_smart_album_view_keeps_its_rules() {
    let mut s = Session::with_demo();
    let id = s.execute("album.createSmart", &json!({"name": "Picks", "rules": {"flag": "pick"}})).unwrap()["id"].as_u64().unwrap();
    s.execute("library.source", &json!({"kind": "album", "id": id})).unwrap();
    s.execute("library.filter", &json!({"rating": 3})).unwrap();
    let r = s.view_rules();
    assert_eq!((r.flag, r.rating, r.album), (Some(Flag::Pick), 3, None));
    assert_eq!(s.source, LibrarySource::Album(lightcraft_catalog::AlbumId(id)));
}
