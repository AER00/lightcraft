//! Faces: the Settings ▸ Faces tab (the on/off toggle and the list of face models) and the dialog that
//! shows a model file's licence terms before it is installed.
//!
//! Adding a model is: pick or drop a `.onnx` file → the engine inspects it (`faces.models.inspect`) →
//! this dialog shows what it is and its terms → the user ticks "I accept" → `faces.models.install`.
//! LightCraft fetches a model only when the user presses Download on a model it has a pinned address for: the same
//! dialog shows its terms first, and accepting them downloads it (`faces.models.download`: checked against its size and
//! SHA-256, then installed). "Open page" opens the model's own page in the browser.

use egui::RichText;
use serde_json::{Value, json};

use super::settings::{check, heading, hint};
use crate::LightcraftApp;
use crate::theme::Tokens;
use crate::widgets::register;

/// How long a read of the model list is reused (it is a few small files, but not for every frame).
const REFRESH_SECS: f64 = 1.5;

fn list_id() -> egui::Id {
    egui::Id::new("faces-model-list")
}

/// The engine's model list, re-read at most every [`REFRESH_SECS`], or at once after an action changed it.
fn models(app: &mut LightcraftApp, ui: &egui::Ui) -> Value {
    let now = ui.input(|i| i.time);
    let epoch = app.caches.faces_epoch;
    if let Some((e, at, v)) = ui.data(|d| d.get_temp::<(u64, f64, Value)>(list_id()))
        && e == epoch
        && now - at < REFRESH_SECS
    {
        return v;
    }
    let v = app.run("faces.models.list", json!({})).unwrap_or(Value::Null);
    ui.data_mut(|d| d.insert_temp(list_id(), (epoch, now, v.clone())));
    v
}

/// A download-style size: decimal megabytes, as file managers and model pages show them.
fn mb(bytes: Option<u64>) -> String {
    match bytes {
        Some(b) if b >= 10_000_000 => format!("{} MB", (b as f64 / 1e6).round() as u64),
        Some(b) if b >= 1_000_000 => format!("{:.1} MB", b as f64 / 1e6),
        Some(b) => format!("{} KB", (b as f64 / 1e3).round().max(1.0) as u64),
        None => String::new(),
    }
}

/// "its input is not an image" → "Its input is not an image."
fn sentence(s: &str) -> String {
    let s = s.trim();
    let mut c = s.chars();
    let mut out: String = c.next().map(|f| f.to_uppercase().collect()).unwrap_or_default();
    out.push_str(c.as_str());
    if !out.is_empty() && !out.ends_with(['.', '!', '?']) {
        out.push('.');
    }
    out
}

/// "Apache-2.0 · commercial use allowed"
fn licence_line(m: &Value) -> String {
    let name = m["licence"]["name"].as_str().filter(|s| !s.is_empty()).unwrap_or("Unknown licence");
    let terms = match m["licence"]["commercial"].as_str() {
        Some("yes") => "commercial use allowed",
        Some("no") => "non-commercial use only",
        _ => "terms unclear",
    };
    format!("{name} · {terms}")
}

fn open_page(app: &mut LightcraftApp, url: &str) {
    if let Some(f) = app.services.open_url.as_mut() {
        let _ = f(url);
    }
}

/// The Settings ▸ Faces tab.
pub fn settings_tab(app: &mut LightcraftApp, ui: &mut egui::Ui, t: &Tokens) {
    let list = models(app, ui);
    heading(ui, t, "Face recognition");
    if list["dir"].is_null() {
        hint(ui, t, "This build has nowhere to keep face models, so the model list is not available. (The desktop app has.)");
        return;
    }
    let mut on = list["enabled"].as_bool().unwrap_or(false);
    if check(ui, "settings.facesEnabled", &mut on, "Recognise faces (experimental)") {
        let _ = app.run("faces.enable", json!({"enabled": on}));
        app.caches.faces_epoch += 1;
    }
    hint(
        ui,
        t,
        "Off by default. LightCraft already reads the face names other apps wrote into your photos; this is for models that suggest who is in a photo. Everything stays on your computer.",
    );
    if list["runtime"].as_bool() != Some(true) {
        hint(ui, t, "Models can be added and chosen now. Running them arrives in a later update.");
    }
    let all: Vec<Value> = list["models"].as_array().cloned().unwrap_or_default();
    let downloads =
        app.session.execute("faces.models.downloads", &json!({})).map(|v| v["downloads"].as_array().cloned().unwrap_or_default()).unwrap_or_default();
    heading(ui, t, "Face detector");
    for m in all.iter().filter(|m| m["role"] == "detector") {
        model_row(app, ui, t, m, downloads.iter().find(|d| d["id"] == m["id"]));
    }
    if !all.iter().any(|m| m["role"] == "detector" && m["installed"] == true) {
        hint(ui, t, "Detect Faces (Photo menu) needs it: a small one-time download.");
    }
    heading(ui, t, "Recognition models");
    for m in all.iter().filter(|m| m["role"] == "embedder") {
        model_row(app, ui, t, m, downloads.iter().find(|d| d["id"] == m["id"]));
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let can = app.services.pick_model_file.is_some();
        let r = ui.add_enabled(can, egui::Button::new("Add a model file…"));
        register(ui.ctx(), "faces:addModel", r.rect);
        if r.clicked() {
            let _ = app.run("dialog.faceModel", json!({}));
        }
        ui.label(RichText::new("or drop a .onnx file on the window").color(t.text_dim));
    });
}

