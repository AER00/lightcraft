//! Persistent library: commands survive a restart (with and without a clean close).

use serde_json::json;

use crate::Session;

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("lc-engine-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn open(dir: &std::path::Path, seed: bool) -> Session {
    let mut s = Session::new();
    s.open_library(dir, seed).unwrap();
    s
}

#[test]
fn edits_survive_restart_without_close() {
    let dir = temp_dir("crash");
    let mut s = open(&dir, true);
    assert!(s.library.as_ref().unwrap().report.created);
    assert!(s.catalog.len() > 5, "seeded demo");
    let id = s.selection.active.unwrap();
    s.execute("photo.rate", &json!({"rating": 5})).unwrap();
    s.execute("photo.flag", &json!({"flag": "reject"})).unwrap();
    s.execute("develop.set", &json!({"control": "light.exposure", "value": 1.25})).unwrap();
    // a slider drag: one undo step, logged once at the end
    s.execute("develop.beginInteraction", &json!({"label": "Contrast"})).unwrap();
    for v in [5, 10, 20, 33] {
        s.execute("develop.set", &json!({"control": "light.contrast", "value": v})).unwrap();
    }
    s.execute("develop.endInteraction", &json!({})).unwrap();
    s.execute("album.create", &json!({"name": "Keepers", "addSelected": true})).unwrap();
    s.execute("preset.create", &json!({"name": "Bright"})).unwrap();
    s.execute("edit.undo", &json!({})).ok();
    s.execute("edit.redo", &json!({})).ok();
    let expect = s.catalog.to_snapshot();
    let info = s.execute("library.info", &json!({})).unwrap();
    assert!(info["logRecords"].as_u64().unwrap() >= 5, "{info}");
    drop(s); // no close: like a crash

    let s2 = open(&dir, true);
    let r = &s2.library.as_ref().unwrap().report;
    assert!(!r.created && r.replayed >= 5, "{r:?}");
    assert_eq!(s2.catalog.to_snapshot(), expect);
    let p = s2.catalog.photo(id).unwrap();
    assert_eq!((p.rating, p.develop.light.exposure, p.develop.light.contrast), (5, 1.25, 33.0));
    assert!(s2.catalog.albums().any(|a| a.name == "Keepers" && a.photos == vec![id]));
    assert!(s2.presets.iter().any(|p| p.name == "Bright" && !p.builtin));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn close_writes_snapshot_and_view_state() {
    let dir = temp_dir("close");
    let mut s = open(&dir, true);
    let ids = s.visible_cloned();
    s.execute("library.select", &json!({"ids": [ids[3].0]})).unwrap();
    s.execute("photo.rate", &json!({"rating": 2})).unwrap();
    let expect = s.catalog.to_snapshot();
    s.close_library().unwrap();
    assert_eq!(std::fs::metadata(dir.join("catalog.log")).unwrap().len(), 0);

    let s2 = open(&dir, true);
    let r = &s2.library.as_ref().unwrap().report;
    assert_eq!(r.replayed, 0);
    assert_eq!(s2.catalog.to_snapshot(), expect);
    assert_eq!(s2.selection.active, Some(ids[3]));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Thumbnails rendered in one session are served from the disk cache in the next, without
/// loading the source; an edit changes the key.
#[test]
fn thumbnails_come_from_the_disk_cache_after_restart() {
    let dir = temp_dir("thumbs");
    let mut s = open(&dir, true);
    let ids: Vec<_> = s.visible_cloned().into_iter().take(3).collect();
    for id in &ids {
        let job = s.thumb_job(*id, 230).unwrap();
        assert_eq!(job.request.max_w, 256, "bucketed");
        let r = job.run();
        assert!(r.loaded.is_some(), "first render decodes the source");
        s.accept(&r);
        let img = r.rendered.unwrap().image;
        assert_eq!(img.width.max(img.height), 256);
    }
    drop(s);

    let mut s = open(&dir, true);
    for id in &ids {
        let r = s.thumb_job(*id, 240).unwrap().run();
        assert!(r.loaded.is_none(), "served from cache");
        let img = r.rendered.unwrap().image;
        assert_eq!(img.width.max(img.height), 256);
    }
    let info = s.execute("library.info", &json!({})).unwrap();
    assert_eq!(info["cache"]["disk"]["hits"], 3, "{info}");
    // an edit is a new key: rendered (from the source), then cached
    s.execute("library.select", &json!({"ids": [ids[0].0]})).unwrap();
    s.execute("develop.set", &json!({"control": "light.exposure", "value": 0.5})).unwrap();
    let r = s.thumb_job(ids[0], 230).unwrap().run();
    assert!(r.loaded.is_some());
    // exact-size renders (loupe, before/after, exports) bypass the thumbnail cache
    assert!(s.render_job(ids[0], 230, 230, true, true).unwrap().cache.is_none());
    assert!(s.render_job(ids[0], 230, 230, false, true).unwrap().cache.is_none());
    s.execute("library.clearPreviews", &json!({})).unwrap();
    assert!(s.thumb_job(ids[1], 230).unwrap().run().loaded.is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn new_library_without_seed_is_empty_and_compacts() {
    let dir = temp_dir("empty");
    let mut s = open(&dir, false);
    assert!(s.catalog.is_empty());
    s.execute("album.create", &json!({"name": "A"})).unwrap();
    s.execute("library.compact", &json!({})).unwrap();
    let info = s.execute("library.info", &json!({})).unwrap();
    assert_eq!(info["logRecords"], 0);
    assert_eq!(info["snapshotSeq"], info["seq"]);
    let s2 = open(&dir, true);
    assert_eq!(s2.catalog.len(), 0, "an existing library is never seeded");
    assert_eq!(s2.catalog.albums().count(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn profile_favorites_and_recent_survive_reopen() {
    let dir = temp_dir("profiles");
    let mut s = open(&dir, true);
    s.execute("profile.favorite", &json!({"id": "lc.bw.sepia"})).unwrap();
    s.execute("profile.favorite", &json!({"id": "lc.vivid"})).unwrap();
    s.execute("profile.favorite", &json!({"id": "lc.vivid"})).unwrap(); // toggled off again
    assert!(s.execute("profile.favorite", &json!({"id": "nope"})).is_err());
    for id in ["lc.neutral", "lc.vivid", "lc.film.cool-fade", "lc.mono", "lc.cine.teal-amber", "lc.muted.bleached", "lc.vivid"] {
        s.execute("develop.profile", &json!({"id": id})).unwrap();
    }
    let want_recent = ["lc.vivid", "lc.muted.bleached", "lc.cine.teal-amber", "lc.mono", "lc.film.cool-fade"];
    assert_eq!(s.profile_recent, want_recent, "newest first, deduplicated, at most 5");
    drop(s); // no clean close: every command persisted already
    let mut s = open(&dir, false);
    assert_eq!(s.profile_favorites, ["lc.bw.sepia"]);
    assert_eq!(s.profile_recent, want_recent);
    let menu = s.execute("profiles.menu", &json!({})).unwrap();
    assert_eq!(menu["favorites"][0]["name"], "Sepia Tone");
    assert_eq!(menu["recent"].as_array().unwrap().len(), 5);
    assert_eq!(menu["groups"][0]["name"], "Basic");
    let list = s.execute("profiles.list", &json!({})).unwrap();
    assert!(list.as_array().unwrap().iter().any(|p| p["id"] == "lc.bw.sepia" && p["favorite"] == true));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Smart albums with a rule set: all / any / none, nested groups, validation, live updates.
#[test]
fn smart_album_rule_sets() {
    let mut s = crate::Session::with_demo();
    s.clock = Box::new(|| "2026-10-02T12:00:00".to_string());
    let fields = s.execute("album.ruleFields", &serde_json::json!({})).unwrap();
    assert!(fields.as_array().unwrap().iter().any(|f| f["field"] == "keywords" && f["ops"].as_array().unwrap().len() > 3));
    let bad = s.execute(
        "album.createSmart",
        &serde_json::json!({"name": "Bad", "rules": {"ruleSet": {"rules": [{"field": "rating", "op": "contains", "value": 1}]}}}),
    );
    assert!(bad.is_err(), "operators are checked");
    let r = s
        .execute(
            "album.createSmart",
            &serde_json::json!({"name": "Good ones", "rules": {"ruleSet": {"match": "all", "rules": [
                {"field": "rating", "op": "gte", "value": 4},
                {"group": {"match": "none", "rules": [{"field": "flag", "op": "is", "value": "reject"}]}}
            ]}}}),
        )
        .unwrap();
    let id = lightcraft_catalog::AlbumId(r["id"].as_u64().unwrap());
    let want = s.catalog.photos().filter(|p| !p.deleted && p.rating >= 4 && p.flag != lightcraft_catalog::Flag::Reject).count();
    assert!(want > 0);
    assert_eq!(s.catalog.album_count(id), want);
    // live: a new 5-star photo joins
    let other = s.catalog.photos().find(|p| p.rating < 4 && !p.deleted).unwrap().id;
    s.execute("photo.rate", &serde_json::json!({"ids": [other.0], "rating": 5})).unwrap();
    assert_eq!(s.catalog.album_count(id), want + 1);
    // the same rules work as a library filter (agents: library.filter {ruleSet})
    s.execute("library.filter", &serde_json::json!({"ruleSet": {"rules": [{"field": "rating", "op": "is", "value": 5}]}})).unwrap();
    assert!(s.visible_cloned().iter().all(|id| s.catalog.photo(*id).unwrap().rating == 5));
    let list = s.execute("albums.list", &serde_json::json!({})).unwrap();
    let a = list.as_array().unwrap().iter().find(|a| a["name"] == "Good ones").unwrap().clone();
    assert!(a.to_string().contains("rating is ≥ 4"), "{a}");
}

/// The filter bar's label multi-select: any of the chosen labels.
#[test]
fn filter_any_of_several_labels() {
    use lightcraft_catalog::ColorLabel;
    let mut s = crate::Session::with_demo();
    let ids: Vec<u64> = s.catalog.photos().take(3).map(|p| p.id.0).collect();
    for (id, l) in ids.iter().zip(["red", "yellow", "green"]) {
        s.execute("photo.label", &serde_json::json!({"ids": [id], "label": l})).unwrap();
    }
    s.execute("library.filter", &serde_json::json!({"labels": ["red", "yellow"]})).unwrap();
    let vis = s.visible_cloned();
    let want = s.catalog.photos().filter(|p| p.in_library() && matches!(p.label, Some(ColorLabel::Red | ColorLabel::Yellow))).count();
    assert_eq!(want, 2);
    assert_eq!(vis.len(), want);
    assert!(vis.iter().all(|id| matches!(s.catalog.photo(*id).unwrap().label, Some(ColorLabel::Red | ColorLabel::Yellow))));
    assert!(s.filter.describe().contains("label red or yellow"));
    s.execute("library.filter", &serde_json::json!({"labels": []})).unwrap();
    assert!(s.visible_cloned().len() > want);
}

/// Quick Collection / target album: B toggles the selection in it (created on first use);
/// another album can be the target; clear empties it; all undoable.
#[test]
fn quick_collection_and_target_album() {
    let mut s = crate::Session::with_demo();
    let ids: Vec<u64> = s.catalog.photos().take(3).map(|p| p.id.0).collect();
    assert!(s.catalog.quick_collection().is_none());
    s.execute("library.select", &serde_json::json!({"ids": [ids[0], ids[1]]})).unwrap();
    let r = s.execute("album.toggleTarget", &serde_json::json!({})).unwrap();
    assert_eq!((r["added"].as_bool(), r["count"].as_u64(), r["name"].as_str()), (Some(true), Some(2), Some("Quick Collection")));
    let quick = s.catalog.quick_collection().expect("created");
    // again: both are in it, so they come out
    assert_eq!(s.execute("album.toggleTarget", &serde_json::json!({})).unwrap()["count"], 0);
    s.execute("edit.undo", &serde_json::json!({})).unwrap();
    assert_eq!(s.catalog.album_count(quick), 2);
    // a regular album as the target
    let alb = s.execute("album.create", &serde_json::json!({"name": "Picks"})).unwrap()["id"].as_u64().unwrap();
    s.execute("album.setTarget", &serde_json::json!({"id": alb})).unwrap();
    s.execute("album.toggleTarget", &serde_json::json!({"ids": [ids[2]]})).unwrap();
    assert_eq!(s.catalog.album_count(lightcraft_catalog::AlbumId(alb)), 1);
    assert_eq!(s.catalog.album_count(quick), 2, "the Quick Collection is untouched");
    let smart = s.execute("album.createSmart", &serde_json::json!({"name": "S", "rules": {"rating": 3}})).unwrap()["id"].as_u64().unwrap();
    assert!(s.execute("album.setTarget", &serde_json::json!({"id": smart})).is_err());
    s.execute("album.setTarget", &serde_json::json!({"id": null})).unwrap();
    s.execute("album.clearQuick", &serde_json::json!({})).unwrap();
    assert_eq!(s.catalog.album_count(quick), 0);
}
