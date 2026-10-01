//! Library management commands: date groups, keyword rename/delete/merge, batch rename, capture
//! time, label names, import review.

use serde_json::json;

use crate::Session;

/// Date headers follow the grid order and the sort's grouping.
#[test]
fn date_groups_follow_sort_and_grouping() {
    let mut s = Session::with_demo();
    let vis = s.visible_cloned();
    let g = s.execute("library.groups", &json!({})).unwrap();
    let groups = g.as_array().unwrap();
    assert!(groups.len() > 3, "{g}");
    let total: u64 = groups.iter().map(|x| x["count"].as_u64().unwrap()).sum();
    assert_eq!(total as usize, vis.len());
    // contiguous runs in grid order, labelled with weekday and date
    let mut next = 0;
    for x in groups {
        assert_eq!(x["start"].as_u64().unwrap(), next);
        next += x["count"].as_u64().unwrap();
        assert!(x["label"].as_str().unwrap().contains(", "), "{x}");
    }
    let months = s.execute("library.groups", &json!({"by": "month"})).unwrap();
    assert!(months.as_array().unwrap().len() < groups.len());
    assert_eq!(months[0]["key"].as_str().unwrap().len(), 7);
    s.execute("library.sort", &json!({"group": "none"})).unwrap();
    assert_eq!(s.execute("library.groups", &json!({})).unwrap(), json!([]));
    s.execute("library.sort", &json!({"group": "year", "key": "fileName"})).unwrap();
    assert_eq!(s.execute("library.groups", &json!({})).unwrap(), json!([]), "no date headers when sorting by name");
    assert_eq!(s.sort.group, lightcraft_catalog::GroupBy::Year);
    assert!(s.execute("library.sort", &json!({"group": "week"})).is_err());
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("lc-libops-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn keywords_of(s: &Session, id: u64) -> Vec<String> {
    s.catalog.photo(lightcraft_catalog::PhotoId(id)).unwrap().meta.keywords.clone()
}

/// Keyword rename / merge / delete: one undo step each, replayed from the op log after a restart,
/// and the keyword filter follows the renamed keyword.
#[test]
fn keyword_rename_merge_delete_undo_and_replay() {
    let dir = temp_dir("kw");
    let mut s = Session::new();
    s.open_library(&dir, true).unwrap();
    let ids: Vec<u64> = s.visible_cloned().iter().take(4).map(|p| p.0).collect();
    s.execute("photo.setMeta", &json!({"ids": [ids[0], ids[1]], "keywords": ["travel|italy|rome", "sea"]})).unwrap();
    s.execute("photo.setMeta", &json!({"ids": [ids[2]], "keywords": ["Italia"]})).unwrap();
    s.execute("photo.setMeta", &json!({"ids": [ids[3]], "keywords": ["travel|france", "sea"]})).unwrap();
    let tree = s.execute("keyword.list", &json!({})).unwrap();
    let travel = tree.as_array().unwrap().iter().find(|n| n["name"] == "travel").unwrap().clone();
    assert_eq!(travel["count"], 3, "{tree}");
    assert_eq!(travel["children"][1]["path"], "travel|italy");
    // filter by a parent keyword: its children match
    s.execute("library.filter", &json!({"keyword": "travel|italy"})).unwrap();
    assert_eq!(s.visible().len(), 2);
    let undo_before = s.undo.len();
    let r = s.execute("keyword.rename", &json!({"from": "travel|italy", "to": "Europe|Italy"})).unwrap();
    assert_eq!(r["changed"], 2);
    assert_eq!(s.undo.len(), undo_before + 1, "one undo step");
    assert_eq!(keywords_of(&s, ids[0]), ["Europe|Italy|rome", "sea"]);
    assert_eq!(s.filter.keyword.as_deref(), Some("Europe|Italy"), "the filter follows the rename");
    assert_eq!(s.visible().len(), 2);
    s.execute("keyword.merge", &json!({"from": ["Italia"], "into": "Europe|Italy"})).unwrap();
    assert_eq!(keywords_of(&s, ids[2]), ["Europe|Italy"]);
    assert_eq!(s.visible().len(), 3);
    s.execute("keyword.delete", &json!({"keyword": "travel"})).unwrap();
    assert_eq!(keywords_of(&s, ids[3]), ["sea"]);
    // suggestions: keywords used together with `sea` first
    let sug = s.execute("keyword.suggest", &json!({"ids": [ids[3]]})).unwrap();
    assert_eq!(sug[0], "Europe|Italy|rome", "{sug}");
    let sug = s.execute("keyword.suggest", &json!({"ids": [ids[3]], "prefix": "eur"})).unwrap();
    assert_eq!(sug, json!(["Europe|Italy|rome", "Europe|Italy"]), "most used first");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(keywords_of(&s, ids[3]), ["travel|france", "sea"]);
    s.execute("edit.redo", &json!({})).unwrap();
    // restart without a clean close: the log replays to the same state
    let expect = s.catalog.to_snapshot();
    drop(s);
    let mut s = Session::new();
    s.open_library(&dir, false).unwrap();
    assert_eq!(s.catalog.to_snapshot(), expect);
    // invalid
    assert!(s.execute("keyword.rename", &json!({"from": "sea", "to": ""})).is_err());
    assert!(s.execute("keyword.merge", &json!({"from": [], "into": "x"})).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}