fn model_row(app: &mut LightcraftApp, ui: &mut egui::Ui, t: &Tokens, m: &Value, dl: Option<&Value>) {
    let id = m["id"].as_str().unwrap_or("").to_string();
    let (installed, selected) = (m["installed"].as_bool() == Some(true), m["selected"].as_bool() == Some(true));
    let detector = m["role"] == "detector";
    let dl_state = if installed { None } else { dl.and_then(|d| d["state"].as_str()) };
    let host = m["downloadHost"].as_str().unwrap_or("").to_string();
    egui::Frame::NONE.inner_margin(egui::Margin::symmetric(0, 3)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(m["name"].as_str().unwrap_or("")).color(t.text));
                    let size = mb(m["sizeBytes"].as_u64());
                    if !size.is_empty() {
                        ui.label(RichText::new(size).color(t.text_dim));
                    }
                    if selected {
                        ui.label(RichText::new("in use").color(t.accent));
                    } else if installed {
                        ui.label(RichText::new("installed").color(t.text_dim));
                    }
                });
                ui.label(RichText::new(licence_line(m)).font(t.font(11.5)).color(if m["licence"]["commercial"] == "yes" {
                    t.text_dim
                } else {
                    t.caution
                }));
                match (dl_state, dl) {
                    (Some("running"), Some(d)) => {
                        let (got, total) = (d["bytes"].as_u64().unwrap_or(0), d["total"].as_u64().unwrap_or(0));
                        let frac = if total > 0 { (got as f32 / total as f32).clamp(0.0, 1.0) } else { 0.0 };
                        let text = if total > 0 && got >= total {
                            "Checking…".to_string()
                        } else {
                            format!("{} of {} from {host}", mb(Some(got)), mb(Some(total)))
                        };
                        let bar = ui.add(egui::ProgressBar::new(frac).desired_width(260.0).text(RichText::new(text).font(t.font(11.5))));
                        register(ui.ctx(), format!("faces:progress:{id}"), bar.rect);
                    }
                    (Some("done"), _) => {
                        ui.label(RichText::new("Downloaded and checked. Installing…").font(t.font(11.5)).color(t.text_dim));
                    }
                    (Some("failed"), Some(d)) => {
                        let why = sentence(d["error"].as_str().unwrap_or("The download failed"));
                        ui.add(egui::Label::new(RichText::new(why).font(t.font(11.5)).color(t.caution)).wrap());
                    }
                    _ => {}
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if installed {
                    let r = ui.button("Remove");
                    register(ui.ctx(), format!("faces:remove:{id}"), r.rect);
                    if r.clicked() {
                        let _ = app.run("faces.models.remove", json!({"id": id}));
                        app.caches.faces_epoch += 1;
                    }
                    if !selected && !detector {
                        let r = ui.button("Use");
                        register(ui.ctx(), format!("faces:use:{id}"), r.rect);
                        if r.clicked() {
                            let _ = app.run("faces.models.select", json!({"id": id}));
                            app.caches.faces_epoch += 1;
                        }
                    }
                } else if dl_state == Some("running") {
                    let r = ui.button("Cancel");
                    register(ui.ctx(), format!("faces:cancelDownload:{id}"), r.rect);
                    if r.clicked() {
                        let _ = app.run("faces.models.downloadCancel", json!({"id": id}));
                        app.caches.faces_dl_watch.retain(|w| w != &id);
                    }
                } else if dl_state != Some("done") {
                    // (a download that has just arrived is installed within a moment: nothing to press)
                    if let Some(url) = m["source"].as_str() {
                        let r = ui
                            .button("Open page")
                            .on_hover_text("Opens the model's own page in your browser, to read about it or get the file yourself.");
                        register(ui.ctx(), format!("faces:get:{id}"), r.rect);
                        if r.clicked() {
                            open_page(app, url);
                        }
                    }
                    if !host.is_empty() {
                        let label = if dl_state == Some("failed") { "Try again" } else { "Download" };
                        let tip = format!(
                            "Shows the model's terms, then downloads {} from {host} and installs it. Nothing else is sent.",
                            mb(m["sizeBytes"].as_u64())
                        );
                        let r = ui.button(label).on_hover_text(tip);
                        register(ui.ctx(), format!("faces:download:{id}"), r.rect);
                        if r.clicked() {
                            open_download_dialog(app, m);
                        }
                    }
                }
            });
        });
    });
}

