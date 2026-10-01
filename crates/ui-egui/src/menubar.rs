//! The menu bar model: File, Edit, View, Photo, Window, Help, generated from the command registry
//! (engine commands with menu paths + [`crate::menus::UI_COMMANDS`]) with live labels, shortcuts,
//! enabled and checked state.
//!
//! One model drives every menu surface: the native macOS menu bar (built by the desktop host), the
//! in-window menu bar (web, Windows, Linux — [`show_in_window`]) and the control channel
//! (`ui.menu.tree`). Items are a command id plus parameters, run through [`run_item`] so they
//! behave exactly like their keyboard shortcuts.

use serde::Serialize;
use serde_json::{Value, json};

use crate::LightcraftApp;
use crate::menus::MenuEntry;
use crate::state::{RightPanel, ViewMode};

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum MenuNode {
    Item {
        id: String,
        #[serde(skip_serializing_if = "Value::is_null")]
        params: Value,
        label: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        shortcut: Option<String>,
        enabled: bool,
        /// `Some` for toggles and radio-style choices.
        #[serde(skip_serializing_if = "Option::is_none")]
        checked: Option<bool>,
    },
    Separator,
    Submenu {
        label: String,
        children: Vec<MenuNode>,
    },
}

impl MenuNode {
    /// `id|params` — a stable key for one item (native menu ids).
    pub fn key(id: &str, params: &Value) -> String {
        if params.is_null() { id.to_string() } else { format!("{id}|{params}") }
    }
}

/// Top-level menus in order (the macOS app menu is added by the host).
pub const MENUS: &[&str] = &["File", "Edit", "View", "Photo", "Window", "Help"];

/// Order and grouping per menu: command ids, `---` separators and `@Submenu` placeholders.
/// Entries with a menu path that aren't listed are appended at the end of their menu.
const LAYOUT: &[(&str, &[&str])] = &[
    (
        "File",
        &[
            "file.addPhotos",
            "---",
            "dialog.newAlbum",
            "dialog.newFolder",
            "dialog.newSmartAlbum",
            "---",
            "file.importPresets",
            "file.exportPresets",
            "---",
            "dialog.export",
            "app.exportPrevious",
            "---",
            "library.toggleAutoWriteXmp",
            "---",
        ],
    ),
    (
        "Edit",
        &["edit.undo", "edit.redo", "---", "develop.copy", "dialog.copySettings", "develop.paste", "---", "library.selectAll", "library.selectNone"],
    ),
    (
        "View",
        &[
            "view.photoGrid",
            "view.squareGrid",
            "view.detail",
            "view.compare",
            "view.survey",
            "---",
            "view.leftPanel",
            "view.filmstrip",
            "view.histogram",
            "---",
            "view.showOriginal",
            "view.beforeAfter",
            "view.beforeAfterSplit",
            "---",
            "view.zoomIn",
            "view.zoomOut",
            "view.zoomToggle",
            "view.zoomFit",
            "view.zoom100",
            "---",
            "view.clipping",
            "view.maskOverlay",
            "view.cropOverlay",
            "---",
            "@Sort",
            "@Stacks",
            "view.filterBar",
            "library.clearFilter",
            "---",
            "compare.swap",
            "compare.makeSelect",
        ],
    ),
    (
        "Photo",
        &[
            "@Add to Album",
            "---",
            "@Set Rating",
            "@Set Flag",
            "@Set Color Label",
            "view.autoAdvance",
            "---",
            "photo.rotateLeft",
            "photo.rotateRight",
            "photo.flipHorizontal",
            "photo.flipVertical",
            "---",
            "version.create",
            "photo.virtualCopy",
            "@Stack",
            "---",
            "develop.auto",
            "develop.treatment",
            "develop.reset",
            "dialog.createPreset",
            "---",
            "photo.saveMetadataToFile",
            "photo.readMetadataFromFile",
            "app.showInFinder",
            "---",
            "photo.delete",
        ],
    ),
    (
        "Window",
        &[
            "panel.edit",
            "panel.crop",
            "panel.remove",
            "panel.masking",
            "panel.redeye",
            "---",
            "panel.presets",
            "panel.versions",
            "panel.activity",
            "panel.info",
            "panel.keywords",
            "---",
            "@Edit Sections",
            "@Tools",
        ],
    ),
    ("Help", &["app.shortcuts", "---", "app.about"]),
];

/// Registry entries that are reached another way (parameterized commands are expanded into
/// submenus below; others duplicate a dialog command).
const HIDDEN: &[&str] = &[
    "photo.rate",
    "photo.flag",
    "photo.pick",
    "photo.reject",
    "photo.unflag",
    "photo.label",
    "library.sort",
    "album.addPhotos",
    "album.create",
    "library.import",
    "preset.create",
];

