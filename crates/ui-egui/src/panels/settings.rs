//! The Settings dialog (⌘,): General, Import, Performance, Interface.
//!
//! Changes apply immediately (no OK/Cancel). Where they are stored:
//! - **app settings** ([`crate::state::AppSettings`]: startup view, delete confirmation, GPU,
//!   preview size, filmstrip/grid badges, last library) and Auto Advance live in the UI state,
//!   saved by the host in its config folder (`ui.json`);
//! - **library settings** (import defaults, XMP sidecars, thumbnail cache size) go through the
//!   `library.preferences` / `library.xmpPreferences` commands into the library's `prefs.json`,
//!   so they travel with the library.

use egui::RichText;
use serde_json::{Value, json};

use crate::LightcraftApp;
use crate::state::{GridBadges, PREVIEW_EDGES, StartupView};
use crate::theme::Tokens;
use crate::widgets::register;

/// (id, label) of the tabs, in order.
pub const TABS: &[(&str, &str)] = &[("general", "General"), ("import", "Import"), ("performance", "Performance"), ("interface", "Interface")];

/// Thumbnail cache sizes offered (MB).
const CACHE_SIZES: [u32; 5] = [512, 1024, 2048, 4096, 8192];

const LABEL_W: f32 = 150.0;

/// The dialog body for `tab` (the tab bar switches `tab`).
pub fn body(app: &mut LightcraftApp, ui: &mut egui::Ui, tab: &mut String) {
    let t = Tokens::get(ui.ctx());
    ui.set_min_width(560.0);
    ui.set_min_height(330.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (id, label) in TABS {
            if crate::widgets::text_button(ui, &format!("settingsTab-{id}"), label, tab == id).clicked() {
                *tab = id.to_string();
            }
        }
    });
    ui.separator();
    ui.add_space(2.0);
    match tab.as_str() {
        "import" => import_tab(app, ui, &t),
        "performance" => performance_tab(app, ui, &t),
        "interface" => interface_tab(app, ui, &t),
        _ => general_tab(app, ui, &t),
    }
}

fn heading(ui: &mut egui::Ui, t: &Tokens, text: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(text).font(t.semibold(12.5)).color(t.text));
}

fn row<R>(ui: &mut egui::Ui, t: &Tokens, label: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(egui::vec2(LABEL_W, 24.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_width(LABEL_W);
            ui.label(RichText::new(label).color(t.text_label));
        });
        add(ui)
    })
    .inner
}

fn hint(ui: &mut egui::Ui, t: &Tokens, text: &str) {
    ui.label(RichText::new(text).size(11.0).color(t.text_dim));
}

/// A checkbox addressable as `check:{id}`; true when toggled.
fn check(ui: &mut egui::Ui, id: &str, value: &mut bool, label: &str) -> bool {
    let r = ui.checkbox(value, label);
    register(ui.ctx(), format!("check:{id}"), r.rect);
    r.changed()
}

/// Mutually exclusive buttons (`button:{id}-{index}`).
fn choices<V: PartialEq + Copy>(ui: &mut egui::Ui, id: &str, options: &[(V, &str)], value: &mut V) -> bool {
    let mut changed = false;
    ui.spacing_mut().item_spacing.x = 4.0;
    for (i, (v, l)) in options.iter().enumerate() {
        if crate::widgets::text_button(ui, &format!("{id}-{i}"), l, *value == *v).clicked() && *value != *v {
            *value = *v;
            changed = true;
        }
    }
    changed
}

// ------------------------------------------------------------------------------------- General

