//! The keyword list's commands (Lightroom Classic's Keyword List): create, edit, move and purge
//! keywords, their attributes, and the default parent for new keywords.

use serde_json::json;

use crate::Session;

/// The keyword filter follows a renamed keyword whatever the case it was typed in, also where a
/// letter's case changes its length (ẞ / ß): it used to be cut by bytes, which could land inside
/// a letter and panic.
#[test]
fn the_filter_follows_a_rename_whatever_the_case() {
    let mut s = Session::with_demo();
    let id = s.visible_cloned()[0].0;
    s.execute("photo.setMeta", &json!({"ids": [id], "keywords": ["ßßa|é"]})).unwrap();
    s.execute("library.filter", &json!({"keyword": "ßßa|é"})).unwrap();
    s.execute("keyword.rename", &json!({"from": "ẞẞA", "to": "Road"})).unwrap();
    assert_eq!(s.filter.keyword.as_deref(), Some("Road|é"));
    s.execute("keyword.merge", &json!({"from": ["ROAD"], "into": "Weg"})).unwrap();
    assert_eq!(s.filter.keyword.as_deref(), Some("Weg|é"));
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("lc-keyword-list-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn keywords_of(s: &Session, id: u64) -> Vec<String> {
    s.catalog.photo(lightcraft_catalog::PhotoId(id)).unwrap().meta.keywords.clone()
}

/// Create Keyword: a name inside a parent, with attributes, given to the selected photos at once,
/// in one undo step; `keyword.info` reads it back. A keyword that exists isn't created again.
#[test]
fn creating_a_keyword_with_attributes_and_the_selection() {
    let mut s = Session::with_demo();
    let id = s.visible_cloned()[0].0;
    s.execute("library.select", &json!({"ids": [id]})).unwrap();
    let undo = s.undo.len();
    let r = s
        .execute("keyword.create", &json!({"name": "Weddings", "parent": "Events", "synonyms": ["marriage"], "person": false, "addToSelected": true}))
        .unwrap();
    assert_eq!(r["keyword"], "Events|Weddings", "{r}");
    assert_eq!(s.undo.len(), undo + 1, "one undo step");
    assert!(keywords_of(&s, id).contains(&"Events|Weddings".to_string()));
    let info = s.execute("keyword.info", &json!({"keyword": "events|weddings"})).unwrap();
    assert_eq!(info["path"], "Events|Weddings");
    assert_eq!(info["synonyms"], json!(["marriage"]));
    assert_eq!((info["includeOnExport"].as_bool(), info["listed"].as_bool(), info["count"].as_u64()), (Some(true), Some(true), Some(1)));
    let err = s.execute("keyword.create", &json!({"name": "weddings", "parent": "events"})).unwrap_err().to_string();
    assert!(err.contains("already"), "{err}");
    assert!(s.execute("keyword.info", &json!({"keyword": "lisbon"})).is_err(), "no such keyword");
}

/// "Put new keywords inside this keyword": new keywords go inside the default parent unless the
/// command says otherwise (`parent: null` = the top level). It belongs to its keyword in the
/// library: it follows a rename, goes with a delete, and undo and redo bring it back with them.
#[test]
fn new_keywords_go_inside_the_default_parent() {
    let dir = temp_dir("default-parent");
    let mut s = Session::new();
    s.open_library(&dir, true).unwrap();
    s.execute("keyword.create", &json!({"name": "Events"})).unwrap();
    s.execute("keyword.setDefaultParent", &json!({"keyword": "events"})).unwrap();
    assert_eq!(s.catalog.default_keyword_parent().as_deref(), Some("Events"));
    assert_eq!(s.execute("keyword.create", &json!({"name": "Birthdays"})).unwrap()["keyword"], "Events|Birthdays");
    assert_eq!(s.execute("keyword.create", &json!({"name": "Travel", "parent": null})).unwrap()["keyword"], "Travel");
    s.execute("keyword.rename", &json!({"from": "Events", "to": "Occasions"})).unwrap();
    assert_eq!(s.catalog.default_keyword_parent().as_deref(), Some("Occasions"), "follows the rename");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.catalog.default_keyword_parent().as_deref(), Some("Events"), "undo takes it back");
    s.execute("edit.redo", &json!({})).unwrap();
    drop(s);
    let mut s = Session::new();
    s.open_library(&dir, false).unwrap();
    assert_eq!(s.catalog.default_keyword_parent().as_deref(), Some("Occasions"), "kept with the library");
    s.execute("keyword.delete", &json!({"keyword": "occasions"})).unwrap();
    assert_eq!(s.catalog.default_keyword_parent(), None, "gone with its keyword");
    assert_eq!(s.execute("keyword.create", &json!({"name": "Graduations"})).unwrap()["keyword"], "Graduations", "at the top level");
    s.execute("edit.undo", &json!({})).unwrap();
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.catalog.default_keyword_parent().as_deref(), Some("Occasions"), "undoing the delete brings it back");
    s.execute("keyword.setDefaultParent", &json!({"keyword": null})).unwrap();
    assert_eq!(s.catalog.default_keyword_parent(), None);
    assert!(s.execute("keyword.setDefaultParent", &json!({"keyword": "lisbon"})).is_err(), "no such keyword");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Edit Keyword: a new name and attributes in one step; attributes not given keep their values.