fn item(id: &str, params: Value, label: impl Into<String>, shortcut: Option<&str>, enabled: bool, checked: Option<bool>) -> MenuNode {
    MenuNode::Item { id: id.into(), params, label: label.into(), shortcut: shortcut.map(str::to_string), enabled, checked }
}

/// Checked state of toggles and radio items.
pub fn checked(app: &LightcraftApp, id: &str) -> Option<bool> {
    let u = &app.ui;
    let panel = |p: RightPanel| Some(u.right == p);
    match id {
        "view.photoGrid" => Some(u.view == ViewMode::PhotoGrid),
        "view.squareGrid" => Some(u.view == ViewMode::SquareGrid),
        "view.detail" => Some(u.view == ViewMode::Detail),
        "view.compare" => Some(u.view == ViewMode::Compare),
        "view.survey" => Some(u.view == ViewMode::Survey),
        "view.leftPanel" => Some(u.left_panel),
        "view.filmstrip" => Some(u.filmstrip),
        "view.histogram" => Some(u.histogram),
        "view.clipping" => Some(u.show_clipping),
        "view.maskOverlay" => Some(u.mask_overlay),
        "view.showOriginal" => Some(u.before_after == crate::state::BeforeAfter::Original),
        "view.beforeAfter" => Some(u.before_after == crate::state::BeforeAfter::SideBySide),
        "view.beforeAfterSplit" => Some(u.before_after == crate::state::BeforeAfter::Split),
        "view.autoAdvance" => Some(u.auto_advance),
        "panel.edit" => panel(RightPanel::Edit),
        "panel.crop" => panel(RightPanel::Crop),
        "panel.remove" => panel(RightPanel::Remove),
        "panel.masking" => panel(RightPanel::Masking),
        "panel.redeye" => panel(RightPanel::RedEye),
        "panel.versions" => panel(RightPanel::Versions),
        "panel.activity" => panel(RightPanel::Activity),
        "panel.info" => panel(RightPanel::Info),
        "panel.keywords" => panel(RightPanel::Keywords),
        "panel.presets" => Some(u.presets),
        "library.toggleAutoWriteXmp" => Some(app.session.xmp.auto_write),
        _ => None,
    }
}

/// Labels that follow the state ("Undo Exposure", "Delete 3 Photos").
fn live_label(app: &LightcraftApp, id: &str, label: &str) -> String {
    let n = app.session.selection.ids.len();
    match id {
        "edit.undo" => app.session.undo.last().map(|e| format!("Undo {}", e.label)).unwrap_or_else(|| "Undo".into()),
        "edit.redo" => app.session.redo.last().map(|e| format!("Redo {}", e.label)).unwrap_or_else(|| "Redo".into()),
        "photo.delete" if n > 1 => format!("Delete {n} Photos"),
        "photo.virtualCopy" if n > 1 => format!("Create {n} Virtual Copies"),
        _ => label.to_string(),
    }
}