fn general_tab(app: &mut LightcraftApp, ui: &mut egui::Ui, t: &Tokens) {
    heading(ui, t, "Library");
    let location = match &app.session.library {
        Some(l) => l.dir.display().to_string(),
        None => "In memory — nothing is saved".to_string(),
    };
    row(ui, t, "Location", |ui| {
        ui.label(RichText::new(location).color(t.text));
    });
    row(ui, t, "", |ui| {
        let can = app.services.pick_folder.is_some();
        let r = ui.add_enabled(can, egui::Button::new("Open Library…"));
        register(ui.ctx(), "button:settingsOpenLibrary", r.rect);
        if r.clicked() {
            let _ = app.run("app.openLibrary", json!({}));
        }
        if !can {
            hint(ui, t, "not available here");
        }
    });
    heading(ui, t, "Startup");
    row(ui, t, "Open in", |ui| {
        choices(
            ui,
            "settingsStartup",
            &[(StartupView::Last, "Last view"), (StartupView::Grid, "Photo Grid"), (StartupView::Detail, "Detail")],
            &mut app.ui.settings.startup_view,
        );
    });
    heading(ui, t, "Culling");
    check(ui, "settings.autoAdvance", &mut app.ui.auto_advance, "Auto Advance: move to the next photo after rating or flagging");
    check(ui, "settings.confirmDelete", &mut app.ui.settings.confirm_delete, "Confirm before moving photos to Recently Deleted");
    heading(ui, t, "External Editor");
    row(ui, t, "Application", |ui| {
        let r = ui.add(egui::TextEdit::singleline(&mut app.ui.settings.external_editor).hint_text("System default").desired_width(220.0));
        register(ui.ctx(), "field:externalEditor", r.rect);
    });
    hint(
        ui,
        t,
        "Photo ▸ Edit in External Editor (⇧⌘E) renders a 16-bit TIFF copy, stacks it with the original and opens it here (an app name on macOS, a program path elsewhere).",
    );
}

// -------------------------------------------------------------------------------------- Import

/// A preset picker: `None` = `none_label`. Returns the new choice when it changed.
fn preset_combo(app: &LightcraftApp, ui: &mut egui::Ui, id: &str, current: Option<&str>, none_label: &str) -> Option<Option<String>> {
    let name = |pid: &str| app.session.presets.iter().find(|p| p.id == pid).map(|p| p.name.clone()).unwrap_or_else(|| format!("{pid} (missing)"));
    let text = current.map(name).unwrap_or_else(|| none_label.to_string());
    let mut out = None;
    let r = egui::ComboBox::from_id_salt(id).width(240.0).selected_text(text).show_ui(ui, |ui| {
        if ui.selectable_label(current.is_none(), none_label).clicked() {
            out = Some(None);
        }
        let mut group = "";
        for p in &app.session.presets {
            if p.group != group {
                group = &p.group;
                ui.label(RichText::new(group).size(10.5).weak());
            }
            if ui.selectable_label(current == Some(p.id.as_str()), &p.name).clicked() {
                out = Some(Some(p.id.clone()));
            }
        }
    });
    register(ui.ctx(), format!("combo:{id}"), r.response.rect);
    out.filter(|v| v.as_deref() != current)
}

/// Cameras of the raws in the library plus those with a stored default, sorted.
fn cameras(app: &LightcraftApp) -> Vec<String> {
    let mut v: Vec<String> = app
        .session
        .catalog
        .photos()
        .filter(|p| p.kind == lightcraft_catalog::MediaKind::Raw && !p.meta.camera.is_empty())
        .map(|p| p.meta.camera.clone())
        .chain(app.session.import_defaults.cameras.iter().map(|c| c.camera.clone()))
        .collect();
    v.sort_by_key(|c| c.to_lowercase());
    v.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    v
}

