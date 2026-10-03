//! The left "My Photos" panel: library sources, albums tree, and date groups.

use egui::{Align2, Rect, Sense, pos2, vec2};
use lightcraft_catalog::{Album, AlbumId, KeywordNode};
use lightcraft_engine::LibrarySource;
use serde_json::json;

use crate::LightcraftApp;
use crate::icons::{Icon, paint};
use crate::theme::Tokens;
use crate::widgets::{icon_button, register};

fn row(
    app: &mut LightcraftApp,
    ui: &mut egui::Ui,
    id: &str,
    icon: Icon,
    label: &str,
    count: Option<usize>,
    selected: bool,
    indent: f32,
) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 29.0), Sense::click());
    register(ui.ctx(), format!("source:{id}"), r);
    let name = match count {
        Some(n) => format!("{label}, {n} photos"),
        None => label.to_string(),
    };
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, &name));
    let inner = r.shrink2(vec2(8.0, 0.0));
    if selected {
        ui.painter().rect_filled(inner, 4.0, t.canvas);
    } else if resp.hovered() {
        ui.painter().rect_filled(inner, 4.0, t.hover.gamma_multiply(0.6));
    }
    paint(
        ui.painter(),
        Rect::from_min_size(pos2(r.left() + 18.0 + indent, r.center().y - 8.0), vec2(16.0, 16.0)),
        icon,
        if selected { t.text } else { t.icon },
    );
    ui.painter().text(
        pos2(r.left() + 42.0 + indent, r.center().y),
        Align2::LEFT_CENTER,
        label,
        t.font(13.5),
        if selected { t.text } else { t.text_label },
    );
    if let Some(n) = count.filter(|_| app.ui.show_counts) {
        ui.painter().text(pos2(r.right() - 18.0, r.center().y), Align2::RIGHT_CENTER, n.to_string(), t.font(12.5), t.text_dim);
    }
    let _ = app;
    resp
}