/// The parameterized submenus.
fn expanded(app: &LightcraftApp, name: &str) -> Option<Vec<MenuNode>> {
    let active = app.session.active().and_then(|id| app.session.catalog.photo(id).cloned());
    let has = active.is_some();
    Some(match name {
        "Set Rating" => (0..=5u8)
            .map(|r| {
                let label = if r == 0 { "None".to_string() } else { "★".repeat(r as usize) };
                let sc = ["0", "1", "2", "3", "4", "5"][r as usize];
                item("photo.rate", json!({"rating": r}), label, Some(sc), has, Some(active.as_ref().is_some_and(|p| p.rating == r)))
            })
            .collect(),
        "Set Flag" => [
            ("pick", "Pick", "P", lightcraft_catalog::Flag::Pick),
            ("reject", "Reject", "X", lightcraft_catalog::Flag::Reject),
            ("none", "Unflagged", "U", lightcraft_catalog::Flag::None),
        ]
        .into_iter()
        .map(|(f, label, sc, flag)| {
            item("photo.flag", json!({"flag": f}), label, Some(sc), has, Some(active.as_ref().is_some_and(|p| p.flag == flag)))
        })
        .collect(),
        "Set Color Label" => {
            let mut v: Vec<MenuNode> = lightcraft_catalog::ColorLabel::ALL
                .iter()
                .zip([Some("6"), Some("7"), Some("8"), Some("9"), None])
                .map(|(l, sc)| {
                    let name = format!("{l:?}");
                    item(
                        "photo.label",
                        json!({"label": name.to_lowercase()}),
                        name,
                        sc,
                        has,
                        Some(active.as_ref().is_some_and(|p| p.label == Some(*l))),
                    )
                })
                .collect();
            v.push(MenuNode::Separator);
            v.push(item("photo.label", json!({"label": "none"}), "None", None, has, Some(active.as_ref().is_some_and(|p| p.label.is_none()))));
            v
        }
        "Sort" => {
            use lightcraft_catalog::SortKey::*;
            let cur = app.session.sort;
            let mut v: Vec<MenuNode> = [
                ("Capture Date", CaptureDate, "captureDate"),
                ("Import Date", ImportDate, "importDate"),
                ("Modified Date", EditDate, "editDate"),
                ("File Name", FileName, "fileName"),
                ("Rating", Rating, "rating"),
                ("File Size", FileSize, "fileSize"),
            ]
            .into_iter()
            .map(|(label, key, k)| item("library.sort", json!({"key": k}), label, None, true, Some(cur.key == key)))
            .collect();
            v.push(MenuNode::Separator);
            v.push(item("library.sort", json!({"ascending": true}), "Ascending", None, true, Some(cur.ascending)));
            v.push(item("library.sort", json!({"ascending": false}), "Descending", None, true, Some(!cur.ascending)));
            v
        }
        "Add to Album" => {
            let sel = !app.session.selection.ids.is_empty() || has;
            let mut albums: Vec<_> = app.session.catalog.albums().filter(|a| !a.folder && !a.is_smart()).map(|a| (a.name.clone(), a.id.0)).collect();
            albums.sort_by_key(|(n, _)| n.to_lowercase());
            let mut v: Vec<MenuNode> =
                albums.into_iter().map(|(name, id)| item("album.addPhotos", json!({"id": id}), name, None, sel, None)).collect();
            if !v.is_empty() {
                v.push(MenuNode::Separator);
            }
            v.push(item("dialog.newAlbum", Value::Null, "New Album…", None, true, None));
            v
        }
        _ => return None,
    })
}

fn node(app: &LightcraftApp, e: &MenuEntry) -> MenuNode {
    item(&e.id, Value::Null, live_label(app, &e.id, &e.label), e.shortcut.as_deref(), e.enabled, checked(app, &e.id))
}

/// Drop leading, trailing and doubled separators (also inside submenus) and empty submenus.
fn tidy(v: Vec<MenuNode>) -> Vec<MenuNode> {
    let mut out: Vec<MenuNode> = Vec::with_capacity(v.len());
    for n in v {
        let n = match n {
            MenuNode::Submenu { label, children } => {
                let children = tidy(children);
                if children.is_empty() {
                    continue;
                }
                MenuNode::Submenu { label, children }
            }
            n => n,
        };
        if n == MenuNode::Separator && out.last().is_none_or(|l| *l == MenuNode::Separator) {
            continue;
        }
        out.push(n);
    }
    while out.last() == Some(&MenuNode::Separator) {
        out.pop();
    }
    out
}

/// The whole menu bar: (title, items) per menu in [`MENUS`] order.
pub fn menu_bar(app: &LightcraftApp) -> Vec<(String, Vec<MenuNode>)> {
    let entries: Vec<MenuEntry> = crate::menus::menu_entries(app).into_iter().filter(|e| !HIDDEN.contains(&e.id.as_str())).collect();
    let mut used = vec![false; entries.len()];
    let mut bar = Vec::new();
    for title in MENUS {
        let layout = LAYOUT.iter().find(|(t, _)| t == title).map(|(_, l)| *l).unwrap_or(&[]);
        let mut items = Vec::new();
        let sub = |name: &str, used: &mut [bool]| -> Vec<MenuNode> {
            let mut children = expanded(app, name).unwrap_or_default();
            let mut dialogs = Vec::new();
            for (i, e) in entries.iter().enumerate() {
                if !used[i] && e.menu.len() == 2 && e.menu[0] == *title && e.menu[1] == name {
                    used[i] = true;
                    // commands that open a dialog go last, after a separator
                    if e.label.ends_with('…') { dialogs.push(node(app, e)) } else { children.push(node(app, e)) }
                }
            }
            if !dialogs.is_empty() {
                children.push(MenuNode::Separator);
                children.extend(dialogs);
            }
            children
        };
        for slot in layout {
            if *slot == "---" {
                items.push(MenuNode::Separator);
            } else if let Some(name) = slot.strip_prefix('@') {
                let children = sub(name, &mut used);
                items.push(MenuNode::Submenu { label: name.to_string(), children });
            } else if let Some(i) = entries.iter().position(|e| e.id == *slot && e.menu.first().map(String::as_str) == Some(title)) {
                used[i] = true;
                items.push(node(app, &entries[i]));
            }
        }
        // everything else that names this menu
        items.push(MenuNode::Separator);
        let mut extra_subs: Vec<String> = Vec::new();
        for (i, e) in entries.iter().enumerate() {
            if used[i] || e.menu.first().map(String::as_str) != Some(title) {
                continue;
            }
            if e.menu.len() == 1 {
                used[i] = true;
                items.push(node(app, e));
            } else if !extra_subs.contains(&e.menu[1]) {
                extra_subs.push(e.menu[1].clone());
            }
        }
        for name in extra_subs {
            let children = sub(&name, &mut used);
            items.push(MenuNode::Submenu { label: name, children });
        }
        bar.push((title.to_string(), tidy(items)));
    }
    bar
}