/// The body of the "Add Face Model" dialog: what the file is, its terms, and the accept box.
/// `info` is the engine's `faces.models.inspect` answer.
pub fn model_dialog(app: &mut LightcraftApp, ui: &mut egui::Ui, t: &Tokens, info: &Value, accepted: &mut bool) {
    let kind = info["kind"].as_str().unwrap_or("unsupported");
    let file = info["fileName"].as_str().unwrap_or("");
    if kind == "unsupported" {
        ui.label(RichText::new("This file cannot be used yet").font(t.semibold(13.5)).color(t.caution));
        ui.add(
            egui::Label::new(
                RichText::new(sentence(info["reason"].as_str().unwrap_or("It is not a face recognition model LightCraft understands")))
                    .color(t.text_label),
            )
            .wrap(),
        );
        ui.label(RichText::new(format!("{file} · {}", mb(info["sizeBytes"].as_u64()))).color(t.text_dim));
        return;
    }
    let m = &info["model"];
    ui.horizontal(|ui| {
        ui.label(RichText::new(m["name"].as_str().unwrap_or(file)).font(t.semibold(14.0)).color(t.text));
        ui.label(RichText::new(mb(info["sizeBytes"].as_u64())).color(t.text_dim));
    });
    if info["alreadyInstalled"] == true {
        ui.label(RichText::new("Already installed. Installing again is harmless.").color(t.text_dim));
    }
    let download = info["download"].is_string();
    if download {
        let host = info["host"].as_str().unwrap_or("its own repository");
        ui.add(
            egui::Label::new(
                RichText::new(format!("Downloads from {host}. Once it has arrived and checked out, LightCraft installs it.")).color(t.text_label),
            )
            .wrap(),
        );
    }
    let commercial = m["licence"]["commercial"].as_str().unwrap_or("unknown");
    ui.label(RichText::new(licence_line(m)).font(t.semibold(12.5)).color(if commercial == "yes" { t.text } else { t.caution }));
    let notice = m["licence"]["notice"].as_str().unwrap_or("");
    if !notice.is_empty() {
        ui.add(egui::Label::new(RichText::new(notice).color(t.text_label)).wrap());
    }
    let provenance = m["provenance"].as_str().unwrap_or("");
    if !provenance.is_empty() {
        ui.add(egui::Label::new(RichText::new(format!("Trained on: {provenance}")).color(t.text_dim)).wrap());
    }
    if kind == "draft" {
        ui.add_space(2.0);
        ui.label(RichText::new("LightCraft does not know this model, so it assumed:").color(t.text_label));
        for a in info["assumptions"].as_array().into_iter().flatten().filter_map(Value::as_str) {
            ui.add(egui::Label::new(RichText::new(format!("•  {a}")).font(t.font(12.0)).color(t.text_dim)).wrap());
        }
    }
    if let Some(url) = m["source"].as_str().or(m["licence"]["url"].as_str()) {
        let r = ui.link("Open the model's page");
        register(ui.ctx(), "faceModel:page", r.rect);
        if r.clicked() {
            open_page(app, url);
        }
    }
    ui.add_space(4.0);
    check(ui, "faceModel.accept", accepted, "I have read these terms and accept them for my own use");
    let closing = if download {
        "Nothing is fetched until you accept. The model is kept on this computer only; LightCraft never uploads or shares it."
    } else {
        "The model is kept on this computer only. LightCraft never uploads or shares it."
    };
    ui.label(RichText::new(closing).font(t.font(11.5)).color(t.text_dim));
}

/// Pressing Download: the model's terms first, in the same dialog as a model file. Accepting them starts the download;
/// nothing is fetched before.
pub fn open_download_dialog(app: &mut LightcraftApp, m: &Value) {
    let info = json!({
        "kind": "known",
        "fileName": m["id"],
        "sizeBytes": m["sizeBytes"],
        "model": m,
        "alreadyInstalled": false,
        "download": m["id"],
        "host": m["downloadHost"],
        "assumptions": [],
        "reason": null,
    });
    app.ui.dialog = Some(crate::state::Dialog::FaceModel { path: String::new(), info, accepted: false });
}

