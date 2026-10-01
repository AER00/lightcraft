//! The Presets column (opens to the left of the Edit panel): grouped presets with hover preview,
//! amount slider, create/favourite.

use std::collections::BTreeMap;

use egui::{Align2, Rect, Sense, pos2, vec2};
use lightcraft_develop::{ControlSpec, Section, Track};
use serde_json::json;

use crate::LightcraftApp;
use crate::icons::{Icon, paint};
use crate::theme::Tokens;
use crate::widgets::{divider, icon_button, register, slider};

pub fn show(app: &mut LightcraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Panel::right("presets_panel")
        .exact_size(t.panel_w)
        .resizable(false)
        .frame(egui::Frame::NONE.fill(t.chrome).stroke(egui::Stroke::new(1.0, t.divider)))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let (hr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::hover());
            ui.painter().text(pos2(hr.left() + 24.0, hr.center().y + 2.0), Align2::LEFT_CENTER, "Presets", t.semibold(15.0), t.text);
            let mut hdr = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(Rect::from_min_max(pos2(hr.right() - 80.0, hr.top()), hr.right_bottom()))
                    .layout(egui::Layout::right_to_left(egui::Align::Center)),
            );
            if icon_button(&mut hdr, "presetCreate", Icon::Plus, vec2(28.0, 28.0), false, app.session.active().is_some(), "Create Preset…").clicked()
            {
                app.ui.dialog = Some(crate::state::Dialog::CreatePreset { name: String::new(), group: "User Presets".into() });
            }
            divider(ui);
            // amount slider (applies to the last applied preset)
            let amt_id = egui::Id::new("preset-amount");
            let last: Option<(String, f64)> = ui.data(|d| d.get_temp(amt_id));
            if let Some((pid, amount)) = last.clone() {
                let spec = ControlSpec {
                    id: "presetAmount",
                    label: "Amount",
                    section: Section::Profile,
                    min: 0.0,
                    max: 200.0,
                    default: 100.0,
                    step: 1.0,
                    decimals: 0,
                    track: Track::Plain,
                };
                let out = slider(ui, &spec, amount, true, None);
                if let Some(v) = out.value {
                    // re-apply from the pre-preset state: undo last preset step then apply with the new amount
                    let _ = app.run("edit.undo", json!({}));
                    let _ = app.run("preset.apply", json!({"id": pid, "amount": v}));
                    ui.data_mut(|d| d.insert_temp(amt_id, (pid, v)));
                }
                divider(ui);
            }
            egui::ScrollArea::vertical().id_salt("presets-scroll").auto_shrink([false, false]).show(ui, |ui| {
                let mut groups: BTreeMap<String, Vec<(String, String, bool)>> = BTreeMap::new();
                for p in &app.session.presets {
                    groups.entry(p.group.clone()).or_default().push((p.id.clone(), p.name.clone(), p.favorite));
                }
                for (g, items) in groups {
                    let open_id = egui::Id::new(("preset-group", &g));
                    let open: bool = ui.data(|d| d.get_temp(open_id)).unwrap_or(true);
                    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
                    register(ui.ctx(), format!("presetGroup:{g}"), r);
                    paint(
                        ui.painter(),
                        Rect::from_center_size(pos2(r.left() + 22.0, r.center().y), vec2(12.0, 12.0)),
                        if open { Icon::ChevronDown } else { Icon::ChevronRight },
                        t.text_label,
                    );
                    ui.painter().text(pos2(r.left() + 36.0, r.center().y), Align2::LEFT_CENTER, &g, t.semibold(13.0), t.text_label);
                    if resp.clicked() {
                        ui.data_mut(|d| d.insert_temp(open_id, !open));
                    }
                    if !open {
                        continue;
                    }
                    for (pid, name, fav) in items {
                        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
                        register(ui.ctx(), format!("preset:{pid}"), r);
                        let active = last.as_ref().is_some_and(|(l, _)| *l == pid);
                        if active {
                            ui.painter().rect_filled(r, 0.0, t.tool_active);
                        } else if resp.hovered() {
                            ui.painter().rect_filled(r, 0.0, t.hover.gamma_multiply(0.7));
                        }
                        ui.painter().text(pos2(r.left() + 40.0, r.center().y), Align2::LEFT_CENTER, &name, t.font(13.0), t.text_label);
                        if fav {
                            paint(
                                ui.painter(),
                                Rect::from_center_size(pos2(r.right() - 20.0, r.center().y), vec2(12.0, 12.0)),
                                Icon::StarFilled,
                                t.star,
                            );
                        }
                        if resp.clicked() {
                            let _ = app.run("preset.apply", json!({"id": pid, "amount": 100}));
                            ui.data_mut(|d| d.insert_temp(amt_id, (pid.clone(), 100.0)));
                            app.toast(ui.ctx(), format!("Preset: {name}"));
                        }
                        resp.context_menu(|ui| {
                            if ui.button(if fav { "Remove from Favorites" } else { "Add to Favorites" }).clicked() {
                                let _ = app.run("preset.favorite", json!({"id": pid}));
                            }
                            if ui.button("Delete Preset").clicked() {
                                let _ = app.run("preset.delete", json!({"id": pid}));
                            }
                        });
                    }
                }
                ui.add_space(30.0);
            });
        });
}