/// Run a menu item. Rating, flag and label items behave like their keys (the active photo only in
/// Compare/Survey; Auto Advance moves on).
pub fn run_item(app: &mut LightcraftApp, id: &str, params: Value) -> Result<Value, String> {
    let mut params = if params.is_null() { json!({}) } else { params };
    let culling_cmd = matches!(id, "photo.rate" | "photo.flag" | "photo.label" | "photo.pick" | "photo.reject" | "photo.unflag");
    if culling_cmd {
        crate::panels::compare::target_active(app, &mut params);
    }
    let r = app.run(id, params);
    if culling_cmd && r.is_ok() && app.ui.auto_advance {
        crate::panels::compare::advance(app);
    }
    r
}

/// Human-readable shortcut text for menus: `Cmd+Shift+Z` → `⌘⇧Z` on macOS, `Ctrl+Shift+Z` elsewhere.
pub fn shortcut_text(sc: &str, mac: bool) -> String {
    if mac {
        let mut mods = String::new();
        let mut key = "";
        for part in sc.split('+') {
            match part {
                "Ctrl" => mods.push('⌃'),
                "Alt" => mods.push('⌥'),
                "Shift" => mods.push('⇧'),
                "Cmd" => mods.push('⌘'),
                k => key = k,
            }
        }
        let key = match key {
            "Delete" => "⌫",
            "Escape" => "⎋",
            "Left" => "←",
            "Right" => "→",
            k => k,
        };
        format!("{mods}{key}")
    } else {
        sc.replace("Cmd", "Ctrl")
    }
}

/// Width of the in-window menu bar's titles.
pub fn bar_width(ui: &egui::Ui) -> f32 {
    let t = crate::theme::Tokens::get(ui.ctx());
    MENUS.iter().map(|m| ui.painter().layout_no_wrap(m.to_string(), t.font(13.0), t.text).size().x + 16.0).sum::<f32>()
        + ui.spacing().item_spacing.x * MENUS.len() as f32
}

/// The in-window menu bar (hosts without a native one): one dropdown per menu, or a single
/// "Menu" button when the space is too narrow. Returns the width used.
pub fn show_in_window(app: &mut LightcraftApp, ui: &mut egui::Ui, max_width: f32) -> f32 {
    let t = crate::theme::Tokens::get(ui.ctx());
    let bar = menu_bar(app);
    let font = t.font(13.0);
    let widths: Vec<f32> = bar.iter().map(|(title, _)| ui.painter().layout_no_wrap(title.clone(), font.clone(), t.text).size().x + 16.0).collect();
    let total: f32 = widths.iter().sum();
    let mut clicked: Option<(String, Value)> = None;
    let start = ui.cursor().left();
    let mac = ui.ctx().os() == egui::os::OperatingSystem::Mac;
    if total <= max_width {
        for (title, items) in &bar {
            let r = ui.add(egui::Button::new(egui::RichText::new(title).font(font.clone()).color(t.text_label)).frame(false));
            crate::widgets::register(ui.ctx(), format!("menu:{title}"), r.rect);
            egui::Popup::menu(&r).show(|ui| nodes_ui(ui, items, mac, &mut clicked));
        }
    } else {
        let r = ui.add(egui::Button::new(egui::RichText::new("Menu").font(font.clone()).color(t.text_label)).frame(false));
        crate::widgets::register(ui.ctx(), "menu:all", r.rect);
        egui::Popup::menu(&r).show(|ui| {
            for (title, items) in &bar {
                ui.menu_button(title, |ui| nodes_ui(ui, items, mac, &mut clicked));
            }
        });
    }
    if let Some((id, params)) = clicked {
        let _ = run_item(app, &id, params);
    }
    ui.cursor().left() - start
}