pub fn show(app: &mut LightcraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Panel::left("left_panel")
        .exact_size(t.left_w)
        .resizable(false)
        .frame(egui::Frame::NONE.fill(t.chrome).stroke(egui::Stroke::new(1.0, t.divider)))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let (hr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::hover());
            ui.painter().text(pos2(hr.left() + 18.0, hr.center().y), Align2::LEFT_CENTER, "My Photos", t.semibold(15.0), t.text);
            let stats: Vec<_> = app.session.catalog.photos().filter(|p| p.in_library()).map(|p| p.flag).collect();
            let total = stats.len();
            let picks = stats.iter().filter(|f| **f == lightcraft_catalog::Flag::Pick).count();
            let deleted = app.session.catalog.photos().filter(|p| p.deleted && !p.local).count();
            egui::ScrollArea::vertical().id_salt("left-scroll").auto_shrink([false, false]).show(ui, |ui| {
                let src = app.session.source;
                for (id, icon, label, count, s) in [
                    ("all", Icon::Photos, "All Photos", Some(total), LibrarySource::All),
                    ("recentlyAdded", Icon::Clock, "Recently Added", None, LibrarySource::RecentlyAdded),
                    ("picks", Icon::FlagPick, "Picks", Some(picks), LibrarySource::Picks),
                ] {
                    if row(app, ui, id, icon, label, count, src == s, 0.0).clicked() {
                        let _ = app.run("library.source", json!({"kind": id}));
                    }
                }
                // photos whose files can't be found (checked every few seconds, not every frame)
                let missing = missing_count(app, ui);
                if (missing > 0 || src == LibrarySource::Missing)
                    && row(app, ui, "missing", Icon::Folder, "Missing Photos", Some(missing), src == LibrarySource::Missing, 0.0).clicked()
                {
                    let _ = app.run("library.source", json!({"kind": "missing"}));
                }
                ui.add_space(10.0);
                // Albums header
                let (ar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
                ui.painter().text(pos2(ar.left() + 18.0, ar.center().y), Align2::LEFT_CENTER, "Albums", t.semibold(13.5), t.text_label);
                let mut hdr = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(Rect::from_min_max(pos2(ar.right() - 50.0, ar.top()), ar.right_bottom()))
                        .layout(egui::Layout::right_to_left(egui::Align::Center)),
                );
                let plus = icon_button(&mut hdr, "albumNew", Icon::Plus, vec2(26.0, 26.0), false, true, "Create Album");
                egui::Popup::menu(&plus).show(|ui| {
                    if ui.button("Create Album…").clicked() {
                        app.ui.dialog = Some(crate::state::Dialog::NewAlbum { name: String::new(), folder: false });
                    }
                    if ui.button("Create Smart Album…").clicked() {
                        app.ui.dialog = Some(crate::state::Dialog::SmartRules {
                            id: None,
                            name: String::new(),
                            rules: lightcraft_catalog::RuleSet { rules: vec![crate::panels::rules_editor::new_rule()], ..Default::default() },
                        });
                    }
                    if ui.button("Create Smart Album from Filter…").clicked() {
                        app.ui.dialog = Some(crate::state::Dialog::NewSmartAlbum { name: String::new() });
                    }
                    if ui.button("Create Folder…").clicked() {
                        app.ui.dialog = Some(crate::state::Dialog::NewAlbum { name: String::new(), folder: true });
                    }
                });
                let albums: Vec<Album> = app.session.catalog.albums().cloned().collect();
                albums_tree(app, ui, &albums, None, 0.0);
                ui.add_space(10.0);
                local_section(app, ui);
                // By date
                let (dr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
                ui.painter().text(pos2(dr.left() + 18.0, dr.center().y), Align2::LEFT_CENTER, "By Date", t.semibold(13.5), t.text_label);
                for g in app.session.catalog.date_groups() {
                    // year → month → day; a click filters by that prefix, the triangle opens a level
                    if date_row(app, ui, &g.year, &g.year, g.count, 0.0) {
                        for (m, n) in &g.months {
                            let label = lightcraft_catalog::dates::group_label(m).split(' ').next().unwrap_or(m).to_string();
                            if date_row(app, ui, m, &label, *n, 16.0) {
                                for (d, n) in g.days.iter().filter(|(d, _)| d.starts_with(m.as_str())) {
                                    let label = lightcraft_catalog::dates::group_label(d);
                                    // "Sunday, 20 September 2026" → "Sunday, 20"
                                    let label = label.rsplitn(3, ' ').nth(2).unwrap_or(&label).to_string();
                                    date_row(app, ui, d, &label, *n, 32.0);
                                }
                            }
                        }
                    }
                }
                keywords_section(app, ui);
                ui.add_space(10.0);
                if row(app, ui, "recentlyDeleted", Icon::Trash, "Recently Deleted", Some(deleted), src == LibrarySource::RecentlyDeleted, 0.0)
                    .clicked()
                {
                    let _ = app.run("library.source", json!({"kind": "recentlyDeleted"}));
                }
            });
        });
}

/// How many library photos have no file (cached in egui memory, refreshed every 5 s).
fn missing_count(app: &mut LightcraftApp, ui: &mut egui::Ui) -> usize {
    let id = egui::Id::new("missing-count");
    let now = ui.input(|i| i.time);
    let rev = app.session.catalog.revision;
    if let Some((n, at, r)) = ui.data(|d| d.get_temp::<(usize, f64, u64)>(id))
        && now - at < 5.0
        && r == rev
    {
        return n;
    }
    let n = lightcraft_engine::cmd::missing::missing(&app.session).len();
    ui.data_mut(|d| d.insert_temp(id, (n, now, rev)));
    n
}

