//! UI-level commands (views, panels, zoom, tools, dialogs) and the menu model shared by the native
//! menu bar, the shortcut handler and the control channel.

use serde::Serialize;
use serde_json::{Value, json};

use crate::LightcraftApp;
use crate::state::{BeforeAfter, Dialog, RightPanel, ViewMode, Zoom};

/// (id, label, shortcut, menu path)
pub type UiCommand = (&'static str, &'static str, Option<&'static str>, &'static str);

pub const UI_COMMANDS: &[UiCommand] = &[
    ("view.photoGrid", "Photo Grid", Some("G"), "View"),
    ("view.squareGrid", "Square Grid", Some("Shift+G"), "View"),
    ("view.detail", "Detail", Some("D"), "View"),
    ("view.filmstrip", "Filmstrip", Some("/"), "View"),
    ("view.leftPanel", "My Photos Panel", Some("Cmd+Shift+L"), "View"),
    ("view.beforeAfter", "Compare Before and After", Some("Y"), "View"),
    ("view.beforeAfterSplit", "Before/After Split", Some("Shift+Y"), "View"),
    ("view.beforeAfterTopBottom", "Before/After Top/Bottom", Some("Alt+Y"), "View"),
    ("view.beforeAfterSplitTopBottom", "Before/After Split Top/Bottom", Some("Alt+Shift+Y"), "View"),
    ("view.showOriginal", "Show Original", Some("\\"), "View"),
    ("view.zoomFit", "Zoom to Fit", Some("Cmd+0"), "View"),
    ("view.zoom100", "Zoom 100%", Some("Cmd+1"), "View"),
    ("view.zoomToggle", "Toggle Zoom", Some("Z"), "View"),
    ("view.zoomIn", "Zoom In", Some("Cmd+="), "View"),
    ("view.zoomOut", "Zoom Out", Some("Cmd+-"), "View"),
    ("view.clipping", "Show Clipping", Some("J"), "View"),
    ("view.histogram", "Histogram", Some("Cmd+Shift+H"), "View"),
    ("view.maskOverlay", "Show Mask Overlay", Some("O"), "View"),
    ("view.visualizeSpots", "Visualize Spots", Some("A"), "View"),
    ("view.cropOverlay", "Cycle Crop Overlay", Some("Shift+O"), "View"),
    ("view.back", "Back to Grid", Some("Escape"), "View"),
    ("view.filterBar", "Filter", None, "View"),
    ("panel.edit", "Edit", Some("E"), "View"),
    ("panel.crop", "Crop & Rotate", Some("C"), "View"),
    ("panel.remove", "Remove", Some("H"), "View"),
    ("panel.masking", "Masking", Some("M"), "View"),
    ("panel.redeye", "Red Eye", None, "View"),
    ("panel.presets", "Presets", Some("Shift+P"), "View"),
    ("panel.info", "Info", Some("I"), "View"),
    ("panel.keywords", "Keywords", Some("K"), "View"),
    ("panel.versions", "Versions", Some("Shift+V"), "View"),
    ("panel.activity", "History", None, "View"),
    ("panel.close", "Close Panel", None, "View"),
    ("section.light", "Light", Some("Cmd+Alt+1"), "View"),
    ("section.color", "Color", Some("Cmd+Alt+2"), "View"),
    ("section.effects", "Effects", Some("Cmd+Alt+3"), "View"),
    ("section.detail", "Detail", Some("Cmd+Alt+4"), "View"),
    ("section.optics", "Optics", Some("Cmd+Alt+5"), "View"),
    ("tool.brush", "Brush", Some("B"), "View"),
    ("tool.linear", "Linear Gradient", Some("L"), "View"),
    ("tool.radial", "Radial Gradient", Some("R"), "View"),
    ("tool.wbPicker", "White Balance Selector", Some("W"), "View"),
    ("tool.none", "No Tool", None, "View"),
    ("dialog.newAlbum", "New Album…", Some("Cmd+N"), "File"),
    ("dialog.newFolder", "New Folder…", Some("Cmd+Shift+N"), "File"),
    ("dialog.createPreset", "Create Preset…", Some("Cmd+Shift+P"), "Photo"),
    ("dialog.copySettings", "Choose Edit Settings to Copy…", Some("Cmd+Shift+C"), "Photo"),
    ("dialog.export", "Export…", Some("Cmd+Shift+E"), "File"),
    ("dialog.mergeHdr", "Photo Merge ▸ HDR…", Some("Ctrl+H"), "Photo"),
    ("dialog.mergePanorama", "Photo Merge ▸ Panorama…", Some("Ctrl+M"), "Photo"),
    ("dialog.mergeHdrPanorama", "Photo Merge ▸ HDR Panorama…", None, "Photo"),
    ("file.addPhotos", "Add Photos…", Some("Cmd+Shift+I"), "File"),
    ("file.importPresets", "Import Presets…", None, "File"),
    ("file.exportPresets", "Export Presets…", None, "File"),
    ("app.about", "About LightCraft", None, "LightCraft"),
    ("app.shortcuts", "Keyboard Shortcuts", Some("Cmd+/"), "Help"),
    ("app.export", "Export Now", None, ""),
    ("app.showInFinder", "Show in Finder", Some("Cmd+R"), "Photo"),
    ("app.exportPrevious", "Export with Previous", Some("Cmd+Alt+Shift+E"), "File"),
];

