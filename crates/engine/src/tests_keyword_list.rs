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
