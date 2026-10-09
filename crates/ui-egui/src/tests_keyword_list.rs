//! The Keyword List in the right panel's Keywords (Lightroom Classic's Keyword List panel),
//! driven through the control channel.

use std::time::Duration;

use serde_json::json;

use crate::headless::Headless;
use crate::{LightcraftApp, Services};

const T: Duration = Duration::from_secs(20);
const SETTLE: Duration = Duration::from_secs(120);

/// The demo library in the Library grid with the Keywords panel open, two photos selected.
fn keywords_panel() -> (Headless, Vec<u64>) {
    let app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services { png: None, ..Default::default() });
    let mut h = Headless::new(app, [1400.0, 1000.0], 1.0);
    let ids: Vec<u64> = h.app.session.visible_cloned().iter().take(2).map(|p| p.0).collect();
    for (method, params) in [
        ("ui.set", json!({"view": "photoGrid", "right": "keywords"})),
        ("engine.execute", json!({"command": "library.select", "params": {"ids": ids}})),
    ] {
        let r = h.request(method, params, T);
        assert_eq!(r["ok"], true, "{r}");
    }
    h.settle(SETTLE);
    (h, ids)
}

fn run(h: &mut Headless, command: &str, params: serde_json::Value) -> serde_json::Value {
    let r = h.request("engine.execute", json!({"command": command, "params": params}), T);
    assert_eq!(r["ok"], true, "{command}: {r}");
    h.settle(SETTLE);
    r
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

/// The list shows every keyword with its photo count, a keyword without photos too; the triangle
/// opens a level.
#[test]
fn the_keyword_list_shows_every_keyword() {
    let (mut h, _) = keywords_panel();
    run(&mut h, "keyword.create", json!({"name": "Weddings", "parent": "Events"}));
    assert!(has(&h, "keywordRow:Events"), "a keyword with no photos is listed");
    assert!(!has(&h, "keywordRow:Events|Weddings"), "inside a closed level");
    ask(&mut h, "ui.clickWidget", json!({"id": "keywordRowToggle:Events"}));
    assert!(has(&h, "keywordRow:Events|Weddings"));
    assert!(has(&h, "keywordCount:mountains"), "photo counts are shown");
}

/// The tick box gives the keyword to every selected photo, then takes it away; with only some of
/// them having it, a click gives it to all.
#[test]
fn the_tick_box_gives_a_keyword_to_the_selection() {
    let (mut h, ids) = keywords_panel();
    run(&mut h, "keyword.create", json!({"name": "Weddings"}));
    // the demo has many keywords: find it with the filter box
    ask(&mut h, "ui.clickWidget", json!({"id": "field:keywordFilter"}));
    ask(&mut h, "ui.text", json!({"text": "wedd"}));
    ask(&mut h, "ui.clickWidget", json!({"id": "keywordCheck:Weddings"}));
    assert!(ids.iter().all(|id| keywords_of(&h, *id).contains(&"Weddings".to_string())), "given to both");
    ask(&mut h, "ui.clickWidget", json!({"id": "keywordCheck:Weddings"}));
    assert!(ids.iter().all(|id| !keywords_of(&h, *id).contains(&"Weddings".to_string())), "taken from both");
    run(&mut h, "photo.setMeta", json!({"ids": [ids[0]], "addKeywords": ["Weddings"]}));
    ask(&mut h, "ui.clickWidget", json!({"id": "keywordCheck:Weddings"}));
    assert!(ids.iter().all(|id| keywords_of(&h, *id).contains(&"Weddings".to_string())), "some had it: now all do");
}

/// Typing in the filter box keeps the keywords whose name holds the text, with their parents.
#[test]
fn the_filter_box_narrows_the_list() {
    let (mut h, _) = keywords_panel();
    run(&mut h, "keyword.create", json!({"name": "Weddings", "parent": "Events"}));
    ask(&mut h, "ui.clickWidget", json!({"id": "field:keywordFilter"}));
    ask(&mut h, "ui.text", json!({"text": "wed"}));
    let rows: Vec<String> = h.app.widgets.iter().filter_map(|(w, _)| w.strip_prefix("keywordRow:").map(str::to_string)).collect();
    assert_eq!(rows, ["Events", "Events|Weddings"]);
}

/// The arrow on a row shows the photos with that keyword.
#[test]
fn the_arrow_shows_the_photos_with_the_keyword() {
    let (mut h, _) = keywords_panel();
    ask(&mut h, "ui.hoverWidget", json!({"id": "keywordRow:mountains"}));
    ask(&mut h, "ui.clickWidget", json!({"id": "keywordShow:mountains"}));
    assert_eq!(h.app.session.filter.keyword.as_deref(), Some("mountains"));
}