fn import_tab(app: &mut LightcraftApp, ui: &mut egui::Ui, t: &Tokens) {
    let d = app.session.import_defaults.clone();
    heading(ui, t, "Raw defaults");
    hint(ui, t, "Settings new raw photos start from. Changing them doesn't touch photos already in the library.");
    row(ui, t, "Raw photos", |ui| {
        if let Some(v) = preset_combo(app, ui, "settingsRawPreset", d.raw_preset.as_deref(), "LightCraft Default") {
            let _ = app.run("library.preferences", json!({"import": {"rawPreset": v}}));
        }
    });
    let mut per = d.per_camera;
    row(ui, t, "", |ui| {
        if check(ui, "settings.perCamera", &mut per, "Use camera-specific defaults") {
            let _ = app.run("library.preferences", json!({"import": {"perCamera": per}}));
        }
    });
    if per {
        let cams = cameras(app);
        if cams.is_empty() {
            hint(ui, t, "No raw photos yet: cameras appear here once their photos are in the library.");
        }
        egui::ScrollArea::vertical().max_height(150.0).id_salt("settingsCameras").show(ui, |ui| {
            for (i, cam) in cams.iter().enumerate() {
                let entry = d.cameras.iter().find(|c| c.camera.eq_ignore_ascii_case(cam));
                row(ui, t, cam, |ui| {
                    // "Raw default" = no entry; otherwise the entry's preset (None = LightCraft Default)
                    const RAW_DEFAULT: &str = "\u{1}raw";
                    let current = match entry {
                        None => Some(RAW_DEFAULT),
                        Some(e) => e.preset.as_deref(),
                    };
                    let mut pick = None;
                    let label = match current {
                        Some(RAW_DEFAULT) => "Same as raw default".to_string(),
                        None => "LightCraft Default".to_string(),
                        Some(pid) => app.session.presets.iter().find(|p| p.id == pid).map(|p| p.name.clone()).unwrap_or_else(|| pid.to_string()),
                    };
                    let id = format!("settingsCamera-{i}");
                    let r = egui::ComboBox::from_id_salt(&id).width(240.0).selected_text(label).show_ui(ui, |ui| {
                        if ui.selectable_label(current == Some(RAW_DEFAULT), "Same as raw default").clicked() {
                            pick = Some(json!({"camera": cam, "remove": true}));
                        }
                        if ui.selectable_label(current.is_none(), "LightCraft Default").clicked() {
                            pick = Some(json!({"camera": cam, "preset": null}));
                        }
                        for p in &app.session.presets {
                            if ui.selectable_label(current == Some(p.id.as_str()), &p.name).clicked() {
                                pick = Some(json!({"camera": cam, "preset": p.id}));
                            }
                        }
                    });
                    register(ui.ctx(), format!("combo:{id}"), r.response.rect);
                    if let Some(c) = pick {
                        let _ = app.run("library.preferences", json!({"camera": c}));
                    }
                });
            }
        });
    }
    heading(ui, t, "Other images (JPEG, PNG, TIFF, HEIC…)");
    row(ui, t, "Non-raw photos", |ui| {
        if let Some(v) = preset_combo(app, ui, "settingsOtherPreset", d.other_preset.as_deref(), "None") {
            let _ = app.run("library.preferences", json!({"import": {"otherPreset": v}}));
        }
    });
    heading(ui, t, "Metadata");
    hint(ui, t, "Added to photos you import that don't already have it.");
    for (key, label, hint_text, value) in
        [("copyright", "Copyright", "© 2026 Your Name", d.copyright.clone()), ("creator", "Creator", "Your Name", d.creator.clone())]
    {
        row(ui, t, label, |ui| {
            let id = egui::Id::new(("settingsMeta", key));
            let mut text: String = ui.data(|m| m.get_temp(id)).unwrap_or(value.clone());
            let r = ui.add(egui::TextEdit::singleline(&mut text).hint_text(hint_text).desired_width(240.0));
            register(ui.ctx(), format!("field:settings.{key}"), r.rect);
            if r.lost_focus() && text.trim() != value {
                let _ = app.run("library.preferences", json!({"import": {key: text.trim()}}));
            }
            if r.has_focus() {
                ui.data_mut(|m| m.insert_temp(id, text));
            } else {
                ui.data_mut(|m| m.remove::<String>(id));
            }
        });
    }
    if !app.session.metadata_presets.is_empty() {
        row(ui, t, "Metadata preset", |ui| {
            let cur = d.metadata_preset.clone();
            let mut pick = None;
            let r = egui::ComboBox::from_id_salt("settingsMetaPreset")
                .width(240.0)
                .selected_text(cur.clone().unwrap_or_else(|| "None".into()))
                .show_ui(ui, |ui| {
                    if ui.selectable_label(cur.is_none(), "None").clicked() {
                        pick = Some(String::new());
                    }
                    for m in &app.session.metadata_presets {
                        if ui.selectable_label(cur.as_deref() == Some(m.name.as_str()), &m.name).clicked() {
                            pick = Some(m.name.clone());
                        }
                    }
                });
            register(ui.ctx(), "combo:settingsMetaPreset", r.response.rect);
            if let Some(n) = pick {
                let _ = app.run("library.preferences", json!({"import": {"metadataPreset": n}}));
            }
        });
    }
    heading(ui, t, "XMP sidecars");
    let mut xmp = app.session.xmp;
    if check(ui, "settings.autoWriteXmp", &mut xmp.auto_write, "Automatically write changes into XMP sidecars") {
        let _ = app.run("library.xmpPreferences", json!({"autoWrite": xmp.auto_write}));
    }
    row(ui, t, "Sidecar names", |ui| {
        use lightcraft_engine::sidecar::SidecarNaming as N;
        let mut n = xmp.naming;
        if choices(ui, "settingsXmpNaming", &[(N::Stem, "IMG_1.xmp"), (N::Full, "IMG_1.CR3.xmp")], &mut n) {
            let naming = if n == N::Full { "full" } else { "stem" };
            let _ = app.run("library.xmpPreferences", json!({"naming": naming}));
        }
    });
    if !cfg!(target_arch = "wasm32") {
        heading(ui, t, "Auto Import");
        hint(ui, t, "Photos that arrive in this folder (tethering, a scanner, a sync app) are added as soon as they're complete.");
        row(ui, t, "Watched folder", |ui| {
            ui.label(RichText::new(d.auto_folder.clone().unwrap_or_else(|| "Off".into())).color(t.text));
            let can = app.services.pick_folder.is_some();
            let r = ui.add_enabled(can, egui::Button::new("Choose…"));
            register(ui.ctx(), "button:settingsAutoFolder", r.rect);
            if r.clicked()
                && let Some(f) = app.services.pick_folder.as_mut().and_then(|f| f())
                && let Err(e) = app.run("library.autoImport", json!({"folder": f}))
            {
                app.toast(ui.ctx(), e);
            }
            if d.auto_folder.is_some() && ui.button("Turn Off").clicked() {
                let _ = app.run("library.autoImport", json!({"folder": null}));
            }
        });
        if d.auto_folder.is_some() {
            row(ui, t, "", |ui| {
                let mut copy = d.auto_copy;
                if ui.checkbox(&mut copy, "Copy into the library (else use the files where they are)").changed() {
                    let _ = app.run("library.autoImport", json!({"copy": copy}));
                }
            });
            row(ui, t, "Album", |ui| {
                let id = egui::Id::new("auto-album");
                let mut name = ui.data(|m| m.get_temp::<String>(id)).unwrap_or_else(|| d.auto_album.clone().unwrap_or_default());
                let r = ui.add(egui::TextEdit::singleline(&mut name).hint_text("None").desired_width(180.0));
                if r.lost_focus() {
                    let _ = app.run("library.autoImport", json!({"album": name.trim()}));
                }
                ui.data_mut(|m| m.insert_temp(id, name));
            });
        }
    }
    if app.session.library.is_none() {
        hint(ui, t, "In-memory session: these settings last until LightCraft quits.");
    }
}

