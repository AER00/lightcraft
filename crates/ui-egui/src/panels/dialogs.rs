//! Modal dialogs (new album, rename, create preset, choose settings to copy, export, about, shortcuts).

use lightcraft_develop::SettingsGroup;
use serde_json::json;

use crate::LightcraftApp;
use crate::state::Dialog;
use crate::theme::Tokens;

pub fn show(app: &mut LightcraftApp, ctx: &egui::Context) {
    let Some(mut dlg) = app.ui.dialog.clone() else { return };
    let t = Tokens::get(ctx);
    let screen = ctx.content_rect();
    ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::new("dim"))).rect_filled(screen, 0.0, egui::Color32::from_black_alpha(140));
    let mut close = false;
    let mut confirm = false;
    let title = match &dlg {
        Dialog::NewAlbum { folder: true, .. } => "Create Folder",
        Dialog::NewAlbum { .. } => "Create Album",
        Dialog::RenameAlbum { .. } => "Rename Album",
        Dialog::CreatePreset { .. } => "Create Preset",
        Dialog::CopySettings { .. } => "Choose Edit Settings to Copy",
        Dialog::Export { .. } => "Export",
        Dialog::About => "About LightCraft",
        Dialog::Shortcuts => "Keyboard Shortcuts",
    };
    egui::Window::new(title).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).default_width(380.0).show(
        ctx,
        |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            match &mut dlg {
                Dialog::NewAlbum { name, .. } | Dialog::RenameAlbum { name, .. } => {
                    let r = ui.add(egui::TextEdit::singleline(name).hint_text("Name").desired_width(f32::INFINITY));
                    r.request_focus();
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        confirm = true;
                    }
                }
                Dialog::CreatePreset { name, group } => {
                    ui.add(egui::TextEdit::singleline(name).hint_text("Preset name").desired_width(f32::INFINITY));
                    ui.add(egui::TextEdit::singleline(group).hint_text("Group").desired_width(f32::INFINITY));
                    ui.label(egui::RichText::new("Includes the current settings except crop, masks and remove.").color(t.text_dim));
                }
                Dialog::CopySettings { groups } => {
                    ui.columns(2, |cols| {
                        for (i, g) in SettingsGroup::ALL.iter().enumerate() {
                            let key = serde_json::to_value(g).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
                            let mut on = groups.contains(&key);
                            if cols[i % 2].checkbox(&mut on, g.label()).changed() {
                                if on {
                                    groups.push(key);
                                } else {
                                    groups.retain(|x| *x != key);
                                }
                            }
                        }
                    });
                }
                Dialog::Export { opts, long_edge, limit_kb, dir } => {
                    use lightcraft_engine::export::{ExportFormat as F, SharpenAmount as A, SharpenFor as S};
                    let n = app.session.selection.ids.len().max(1);
                    ui.label(egui::RichText::new(format!("{n} photo{}", if n == 1 { "" } else { "s" })).color(t.text_dim));
                    ui.horizontal(|ui| {
                        ui.label("Format");
                        for (f, l) in [(F::Jpeg, "JPEG"), (F::Png, "PNG"), (F::Tiff, "TIFF"), (F::Webp, "WebP"), (F::Avif, "AVIF")] {
                            ui.selectable_value(&mut opts.format, f, l);
                        }
                    });
                    if matches!(opts.format, F::Jpeg | F::Avif) {
                        ui.add(egui::Slider::new(&mut opts.quality, 1..=100).text("Quality"));
                    }
                    if opts.format == F::Jpeg {
                        ui.add(egui::Slider::new(limit_kb, 0..=20_000).text("Limit file size (KB, 0 = off)"));
                    }
                    let mut full = *long_edge == 0;
                    if ui.checkbox(&mut full, "Full size").changed() {
                        *long_edge = if full { 0 } else { 2048 };
                    }
                    if !full {
                        ui.add(egui::Slider::new(long_edge, 256..=12_000).text("Long edge (px)"));
                    }
                    ui.horizontal(|ui| {
                        ui.label("Sharpen for");
                        for (v, l) in [(S::None, "None"), (S::Screen, "Screen"), (S::Matte, "Matte paper"), (S::Glossy, "Glossy paper")] {
                            ui.selectable_value(&mut opts.sharpen, v, l);
                        }
                    });
                    if opts.sharpen != S::None {
                        ui.horizontal(|ui| {
                            ui.label("Amount");
                            for (v, l) in [(A::Low, "Low"), (A::Standard, "Standard"), (A::High, "High")] {
                                ui.selectable_value(&mut opts.sharpen_amount, v, l);
                            }
                        });
                    }
                    ui.horizontal(|ui| {
                        ui.label("File name");
                        ui.add(egui::TextEdit::singleline(&mut opts.naming).hint_text("{name}-{seq}").desired_width(f32::INFINITY));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Folder");
                        ui.add(egui::TextEdit::singleline(dir).desired_width(f32::INFINITY));
                    });
                }
                Dialog::About => {
                    ui.label(egui::RichText::new("LightCraft").font(t.semibold(20.0)).color(t.text));
                    ui.label(format!("Version {} — a clean-room, pure-Rust photo library and raw developer.", env!("CARGO_PKG_VERSION")));
                    ui.label("MIT OR Apache-2.0. Fonts: Source Sans 3 (OFL). Icons: original.");
                }
                Dialog::Shortcuts => {
                    egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                        egui::Grid::new("shortcuts").striped(true).show(ui, |ui| {
                            for (id, label, sc, _) in crate::menus::UI_COMMANDS {
                                if let Some(sc) = sc {
                                    ui.label(*label);
                                    ui.label(*sc);
                                    ui.label(egui::RichText::new(*id).color(t.text_dim));
                                    ui.end_row();
                                }
                            }
                            for c in lightcraft_engine::command_specs() {
                                if let Some(sc) = c.shortcut {
                                    ui.label(c.label);
                                    ui.label(sc);
                                    ui.label(egui::RichText::new(c.id).color(t.text_dim));
                                    ui.end_row();
                                }
                            }
                        });
                    });
                }
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let informational = matches!(dlg, Dialog::About | Dialog::Shortcuts);
                if !informational && ui.button("Cancel").clicked() {
                    close = true;
                }
                if ui.button(if informational { "Close" } else { "OK" }).clicked() {
                    if informational {
                        close = true;
                    } else {
                        confirm = true;
                    }
                }
            });
        },
    );
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        close = true;
    }
    if confirm {
        let _ = confirm_dialog(app, &dlg);
        close = true;
    }
    app.ui.dialog = if close { None } else { Some(dlg) };
}

/// Apply a dialog's action (also used by `ui.dialog.confirm`).
pub fn confirm_dialog(app: &mut LightcraftApp, dlg: &Dialog) -> Result<serde_json::Value, String> {
    match dlg {
        Dialog::NewAlbum { name, folder } => app.run("album.create", json!({"name": name, "folder": folder, "addSelected": !folder})),
        Dialog::RenameAlbum { id, name } => app.run("album.rename", json!({"id": id, "name": name})),
        Dialog::CreatePreset { name, group } => {
            app.run("preset.create", json!({"name": if name.is_empty() { "My Preset" } else { name }, "group": group}))
        }
        Dialog::CopySettings { groups } => app.run("develop.copy", json!({"groups": groups})),
        Dialog::Export { opts, long_edge, limit_kb, dir } => app.run(
            "app.export",
            json!({
                "format": opts.format, "quality": opts.quality, "longEdge": long_edge, "limitKb": limit_kb,
                "sharpen": opts.sharpen, "sharpenAmount": opts.sharpen_amount, "naming": opts.naming, "dir": dir,
            }),
        ),
        Dialog::About | Dialog::Shortcuts => Ok(serde_json::Value::Null),
    }
}