fn nodes_ui(ui: &mut egui::Ui, nodes: &[MenuNode], mac: bool, clicked: &mut Option<(String, Value)>) {
    ui.set_min_width(220.0);
    for n in nodes {
        match n {
            MenuNode::Separator => {
                ui.separator();
            }
            MenuNode::Submenu { label, children } => {
                ui.menu_button(format!("      {label}"), |ui| nodes_ui(ui, children, mac, clicked));
            }
            MenuNode::Item { id, params, label, shortcut, enabled, checked } => {
                // a gutter for check marks, like native menus
                let mut b = egui::Button::new(format!("      {label}"));
                if let Some(sc) = shortcut {
                    b = b.shortcut_text(shortcut_text(sc, mac));
                }
                let r = ui.add_enabled(*enabled, b);
                if *checked == Some(true) {
                    let t = crate::theme::Tokens::get(ui.ctx());
                    let c = egui::Rect::from_center_size(egui::pos2(r.rect.left() + 11.0, r.rect.center().y), egui::vec2(13.0, 13.0));
                    crate::icons::paint(ui.painter(), c, crate::icons::Icon::Check, if *enabled { t.text } else { t.text_disabled });
                }
                if r.clicked() {
                    *clicked = Some((id.clone(), params.clone()));
                    ui.close();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> LightcraftApp {
        LightcraftApp::new(lightcraft_engine::Session::with_demo(), Default::default())
    }

    fn find<'a>(nodes: &'a [MenuNode], id: &str) -> Option<&'a MenuNode> {
        nodes.iter().find_map(|n| match n {
            MenuNode::Item { id: i, .. } if i == id => Some(n),
            MenuNode::Submenu { children, .. } => find(children, id),
            _ => None,
        })
    }

    #[test]
    fn bar_follows_the_registry_and_state() {
        let mut app = app();
        let bar = menu_bar(&app);
        let titles: Vec<&str> = bar.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(titles, MENUS);
        let all: Vec<MenuNode> = bar.iter().flat_map(|(_, v)| v.clone()).collect();
        // every engine command with a menu path is reachable (parameterized ones via submenus)
        for c in lightcraft_engine::command_specs().iter().filter(|c| !c.menu.is_empty() && !HIDDEN.contains(&c.id)) {
            assert!(find(&all, c.id).is_some(), "{} missing from the menu bar", c.id);
        }
        for id in ["photo.rate", "photo.flag", "photo.label", "library.sort", "album.addPhotos", "view.compare", "stack.group", "photo.virtualCopy"] {
            assert!(find(&all, id).is_some(), "{id}");
        }
        // no doubled or dangling separators
        fn check(v: &[MenuNode]) {
            assert_ne!(v.first(), Some(&MenuNode::Separator));
            assert_ne!(v.last(), Some(&MenuNode::Separator));
            for w in v.windows(2) {
                assert!(!(w[0] == MenuNode::Separator && w[1] == MenuNode::Separator));
            }
            for n in v {
                if let MenuNode::Submenu { children, .. } = n {
                    check(children);
                }
            }
        }
        bar.iter().for_each(|(_, v)| check(v));
        // live state: undo label, enabled, checked
        let Some(MenuNode::Item { enabled, .. }) = find(&all, "edit.undo") else { panic!() };
        assert!(!enabled);
        run_item(&mut app, "photo.rate", json!({"rating": 3})).unwrap();
        let bar = menu_bar(&app);
        let all: Vec<MenuNode> = bar.iter().flat_map(|(_, v)| v.clone()).collect();
        let Some(MenuNode::Item { label, enabled, .. }) = find(&all, "edit.undo") else { panic!() };
        assert_eq!((label.as_str(), *enabled), ("Undo Set Rating", true));
        let rated: Vec<_> = all
            .iter()
            .filter_map(|n| match n {
                MenuNode::Submenu { label, children } if label == "Set Rating" => Some(children.clone()),
                _ => None,
            })
            .flatten()
            .filter(|n| matches!(n, MenuNode::Item { checked: Some(true), .. }))
            .collect();
        assert!(matches!(&rated[..], [MenuNode::Item { params, .. }] if params["rating"] == 3));
        let Some(MenuNode::Item { checked, .. }) = find(&all, "view.filmstrip") else { panic!() };
        assert_eq!(*checked, Some(app.ui.filmstrip));
    }

    #[test]
    fn shortcut_text_per_platform() {
        assert_eq!(shortcut_text("Cmd+Shift+Z", true), "⌘⇧Z");
        assert_eq!(shortcut_text("Cmd+Shift+Z", false), "Ctrl+Shift+Z");
        assert_eq!(shortcut_text("Delete", true), "⌫");
    }
}
