//! The right-hand panel next to the tool strip: Edit, Crop, Remove, Masking, Red Eye, Info,
//! Keywords, Versions, Activity.

use egui::{Align2, Rect, Sense, pos2, vec2};
use lightcraft_catalog::PhotoId;
use serde_json::json;

use crate::LightcraftApp;
use crate::icons::{Icon, paint};
use crate::state::RightPanel;
use crate::theme::Tokens;
use crate::widgets::{divider, register, slider, text_button};

pub fn show(app: &mut LightcraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Panel::right("right_panel")
        .exact_size(t.panel_w)
        .resizable(false)
        .frame(egui::Frame::NONE.fill(t.chrome).stroke(egui::Stroke::new(1.0, t.divider)))
        .show(ui, |ui| {
            let Some(id) = app.session.active() else {
                let r = ui.max_rect();
                super::empty_message(ui, r, "No photo selected", "Select a photo to edit");
                return;
            };
            egui::ScrollArea::vertical().id_salt("right-scroll").auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                match app.ui.right {
                    RightPanel::Edit => super::edit::show(app, ui, id),
                    RightPanel::Crop => crop(app, ui, id),
                    RightPanel::Remove => remove(app, ui, id),
                    RightPanel::Masking => super::masking::show(app, ui, id),
                    RightPanel::RedEye => red_eye(app, ui),
                    RightPanel::Info => info(app, ui, id),
                    RightPanel::Keywords => keywords(app, ui, id),
                    RightPanel::Versions => versions(app, ui, id),
                    RightPanel::Activity => activity(app, ui, id),
                    RightPanel::None => {}
                }
            });
        });
}

pub fn header(ui: &mut egui::Ui, title: &str) {
    let t = Tokens::get(ui.ctx());
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::hover());
    ui.painter().text(pos2(r.left() + 24.0, r.center().y + 2.0), Align2::LEFT_CENTER, title, t.semibold(15.0), t.text);
}

pub fn label_row(ui: &mut egui::Ui, label: &str, value: &str) {
    let t = Tokens::get(ui.ctx());
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
    ui.painter().text(pos2(r.left() + 24.0, r.center().y), Align2::LEFT_CENTER, label, t.font(12.5), t.text_dim);
    ui.painter().text(pos2(r.left() + 110.0, r.center().y), Align2::LEFT_CENTER, value, t.font(12.5), t.text_label);
}

fn padded(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::NONE.inner_margin(egui::Margin { left: 24, right: 22, top: 6, bottom: 6 }).show(ui, add);
}

