//! Modal dialogs (new album, rename, create preset, choose settings to copy, export, about, shortcuts).

use lightcraft_develop::{ControlSpec, Section, SettingsGroup, Track};
use serde_json::json;

use crate::LightcraftApp;
use crate::state::Dialog;
use crate::theme::Tokens;

pub fn show(app: &mut LightcraftApp, ctx: &egui::Context) {
    let Some(mut dlg) = app.ui.dialog.clone() else { return };
    let t = Tokens::get(ctx);
    let screen = ctx.content_rect();
    // The backdrop is an area below the dialog window (a bare `Middle` layer painter would be
    // painted after every area — i.e. over the dialog too).
    egui::Area::new(egui::Id::new("dialog-dim")).order(egui::Order::Middle).fixed_pos(screen.min).interactable(false).show(ctx, |ui| {
        ui.painter().rect_filled(screen, 0.0, egui::Color32::from_black_alpha(140));
    });
    let mut close = false;
    let mut confirm = false;
    let title = match &dlg {
        Dialog::NewAlbum { folder: true, .. } => "Create Folder",
        Dialog::NewAlbum { .. } => "Create Album",
        Dialog::RenameAlbum { .. } => "Rename Album",
        Dialog::Rename { .. } => "Rename Photos",
        Dialog::RenameKeyword { .. } => "Rename Keyword",
        Dialog::MergeKeywords { .. } => "Merge Keywords",
        Dialog::NewSmartAlbum { .. } => "Create Smart Album",
        Dialog::AutoStack { .. } => "Auto-Stack by Capture Time",
        Dialog::CreatePreset { .. } => "Create Preset",
        Dialog::CopySettings { .. } => "Choose Edit Settings to Copy",
        Dialog::Export { .. } => "Export",
        Dialog::Merge { opts } => opts.title(),
        Dialog::About => "About LightCraft",
        Dialog::Shortcuts => "Keyboard Shortcuts",
    };
    let frame = egui::Frame::window(&ctx.global_style()).inner_margin(egui::Margin::symmetric(16, 12));
    let shown = egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .frame(frame)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .default_width(380.0)
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            match &mut dlg {
                Dialog::AutoStack { gap } => {
                    ui.label(egui::RichText::new("Stack photos taken within this time of each other:").color(t.text_label));
                    ui.add(
                        egui::Slider::new(gap, 0.0..=86400.0)
                            .logarithmic(true)
                            .smallest_positive(1.0)
                            .custom_formatter(|v, _| crate::panels::dialogs::fmt_gap(v))
                            .custom_parser(|s| s.trim().trim_end_matches('s').parse().ok()),
                    );
                    let preview = app.session.execute("stack.auto", &json!({"gap": *gap, "preview": true})).unwrap_or_default();
                    let scope = if app.session.selection.ids.len() > 1 { "the selected photos" } else { "the photos in view" };
                    ui.label(
                        egui::RichText::new(format!("Creates {} stacks from {} of {scope}", preview["stacks"], preview["photos"])).color(t.text_dim),
                    );
                }
                Dialog::NewSmartAlbum { name } => {
                    let r = ui.add(egui::TextEdit::singleline(name).hint_text("Name").desired_width(f32::INFINITY));
                    r.request_focus();
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        confirm = true;
                    }
                    let rules = app.session.view_rules();
                    let n = app.session.catalog.query(&rules, &Default::default()).len();
                    ui.label(egui::RichText::new(format!("Matches: {}", rules.describe())).color(t.text_label));
                    ui.label(
                        egui::RichText::new(format!("{n} photo{} now · updates automatically as photos change", if n == 1 { "" } else { "s" }))
                            .color(t.text_dim),
                    );
                }
                Dialog::Rename { template, start } => {
                    let n = app.session.targets(&json!({})).len();
                    ui.label(egui::RichText::new(format!("{n} photo{}", if n == 1 { "" } else { "s" })).color(t.text_dim));
                    field(ui, "Template", |ui| {
                        let r = ui.add(egui::TextEdit::singleline(template).hint_text("{name}").desired_width(f32::INFINITY));
                        crate::widgets::register(ui.ctx(), "field:renameTemplate", r.rect);
                    });
                    field(ui, "Presets", |ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for (i, (label, tpl)) in
                            [("Name", "{name}"), ("Date–Name", "{date}_{name}"), ("Title–Seq", "{title}-{seq:3}"), ("Custom–Seq", "Photo-{seq:3}")].iter().enumerate()
                        {
                            if crate::widgets::text_button(ui, &format!("renamePreset-{i}"), label, template == tpl).clicked() {
                                *template = tpl.to_string();
                            }
                        }
                    });
                    field(ui, "Start at", |ui| ui.add(egui::DragValue::new(start).range(0..=999_999)));
                    ui.label(
                        egui::RichText::new("Tokens: {name} {seq} {seq:3} {date} {date:%Y-%m-%d} {camera} {title}. Files are renamed on disk (with their XMP sidecars); existing names get -1, -2…")
                            .color(t.text_dim),
                    );
                    let preview = app.session.execute("photo.renamePreview", &json!({"template": template, "start": start})).unwrap_or_default();
                    egui::Grid::new("rename-preview").num_columns(3).spacing([8.0, 2.0]).show(ui, |ui| {
                        for pl in preview.as_array().into_iter().flatten().take(6) {
                            ui.label(egui::RichText::new(pl["from"].as_str().unwrap_or("")).color(t.text_dim));
                            ui.label(egui::RichText::new("→").color(t.text_dim));
                            ui.label(egui::RichText::new(pl["to"].as_str().unwrap_or("")).color(t.text));
                            ui.end_row();
                        }
                    });
                    let more = preview.as_array().map_or(0, Vec::len).saturating_sub(6);
                    if more > 0 {
                        ui.label(egui::RichText::new(format!("… and {more} more")).color(t.text_dim));
                    }
                }
                Dialog::RenameKeyword { from, to } => {
                    let n = app.session.catalog.photos().filter(|p| p.meta.keywords.iter().any(|k| lightcraft_catalog::keywords::is_under(k, from))).count();
                    let r = ui.add(egui::TextEdit::singleline(to).hint_text("New name").desired_width(f32::INFINITY));
                    crate::widgets::register(ui.ctx(), "field:keywordName", r.rect);
                    r.request_focus();
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        confirm = true;
                    }
                    ui.label(
                        egui::RichText::new(format!(
                            "Renames “{from}” on {n} photo{} (keywords below it too). Use | for levels, e.g. Travel|Italy. An existing name merges the two.",
                            if n == 1 { "" } else { "s" }
                        ))
                        .color(t.text_dim),
                    );
                }
                Dialog::MergeKeywords { from, into } => {
                    ui.label(egui::RichText::new(format!("Replace {} with:", from.iter().map(|f| format!("“{f}”")).collect::<Vec<_>>().join(", "))).color(t.text_label));
                    let r = ui.add(egui::TextEdit::singleline(into).hint_text("Keyword").desired_width(f32::INFINITY));
                    crate::widgets::register(ui.ctx(), "field:keywordInto", r.rect);
                    r.request_focus();
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        confirm = true;
                    }
                    let options: Vec<String> = app
                        .session
                        .catalog
                        .keyword_suggestions(from, into, 12)
                        .into_iter()
                        .filter(|k| !from.iter().any(|f| lightcraft_catalog::keywords::is_under(k, f)))
                        .collect();
                    ui.horizontal_wrapped(|ui| {
                        for k in options {
                            if ui.add(egui::Button::new(egui::RichText::new(&k).color(t.text_label)).corner_radius(10.0)).clicked() {
                                *into = k;
                            }
                        }
                    });
                }
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
                    use lightcraft_engine::export::{Anchor as P, ExportFormat as F, MetadataPolicy as M, SharpenAmount as A, SharpenFor as S};
                    let n = app.session.selection.ids.len().max(1);
                    ui.label(egui::RichText::new(format!("{n} photo{}", if n == 1 { "" } else { "s" })).color(t.text_dim));
                    ui.add_space(4.0);
                    choices(
                        ui,
                        "Format",
                        "exportFormat",
                        &[(F::Jpeg, "JPEG"), (F::Png, "PNG"), (F::Tiff, "TIFF"), (F::Webp, "WebP"), (F::Avif, "AVIF")],
                        &mut opts.format,
                    );
                    if matches!(opts.format, F::Jpeg | F::Avif) {
                        let mut q = opts.quality as f64;
                        if num(ui, &QUALITY, &mut q) {
                            opts.quality = q as u8;
                        }
                    }
                    if opts.format == F::Jpeg {
                        let mut k = *limit_kb as f64;
                        if num(ui, &LIMIT_KB, &mut k) {
                            *limit_kb = k as u32;
                        }
                    }
                    let mut full = *long_edge == 0;
                    if ui.checkbox(&mut full, "Full size").changed() {
                        *long_edge = if full { 0 } else { 2048 };
                    }
                    if !full {
                        let mut e = *long_edge as f64;
                        if num(ui, &LONG_EDGE, &mut e) {
                            *long_edge = e as u32;
                        }
                    }
                    choices(
                        ui,
                        "Sharpen",
                        "exportSharpen",
                        &[(S::None, "None"), (S::Screen, "Screen"), (S::Matte, "Matte"), (S::Glossy, "Glossy")],
                        &mut opts.sharpen,
                    );
                    if opts.sharpen != S::None {
                        choices(
                            ui,
                            "Amount",
                            "exportSharpenAmount",
                            &[(A::Low, "Low"), (A::Standard, "Standard"), (A::High, "High")],
                            &mut opts.sharpen_amount,
                        );
                    }
                    choices(
                        ui,
                        "Metadata",
                        "exportMetadata",
                        &[(M::All, "All"), (M::AllExceptCamera, "No camera"), (M::Copyright, "Copyright"), (M::None, "None")],
                        &mut opts.metadata,
                    );
                    if !matches!(opts.metadata, M::None | M::Copyright) {
                        ui.checkbox(&mut opts.remove_location, "Remove location info");
                    }
                    let mut wm_on = opts.watermark.is_some();
                    if ui.checkbox(&mut wm_on, "Watermark").changed() {
                        opts.watermark = wm_on.then(|| lightcraft_engine::export::Watermark { text: "© ".into(), ..Default::default() });
                    }
                    if let Some(wm) = &mut opts.watermark {
                        field(ui, "Text", |ui| {
                            ui.add(egui::TextEdit::singleline(&mut wm.text).hint_text("© Your Name").desired_width(f32::INFINITY))
                        });
                        choices(
                            ui,
                            "Position",
                            "exportWmAnchor",
                            &[(P::TopLeft, "↖"), (P::TopRight, "↗"), (P::Center, "•"), (P::BottomLeft, "↙"), (P::BottomRight, "↘")],
                            &mut wm.anchor,
                        );
                        let mut size = wm.size as f64 * 100.0;
                        if num(ui, &WM_SIZE, &mut size) {
                            wm.size = (size / 100.0) as f32;
                        }
                        let mut op = wm.opacity as f64 * 100.0;
                        if num(ui, &WM_OPACITY, &mut op) {
                            wm.opacity = (op / 100.0) as f32;
                        }
                        ui.checkbox(&mut wm.shadow, "Shadow");
                    }
                    ui.add_space(4.0);
                    field(ui, "File name", |ui| {
                        ui.add(egui::TextEdit::singleline(&mut opts.naming).hint_text("{name}-{seq}").desired_width(f32::INFINITY))
                    });
                    field(ui, "Folder", |ui| ui.add(egui::TextEdit::singleline(dir).desired_width(f32::INFINITY)));
                }
                Dialog::Merge { opts } => crate::merge::body(app, ui, opts),
                Dialog::About => {
                    ui.label(egui::RichText::new("LightCraft").font(t.semibold(20.0)).color(t.text));
                    ui.label(format!("Version {} — a clean-room, pure-Rust photo library and raw developer.", env!("CARGO_PKG_VERSION")));
                    ui.label("MIT OR Apache-2.0. Font: Inter (OFL). Icons: original.");
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
                            for (sc, id, _) in crate::shortcuts::ALIASES {
                                let label = crate::menus::UI_COMMANDS
                                    .iter()
                                    .find(|c| c.0 == *id)
                                    .map(|c| c.1)
                                    .or_else(|| lightcraft_engine::find_command(id).map(|c| c.label))
                                    .unwrap_or(id);
                                ui.label(label);
                                ui.label(*sc);
                                ui.label(egui::RichText::new(*id).color(t.text_dim));
                                ui.end_row();
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
                let ok = match dlg {
                    Dialog::Merge { .. } => "Merge",
                    _ if informational => "Close",
                    _ => "OK",
                };
                if ui.button(ok).clicked() {
                    if informational {
                        close = true;
                    } else {
                        confirm = true;
                    }
                }
            });
        });
    if let Some(w) = shown {
        ctx.move_to_top(w.response.layer_id);
    }
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
/// `90` → `1 min 30 s`.
pub fn fmt_gap(v: f64) -> String {
    let v = v.round() as u64;
    match v {
        0..60 => format!("{v} s"),
        60..3600 if v.is_multiple_of(60) => format!("{} min", v / 60),
        60..3600 => format!("{} min {} s", v / 60, v % 60),
        3600..86400 if v.is_multiple_of(3600) => format!("{} h", v / 3600),
        3600..86400 => format!("{} h {} min", v / 3600, v % 3600 / 60),
        _ => "1 day".into(),
    }
}