// --------------------------------------------------------------------------------- Performance

fn performance_tab(app: &mut LightcraftApp, ui: &mut egui::Ui, t: &Tokens) {
    use lightcraft_engine::gpu;
    heading(ui, t, "Rendering");
    check(ui, "settings.gpu", &mut app.ui.settings.gpu, "Use the GPU for rendering");
    let status = if !gpu::available() {
        "No usable GPU found: rendering on the CPU".to_string()
    } else {
        format!("GPU: {}", gpu::adapter_name().unwrap_or_else(|| "starting…".into()))
    };
    hint(ui, t, &status);
    row(ui, t, "Preview size", |ui| {
        let opts: Vec<(u32, String)> = PREVIEW_EDGES.iter().map(|e| (*e, format!("{e} px"))).collect();
        let opts: Vec<(u32, &str)> = opts.iter().map(|(e, l)| (*e, l.as_str())).collect();
        choices(ui, "settingsPreview", &opts, &mut app.ui.settings.preview_edge);
    });
    hint(ui, t, "Largest long edge the Detail view renders at; larger is sharper on big displays but slower.");
    row(ui, t, "Memory for caches", |ui| {
        let auto = format!("Automatic ({} MB)", lightcraft_engine::memory::default_budget() >> 20);
        let opts = [(0u32, auto.as_str()), (512, "512 MB"), (1024, "1 GB"), (2048, "2 GB"), (4096, "4 GB")];
        choices(ui, "settingsMemory", &opts, &mut app.ui.settings.memory_mb);
    });
    heading(ui, t, "Thumbnail cache");
    let cur = (app.session.cache_bytes() >> 20) as u32;
    row(ui, t, "Size limit", |ui| {
        let opts = [(512u32, "512 MB"), (1024, "1 GB"), (2048, "2 GB"), (4096, "4 GB"), (8192, "8 GB")];
        let mut v = if CACHE_SIZES.contains(&cur) { cur } else { 2048 };
        if choices(ui, "settingsCache", &opts, &mut v) {
            let _ = app.run("library.preferences", json!({"cacheMb": v}));
        }
    });
    let used = app.session.media.rendered.disk().map(|d| d.size());
    row(ui, t, "In use", |ui| {
        ui.label(RichText::new(used.map(|b| format!("{:.1} MB", b as f64 / 1048576.0)).unwrap_or_else(|| "memory only".into())).color(t.text));
        let r = ui.button("Clear Cache");
        register(ui.ctx(), "button:settingsClearCache", r.rect);
        if r.clicked() {
            let _ = app.run("library.clearPreviews", json!({}));
        }
    });
}