/// Photo ▸ Detect Faces without the detector: offer to download it (its terms first), the same as in Settings.
pub fn offer_detector(app: &mut LightcraftApp) {
    let list = app.session.execute("faces.models.list", &json!({})).unwrap_or(Value::Null);
    if let Some(m) = list["models"].as_array().and_then(|a| a.iter().find(|m| m["role"] == "detector" && m["installed"] != true)) {
        open_download_dialog(app, m);
    }
}

/// Inspect `path` and open the dialog for it (also what dropping a `.onnx` file on the window does).
pub fn open_dialog(app: &mut LightcraftApp, path: &str) -> Result<Value, String> {
    let info = app.run("faces.models.inspect", json!({"path": path}))?;
    app.ui.dialog = Some(crate::state::Dialog::FaceModel { path: path.to_string(), info, accepted: false });
    Ok(Value::Null)
}

/// The dialog's OK (the engine refuses without the acceptance): for a download, start it and watch it; for a file,
/// install it.
pub fn install(app: &mut LightcraftApp, path: &str, info: &Value, accepted: bool) -> Result<Value, String> {
    if !accepted {
        return Err("Tick the box to accept the model's terms first".into());
    }
    app.caches.faces_epoch += 1;
    if let Some(id) = info["download"].as_str() {
        let r = app.run("faces.models.download", json!({"id": id, "acknowledged": true}));
        if r.is_ok() {
            app.caches.faces_dl_watch.push(id.to_string());
            // the progress is in Settings: go there
            app.ui.dialog = Some(crate::state::Dialog::Settings { tab: "faces".into() });
        }
        return r;
    }
    app.run("faces.models.install", json!({"path": path, "acknowledged": true}))
}

/// Sort the watched downloads: those that have been installed (to announce), and those still to watch (running, or
/// arrived and being installed). A failed or cancelled one is dropped: the model's row in Settings says what happened.
fn due(rows: &[Value], watched: Vec<String>) -> (Vec<String>, Vec<String>) {
    let (mut installed, mut keep) = (Vec::new(), Vec::new());
    for id in watched {
        match rows.iter().find(|r| r["id"] == id.as_str()).and_then(|r| r["state"].as_str()) {
            Some("running" | "done") => keep.push(id),
            Some("installed") => installed.push(id),
            _ => {}
        }
    }
    (installed, keep)
}

/// Called every frame: follows the downloads the user started. Keeps redrawing while one runs (the progress bar), installs
/// what has arrived (the engine does that when asked) and says so when a model is installed.
pub fn pump(app: &mut LightcraftApp, ctx: &egui::Context) {
    if app.caches.faces_dl_watch.is_empty() {
        return;
    }
    let Ok(v) = app.session.execute("faces.models.downloads", &json!({})) else {
        app.caches.faces_dl_watch.clear();
        return;
    };
    let rows: Vec<Value> = v["downloads"].as_array().cloned().unwrap_or_default();
    let (installed, keep) = due(&rows, std::mem::take(&mut app.caches.faces_dl_watch));
    app.caches.faces_dl_watch = keep;
    for id in installed {
        let name = app
            .session
            .execute("faces.models.list", &json!({}))
            .ok()
            .and_then(|l| {
                l["models"].as_array().and_then(|a| a.iter().find(|m| m["id"] == id.as_str()).and_then(|m| m["name"].as_str().map(str::to_string)))
            })
            .unwrap_or_else(|| id.clone());
        let _ = app.session.execute("faces.models.downloadCancel", &json!({"id": id}));
        app.caches.faces_epoch += 1;
        let text = format!("{name} is installed");
        app.ui.status = text.clone();
        app.toast(ctx, text);
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(120));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, state: &str) -> Value {
        json!({"id": id, "state": state})
    }

    #[test]
    fn an_installed_download_is_announced_once_and_the_rest_are_followed() {
        let rows = vec![row("a", "running"), row("b", "done"), row("c", "installed"), row("d", "failed"), row("e", "cancelled")];
        let watched = ["a", "b", "c", "d", "e", "gone"].map(String::from).to_vec();
        let (installed, keep) = due(&rows, watched);
        assert_eq!(installed, ["c"]);
        // running and just-arrived ones are watched; failed, cancelled and vanished ones are dropped
        assert_eq!(keep, ["a", "b"]);
        assert_eq!(due(&[], vec!["x".into()]), (vec![], vec![]));
    }
}