fn crop(app: &mut LightcraftApp, ui: &mut egui::Ui, id: PhotoId) {
    let d = app.session.develop_of(id).unwrap_or_default();
    header(ui, "Crop");
    padded(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label("Aspect Ratio");
            let cur = d.crop.aspect.map(|(w, h)| format!("{} × {}", w as f64 / 100.0, h as f64 / 100.0)).unwrap_or_else(|| "Free".into());
            let r = ui.add(egui::Button::new(cur).frame(false));
            register(ui.ctx(), "button:cropAspect", r.rect);
            egui::Popup::menu(&r).show(|ui| {
                for (label, a) in [
                    ("Free", "free"),
                    ("Original", "original"),
                    ("1 × 1", "1x1"),
                    ("4 × 5 / 8 × 10", "4x5"),
                    ("8.5 × 11", "8.5x11"),
                    ("5 × 7", "5x7"),
                    ("2 × 3 / 4 × 6", "2x3"),
                    ("4 × 3", "4x3"),
                    ("16 × 9", "16x9"),
                    ("16 × 10", "16x10"),
                ] {
                    if ui.button(label).clicked() {
                        let _ = app.run("crop.aspect", json!({"aspect": a}));
                    }
                }
            });
        });
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if text_button(ui, "cropRotateLeft", "Rotate Left", false).clicked() {
                let _ = app.run("photo.rotateLeft", json!({}));
            }
            if text_button(ui, "cropRotateRight", "Rotate Right", false).clicked() {
                let _ = app.run("photo.rotateRight", json!({}));
            }
        });
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if text_button(ui, "cropFlipH", "Flip H", d.crop.flip_h).clicked() {
                let _ = app.run("photo.flipHorizontal", json!({}));
            }
            if text_button(ui, "cropFlipV", "Flip V", d.crop.flip_v).clicked() {
                let _ = app.run("photo.flipVertical", json!({}));
            }
            if text_button(ui, "cropSwapAspect", "Swap Aspect (X)", false).clicked() {
                let _ = app.run("crop.rotateAspect", json!({}));
            }
            if text_button(ui, "cropReset", "Reset", false).clicked() {
                let _ = app.run("crop.reset", json!({}));
            }
        });
    });
    let ang = lightcraft_develop::controls::find("crop.angle").copied();
    if let Some(spec) = ang {
        let out = slider(ui, &spec, d.crop.geometry.angle, true, Some("Straighten"));
        super::edit::apply_slider_out(app, &spec, out, |app, v| app.run("crop.straighten", json!({"angle": v})));
    }
    padded(ui, |ui| {
        ui.label("Overlay");
        {
            use crate::state::CropOverlay as O;
            let opts = [(O::Thirds, "Thirds", "thirds"), (O::Grid, "Grid", "grid"), (O::Golden, "Golden", "golden"), (O::None, "None", "none")];
            let items: Vec<(&str, &str)> = opts.iter().map(|(_, l, k)| (*l, *k)).collect();
            let active = opts.iter().position(|(o, _, _)| *o == app.ui.crop_overlay);
            if let Some(i) = crate::widgets::segmented(ui, "cropOverlay", &items, active, 4) {
                app.ui.crop_overlay = opts[i].0;
            }
        }
    });
    divider(ui);
    header(ui, "Geometry");
    padded(ui, |ui| {
        ui.label("Upright");
        {
            use lightcraft_develop::Upright;
            let modes = [
                ("Off", Upright::Off, "off"),
                ("Auto", Upright::Auto, "auto"),
                ("Guided", Upright::Guided, "guided"),
                ("Level", Upright::Level, "level"),
                ("Vertical", Upright::Vertical, "vertical"),
                ("Full", Upright::Full, "full"),
            ];
            let items: Vec<(&str, &str)> = modes.iter().map(|(l, _, k)| (*l, *k)).collect();
            let active = modes.iter().position(|(_, m, _)| *m == d.geometry.upright);
            if let Some(i) = crate::widgets::segmented(ui, "upright", &items, active, 3) {
                let (_, mode, key) = modes[i];
                let _ = app.run("geometry.upright", json!({"mode": key}));
                app.ui.tool = if mode == Upright::Guided { "guidedUpright".into() } else { String::new() };
            }
        }
        let guided = d.geometry.upright == lightcraft_develop::Upright::Guided;
        ui.horizontal_wrapped(|ui| {
            if !guided && d.geometry.upright != lightcraft_develop::Upright::Off && text_button(ui, "uprightUpdate", "Update", false).clicked() {
                let mode = serde_json::to_value(d.geometry.upright).unwrap_or_default();
                let _ = app.run("geometry.upright", json!({"mode": mode}));
            }
            if guided {
                let drawing = app.ui.tool == "guidedUpright";
                if text_button(ui, "uprightDraw", "Draw Guides", drawing).clicked() {
                    app.ui.tool = if drawing { String::new() } else { "guidedUpright".into() };
                }
                if !d.geometry.guides.is_empty() && text_button(ui, "uprightClear", "Clear Guides", false).clicked() {
                    let _ = app.run("geometry.guides", json!({"guides": []}));
                }
            }
        });
        if guided {
            ui.label(
                egui::RichText::new(format!("{} of 4 guides — drag along lines that should be vertical or horizontal.", d.geometry.guides.len()))
                    .size(11.0)
                    .color(Tokens::get(ui.ctx()).text_dim),
            );
        }
        ui.add_space(4.0);
        let mut c = d.geometry.constrain_crop;
        if ui.checkbox(&mut c, "Constrain Crop").changed() {
            let _ = app.run("develop.merge", json!({"settings": {"geometry": {"constrain_crop": c}}, "label": "Constrain Crop"}));
        }
    });
    for spec in lightcraft_develop::controls::in_section(lightcraft_develop::Section::Geometry).filter(|c| c.id != "crop.angle") {
        let v = lightcraft_develop::controls::get(&d, spec.id).unwrap_or(spec.default);
        let out = slider(ui, spec, v, true, None);
        super::edit::apply_slider_out(app, spec, out, |app, v| app.run("develop.set", json!({"control": spec.id, "value": v})));
    }
}