fn panel(app: &mut LightcraftApp, ctx: &egui::Context, p: RightPanel, name: &str) {
    if app.ui.right == p {
        app.ui.right = RightPanel::None;
        app.toast(ctx, format!("{name} Off"));
    } else {
        app.ui.right = p;
        app.toast(ctx, format!("{name} On"));
        if p.is_edit_tool() && !matches!(app.ui.view, ViewMode::Detail) {
            app.ui.view = ViewMode::Detail;
        }
    }
    if p != RightPanel::Masking && app.ui.tool != "wbPicker" {
        app.ui.tool.clear();
    }
    let _ = app.session.end_interaction();
}

/// Handle UI commands; `None` means "not a UI command — send it to the engine".
pub fn run_ui_command(app: &mut LightcraftApp, id: &str, p: &Value) -> Option<Result<Value, String>> {
    let ctx = egui::Context::default();
    let r: Result<Value, String> = match id {
        "view.photoGrid" => {
            app.ui.view = ViewMode::PhotoGrid;
            Ok(Value::Null)
        }
        "view.squareGrid" => {
            app.ui.view = ViewMode::SquareGrid;
            Ok(Value::Null)
        }
        "view.detail" => {
            app.ui.view = ViewMode::Detail;
            Ok(Value::Null)
        }
        "view.back" => {
            if app.ui.dialog.is_some() {
                app.ui.dialog = None;
            } else if !app.ui.tool.is_empty() {
                app.ui.tool.clear();
            } else if app.ui.view == ViewMode::Detail {
                app.ui.view = ViewMode::PhotoGrid;
            }
            Ok(Value::Null)
        }
        "view.filmstrip" => {
            app.ui.filmstrip = !app.ui.filmstrip;
            Ok(Value::Null)
        }
        "view.leftPanel" => {
            app.ui.left_panel = !app.ui.left_panel;
            Ok(Value::Null)
        }
        "view.beforeAfter" => {
            app.ui.before_after = if app.ui.before_after == BeforeAfter::SideBySide { BeforeAfter::Off } else { BeforeAfter::SideBySide };
            Ok(Value::Null)
        }
        "view.beforeAfterSplit" => {
            app.ui.before_after = if app.ui.before_after == BeforeAfter::Split { BeforeAfter::Off } else { BeforeAfter::Split };
            Ok(Value::Null)
        }
        "view.beforeAfterTopBottom" => {
            app.ui.before_after = if app.ui.before_after == BeforeAfter::TopBottom { BeforeAfter::Off } else { BeforeAfter::TopBottom };
            Ok(Value::Null)
        }
        "view.beforeAfterSplitTopBottom" => {
            app.ui.before_after = if app.ui.before_after == BeforeAfter::SplitTopBottom { BeforeAfter::Off } else { BeforeAfter::SplitTopBottom };
            Ok(Value::Null)
        }
        "view.showOriginal" => {
            app.ui.before_after = if app.ui.before_after == BeforeAfter::Original { BeforeAfter::Off } else { BeforeAfter::Original };
            Ok(Value::Null)
        }
        "view.zoomFit" => {
            app.ui.zoom = Zoom::Fit;
            Ok(Value::Null)
        }
        "view.zoom100" => {
            app.ui.zoom = Zoom::Percent(100);
            Ok(Value::Null)
        }
        "view.zoomToggle" => {
            app.ui.zoom = if app.ui.zoom == Zoom::Fit { Zoom::Percent(100) } else { Zoom::Fit };
            Ok(Value::Null)
        }
        "view.zoomIn" | "view.zoomOut" => {
            let steps = [25u32, 50, 100, 200, 400, 800];
            let cur = match app.ui.zoom {
                Zoom::Percent(p) => p,
                _ => 25,
            };
            let next = if id == "view.zoomIn" {
                steps.iter().find(|s| **s > cur).copied().unwrap_or(800)
            } else {
                steps.iter().rev().find(|s| **s < cur).copied().unwrap_or(0)
            };
            app.ui.zoom = if next == 0 { Zoom::Fit } else { Zoom::Percent(next) };
            Ok(Value::Null)
        }
        "view.clipping" => {
            app.ui.show_clipping = !app.ui.show_clipping;
            Ok(Value::Null)
        }
        "view.histogram" => {
            app.ui.histogram = !app.ui.histogram;
            Ok(Value::Null)
        }
        "view.maskOverlay" => {
            app.ui.mask_overlay = !app.ui.mask_overlay;
            Ok(Value::Null)
        }
        "view.visualizeSpots" => {
            // like Lightroom's A: opens the Remove tool with the view on, or toggles it there
            if app.ui.right == RightPanel::Remove {
                app.ui.visualize_spots = !app.ui.visualize_spots;
            } else {
                app.ui.right = RightPanel::Remove;
                app.ui.visualize_spots = true;
            }
            Ok(Value::Null)
        }
        "view.cropOverlay" => {
            use crate::state::CropOverlay::*;
            app.ui.crop_overlay = match app.ui.crop_overlay {
                Thirds => Grid,
                Grid => Golden,
                Golden => Diagonal,
                Diagonal => None,
                None => Thirds,
            };
            Ok(Value::Null)
        }
        "view.filterBar" => {
            app.ui.left_panel = true;
            Ok(Value::Null)
        }
        "panel.edit" => {
            panel(app, &ctx, RightPanel::Edit, "Edit");
            Ok(Value::Null)
        }
        "panel.crop" => {
            panel(app, &ctx, RightPanel::Crop, "Crop, Rotate, Geometry");
            Ok(Value::Null)
        }
        "panel.remove" => {
            panel(app, &ctx, RightPanel::Remove, "Remove");
            if app.ui.right == RightPanel::Remove && app.ui.tool.is_empty() {
                app.ui.tool = "remove".into();
            }
            Ok(Value::Null)
        }
        "panel.masking" => {
            panel(app, &ctx, RightPanel::Masking, "Masking");
            Ok(Value::Null)
        }
        "panel.redeye" => {
            panel(app, &ctx, RightPanel::RedEye, "Red Eye");
            Ok(Value::Null)
        }
        "panel.info" => {
            panel(app, &ctx, RightPanel::Info, "Info");
            Ok(Value::Null)
        }
        "panel.keywords" => {
            panel(app, &ctx, RightPanel::Keywords, "Keywords");
            Ok(Value::Null)
        }
        "panel.versions" => {
            panel(app, &ctx, RightPanel::Versions, "Versions");
            Ok(Value::Null)
        }
        "panel.activity" => {
            panel(app, &ctx, RightPanel::Activity, "History");
            Ok(Value::Null)
        }
        "panel.presets" => {
            app.ui.presets = !app.ui.presets;
            if app.ui.presets && app.ui.view != ViewMode::Detail {
                app.ui.view = ViewMode::Detail;
            }
            Ok(Value::Null)
        }
        "panel.close" => {
            app.ui.right = RightPanel::None;
            app.ui.presets = false;
            Ok(Value::Null)
        }
        s if s.starts_with("section.") => {
            let sec = &s["section.".len()..];
            if app.ui.right != RightPanel::Edit {
                app.ui.right = RightPanel::Edit;
            }
            app.ui.toggle_section(sec);
            Ok(Value::Null)
        }
        s if s.starts_with("tool.") => {
            let tool = &s["tool.".len()..];
            match tool {
                "none" => app.ui.tool.clear(),
                "brush" => {
                    app.ui.right = RightPanel::Masking;
                    app.ui.view = ViewMode::Detail;
                    app.ui.tool = "brush".into();
                }
                "linear" | "radial" => {
                    app.ui.right = RightPanel::Masking;
                    app.ui.view = ViewMode::Detail;
                    app.ui.tool = tool.into();
                    return Some(app.session.execute("mask.add", &json!({"kind": tool})).map_err(|e| e.to_string()));
                }
                "wbPicker" => {
                    app.ui.right = RightPanel::Edit;
                    app.ui.view = ViewMode::Detail;
                    app.ui.tool = "wbPicker".into();
                }
                other => return Some(Err(format!("unknown tool `{other}`"))),
            }
            Ok(Value::Null)
        }
        "dialog.newFolder" => {
            app.ui.dialog = Some(Dialog::NewAlbum { name: p.get("name").and_then(Value::as_str).unwrap_or("").into(), folder: true });
            Ok(Value::Null)
        }
        "dialog.newAlbum" => {
            app.ui.dialog = Some(Dialog::NewAlbum { name: p.get("name").and_then(Value::as_str).unwrap_or("").into(), folder: false });
            Ok(Value::Null)
        }
        "dialog.createPreset" => {
            app.ui.dialog = Some(Dialog::CreatePreset { name: String::new(), group: "User Presets".into() });
            Ok(Value::Null)
        }
        "dialog.copySettings" => {
            let groups =
                app.session.copy_groups.iter().filter_map(|g| serde_json::to_value(g).ok().and_then(|v| v.as_str().map(str::to_string))).collect();
            app.ui.dialog = Some(Dialog::CopySettings { groups });
            Ok(Value::Null)
        }
        "dialog.export" => {
            let prev = app.session.last_export.clone().unwrap_or_default();
            let u = |k: &str, d: u64| prev.get(k).and_then(Value::as_u64).unwrap_or(d);
            let dir = prev.get("dir").and_then(Value::as_str).map(str::to_string).unwrap_or_else(crate::control::default_export_dir);
            app.ui.dialog = Some(Dialog::Export {
                opts: lightcraft_engine::export::ExportOptions::from_json(&prev),
                long_edge: u("longEdge", 2048) as u32,
                limit_kb: u("limitKb", 0) as u32,
                dir,
            });
            Ok(Value::Null)
        }
        "dialog.mergeHdr" => crate::merge::open(app, "merge.hdr"),
        "dialog.mergePanorama" => crate::merge::open(app, "merge.panorama"),
        "dialog.mergeHdrPanorama" => crate::merge::open(app, "merge.hdrPanorama"),
        "app.about" => {
            app.ui.dialog = Some(Dialog::About);
            Ok(Value::Null)
        }
        "app.shortcuts" => {
            app.ui.dialog = Some(Dialog::Shortcuts);
            Ok(Value::Null)
        }
        "file.addPhotos" => {
            let paths = match p.get("paths").and_then(Value::as_array) {
                Some(a) => a.iter().filter_map(Value::as_str).map(str::to_string).collect(),
                None => app.services.pick_files.as_mut().map(|f| f()).unwrap_or_default(),
            };
            if paths.is_empty() {
                return Some(Ok(Value::Null));
            }
            return Some(app.session.execute("library.import", &json!({"paths": paths})).map_err(|e| e.to_string()));
        }
        "file.importPresets" => {
            let paths = match p.get("paths").and_then(Value::as_array) {
                Some(a) => a.iter().filter_map(Value::as_str).map(str::to_string).collect(),
                None => match app.services.pick_preset_files.as_mut() {
                    Some(f) => f(),
                    None => return Some(Err("no file dialog on this platform".into())),
                },
            };
            if paths.is_empty() {
                return Some(Ok(Value::Null));
            }
            let r = app.session.execute("preset.import", &json!({"paths": paths})).map_err(|e| e.to_string());
            if let Ok(v) = &r {
                let n = v["imported"].as_array().map_or(0, Vec::len);
                let failed = v["failed"].as_array().map_or(0, Vec::len);
                let msg = match (n, failed) {
                    (0, 0) => "No new presets".to_string(),
                    (n, 0) => format!("Imported {n} preset{}", if n == 1 { "" } else { "s" }),
                    (n, f) => format!("Imported {n} preset{}, {f} file{} not readable", if n == 1 { "" } else { "s" }, if f == 1 { "" } else { "s" }),
                };
                app.toast(&ctx, msg);
                if n > 0 {
                    app.ui.presets = true;
                }
            }
            return Some(r);
        }
        "file.exportPresets" => {
            let group = p.get("group").and_then(Value::as_str).map(str::to_string);
            let path = match p.get("path").and_then(Value::as_str) {
                Some(x) => Some(x.to_string()),
                None => {
                    let name = format!("{}.lcpreset", group.as_deref().unwrap_or("LightCraft Presets"));
                    match app.services.save_preset_file.as_mut() {
                        Some(f) => f(&name),
                        None => return Some(Err("no file dialog on this platform".into())),
                    }
                }
            };
            let Some(path) = path else { return Some(Ok(Value::Null)) };
            let mut params = json!({"path": path});
            if let Some(g) = group {
                params["group"] = json!(g);
            }
            if let Some(ids) = p.get("ids") {
                params["ids"] = ids.clone();
            }
            let r = app.session.execute("preset.export", &params).map_err(|e| e.to_string());
            if let Ok(v) = &r {
                app.toast(&ctx, format!("Exported {} preset{}", v["count"], if v["count"] == 1 { "" } else { "s" }));
            }
            return Some(r);
        }
        "app.export" => crate::control::export_active(app, p),
        "app.showInFinder" => show_in_finder(app),
        "app.exportPrevious" => match app.session.last_export.clone() {
            Some(prev) => crate::control::export_active(app, &prev),
            None => Err("nothing exported yet — use Export…".into()),
        },
        _ => return None,
    };
    Some(r)
}