pub fn confirm_dialog(app: &mut LightcraftApp, dlg: &Dialog) -> Result<serde_json::Value, String> {
    match dlg {
        Dialog::NewAlbum { name, folder } => app.run("album.create", json!({"name": name, "folder": folder, "addSelected": !folder})),
        Dialog::RenameAlbum { id, name } => app.run("album.rename", json!({"id": id, "name": name})),
        Dialog::Rename { template, start } => app.run("photo.rename", json!({"template": template, "start": start})),
        Dialog::RenameKeyword { from, to } => app.run("keyword.rename", json!({"from": from, "to": to})),
        Dialog::MergeKeywords { from, into } => app.run("keyword.merge", json!({"from": from, "into": into})),
        Dialog::AutoStack { gap } => app.run("stack.auto", json!({"gap": gap})),
        Dialog::NewSmartAlbum { name } => app.run("album.createSmart", json!({"name": if name.trim().is_empty() { "Smart Album" } else { name }})),
        Dialog::CreatePreset { name, group } => {
            app.run("preset.create", json!({"name": if name.is_empty() { "My Preset" } else { name }, "group": group}))
        }
        Dialog::CopySettings { groups } => app.run("develop.copy", json!({"groups": groups})),
        Dialog::Export { opts, long_edge, limit_kb, dir } => app.run(
            "app.export",
            json!({
                "format": opts.format, "quality": opts.quality, "longEdge": long_edge, "limitKb": limit_kb,
                "sharpen": opts.sharpen, "sharpenAmount": opts.sharpen_amount, "naming": opts.naming, "dir": dir,
                "metadata": opts.metadata, "removeLocation": opts.remove_location, "watermark": opts.watermark,
            }),
        ),
        Dialog::Merge { opts } => crate::merge::start_final(app, opts),
        Dialog::About | Dialog::Shortcuts => Ok(serde_json::Value::Null),
    }
}