fn remove(app: &mut LightcraftApp, ui: &mut egui::Ui, id: PhotoId) {
    let d = app.session.develop_of(id).unwrap_or_default();
    header(ui, "Remove");
    padded(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            for (label, tool) in [("Remove", "remove"), ("Heal", "heal"), ("Clone", "clone")] {
                if text_button(ui, &format!("removeMode-{tool}"), label, app.ui.tool == tool).clicked() {
                    app.ui.tool = tool.into();
                }
            }
        });
        ui.add_space(8.0);
        ui.label(format!("{} spot(s) on this photo", d.spots.len()));
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Paint over a distraction on the photo to remove it.").color(Tokens::get(ui.ctx()).text_dim));
    });
    let spec = lightcraft_develop::ControlSpec {
        id: "ui.removeSize",
        label: "Size",
        section: lightcraft_develop::Section::Detail,
        min: 1.0,
        max: 100.0,
        default: 20.0,
        step: 1.0,
        decimals: 0,
        track: lightcraft_develop::Track::Plain,
    };
    let out = slider(ui, &spec, (app.ui.remove_size * 1000.0) as f64, true, None);
    if let Some(v) = out.value {
        app.ui.remove_size = (v / 1000.0) as f32;
    }
    padded(ui, |ui| {
        if !d.spots.is_empty() && text_button(ui, "removeClear", "Delete all spots", false).clicked() {
            for i in (0..d.spots.len()).rev() {
                let _ = app.run("spot.delete", json!({"index": i}));
            }
        }
    });
}

fn red_eye(app: &mut LightcraftApp, ui: &mut egui::Ui) {
    header(ui, "Red Eye");
    padded(ui, |ui| {
        ui.label("Click and drag over an eye to correct it.");
        if text_button(ui, "redeyeAuto", "Auto Correct", false).clicked() {
            app.ui.status = "Red eye correction arrives in M8".into();
        }
    });
}

fn info(app: &mut LightcraftApp, ui: &mut egui::Ui, id: PhotoId) {
    let Some(p) = app.session.catalog.photo(id).cloned() else { return };
    header(ui, "Info");
    let t = Tokens::get(ui.ctx());
    padded(ui, |ui| {
        ui.label(egui::RichText::new(&p.file_name).font(t.semibold(14.0)).color(t.text));
        ui.label(egui::RichText::new(format!("{} × {}  ·  {}", p.width, p.height, p.format)).color(t.text_dim));
        if let Some(name) = &p.copy_name {
            let of = p.copy_of.and_then(|m| app.session.catalog.photo(m)).map(|m| m.file_name.clone()).unwrap_or_else(|| "a removed photo".into());
            ui.label(egui::RichText::new(format!("Virtual copy “{name}” of {of}")).color(t.text_label));
        }
        ui.add_space(8.0);
        if let Some(r) = ui.horizontal(|ui| crate::widgets::stars(ui, "info", p.rating, 20.0)).inner {
            let _ = app.run("photo.rate", json!({"rating": r}));
        }
    });
    divider(ui);
    let mut title = p.meta.title.clone();
    let mut caption = p.meta.caption.clone();
    let mut copyright = p.meta.copyright.clone();
    padded(ui, |ui| {
        for (label, val, key) in [("Title", &mut title, "title"), ("Caption", &mut caption, "caption"), ("Copyright", &mut copyright, "copyright")] {
            ui.label(egui::RichText::new(label).color(t.text_dim));
            let r = ui.add(egui::TextEdit::singleline(val).desired_width(f32::INFINITY));
            register(ui.ctx(), format!("field:{key}"), r.rect);
            if r.lost_focus() {
                let _ = app.run("photo.setMeta", json!({key: val.clone()}));
            }
            ui.add_space(6.0);
        }
    });
    divider(ui);
    header(ui, "Camera");
    let m = &p.meta;
    label_row(ui, "Captured", p.captured.as_deref().unwrap_or("—"));
    label_row(ui, "Camera", &m.camera);
    label_row(ui, "Lens", &m.lens);
    label_row(ui, "Focal", &m.focal_mm.map(|f| format!("{f:.0} mm")).unwrap_or_default());
    label_row(ui, "Aperture", &m.aperture.map(|f| format!("f/{f:.1}")).unwrap_or_default());
    label_row(ui, "Shutter", &m.shutter);
    label_row(ui, "ISO", &m.iso.map(|f| f.to_string()).unwrap_or_default());
    label_row(ui, "Location", &m.location);
    label_row(ui, "File size", &format!("{:.1} MB", p.file_size as f64 / 1e6));
}

