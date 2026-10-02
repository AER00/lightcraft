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
    if let Some(n) = count {
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
            let stats: Vec<_> = app.session.catalog.photos().filter(|p| !p.deleted).map(|p| p.flag).collect();
            let total = stats.len();
            let picks = stats.iter().filter(|f| **f == lightcraft_catalog::Flag::Pick).count();
            let deleted = app.session.catalog.photos().filter(|p| p.deleted).count();
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
                // By date
                let (dr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
                ui.painter().text(pos2(dr.left() + 18.0, dr.center().y), Align2::LEFT_CENTER, "By Date", t.semibold(13.5), t.text_label);
                for g in app.session.catalog.date_groups() {
                    let sel = app.session.filter.date.as_deref() == Some(g.year.as_str());
                    if row(app, ui, &format!("year:{}", g.year), Icon::Clock, &g.year, Some(g.count), sel, 0.0).clicked() {
                        let v = if sel { serde_json::Value::Null } else { json!(g.year) };
                        let _ = app.run("library.filter", json!({"date": v}));
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
            let mut resp = row(app, ui, &format!("album:{}", a.id.0), icon, &a.name, Some(n), sel, indent);
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

fn folder_menu(app: &mut LightcraftApp, resp: &egui::Response, a: &Album) {
    resp.context_menu(|ui| {
        if !a.folder && !a.is_smart() && ui.button("Add Selected Photos").clicked() {
            let _ = app.run("album.addPhotos", json!({"id": a.id.0}));
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