// ------------------------------------------------------------------------------------------ dialog widgets

/// Width of the label column in dialogs.
const LABEL_W: f32 = 78.0;

const fn spec(id: &'static str, label: &'static str, min: f64, max: f64, default: f64, step: f64) -> ControlSpec {
    ControlSpec { id, label, section: Section::Light, min, max, default, step, decimals: 0, track: Track::Plain }
}
const QUALITY: ControlSpec = spec("export.quality", "Quality", 1.0, 100.0, 90.0, 1.0);
const LIMIT_KB: ControlSpec = spec("export.limitKb", "Limit file size (KB, 0 = off)", 0.0, 20_000.0, 0.0, 10.0);
const LONG_EDGE: ControlSpec = spec("export.longEdge", "Long edge (px)", 256.0, 12_000.0, 2048.0, 16.0);
const WM_SIZE: ControlSpec = spec("export.watermarkSize", "Size (% of short edge)", 1.0, 15.0, 3.5, 0.5);
const WM_OPACITY: ControlSpec = spec("export.watermarkOpacity", "Opacity (%)", 5.0, 100.0, 70.0, 1.0);

/// A themed slider row editing `v`; true when it changed.
fn num(ui: &mut egui::Ui, spec: &ControlSpec, v: &mut f64) -> bool {
    match crate::widgets::slider(ui, spec, *v, true, None).value {
        Some(n) => {
            *v = n;
            true
        }
        None => false,
    }
}

/// A labelled row (fixed label column).
fn field<R>(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let t = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(egui::vec2(LABEL_W, 24.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_width(LABEL_W);
            ui.label(egui::RichText::new(label).color(t.text_label));
        });
        add(ui)
    })
    .inner
}

/// A labelled row of mutually exclusive choice buttons (ids `button:{id}-{index}`).
fn choices<V: PartialEq + Copy>(ui: &mut egui::Ui, label: &str, id: &str, options: &[(V, &str)], value: &mut V) {
    field(ui, label, |ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (i, (v, l)) in options.iter().enumerate() {
            if crate::widgets::text_button(ui, &format!("{id}-{i}"), l, *value == *v).clicked() {
                *value = *v;
            }
        }
    });
}
