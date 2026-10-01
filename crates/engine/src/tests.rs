use serde_json::{Value, json};

use crate::{Session, command_specs};

fn demo() -> Session {
    Session::with_demo()
}

fn active_dev(s: &Session) -> lightcraft_develop::DevelopSettings {
    (*s.develop_of(s.active().unwrap()).unwrap()).clone()
}

#[test]
fn demo_library_loads() {
    let mut s = demo();
    assert_eq!(s.visible().len(), 24);
    assert!(s.active().is_some());
    let st = s.execute("catalog.stats", &json!({})).unwrap();
    assert_eq!(st["photos"], 24);
    let albums = s.execute("albums.list", &json!({})).unwrap();
    assert!(albums.as_array().unwrap().iter().any(|a| a["folder"] == true));
}

#[test]
fn rating_flag_undo_redo() {
    let mut s = demo();
    let id = s.active().unwrap();
    s.execute("photo.rate", &json!({"rating": 5})).unwrap();
    s.execute("photo.flag", &json!({"flag": "reject"})).unwrap();
    assert_eq!(s.catalog.photo(id).unwrap().rating, 5);
    s.execute("edit.undo", &json!({})).unwrap();
    assert_ne!(s.catalog.photo(id).unwrap().flag, lightcraft_catalog::Flag::Reject);
    s.execute("edit.redo", &json!({})).unwrap();
    assert_eq!(s.catalog.photo(id).unwrap().flag, lightcraft_catalog::Flag::Reject);
    assert!(s.execute("photo.rate", &json!({"rating": 7})).is_err());
}

#[test]
fn develop_set_and_interaction_coalesces() {
    let mut s = demo();
    s.execute("develop.set", &json!({"control": "light.exposure", "value": 0.5})).unwrap();
    assert_eq!(active_dev(&s).light.exposure, 0.5);
    let undo_before = s.undo.len();
    s.execute("develop.beginInteraction", &json!({"label": "Exposure"})).unwrap();
    for v in [0.6, 0.8, 1.2] {
        s.execute("develop.set", &json!({"control": "light.exposure", "value": v})).unwrap();
    }
    s.execute("develop.endInteraction", &json!({})).unwrap();
    assert_eq!(s.undo.len(), undo_before + 1, "a drag is one undo step");
    assert_eq!(active_dev(&s).light.exposure, 1.2);
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(active_dev(&s).light.exposure, 0.5);
    // unknown control and bad values are rejected
    assert!(s.execute("develop.set", &json!({"control": "nope", "value": 1})).is_err());
    assert!(s.execute("develop.set", &json!({"values": {"light.contrast": "x"}})).is_err());
    // clamped
    s.execute("develop.set", &json!({"values": {"light.contrast": 500}})).unwrap();
    assert_eq!(active_dev(&s).light.contrast, 100.0);
}

#[test]
fn copy_paste_sync_presets() {
    let mut s = demo();
    let vis = s.visible_cloned();
    s.execute("develop.set", &json!({"values": {"effects.clarity": 30, "light.shadows": 20}})).unwrap();
    s.execute("develop.copy", &json!({})).unwrap();
    s.execute("library.select", &json!({"ids": [vis[1].0, vis[2].0]})).unwrap();
    s.execute("develop.paste", &json!({})).unwrap();
    assert_eq!(s.develop_of(vis[2]).unwrap().effects.clarity, 30.0);
    s.execute("preset.apply", &json!({"id": "lc.bw-high-contrast", "amount": 100})).unwrap();
    assert_eq!(s.develop_of(vis[1]).unwrap().treatment, lightcraft_develop::Treatment::Bw);
    let r = s.execute("preset.create", &json!({"name": "Mine", "groups": ["light", "effects"]})).unwrap();
    assert!(r["id"].as_str().unwrap().starts_with("user."));
    assert!(s.execute("preset.delete", &json!({"id": "lc.moody"})).is_err());
}

#[test]
fn albums_crud() {
    let mut s = demo();
    let f = s.execute("album.create", &json!({"name": "Trips", "folder": true})).unwrap()["id"].as_u64().unwrap();
    let a = s.execute("album.create", &json!({"name": "Best", "parent": f, "addSelected": true})).unwrap()["id"].as_u64().unwrap();
    s.execute("library.source", &json!({"kind": "album", "id": a})).unwrap();
    assert_eq!(s.visible().len(), 1);
    s.execute("album.rename", &json!({"id": a, "name": "Best of"})).unwrap();
    s.execute("library.source", &json!({"kind": "all"})).unwrap();
    s.execute("library.selectAll", &json!({})).unwrap();
    s.execute("album.addPhotos", &json!({"id": a})).unwrap();
    assert_eq!(s.catalog.album(lightcraft_catalog::AlbumId(a)).unwrap().photos.len(), 24);
    assert!(s.execute("album.delete", &json!({"id": f})).is_err(), "non-empty folder");
    s.execute("album.delete", &json!({"id": a})).unwrap();
    s.execute("album.delete", &json!({"id": f})).unwrap();
}

#[test]
fn filters_and_delete_restore() {
    let mut s = demo();
    s.execute("library.filter", &json!({"rating": 4})).unwrap();
    let n = s.visible().len();
    assert!(n > 0 && n < 24);
    s.execute("library.clearFilter", &json!({})).unwrap();
    s.execute("photo.delete", &json!({})).unwrap();
    assert_eq!(s.visible().len(), 23);
    s.execute("library.source", &json!({"kind": "recentlyDeleted"})).unwrap();
    assert_eq!(s.visible().len(), 1);
    s.execute("photo.restore", &json!({})).unwrap();
    s.execute("library.source", &json!({"kind": "all"})).unwrap();
    assert_eq!(s.visible().len(), 24);
}

