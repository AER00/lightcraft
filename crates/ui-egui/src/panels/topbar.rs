//! The top bar: sidebar toggle, back/forward, search, filter, and the right-hand icons.

use egui::{Align2, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use serde_json::json;

use crate::LightcraftApp;
use crate::icons::{Icon, paint};
use crate::theme::Tokens;
use crate::widgets::{icon_button, register};

pub fn show(app: &mut LightcraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let left = if app.integrated_titlebar { 78 } else { 10 };
    egui::Panel::top("top_bar")
        .exact_size(t.top_bar_h)
        .frame(egui::Frame::NONE.fill(t.chrome).inner_margin(egui::Margin { left, right: 12, top: 0, bottom: 0 }))
        .show(ui, |ui| {
            let full = ui.max_rect();
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if icon_button(ui, "sidebar", Icon::Sidebar, vec2(30.0, 30.0), app.ui.left_panel, true, "Show/hide My Photos panel").clicked() {
                    let _ = app.run("view.leftPanel", json!({}));
                }
                ui.add_space(8.0);
                if icon_button(ui, "back", Icon::Back, vec2(30.0, 30.0), false, true, "Back").clicked() {
                    let _ = app.run("view.back", json!({}));
                }
                icon_button(ui, "forward", Icon::Forward, vec2(30.0, 30.0), false, false, "Forward");
            });
            // search field (centred on the window)
            let sw = 640.0f32.min(full.width() - 460.0).max(200.0);
            let sr = Rect::from_center_size(pos2(full.center().x, full.center().y), vec2(sw, 28.0));
            let id = egui::Id::new("search-field");
            let focused = ui.memory(|m| m.has_focus(id));
            ui.painter().rect(
                sr,
                4.0,
                if focused { t.canvas } else { t.field },
                Stroke::new(1.0, if focused { t.accent } else { t.field_border }),
                StrokeKind::Inside,
            );
            register(ui.ctx(), "field:search", sr);
            let mut child =
                ui.new_child(egui::UiBuilder::new().max_rect(sr.shrink2(vec2(10.0, 4.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
            let empty = app.ui.search.is_empty();
            if empty && !focused {
                let g = child.painter().layout_no_wrap("Search Photos".into(), t.font(13.5), t.text_dim);
                let w = g.size().x + 24.0;
                let x0 = sr.center().x - w / 2.0;
                paint(child.painter(), Rect::from_min_size(pos2(x0, sr.center().y - 8.0), vec2(16.0, 16.0)), Icon::Search, t.text_dim);
                child.painter().galley(pos2(x0 + 24.0, sr.center().y - g.size().y / 2.0), g, t.text_dim);
            }
            let resp = child.add(
                egui::TextEdit::singleline(&mut app.ui.search)
                    .id(id)
                    .frame(egui::Frame::NONE)
                    .desired_width(sr.width() - 20.0)
                    .font(t.font(13.5))
                    .text_color(t.text),
            );
            if resp.changed() {
                let q = app.ui.search.clone();
                let _ = app.run("library.filter", json!({"text": q}));
            }
            // filter icon right of the search field
            let fr = Rect::from_center_size(pos2(sr.right() + 22.0, sr.center().y), vec2(28.0, 28.0));
            let fresp = ui.interact(fr, egui::Id::new("filter-btn"), Sense::click());
            register(ui.ctx(), "icon:filter", fr);
            let filtering = app.session.filter != Default::default();
            paint(
                ui.painter(),
                fr.shrink(6.0),
                Icon::Filter,
                if filtering {
                    t.accent
                } else if fresp.hovered() {
                    t.text
                } else {
                    t.icon
                },
            );
            if fresp.clicked() {
                let _ = app.run("view.filterBar", json!({}));
            }
            // right icons
            let mut x = full.right() - 18.0;
            for (id, icon, tip, cmd) in [
                ("cloud", Icon::Cloud, "Local library — no cloud account needed", ""),
                ("help", Icon::Help, "Keyboard shortcuts", "app.shortcuts"),
                ("share", Icon::Share, "Export", "dialog.export"),
                ("bell", Icon::Bell, "Activity", "panel.activity"),
            ] {
                let r = Rect::from_center_size(pos2(x, full.center().y), vec2(28.0, 28.0));
                let resp = ui.interact(r, egui::Id::new(("top", id)), Sense::click()).on_hover_text(tip);
                register(ui.ctx(), format!("icon:{id}"), r);
                paint(ui.painter(), r.shrink(5.0), icon, if resp.hovered() { t.text } else { t.icon });
                if resp.clicked() && !cmd.is_empty() {
                    let _ = app.run(cmd, json!({}));
                }
                x -= 40.0;
            }
            let _ = Align2::CENTER_CENTER;
        });
}