fn keywords(app: &mut LightcraftApp, ui: &mut egui::Ui, id: PhotoId) {
    let Some(p) = app.session.catalog.photo(id).cloned() else { return };
    header(ui, "Keywords");
    let t = Tokens::get(ui.ctx());
    padded(ui, |ui| {
        let kid = egui::Id::new("kw-input");
        let mut text = ui.data_mut(|d| d.get_temp::<String>(kid).unwrap_or_default());
        let r = ui.add(egui::TextEdit::singleline(&mut text).hint_text("Add keyword").desired_width(f32::INFINITY));
        register(ui.ctx(), "field:keyword", r.rect);
        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !text.trim().is_empty() {
            let kws: Vec<String> = text.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
            let _ = app.run("photo.setMeta", json!({"addKeywords": kws}));
            text.clear();
        }
        ui.data_mut(|d| d.insert_temp(kid, text));
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            for k in &p.meta.keywords {
                let r = ui.add(egui::Button::new(egui::RichText::new(format!("{k}  ×")).color(t.text_label)).corner_radius(10.0));
                if r.clicked() {
                    let _ = app.run("photo.setMeta", json!({"removeKeywords": [k]}));
                }
            }
        });
        ui.add_space(10.0);
        ui.label(egui::RichText::new("Keywords in library").color(t.text_dim));
        let all = app.session.catalog.keywords();
        ui.horizontal_wrapped(|ui| {
            for (k, n) in all {
                if !p.meta.keywords.contains(&k)
                    && ui.add(egui::Button::new(egui::RichText::new(format!("{k} {n}")).color(t.text_dim)).frame(false)).clicked()
                {
                    let _ = app.run("photo.setMeta", json!({"addKeywords": [k]}));
                }
            }
        });
    });
}

fn versions(app: &mut LightcraftApp, ui: &mut egui::Ui, id: PhotoId) {
    let Some(p) = app.session.catalog.photo(id).cloned() else { return };
    header(ui, "Versions");
    padded(ui, |ui| {
        if text_button(ui, "versionCreate", "Create Version", false).clicked() {
            let _ = app.run("version.create", json!({}));
        }
        ui.add_space(8.0);
        for (i, v) in p.versions.iter().enumerate() {
            ui.horizontal(|ui| {
                if ui.button(&v.name).on_hover_text(&v.created).clicked() {
                    let _ = app.run("version.restore", json!({"index": i}));
                }
                if ui.small_button("×").clicked() {
                    let _ = app.run("version.delete", json!({"index": i}));
                }
            });
        }
        if p.versions.is_empty() {
            ui.label("No versions yet.");
        }
    });
}

fn activity(app: &mut LightcraftApp, ui: &mut egui::Ui, id: PhotoId) {
    let Some(p) = app.session.catalog.photo(id).cloned() else { return };
    header(ui, "History");
    let t = Tokens::get(ui.ctx());
    padded(ui, |ui| {
        if p.history.is_empty() {
            ui.label(egui::RichText::new("No edits yet.").color(t.text_dim));
        }
        for (i, h) in p.history.iter().enumerate().rev() {
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
            register(ui.ctx(), format!("history:{i}"), r);
            if resp.hovered() {
                ui.painter().rect_filled(r, 3.0, t.hover);
            }
            paint(ui.painter(), Rect::from_min_size(r.min + vec2(0.0, 4.0), vec2(16.0, 16.0)), Icon::Clock, t.icon);
            ui.painter().text(pos2(r.left() + 24.0, r.center().y), Align2::LEFT_CENTER, &h.label, t.font(12.5), t.text_label);
            if resp.clicked() {
                let _ = app.run("history.restore", json!({"index": i}));
            }
        }
    });
}
