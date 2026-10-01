//! Photo Grid (justified rows) and Square Grid. Virtualized: only visible cells request thumbnails.

use std::collections::HashSet;

use egui::{Align2, Color32, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use lightcraft_catalog::{Flag, PhotoId};
use serde_json::json;

use crate::LightcraftApp;
use crate::icons::{Icon, paint};
use crate::render::Slot;
use crate::state::ViewMode;
use crate::theme::Tokens;
use crate::widgets::register;

/// Thumbnail render size (pixels, long edge) for a cell of `pts` points.
fn thumb_px(pts: f32, ppp: f32) -> usize {
    let px = (pts * ppp).ceil() as usize;
    if px <= 256 {
        256
    } else if px <= 384 {
        384
    } else {
        512
    }
}

pub fn show(app: &mut LightcraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let ids = app.session.visible_cloned();
    // header: source title + count
    let (hr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::hover());
    let title = app.session.source.label(&app.session.catalog);
    ui.painter().rect_filled(hr, 0.0, t.canvas);
    ui.painter().text(pos2(hr.left() + 20.0, hr.center().y), Align2::LEFT_CENTER, &title, t.semibold(17.0), t.text);
    let sel_n = app.session.selection.ids.len();
    let cnt = if sel_n > 1 { format!("{sel_n} of {} photos", ids.len()) } else { format!("{} photos", ids.len()) };
    ui.painter().text(pos2(hr.right() - 20.0, hr.center().y), Align2::RIGHT_CENTER, cnt, t.font(12.5), t.text_dim);
    app.canvas_rect = Some(ui.max_rect());
    if ids.is_empty() {
        super::empty_message(ui, ui.max_rect(), "No photos", "Add photos with File → Add Photos (Cmd+Shift+I), or drop them here");
        return;
    }
    let ppp = ui.ctx().pixels_per_point();
    let square = app.ui.view == ViewMode::SquareGrid;
    let target = app.ui.thumb_size;
    let avail_w = ui.available_width() - 8.0;
    // layout rows
    struct Cell {
        id: PhotoId,
        rect: Rect,
    }
    let mut cells: Vec<Cell> = Vec::with_capacity(ids.len());
    let gap = if square { 1.0 } else { 6.0 };
    let mut y = 4.0f32;
    if square {
        let cols = ((avail_w + gap) / (target + gap)).floor().max(1.0);
        let cw = (avail_w - gap * (cols - 1.0)) / cols;
        for (i, id) in ids.iter().enumerate() {
            let (c, r) = ((i as f32) % cols, (i as f32 / cols).floor());
            cells.push(Cell { id: *id, rect: Rect::from_min_size(pos2(4.0 + c * (cw + gap), y + r * (cw + gap)), vec2(cw, cw)) });
        }
    } else {
        // justified rows: accumulate aspect ratios until the row is full
        let aspects: Vec<f32> = ids
            .iter()
            .map(|id| {
                let p = app.session.catalog.photo(*id);
                let (w, h) = p.map(|p| (p.width.max(1) as f32, p.height.max(1) as f32)).unwrap_or((3.0, 2.0));
                let swap = p.is_some_and(|p| p.develop.orientation.swaps_axes());
                let crop = p.map(|p| p.develop.crop.geometry.rect).unwrap_or(lightcraft_geom::Rect::UNIT);
                let (w, h) = if swap { (h, w) } else { (w, h) };
                (w * crop.width() as f32) / (h * crop.height() as f32).max(1e-3)
            })
            .collect();
        let mut i = 0;
        while i < ids.len() {
            let mut sum = 0.0;
            let mut j = i;
            while j < ids.len() {
                sum += aspects[j];
                let h = (avail_w - gap * (j - i) as f32) / sum;
                j += 1;
                if h <= target {
                    break;
                }
            }
            let full = j < ids.len() || (avail_w - gap * (j - i - 1) as f32) / sum <= target;
            let h = if full { (avail_w - gap * (j - i - 1) as f32) / sum } else { target };
            let mut x = 4.0;
            for k in i..j {
                let w = aspects[k] * h;
                cells.push(Cell { id: ids[k], rect: Rect::from_min_size(pos2(x, y), vec2(w, h)) });
                x += w + gap;
            }
            y += h + gap;
            i = j;
        }
    }
    let total_h = cells.last().map(|c| c.rect.bottom()).unwrap_or(0.0) + 12.0;
    let active = app.session.selection.active;
    // keep the active photo in view when it changes by keyboard
    let scroll_to: Option<Rect> = {
        let key = egui::Id::new("grid-last-active");
        let last: Option<PhotoId> = ui.data(|d| d.get_temp(key));
        ui.data_mut(|d| d.insert_temp(key, active));
        if last != active { active.and_then(|a| cells.iter().find(|c| c.id == a).map(|c| c.rect)) } else { None }
    };
    let mut visible_ids = HashSet::new();
    egui::ScrollArea::vertical().id_salt("grid-scroll").auto_shrink([false, false]).show_viewport(ui, |ui, viewport| {
        let (area, _) = ui.allocate_exact_size(vec2(ui.available_width(), total_h), Sense::hover());
        let origin = area.min;
        if let Some(r) = scroll_to {
            ui.scroll_to_rect(r.translate(origin.to_vec2()), None);
        }
        let prefetch = viewport.expand2(vec2(0.0, viewport.height()));
        for c in &cells {
            if !c.rect.intersects(prefetch) {
                continue;
            }
            visible_ids.insert(c.id);
            let r = c.rect.translate(origin.to_vec2());
            let onscreen = c.rect.intersects(viewport);
            cell(app, ui, c.id, r, square, onscreen, ppp);
        }
    });
    app.renderer.evict_thumbs(&visible_ids, 600);
    let _ = (Color32::BLACK, StrokeKind::Inside, Stroke::NONE);
}

fn cell(app: &mut LightcraftApp, ui: &mut egui::Ui, id: PhotoId, r: Rect, square: bool, onscreen: bool, ppp: f32) {
    let t = Tokens::get(ui.ctx());
    let Some(photo) = app.session.catalog.photo(id).cloned() else { return };
    let resp = ui.interact(r, egui::Id::new(("cell", id.0)), Sense::click_and_drag());
    register(ui.ctx(), format!("thumb:{}", id.0), r);
    let selected = app.session.selection.contains(id);
    let active = app.session.selection.active == Some(id);
    let p = ui.painter();
    let img_rect = if square {
        p.rect_filled(r, 0.0, if selected { t.cell_selected } else { t.cell });
        Rect::from_min_max(r.min + vec2(10.0, 24.0), r.max - vec2(10.0, 10.0))
    } else {
        r
    };
    // thumbnail
    let size = thumb_px(img_rect.width().max(img_rect.height()), ppp);
    if let Some(job) = app.session.thumb_job(id, size) {
        app.renderer.request(Slot::Thumb(id), job, if onscreen { 10 } else { 5 });
    }
    if let Some(tex) = app.renderer.textures.get(&Slot::Thumb(id)) {
        let [tw, th] = tex.size;
        let fit = if square {
            let s = (img_rect.width() / tw as f32).min(img_rect.height() / th as f32);
            Rect::from_center_size(img_rect.center(), vec2(tw as f32 * s, th as f32 * s))
        } else {
            img_rect
        };
        p.image(tex.tex.id(), fit, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        if active {
            p.rect_stroke(fit.expand(if square { 2.0 } else { 0.0 }), 0.0, Stroke::new(2.0, Color32::WHITE), StrokeKind::Outside);
        } else if selected {
            p.rect_stroke(fit, 0.0, Stroke::new(2.0, Color32::from_gray(170)), StrokeKind::Outside);
        }
    } else {
        p.rect_filled(img_rect.shrink(if square { 20.0 } else { 0.0 }), 0.0, Color32::from_gray(38));
    }
    // labels and badges
    if square && app.ui.show_filenames {
        let name = photo.file_name.rsplit_once('.').map(|(n, _)| n.to_string()).unwrap_or(photo.file_name.clone());
        p.text(pos2(r.left() + 8.0, r.top() + 12.0), Align2::LEFT_CENTER, name, t.font(10.5), t.text_dim);
        let fmt = photo.format.clone();
        let g = p.layout_no_wrap(fmt, t.semibold(9.0), t.text_label);
        let br = Rect::from_min_size(pos2(r.right() - g.size().x - 14.0, r.top() + 5.0), g.size() + vec2(8.0, 3.0));
        p.rect_filled(br, 2.0, Color32::from_gray(26));
        p.galley(br.min + vec2(4.0, 1.5), g, t.text_label);
    }
    let show_badges = resp.hovered() || selected || photo.rating > 0 || photo.flag != Flag::None;
    if show_badges {
        let bar = Rect::from_min_max(pos2(img_rect.left(), img_rect.bottom() - 24.0), img_rect.right_bottom());
        if resp.hovered() || selected {
            p.rect_filled(bar, 0.0, Color32::from_black_alpha(120));
        }
        let mut x = bar.left() + 6.0;
        for i in 0..photo.rating {
            paint(p, Rect::from_min_size(pos2(x + i as f32 * 13.0, bar.center().y - 6.0), vec2(12.0, 12.0)), Icon::StarFilled, t.star);
        }
        x += photo.rating as f32 * 13.0 + 4.0;
        match photo.flag {
            Flag::Pick => paint(p, Rect::from_min_size(pos2(x, bar.center().y - 7.0), vec2(14.0, 14.0)), Icon::FlagPick, t.pick),
            Flag::Reject => paint(p, Rect::from_min_size(pos2(x, bar.center().y - 7.0), vec2(14.0, 14.0)), Icon::FlagReject, t.reject),
            Flag::None => {}
        }
        if photo.is_edited() {
            paint(p, Rect::from_min_size(pos2(bar.right() - 20.0, bar.center().y - 7.0), vec2(14.0, 14.0)), Icon::Sliders, t.text_label);
        }
    }
    if photo.flag == Flag::Reject {
        p.rect_filled(img_rect, 0.0, Color32::from_black_alpha(110));
    }
    // interaction
    if resp.clicked() {
        let m = ui.input(|i| i.modifiers);
        let mode = if m.shift {
            "range"
        } else if m.command {
            "toggle"
        } else {
            "replace"
        };
        let _ = app.run("library.select", json!({"ids": [id.0], "mode": mode}));
    }
    if resp.double_clicked() {
        let _ = app.run("library.select", json!({"ids": [id.0]}));
        let _ = app.run("view.detail", json!({}));
    }
    resp.context_menu(|ui| context_menu(app, ui, id));
}

pub fn context_menu(app: &mut LightcraftApp, ui: &mut egui::Ui, id: PhotoId) {
    if !app.session.selection.contains(id) {
        let _ = app.run("library.select", json!({"ids": [id.0]}));
    }
    if ui.button("Open in Detail").clicked() {
        let _ = app.run("view.detail", json!({}));
    }
    ui.separator();
    ui.menu_button("Set Rating", |ui| {
        for r in 0..=5 {
            if ui.button(if r == 0 { "No Stars".to_string() } else { "★".repeat(r) }).clicked() {
                let _ = app.run("photo.rate", json!({"rating": r}));
            }
        }
    });
    ui.menu_button("Set Flag", |ui| {
        for (l, f) in [("Pick", "pick"), ("Reject", "reject"), ("Unflagged", "none")] {
            if ui.button(l).clicked() {
                let _ = app.run("photo.flag", json!({"flag": f}));
            }
        }
    });
    ui.menu_button("Add to Album", |ui| {
        let albums: Vec<_> = app.session.catalog.albums().filter(|a| !a.folder).map(|a| (a.id.0, a.name.clone())).collect();
        for (aid, name) in albums {
            if ui.button(name).clicked() {
                let _ = app.run("album.addPhotos", json!({"id": aid}));
            }
        }
    });
    ui.separator();
    if ui.button("Copy Edit Settings").clicked() {
        let _ = app.run("develop.copy", json!({}));
    }
    if ui.add_enabled(app.session.clipboard.is_some(), egui::Button::new("Paste Edit Settings")).clicked() {
        let _ = app.run("develop.paste", json!({}));
    }
    if ui.button("Reset Edits").clicked() {
        let _ = app.run("develop.reset", json!({}));
    }
    ui.menu_button("Photo Merge", |ui| {
        let n = app.session.targets(&json!({})).len();
        for (id, label) in [("dialog.mergeHdr", "HDR…"), ("dialog.mergePanorama", "Panorama…"), ("dialog.mergeHdrPanorama", "HDR Panorama…")] {
            if ui.add_enabled(n >= 2, egui::Button::new(label)).clicked() {
                let _ = app.run(id, json!({}));
            }
        }
    });
    ui.separator();
    if ui.button("Rotate Left").clicked() {
        let _ = app.run("photo.rotateLeft", json!({}));
    }
    if ui.button("Rotate Right").clicked() {
        let _ = app.run("photo.rotateRight", json!({}));
    }
    ui.separator();
    if ui.button("Delete Photo").clicked() {
        let _ = app.run("photo.delete", json!({}));
    }
}