pub fn ui_enabled(app: &LightcraftApp, id: &str) -> bool {
    match id {
        s if s.starts_with("panel.") || s.starts_with("tool.") || s.starts_with("section.") => app.session.active().is_some() || s == "panel.close",
        "app.export" | "dialog.export" | "dialog.createPreset" | "dialog.copySettings" => app.session.active().is_some(),
        "app.exportPrevious" => app.session.active().is_some() && app.session.last_export.is_some(),
        "app.showInFinder" => {
            app.services.reveal.is_some()
                && app
                    .session
                    .active()
                    .and_then(|id| app.session.catalog.photo(id))
                    .is_some_and(|p| matches!(p.source, lightcraft_engine::catalog::Source::File { .. }))
        }
        "file.exportPresets" => app.session.presets.iter().any(|p| !p.builtin),
        s if s.starts_with("dialog.merge") => app.session.targets(&serde_json::json!({})).len() >= 2 && app.merge.final_task.is_none(),
        _ => true,
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct MenuEntry {
    pub id: String,
    pub label: String,
    pub menu: Vec<String>,
    pub shortcut: Option<String>,
    pub enabled: bool,
}

/// The flattened menu model (UI commands + engine commands with menu paths).
pub fn menu_entries(app: &LightcraftApp) -> Vec<MenuEntry> {
    let mut v: Vec<MenuEntry> = UI_COMMANDS
        .iter()
        .filter(|c| !c.3.is_empty())
        .map(|(id, label, sc, m)| MenuEntry {
            id: id.to_string(),
            label: label.to_string(),
            menu: vec![m.to_string()],
            shortcut: sc.map(str::to_string),
            enabled: ui_enabled(app, id),
        })
        .collect();
    for c in app.session.commands() {
        if !c.menu.is_empty() {
            v.push(MenuEntry {
                id: c.id.into(),
                label: c.label.into(),
                menu: c.menu.iter().map(|s| s.to_string()).collect(),
                shortcut: c.shortcut.map(str::to_string),
                enabled: c.enabled,
            });
        }
    }
    v
}

/// Reveal the active photo's original in the system file manager.
fn show_in_finder(app: &mut LightcraftApp) -> Result<Value, String> {
    let id = app.session.active().ok_or("no photo selected")?;
    let path = match app.session.catalog.photo(id).map(|p| p.source.clone()) {
        Some(lightcraft_engine::catalog::Source::File { path }) => path,
        _ => return Err("this photo has no file (demo scene)".into()),
    };
    let reveal = app.services.reveal.as_mut().ok_or("not available here")?;
    reveal(&path)?;
    Ok(json!({"path": path}))
}
