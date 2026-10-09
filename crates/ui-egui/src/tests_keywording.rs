//! The Keywording box at the top of the right panel's Keywords: the selection's keywords, driven
//! through the control channel.

use std::time::Duration;

use serde_json::json;

use crate::headless::Headless;
use crate::{LightcraftApp, Services};

const T: Duration = Duration::from_secs(20);
const SETTLE: Duration = Duration::from_secs(120);

/// The Library grid with the Keywords panel open and two photos selected: the first with beach and
/// Weddings, the second with beach.
fn two_selected() -> (Headless, Vec<u64>) {
    let app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services { png: None, ..Default::default() });
    let mut h = Headless::new(app, [1400.0, 1000.0], 1.0);
    let ids: Vec<u64> = h.app.session.visible_cloned().iter().take(2).map(|p| p.0).collect();
    for (method, params) in [
        ("ui.set", json!({"view": "photoGrid", "right": "keywords"})),
        ("engine.execute", json!({"command": "photo.setMeta", "params": {"ids": [ids[0]], "keywords": ["beach", "Weddings"]}})),
        ("engine.execute", json!({"command": "photo.setMeta", "params": {"ids": [ids[1]], "keywords": ["beach"]}})),
        ("engine.execute", json!({"command": "library.select", "params": {"ids": ids}})),
    ] {
        let r = h.request(method, params, T);
        assert_eq!(r["ok"], true, "{r}");
    }
    h.settle(SETTLE);
    (h, ids)
}

fn ask(h: &mut Headless, method: &str, params: serde_json::Value) {
    let r = h.request(method, params.clone(), T);
    assert_eq!(r["ok"], true, "{method} {params}: {r}");
    h.settle(SETTLE);
}

fn has(h: &Headless, id: &str) -> bool {
    h.app.widgets.iter().any(|(w, _)| w == id)
}

fn keywords_of(h: &Headless, id: u64) -> Vec<String> {
    h.app.session.catalog.photo(lightcraft_catalog::PhotoId(id)).unwrap().meta.keywords.clone()
}

/// The chips are the keywords of every selected photo, not only the active one's; a keyword only
/// some of them have is marked.
#[test]
fn the_chips_are_the_selections_keywords() {
    let (h, _) = two_selected();
    assert!(
        has(&h, "keywordChip:beach") && has(&h, "keywordChip:Weddings"),
        "{:?}",
        h.app.widgets.iter().filter(|(w, _)| w.starts_with("keywordChip")).collect::<Vec<_>>()
    );
    assert!(has(&h, "keywordChipPartial:Weddings"), "only one of the two has Weddings");
    assert!(!has(&h, "keywordChipPartial:beach"), "both have beach");
}

/// Only the × removes a keyword, from every selected photo; a click on the chip itself doesn't.
#[test]
fn only_the_cross_removes_a_keyword() {
    let (mut h, ids) = two_selected();
    ask(&mut h, "ui.clickWidget", json!({"id": "keywordChip:beach"}));
    assert!(ids.iter().all(|id| keywords_of(&h, *id).contains(&"beach".to_string())), "a click on the chip keeps it");
    ask(&mut h, "ui.clickWidget", json!({"id": "keywordChipRemove:beach"}));
    assert!(ids.iter().all(|id| !keywords_of(&h, *id).contains(&"beach".to_string())), "× took it off both");
}

/// A chip's menu acts on the selection, never on the whole library: give a keyword only some have
/// to all of them, take it off them, show its photos, or edit it (Edit Keyword Tag). Deleting a
/// keyword from every photo of the library is the Keyword List's.
#[test]
fn a_chips_menu_acts_on_the_selection() {
    let (mut h, ids) = two_selected();
    ask(&mut h, "ui.clickWidget", json!({"id": "keywordChip:Weddings", "button": "right"}));
    for item in ["add", "remove", "show", "edit"] {
        assert!(has(&h, &format!("keywordChipMenu:{item}")), "{item}");
    }
    assert!(!has(&h, "keywordChipMenu:delete"), "no library-wide delete");
    ask(&mut h, "ui.clickWidget", json!({"id": "keywordChipMenu:add"}));
    assert!(ids.iter().all(|id| keywords_of(&h, *id).contains(&"Weddings".to_string())), "given to both");
    ask(&mut h, "ui.clickWidget", json!({"id": "keywordChip:Weddings", "button": "right"}));
    assert!(!has(&h, "keywordChipMenu:add"), "both have it now");
    ask(&mut h, "ui.clickWidget", json!({"id": "keywordChipMenu:edit"}));
    assert!(matches!(&h.app.ui.dialog, Some(crate::state::Dialog::KeywordTag { editing: Some(k), .. }) if k == "Weddings"), "{:?}", h.app.ui.dialog);
}