#[test]
fn masks_crop_and_render() {
    let mut s = demo();
    s.execute("mask.add", &json!({"kind": "radial", "center": [0.5, 0.5]})).unwrap();
    s.execute("mask.adjust", &json!({"values": {"exposure": 1.0}})).unwrap();
    s.execute("mask.brushStroke", &json!({"points": [[0.2, 0.2], [0.3, 0.3]], "size": 0.05})).unwrap();
    let d = active_dev(&s);
    assert_eq!(d.masks.len(), 1);
    assert_eq!(d.masks[0].components.len(), 2);
    s.execute("crop.aspect", &json!({"aspect": "1x1"})).unwrap();
    s.execute("crop.straighten", &json!({"angle": 5.0})).unwrap();
    let id = s.active().unwrap();
    let r = s.render_now(id, 200, 200).unwrap();
    assert_eq!((r.image.width, r.image.height), (200, 200));
    s.execute("develop.auto", &json!({})).unwrap();
    s.execute("develop.wb", &json!({"mode": "auto"})).unwrap();
    s.execute("develop.wbPick", &json!({"x": 0.5, "y": 0.5})).unwrap();
}

#[test]
fn versions_history_and_reset() {
    let mut s = demo();
    s.execute("develop.set", &json!({"control": "light.exposure", "value": 1.0})).unwrap();
    s.execute("version.create", &json!({"name": "bright"})).unwrap();
    s.execute("develop.reset", &json!({})).unwrap();
    assert!(active_dev(&s).is_unedited());
    s.execute("version.restore", &json!({"index": 0})).unwrap();
    assert_eq!(active_dev(&s).light.exposure, 1.0);
    let h = s.execute("history.list", &json!({})).unwrap();
    assert!(h["history"].as_array().unwrap().len() >= 3);
}

#[test]
fn op_log_replay_reproduces_catalog() {
    let mut s = demo();
    let snap = s.catalog.to_snapshot();
    let _ = s.drain_log();
    s.execute("photo.rate", &json!({"rating": 2})).unwrap();
    s.execute("develop.set", &json!({"control": "effects.dehaze", "value": 40})).unwrap();
    s.execute("album.create", &json!({"name": "X"})).unwrap();
    s.execute("edit.undo", &json!({})).unwrap();
    let log: String = s.drain_log().iter().map(lightcraft_catalog::Catalog::op_to_log_line).collect();
    let mut c = lightcraft_catalog::Catalog::from_snapshot(&snap).unwrap();
    c.replay(&log).unwrap();
    assert_eq!(c.to_snapshot(), s.catalog.to_snapshot());
}

/// Every command runs (or fails cleanly with an error) with empty params and has metadata.
#[test]
fn command_sweep() {
    let mut s = demo();
    let mut ids = std::collections::HashSet::new();
    for c in command_specs() {
        assert!(ids.insert(c.id), "duplicate {}", c.id);
        assert!(!c.label.is_empty() && !c.params.is_empty(), "{}", c.id);
        let _ = s.execute(c.id, &Value::Null);
    }
    assert!(s.commands().len() >= 70, "{}", s.commands().len());
}

#[test]
fn upright_and_guides_commands() {
    let mut s = demo();
    let r = s.execute("geometry.upright", &json!({"mode": "level"})).unwrap();
    assert_eq!(r["mode"], "level");
    let d = active_dev(&s);
    assert_eq!(d.geometry.upright, lightcraft_develop::Upright::Level);
    assert!(d.geometry.upright_transform.is_some(), "analysis result is stored");
    s.execute("geometry.upright", &json!({"mode": "off"})).unwrap();
    assert!(active_dev(&s).geometry.upright_transform.is_none());
    for i in 0..5 {
        let x = 0.1 + 0.15 * i as f64;
        s.execute("geometry.guides", &json!({"guides": [[x, 0.2, x + 0.02, 0.8]], "add": true})).unwrap();
    }
    let d = active_dev(&s);
    assert_eq!(d.geometry.upright, lightcraft_develop::Upright::Guided);
    assert_eq!(d.geometry.guides.len(), 4, "at most four guides");
    assert!((d.geometry.guides[0].0.x - 0.25).abs() < 1e-9, "oldest dropped");
    let id = s.active().unwrap();
    assert!(s.render_now(id, 160, 160).is_ok());
    assert!(s.execute("geometry.upright", &json!({"mode": "sideways"})).is_err());
}

#[test]
fn memory_report_counts_decoded_sources_and_renders() {
    let mut s = demo();
    let r = s.execute("library.memory", &json!({})).unwrap();
    assert_eq!(r["previewSources"]["count"], 0);
    assert!(r["gpu"]["allocated"].is_u64() && r.get("engineBytes").is_some());
    let id = s.active().unwrap();
    s.render_now(id, 800, 600).unwrap();
    let m = s.memory_report();
    assert_eq!(m.preview_sources.count, 1);
    // a 2560 px linear float source: 12 bytes per pixel
    assert!(m.preview_sources.bytes >= 12 * 2560 * 1000, "{:?}", m.preview_sources);
    assert_eq!(m.engine_bytes, m.thumb_sources.bytes + m.preview_sources.bytes + m.full_source.bytes + m.rendered.bytes);
    let r = s.thumb_job(id, 200).unwrap().run();
    s.accept(&r);
    let m = s.memory_report();
    assert_eq!(m.thumb_sources.count, 1);
    assert!(m.rendered.count >= 1, "the thumbnail render is cached");
}