/// Folders on this computer to browse without adding (Lightroom's Local): Pictures, Desktop,
/// Downloads, the home folder, the folder being browsed, and Browse Folder….
fn local_section(app: &mut LightcraftApp, ui: &mut egui::Ui) {
    if cfg!(target_arch = "wasm32") {
        return;
    }
    let t = Tokens::get(ui.ctx());
    let (lr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
    ui.painter().text(pos2(lr.left() + 18.0, lr.center().y), Align2::LEFT_CENTER, "Local", t.semibold(13.5), t.text_label);
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_default();
    let mut places: Vec<(String, String)> = Vec::new();
    if !home.is_empty() {
        for (name, sub) in [("Pictures", "Pictures"), ("Desktop", "Desktop"), ("Downloads", "Downloads"), ("Home", "")] {
            let p = if sub.is_empty() { home.clone() } else { format!("{home}/{sub}") };
            if std::path::Path::new(&p).is_dir() {
                places.push((name.to_string(), p));
            }
        }
    }
    let browsing = app.session.browse.clone().filter(|_| app.session.source == LibrarySource::Folder);
    if let Some(b) = &browsing
        && !places.iter().any(|(_, p)| *p == b.path)
    {
        let name = std::path::Path::new(&b.path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| b.path.clone());
        places.push((name, b.path.clone()));
    }
    let current = browsing.as_ref().map(|b| b.path.clone());
    for (name, path) in places {
        folder_tree(app, ui, &name, &path, 0.0, current.as_deref());
    }
    if app.services.pick_folder.is_some() && row(app, ui, "local:browse", Icon::Plus, "Browse Folder…", None, false, 0.0).clicked() {
        let picked = app.services.pick_folder.as_mut().and_then(|f| f());
        if let Some(path) = picked
            && let Err(e) = app.run("library.browse", json!({"path": path}))
        {
            app.toast(ui.ctx(), e);
        }
    }
    ui.add_space(10.0);
}

/// The subfolders of `path` (not hidden ones), sorted; listed at most every 2 s per folder.
fn subfolders(ui: &egui::Ui, path: &str) -> Vec<(String, String)> {
    let id = egui::Id::new(("subfolders", path.to_string()));
    let now = ui.input(|i| i.time);
    if let Some((t, v)) = ui.data(|d| d.get_temp::<(f64, Vec<(String, String)>)>(id))
        && now - t < 2.0
    {
        return v;
    }
    let mut v: Vec<(String, String)> = std::fs::read_dir(path)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    (!name.starts_with('.')).then(|| (name, e.path().to_string_lossy().to_string()))
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort_by_key(|(n, _)| n.to_lowercase());
    ui.data_mut(|d| d.insert_temp(id, (now, v.clone())));
    v
}

/// A folder on disk with a disclosure triangle: click browses it, the triangle lists its
/// subfolders (expanded on the way to the folder being browsed).
fn folder_tree(app: &mut LightcraftApp, ui: &mut egui::Ui, name: &str, path: &str, indent: f32, current: Option<&str>) {
    let t = Tokens::get(ui.ctx());
    let open_id = egui::Id::new(("folder-open", path.to_string()));
    let on_the_way = current.is_some_and(|c| c != path && std::path::Path::new(c).starts_with(path));
    let mut open: bool = ui.data(|d| d.get_temp(open_id)).unwrap_or(on_the_way);
    let sel = current == Some(path);
    let resp = row(app, ui, &format!("local:{path}"), Icon::Folder, name, None, sel, indent + 12.0).on_hover_text(path);
    let c = pos2(resp.rect.left() + 10.0 + indent, resp.rect.center().y);
    let tri = Rect::from_center_size(c, vec2(14.0, 14.0));
    let tr = ui.interact(tri, egui::Id::new(("folder-tri", path.to_string())), Sense::click());
    register(ui.ctx(), format!("folderToggle:{path}"), tri);
    let col = if tr.hovered() { t.text } else { t.text_dim };
    let pts = if open {
        vec![c + vec2(-4.0, -2.0), c + vec2(4.0, -2.0), c + vec2(0.0, 3.0)]
    } else {
        vec![c + vec2(-2.0, -4.0), c + vec2(3.0, 0.0), c + vec2(-2.0, 4.0)]
    };
    ui.painter().add(egui::Shape::convex_polygon(pts, col, egui::Stroke::NONE));
    if tr.clicked() {
        open = !open;
        ui.data_mut(|d| d.insert_temp(open_id, open));
    } else if resp.clicked()
        && let Err(e) = app.run("library.browse", json!({"path": path}))
    {
        app.toast(ui.ctx(), e);
    }
    if open && indent < 12.0 * 8.0 {
        for (n, p) in subfolders(ui, path) {
            folder_tree(app, ui, &n, &p, indent + 12.0, current);
        }
    }
}

/// One By Date row (`key`: `YYYY`, `YYYY-MM` or `YYYY-MM-DD`); returns whether it is open.
fn date_row(app: &mut LightcraftApp, ui: &mut egui::Ui, key: &str, label: &str, count: usize, indent: f32) -> bool {
    let t = Tokens::get(ui.ctx());
    let open_id = egui::Id::new(("date-open", key.to_string()));
    let mut open: bool = ui.data(|d| d.get_temp(open_id)).unwrap_or(false);
    let sel = app.session.filter.date.as_deref() == Some(key);
    let resp = row(app, ui, &format!("date:{key}"), Icon::Clock, label, Some(count), sel, indent);
    if key.len() < 10 {
        let c = pos2(resp.rect.left() + 10.0 + indent, resp.rect.center().y);
        let tri = Rect::from_center_size(c, vec2(14.0, 14.0));
        let tr = ui.interact(tri, egui::Id::new(("date-tri", key.to_string())), Sense::click());
        register(ui.ctx(), format!("dateToggle:{key}"), tri);
        let col = if tr.hovered() { t.text } else { t.text_dim };
        let pts = if open {
            vec![c + vec2(-4.0, -2.0), c + vec2(4.0, -2.0), c + vec2(0.0, 3.0)]
        } else {
            vec![c + vec2(-2.0, -4.0), c + vec2(3.0, 0.0), c + vec2(-2.0, 4.0)]
        };
        ui.painter().add(egui::Shape::convex_polygon(pts, col, egui::Stroke::NONE));
        if tr.clicked() {
            open = !open;
            ui.data_mut(|d| d.insert_temp(open_id, open));
        }
    }
    if resp.clicked() {
        let v = if sel { serde_json::Value::Null } else { json!(key) };
        let _ = app.run("library.filter", json!({"date": v}));
    }
    open
}

fn albums_tree(app: &mut LightcraftApp, ui: &mut egui::Ui, all: &[Album], parent: Option<AlbumId>, indent: f32) {
    let mut kids: Vec<&Album> = all.iter().filter(|a| a.parent == parent).collect();
    kids.sort_by_key(|a| (!a.folder, a.name.to_lowercase()));
    for a in kids {
        if a.folder {
            let open_id = egui::Id::new(("folder-open", a.id.0));
            let open: bool = ui.data(|d| d.get_temp(open_id)).unwrap_or(true);
            let resp = row(app, ui, &format!("folder:{}", a.id.0), Icon::Folder, &a.name, None, false, indent);
            if resp.clicked() {
                ui.data_mut(|d| d.insert_temp(open_id, !open));
            }
            folder_menu(app, &resp, a);
            if open {
                albums_tree(app, ui, all, Some(a.id), indent + 16.0);
            }
        } else {
            let sel = app.session.source == LibrarySource::Album(a.id);
            let icon = if a.is_smart() { Icon::SmartAlbum } else { Icon::Album };
            let n = app.session.catalog.album_count(a.id);
            // the album B adds to is marked "+"
            let target =
                app.session.target_album.filter(|t| app.session.catalog.album(*t).is_some()).or_else(|| app.session.catalog.quick_collection());
            let label = if target == Some(a.id) { format!("{} +", a.name) } else { a.name.clone() };
            let mut resp = row(app, ui, &format!("album:{}", a.id.0), icon, &label, Some(n), sel, indent);
            if !a.is_smart() {
                drop_target(app, ui, &resp, a);
            }
            if let Some(rules) = &a.smart {
                resp = resp.on_hover_text(format!("Smart album: {}", rules.describe()));
            }
            if resp.clicked() {
                let _ = app.run("library.source", json!({"kind": "album", "id": a.id.0}));
            }
            folder_menu(app, &resp, a);
        }
    }
}

/// An album row while photos are dragged from the grid: highlighted under the pointer; a
/// release there adds them.
fn drop_target(app: &mut LightcraftApp, ui: &mut egui::Ui, resp: &egui::Response, a: &Album) {
    let Some(ids) = app.ui.dragging_photos.clone() else { return };
    let over = ui.input(|i| i.pointer.latest_pos()).is_some_and(|p| resp.rect.contains(p));
    if !over {
        return;
    }
    let t = Tokens::get(ui.ctx());
    ui.painter().rect_stroke(resp.rect.shrink2(vec2(8.0, 1.0)), 4.0, egui::Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
    if ui.input(|i| i.pointer.any_released()) {
        let n = ids.len();
        match app.run("album.addPhotos", json!({"id": a.id.0, "ids": ids})) {
            Ok(_) => app.toast(ui.ctx(), format!("Added {n} photo{} to “{}”", if n == 1 { "" } else { "s" }, a.name)),
            Err(e) => app.toast(ui.ctx(), e),
        }
        app.ui.dragging_photos = None;
    }
}

fn folder_menu(app: &mut LightcraftApp, resp: &egui::Response, a: &Album) {
    resp.context_menu(|ui| {
        if !a.folder && !a.is_smart() && ui.button("Add Selected Photos").clicked() {
            let _ = app.run("album.addPhotos", json!({"id": a.id.0}));
        }
        if !a.folder && !a.is_smart() {
            let is_target = app.session.target_album == Some(a.id) || (app.session.target_album.is_none() && a.quick);
            if !is_target && ui.button("Set as Target Album (B adds to it)").clicked() {
                let _ = app.run("album.setTarget", json!({"id": if a.quick { serde_json::Value::Null } else { json!(a.id.0) }}));
            }
            if is_target && !a.quick && ui.button("Stop Using as Target Album").clicked() {
                let _ = app.run("album.setTarget", json!({"id": null}));
            }
        }
        if a.quick && ui.button("Clear Quick Collection").clicked() {
            let _ = app.run("album.clearQuick", json!({}));
        }
        if a.is_smart() && ui.button("Edit Smart Album…").clicked() {
            // older smart albums keep their filter fields; the editor works on the rule set
            let rules = a.smart.as_ref().and_then(|f| f.rule_set.clone()).unwrap_or_default();
            app.ui.dialog = Some(crate::state::Dialog::SmartRules { id: Some(a.id.0), name: a.name.clone(), rules });
        }
        if a.is_smart() && ui.button("Update Rules from Current Filter").clicked() {
            let _ = app.run("album.setRules", json!({"id": a.id.0, "fromView": true}));
        }
        if !a.folder {
            // export: show the album, select its photos, then the dialog / a preset
            let show_all = |app: &mut LightcraftApp| {
                let _ = app.run("library.source", json!({"kind": "album", "id": a.id.0}));
                let _ = app.run("library.selectAll", json!({}));
            };
            let has_photos = app.session.catalog.album_count(a.id) > 0;
            if ui.add_enabled(has_photos, egui::Button::new("Export Album…")).clicked() {
                show_all(app);
                let _ = app.run("dialog.export", json!({}));
            }
            ui.add_enabled_ui(has_photos, |ui| {
                ui.menu_button("Export Album with Preset", |ui| {
                    for (p, _) in app.session.all_export_presets() {
                        if ui.button(&p.name).clicked() {
                            show_all(app);
                            if let Err(e) = app.run("app.export", json!({"preset": p.name, "background": true})) {
                                app.toast(ui.ctx(), e);
                            }
                        }
                    }
                });
            });
            ui.separator();
        }
        // move into another folder (not into itself or one of its own subfolders)
        let mut folders: Vec<(u64, String)> =
            app.session.catalog.albums().filter(|f| f.folder && !is_within(app, f.id, a.id)).map(|f| (f.id.0, f.name.clone())).collect();
        folders.sort_by_key(|(_, n)| n.to_lowercase());
        ui.menu_button("Move to", |ui| {
            if ui.add_enabled(a.parent.is_some(), egui::Button::new("Top Level")).clicked() {
                let _ = app.run("album.move", json!({"id": a.id.0, "parent": null}));
            }
            for (fid, name) in &folders {
                if ui.add_enabled(a.parent.map(|p| p.0) != Some(*fid), egui::Button::new(name)).clicked() {
                    let _ = app.run("album.move", json!({"id": a.id.0, "parent": fid}));
                }
            }
        });
        if ui.button("Rename…").clicked() {
            app.ui.dialog = Some(crate::state::Dialog::RenameAlbum { id: a.id.0, name: a.name.clone() });
        }
        if ui.button("Delete").clicked() {
            let _ = app.run("album.delete", json!({"id": a.id.0}));
        }
    });
}

/// Whether `id` is `ancestor` or lies inside it.
fn is_within(app: &LightcraftApp, id: lightcraft_catalog::AlbumId, ancestor: lightcraft_catalog::AlbumId) -> bool {
    let mut cur = Some(id);
    let mut guard = 0;
    while let Some(c) = cur {
        if c == ancestor {
            return true;
        }
        cur = app.session.catalog.album(c).and_then(|x| x.parent);
        guard += 1;
        if guard > 64 {
            break;
        }
    }
    false
}

/// "Keywords": the library's keyword tree with photo counts (`a|b|c` keywords nest). A click
/// filters the grid by the keyword (children included), the triangle opens a level, and the
/// context menu renames, merges or deletes the keyword across the library.
fn keywords_section(app: &mut LightcraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let tree = app.session.catalog.keyword_tree();
    if tree.is_empty() {
        return;
    }
    ui.add_space(10.0);
    let (kr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
    ui.painter().text(pos2(kr.left() + 18.0, kr.center().y), Align2::LEFT_CENTER, "Keywords", t.semibold(13.5), t.text_label);
    keyword_rows(app, ui, &tree, 0.0);
}

fn keyword_rows(app: &mut LightcraftApp, ui: &mut egui::Ui, nodes: &[KeywordNode], indent: f32) {
    let t = Tokens::get(ui.ctx());
    for n in nodes {
        let open_id = egui::Id::new(("kw-open", n.path.to_lowercase()));
        let mut open: bool = ui.data(|d| d.get_temp(open_id)).unwrap_or(false);
        let sel = app.session.filter.keyword.as_deref().is_some_and(|k| k.eq_ignore_ascii_case(&n.path));
        let resp = row(app, ui, &format!("keyword:{}", n.path), Icon::Tag, &n.name, Some(n.count), sel, indent);
        if !n.children.is_empty() {
            // disclosure triangle left of the icon
            let c = pos2(resp.rect.left() + 10.0 + indent, resp.rect.center().y);
            let tri = Rect::from_center_size(c, vec2(14.0, 14.0));
            let tr = ui.interact(tri, egui::Id::new(("kw-tri", n.path.to_lowercase())), Sense::click());
            register(ui.ctx(), format!("keywordToggle:{}", n.path), tri);
            let col = if tr.hovered() { t.text } else { t.text_dim };
            let pts = if open {
                vec![c + vec2(-4.0, -2.0), c + vec2(4.0, -2.0), c + vec2(0.0, 3.0)]
            } else {
                vec![c + vec2(-2.0, -4.0), c + vec2(3.0, 0.0), c + vec2(-2.0, 4.0)]
            };
            ui.painter().add(egui::Shape::convex_polygon(pts, col, egui::Stroke::NONE));
            if tr.clicked() {
                open = !open;
                ui.data_mut(|d| d.insert_temp(open_id, open));
            }
        }
        let resp = resp.on_hover_text(if n.children.is_empty() { n.path.clone() } else { format!("{} (includes the keywords below it)", n.path) });
        if resp.clicked() {
            let v = if sel { serde_json::Value::Null } else { json!(n.path) };
            let _ = app.run("library.filter", json!({"keyword": v}));
        }
        resp.context_menu(|ui| {
            let has_sel = app.session.active().is_some();
            if ui.add_enabled(has_sel, egui::Button::new("Add to Selected Photos")).clicked() {
                let _ = app.run("photo.setMeta", json!({"addKeywords": [n.path]}));
            }
            if ui.add_enabled(has_sel, egui::Button::new("Remove from Selected Photos")).clicked() {
                let _ = app.run("photo.setMeta", json!({"removeKeywords": [n.path]}));
            }
            ui.separator();
            if ui.button("Rename Keyword…").clicked() {
                app.ui.dialog = Some(crate::state::Dialog::RenameKeyword { from: n.path.clone(), to: n.path.clone() });
            }
            if ui.button("Merge into…").clicked() {
                app.ui.dialog = Some(crate::state::Dialog::MergeKeywords { from: vec![n.path.clone()], into: String::new() });
            }
            if ui.button("Delete Keyword").clicked() {
                let _ = app.run("keyword.delete", json!({"keyword": n.path}));
            }
        });
        if open && !n.children.is_empty() {
            keyword_rows(app, ui, &n.children, indent + 16.0);
        }
    }
}