// ---------------------------------------------------------------------------------- Interface

fn interface_tab(app: &mut LightcraftApp, ui: &mut egui::Ui, t: &Tokens) {
    heading(ui, t, "Filmstrip");
    check(ui, "settings.filmNames", &mut app.ui.settings.film_names, "Show file names");
    check(ui, "settings.filmBadges", &mut app.ui.settings.film_badges, "Show ratings, flags and edit badges");
    heading(ui, t, "Grid");
    row(ui, t, "Ratings & flags", |ui| {
        choices(
            ui,
            "settingsGridBadges",
            &[(GridBadges::Auto, "When rated or hovered"), (GridBadges::Always, "Always"), (GridBadges::Never, "Never")],
            &mut app.ui.settings.grid_badges,
        );
    });
    check(ui, "settings.showFilenames", &mut app.ui.show_filenames, "Square Grid: show file names and formats");
    heading(ui, t, "Detail");
    check(ui, "settings.navigator", &mut app.ui.navigator, "Show the Navigator while zoomed in");
    row(ui, t, "Info overlay", |ui| {
        use crate::state::InfoOverlay as I;
        choices(ui, "settingsInfo", &[(I::Off, "Off"), (I::Basic, "File & date"), (I::Exposure, "Exposure")], &mut app.ui.info_overlay);
    });
}

// ------------------------------------------------------------------------------- Open Library…

/// `app.openLibrary {path?}`: close the current library and open (or create) the one at `path`,
/// or a folder chosen in a dialog. Remembered as the library to open at launch.
pub fn open_library(app: &mut LightcraftApp, p: &Value) -> Result<Value, String> {
    let path = match p.get("path").and_then(Value::as_str) {
        Some(x) => x.to_string(),
        None => match app.services.pick_folder.as_mut() {
            Some(pick) => match pick() {
                Some(x) => x,
                None => return Ok(Value::Null),
            },
            None => return Err("no folder dialog on this platform".into()),
        },
    };
    app.session.close_library().map_err(|e| e.to_string())?;
    app.session.open_library(&path, false).map_err(|e| e.to_string())?;
    app.renderer.forget_all();
    app.ui.compare = None;
    app.ui.settings.library_path = path.clone();
    Ok(json!({"path": path, "photos": app.session.catalog.len()}))
}