#[test]
fn editing_a_keyword_changes_what_it_is_given() {
    let mut s = Session::with_demo();
    s.execute("keyword.create", &json!({"name": "Weddings", "synonyms": ["marriage"]})).unwrap();
    let undo = s.undo.len();
    s.execute("keyword.edit", &json!({"keyword": "weddings", "name": "Marriages", "includeOnExport": false})).unwrap();
    assert_eq!(s.undo.len(), undo + 1);
    let info = s.execute("keyword.info", &json!({"keyword": "Marriages"})).unwrap();
    assert_eq!((info["includeOnExport"].as_bool(), info["synonyms"].clone()), (Some(false), json!(["marriage"])));
    assert!(s.execute("keyword.info", &json!({"keyword": "Weddings"})).is_err());
}

/// Move Keyword: inside another or to the top level. Onto a name that is taken it is refused with
/// a message saying it would merge, unless `merge: true`.
#[test]
fn moving_a_keyword_merges_only_when_asked() {
    let mut s = Session::with_demo();
    let ids: Vec<u64> = s.visible_cloned().iter().take(2).map(|p| p.0).collect();
    s.execute("photo.setMeta", &json!({"ids": [ids[0]], "keywords": ["Rome"]})).unwrap();
    s.execute("photo.setMeta", &json!({"ids": [ids[1]], "keywords": ["Europe|Rome"]})).unwrap();
    let err = s.execute("keyword.move", &json!({"keyword": "rome", "parent": "europe"})).unwrap_err().to_string();
    assert!(err.contains("merge"), "{err}");
    s.execute("keyword.move", &json!({"keyword": "rome", "parent": "europe", "merge": true})).unwrap();
    assert_eq!(keywords_of(&s, ids[0]), ["Europe|Rome"]);
    s.execute("keyword.move", &json!({"keyword": "europe|rome", "parent": null})).unwrap();
    assert_eq!((keywords_of(&s, ids[0]), keywords_of(&s, ids[1])), (vec!["Rome".to_string()], vec!["Rome".to_string()]));
    assert!(s.execute("keyword.move", &json!({"keyword": "rome"})).is_err(), "`parent` must be said, null for the top level");
}

/// Purge Unused Keywords takes off the list the keywords no photo has, in one undo step.
#[test]
fn purging_unused_keywords() {
    let mut s = Session::with_demo();
    s.execute("keyword.create", &json!({"name": "Weddings"})).unwrap();
    let r = s.execute("keyword.purgeUnused", &json!({})).unwrap();
    assert_eq!(r["purged"], 1, "{r}");
    assert!(s.execute("keyword.info", &json!({"keyword": "Weddings"})).is_err());
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(s.execute("keyword.info", &json!({"keyword": "Weddings"})).is_ok());
}

/// Moving a keyword into a parent that doesn't exist yet makes that parent, and the keyword filter
/// follows the keyword there (the grid kept filtering by the old path, and showed nothing).
#[test]
fn the_filter_follows_a_keyword_moved_into_a_new_parent() {
    let mut s = Session::with_demo();
    let id = s.visible_cloned()[0].0;
    s.execute("photo.setMeta", &json!({"ids": [id], "keywords": ["Rome"]})).unwrap();
    s.execute("library.filter", &json!({"keyword": "Rome"})).unwrap();
    s.execute("keyword.move", &json!({"keyword": "rome", "parent": "Italy"})).unwrap();
    assert_eq!(keywords_of(&s, id), ["Italy|Rome"]);
    assert_eq!(s.filter.keyword.as_deref(), Some("Italy|Rome"));
}

/// Keywords given to photos are stored cleaned (" beach " is "beach", "Travel | Italy" is
/// "Travel|Italy"), so the list's actions find them; one a photo has already, whatever the case of
/// any letter, isn't added twice.
#[test]
fn keywords_given_to_photos_are_stored_cleaned() {
    let mut s = Session::with_demo();
    let id = s.visible_cloned()[0].0;
    s.execute("photo.setMeta", &json!({"ids": [id], "keywords": [" beach ", "Travel | Italy", "Ärzte"]})).unwrap();
    s.execute("photo.setMeta", &json!({"ids": [id], "addKeywords": [" sea ", "ÄRZTE", " "]})).unwrap();
    assert_eq!(keywords_of(&s, id), ["beach", "Travel|Italy", "Ärzte", "sea"]);
    s.execute("photo.setMeta", &json!({"ids": [id], "removeKeywords": ["travel | italy"]})).unwrap();
    assert_eq!(keywords_of(&s, id), ["beach", "Ärzte", "sea"]);
    assert!(s.execute("keyword.info", &json!({"keyword": "beach"})).is_ok());
}

/// Rename, merge and delete say how many photos they changed, as before the keyword list: the
/// list's own changes aren't photos.
#[test]
fn keyword_actions_count_the_photos_they_change() {
    let mut s = Session::with_demo();
    let id = s.visible_cloned()[0].0;
    s.execute("keyword.create", &json!({"name": "Weddings", "synonyms": ["marriage"]})).unwrap();
    assert_eq!(s.execute("keyword.rename", &json!({"from": "Weddings", "to": "Marriages"})).unwrap()["changed"], 0);
    s.execute("photo.setMeta", &json!({"ids": [id], "addKeywords": ["Marriages"]})).unwrap();
    assert_eq!(s.execute("keyword.rename", &json!({"from": "Marriages", "to": "Weddings"})).unwrap()["changed"], 1);
    assert_eq!(s.execute("keyword.delete", &json!({"keyword": "weddings"})).unwrap()["changed"], 1);
}
