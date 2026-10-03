//! LightCraft's egui frontend: a Lightroom-style UI over `lightcraft-engine`.
//!
//! The UI is thin: every action goes through [`LightcraftApp::run`], which handles UI commands
//! (views, panels, zoom — see [`menus::UI_COMMANDS`]) and forwards everything else to the engine.
//! The same entry point serves menus, shortcuts, buttons and the control channel ([`control`]).
#![forbid(unsafe_code)]

pub mod control;
pub mod export_task;
pub mod headless;
pub mod icons;
pub mod import;
pub mod links;
pub mod menubar;
pub mod menus;
pub mod merge;
pub mod panels;
pub mod render;
pub mod shortcuts;
pub mod softpaint;
pub mod state;
pub mod theme;
pub mod widgets;

#[cfg(test)]
mod tests_masking;

use std::sync::mpsc::{Receiver, Sender};

use lightcraft_engine::Session;
use serde_json::Value;

pub use control::{ControlRequest, ControlResponse};
pub use state::UiState;

pub type PickFiles = Box<dyn FnMut() -> Vec<String>>;
/// A save dialog: suggested file name → chosen path (`None` = cancelled).
pub type SaveFile = Box<dyn FnMut(&str) -> Option<String>>;
pub type WriteFn = Box<dyn FnMut(&str, &[u8]) -> Result<(), String>>;
/// A writer other threads can use (background export).
pub type SharedWrite = std::sync::Arc<dyn Fn(&str, &[u8]) -> Result<(), String> + Send + Sync>;
pub type PngEncode = Box<dyn Fn(&lightcraft_raster::Rgba8) -> Vec<u8>>;
/// A folder chooser (`None` = cancelled).
pub type PickFolder = Box<dyn FnMut() -> Option<String>>;
/// Reveal a file in the system file manager (Finder / Explorer / the folder on Linux).
pub type RevealFn = Box<dyn FnMut(&str) -> Result<(), String>>;
/// Open a URL in the user's browser.
pub type OpenUrlFn = Box<dyn FnMut(&str) -> Result<(), String>>;

/// Platform services injected by the host app (desktop or web).
#[derive(Default)]
pub struct Services {
    /// Show an open dialog for photos; returns paths.
    pub pick_files: Option<PickFiles>,
    /// Open dialog for preset files (`.lcpreset`, `.xmp`, `.lrtemplate`, `.zip`, `.dng`).
    pub pick_preset_files: Option<PickFiles>,
    /// Save dialog for an exported `.lcpreset` file.
    pub save_preset_file: Option<SaveFile>,
    pub write: Option<WriteFn>,
    /// Thread-safe writer: with it, UI-started exports run in the background (desktop only).
    pub write_shared: Option<SharedWrite>,
    /// PNG encoder (the host links an image encoder; the UI crate stays codec-free).
    pub png: Option<PngEncode>,
    /// Show a file in the system file manager (desktop only).
    pub reveal: Option<RevealFn>,
    /// Choose a folder (Settings → General → Open Library…; desktop only).
    pub pick_folder: Option<PickFolder>,
    /// Open a web link in the browser (Help menu, About, Discord button).
    pub open_url: Option<OpenUrlFn>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Perf {
    pub frame_ms: f64,
    pub fps: f64,
}

pub struct LightcraftApp {
    pub session: Session,
    pub ui: UiState,
    pub services: Services,
    pub renderer: render::Renderer,
    pub perf: Perf,
    /// macOS: the host draws the traffic lights over our top bar.
    pub integrated_titlebar: bool,
    /// The host installed a native menu bar (no in-window menus then).
    pub native_menu: bool,
    /// Shortcuts the native menu bar currently handles (`Cmd+Z`, `G`…): the egui shortcut handler
    /// leaves them alone so nothing fires twice.
    pub native_shortcuts: std::collections::HashSet<String>,
    /// The host is [`headless::Headless`] (it answers viewport screenshot commands itself).
    pub headless_host: bool,
    control_rx: Option<Receiver<ControlRequest>>,
    pending_screenshots: Vec<PendingShot>,
    screenshot_token: u64,
    /// Offscreen context for headless screenshots of the windowed app.
    shadow: Option<headless::HeadlessView>,
    /// Synthetic input events (from the control channel) injected one step per frame.
    pub synthetic: Vec<egui::Event>,
    /// Modifiers announced for synthetic input (held from a button down to its release).
    synthetic_mods: egui::Modifiers,
    /// Clear `synthetic_mods` on the next frame.
    synthetic_mods_release: bool,
    styled: bool,
    fonts_ready: bool,
    last_time: f64,
    /// Rect of the photo canvas and the displayed image (screen points) from the last frame.
    pub canvas_rect: Option<egui::Rect>,
    pub image_rect: Option<egui::Rect>,
    /// Widget registry from the last frame (automation ids → rects).
    pub widgets: Vec<(String, egui::Rect)>,
    /// In-progress on-canvas gesture (brush stroke points, gradient drag…).
    pub gesture: Option<panels::detail::Gesture>,
    /// What the loupe drew last frame: photo and source ("render", "cached", "embedded", "small",
    /// "thumb", "none").
    pub loupe_shown: Option<(lightcraft_catalog::PhotoId, &'static str)>,
    /// Photo Merge dialog previews and background merges.
    pub merge: merge::MergeState,
    /// An import in progress (the import review dialog's batches).
    pub import: Option<import::ImportTask>,
    /// A background export in progress.
    pub export: Option<export_task::ExportTask>,
    /// The files of the last finished background export (`ui.inspect` → `export.last`).
    pub last_export_result: Option<Value>,
    /// The look the loupe shows while the pointer rests on a preset or profile (set by the
    /// panels each frame; nothing is committed, no history entry).
    pub hover_preview: Option<HoverPreview>,
    /// The window is in full screen (as last reported by the host, or as last requested).
    pub window_is_fullscreen: bool,
    /// The GPU preference last applied (`app.gpu`), to apply Settings changes once.
    gpu_applied: Option<bool>,
    /// The memory budget setting last applied (MB, 0 = automatic).
    memory_applied: Option<u32>,
}

impl LightcraftApp {
    pub fn new(session: Session, services: Services) -> Self {
        // GPU device + kernels off the UI thread, before the first photo is opened
        lightcraft_engine::gpu::warm_up();
        Self {
            session,
            ui: UiState::default(),
            services,
            renderer: render::Renderer::default(),
            perf: Perf::default(),
            integrated_titlebar: false,
            native_menu: false,
            native_shortcuts: Default::default(),
            headless_host: false,
            control_rx: None,
            pending_screenshots: vec![],
            screenshot_token: 0,
            shadow: None,
            synthetic: vec![],
            synthetic_mods: egui::Modifiers::NONE,
            synthetic_mods_release: false,
            styled: false,
            fonts_ready: false,
            last_time: 0.0,
            canvas_rect: None,
            image_rect: None,
            widgets: vec![],
            gesture: None,
            loupe_shown: None,
            merge: merge::MergeState::default(),
            import: None,
            export: None,
            last_export_result: None,
            hover_preview: None,
            window_is_fullscreen: false,
            gpu_applied: None,
            memory_applied: None,
        }
    }

    pub fn with_control(mut self, rx: Receiver<ControlRequest>) -> Self {
        self.control_rx = Some(rx);
        // keep CPU copies of photo textures so `ui.screenshot {"headless": true}` can draw them
        self.renderer.keep_pixels = true;
        self
    }

    /// Run a UI or engine command by id. The single entry point for every frontend path.
    pub fn run(&mut self, id: &str, params: Value) -> Result<Value, String> {
        if let Some(r) = menus::run_ui_command(self, id, &params) {
            return r;
        }
        let r = self.session.execute(id, &params).map_err(|e| e.to_string());
        if let Err(e) = &r {
            self.ui.status = e.clone();
        }
        r
    }

    /// Show a transient toast at the bottom of the canvas (like the reference app's HUD).
    /// Announce the start and end of a Build Previews run.
    fn preview_build_status(&mut self, ctx: &egui::Context) {
        use std::sync::atomic::Ordering;
        let Some(b) = self.session.preview_build.clone() else { return };
        let key = std::sync::Arc::as_ptr(&b) as usize;
        if b.finished.load(Ordering::Relaxed) {
            if self.ui.preview_build_seen != Some((key, true)) {
                self.ui.preview_build_seen = Some((key, true));
                let (done, failed) = (b.done.load(Ordering::Relaxed), b.failed.load(Ordering::Relaxed));
                let mut msg = format!("Previews ready for {done} photo{}", if done == 1 { "" } else { "s" });
                if failed > 0 {
                    msg.push_str(&format!(" · {failed} couldn't be rendered"));
                }
                self.toast(ctx, msg);
            }
        } else {
            if self.ui.preview_build_seen != Some((key, false)) {
                self.ui.preview_build_seen = Some((key, false));
                self.toast(ctx, format!("Building previews for {} photos…", b.total));
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }

    pub fn toast(&mut self, ctx: &egui::Context, text: impl Into<String>) {
        let t = ctx.input(|i| i.time);
        self.ui.toast = Some((text.into(), t + 1.4));
    }

    fn drain_control(&mut self, ctx: &egui::Context) {
        let Some(rx) = self.control_rx.take() else { return };
        while let Ok(req) = rx.try_recv() {
            let reply = req.reply.clone();
            match control::handle(self, ctx, &req) {
                control::Outcome::Done(v) => {
                    let _ = reply.send(v);
                }
                control::Outcome::Screenshot { path, headless } => {
                    self.screenshot_token += 1;
                    let now = now_ms();
                    self.pending_screenshots.push(PendingShot {
                        token: self.screenshot_token,
                        path,
                        reply,
                        headless: headless && !self.headless_host,
                        not_before: now + 120.0,
                        deadline: now + SCREENSHOT_SETTLE_MS,
                        frames: 0,
                        sent_at: None,
                    });
                }
            }
        }
        self.control_rx = Some(rx);
    }

    /// Advance pending screenshots: wait (≥ 3 frames, ≥ 120 ms) until no renders are in flight
    /// (or a timeout), then capture — via the host's compositor, or headlessly
    /// ([`Self::headless_screenshot`]) when asked or when the compositor delivers nothing.
    fn issue_screenshots(&mut self, ctx: &egui::Context) {
        if self.pending_screenshots.is_empty() {
            return;
        }
        let now = now_ms();
        let busy = self.renderer.in_flight() > 0 || self.merge.busy();
        let mut shots = std::mem::take(&mut self.pending_screenshots);
        let mut shadow_ticked = false;
        shots.retain_mut(|s| {
            if let Some(sent) = s.sent_at {
                if now - sent < SCREENSHOT_FALLBACK_MS || self.headless_host {
                    return true;
                }
                // The compositor delivered nothing (display asleep, window occluded): go headless.
                log::warn!("ui.screenshot: no frame from the compositor after {SCREENSHOT_FALLBACK_MS} ms; rendering headlessly");
                s.sent_at = None;
                s.headless = true;
                s.deadline = now + SCREENSHOT_SETTLE_MS;
            }
            s.frames += 1;
            let ready = now >= s.not_before && s.frames >= 3 && (!busy || now > s.deadline);
            if s.headless {
                // The shadow frame requests the renders the UI needs, even while the window shows nothing.
                let img = if ready || !shadow_ticked { self.headless_screenshot(ctx, ready) } else { None };
                shadow_ticked = true;
                if let Some(img) = img {
                    let _ = s.reply.send(control::save_screenshot(self, &img, s.path.as_deref()));
                    return false;
                }
                true
            } else {
                if ready {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(s.token)));
                    s.sent_at = Some(now);
                }
                true
            }
        });
        shots.append(&mut self.pending_screenshots);
        self.pending_screenshots = shots;
        if !self.pending_screenshots.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
    }

    /// Draw the UI into an offscreen context at the window's size; with `capture`, rasterize it
    /// on the CPU (photo textures come from the renderer's CPU copies).
    pub fn headless_screenshot(&mut self, main: &egui::Context, capture: bool) -> Option<egui::ColorImage> {
        let mut view = self.shadow.take().unwrap_or_default();
        let size = main.input(|i| i.content_rect()).size();
        let size = if size.x >= 1.0 && size.y >= 1.0 { size } else { egui::vec2(1600.0, 1000.0) };
        let ppp = main.pixels_per_point();
        let time = main.input(|i| i.time);
        if view.frames() == 0 {
            // warm-up pass: activates our fonts (pending font definitions live in `Memory`, which
            // is replaced below)
            view.run(headless::HeadlessView::raw_input(size, ppp, time, vec![]), |_| {});
        }
        // same scroll offsets, open sections, style… as the window
        let memory = main.memory(|m| m.clone());
        view.ctx.memory_mut(|m| *m = memory);
        view.run(headless::HeadlessView::raw_input(size, ppp, time, vec![]), |ui| self.ui(ui));
        let img = capture.then(|| {
            let mut tex = self.renderer.cpu_textures();
            if let (Some((t, ..)), Some(px)) = (&self.merge.preview, &self.merge.preview_pixels) {
                tex.insert(t.id(), crate::softpaint::CpuTexture::linear(px.clone()));
            }
            view.paint(&tex)
        });
        self.shadow = Some(view);
        img
    }

    fn collect_screenshots(&mut self, ctx: &egui::Context) {
        if self.pending_screenshots.is_empty() {
            return;
        }
        let events: Vec<_> = ctx.input(|i| {
            i.raw
                .events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Screenshot { user_data, image, .. } => {
                        let token = user_data.data.as_ref().and_then(|d| d.downcast_ref::<u64>()).copied()?;
                        Some((token, image.clone()))
                    }
                    _ => None,
                })
                .collect()
        });
        for (token, image) in events {
            if let Some(i) = self.pending_screenshots.iter().position(|s| s.token == token) {
                let s = self.pending_screenshots.remove(i);
                let _ = s.reply.send(control::save_screenshot(self, &image, s.path.as_deref()));
            }
        }
    }

    /// Per-frame logic before layout (control channel, renders, shortcuts, drops).
    pub fn logic(&mut self, ctx: &egui::Context) {
        if !self.styled {
            theme::install_fonts(ctx);
            theme::apply(ctx);
            self.styled = true;
        } else {
            self.fonts_ready = true;
        }
        let now = ctx.input(|i| i.time);
        let dt = now - self.last_time;
        if dt > 0.0 {
            self.perf.fps = self.perf.fps * 0.9 + (1.0 / dt).min(240.0) * 0.1;
        }
        self.last_time = now;
        self.drain_control(ctx);
        self.apply_settings(ctx);
        if !self.synthetic.is_empty() {
            ctx.request_repaint();
        }
        self.renderer.poll(ctx, &mut self.session);
        merge::poll(self, ctx);
        import::tick(self, ctx);
        self.preview_build_status(ctx);
        self.session.persist_if_dirty();
        self.collect_screenshots(ctx);
        self.issue_screenshots(ctx);
        if self.fonts_ready {
            shortcuts::handle(self, ctx);
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let dropped: Vec<String> =
                ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_string_lossy().to_string()).filter(|p| !p.is_empty()).collect());
            // preset files import as presets, everything else as photos
            let (presets, photos): (Vec<String>, Vec<String>) = dropped.into_iter().partition(|p| is_preset_file(p));
            if !presets.is_empty() {
                let _ = self.run("file.importPresets", serde_json::json!({"paths": presets}));
            }
            if !photos.is_empty() {
                let _ = self.run("library.import", serde_json::json!({"paths": photos}));
            }
        }
    }

    /// Apply settings that act outside the UI state: GPU rendering and window full screen.
    fn apply_settings(&mut self, ctx: &egui::Context) {
        if self.gpu_applied != Some(self.ui.settings.gpu) {
            self.gpu_applied = Some(self.ui.settings.gpu);
            let _ = self.session.execute("app.gpu", &serde_json::json!({"enabled": self.ui.settings.gpu}));
        }
        let mb = self.ui.settings.memory_mb;
        // automatic at startup: leave the engine's default alone
        if self.memory_applied != Some(mb) && (mb > 0 || self.memory_applied.is_some()) {
            let mb = if mb == 0 { (lightcraft_engine::memory::default_budget() >> 20) as u32 } else { mb };
            let _ = self.session.execute("app.memoryBudget", &serde_json::json!({"mb": mb}));
        }
        self.memory_applied = Some(mb);
        if let Some(fs) = ctx.input(|i| i.viewport().fullscreen) {
            self.window_is_fullscreen = fs;
        }
        if let Some(on) = self.ui.window_fullscreen.take() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(on));
            self.window_is_fullscreen = on;
        }
    }

    /// Inject synthetic events (pointer events one per frame; key sequences up to the release).
    pub fn raw_input_hook(&mut self, raw: &mut egui::RawInput) {
        // the frame after a synthetic release / key: modifiers back up
        if std::mem::take(&mut self.synthetic_mods_release) && self.synthetic_mods != egui::Modifiers::NONE {
            self.synthetic_mods = egui::Modifiers::NONE;
            raw.events.push(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        }
        if self.synthetic.is_empty() {
            return;
        }
        let n = match self.synthetic[0] {
            egui::Event::PointerMoved(_) | egui::Event::PointerButton { .. } | egui::Event::MouseWheel { .. } => 1,
            _ => self.synthetic.iter().position(|e| matches!(e, egui::Event::Key { pressed: false, .. })).map_or(self.synthetic.len(), |i| i + 1),
        };
        if let Some(egui::Event::PointerMoved(p) | egui::Event::PointerButton { pos: p, .. }) = self.synthetic.first() {
            raw.events.push(egui::Event::PointerMoved(*p));
        }
        // egui's `input.modifiers` follow `ModifiersChanged` events, not the modifiers carried by
        // pointer/key events: announce the injected events' modifiers (held from button down to
        // the frame after up), so ⌥-drag, ⇧-click … work through the control channel
        let carried = self.synthetic[..n].iter().find_map(|e| match e {
            egui::Event::PointerButton { pressed, modifiers, .. } => Some((*modifiers, !pressed)),
            egui::Event::Key { modifiers, .. } => Some((*modifiers, true)),
            _ => None,
        });
        if let Some((m, ends)) = carried {
            if m != self.synthetic_mods {
                self.synthetic_mods = m;
                raw.events.push(egui::Event::ModifiersChanged(m));
            }
            self.synthetic_mods_release = ends;
        }
        raw.events.extend(self.synthetic.drain(..n));
    }

    /// Lay out the whole window.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if !self.fonts_ready {
            ctx.request_repaint();
            return;
        }
        let t0 = now_ms();
        if std::mem::take(&mut self.ui.quit) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        // panels set it again this frame while the pointer rests on a preset or profile
        self.hover_preview = None;
        if self.ui.fullscreen {
            // full-screen preview: the photo alone on black
            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(egui::Color32::BLACK)).show(ui, |ui| panels::detail::show(self, ui));
            panels::dialogs::show(self, &ctx);
            panels::toast(self, &ctx);
            self.widgets = widgets::take_registry(&ctx);
            self.perf.frame_ms = now_ms() - t0;
            return;
        }
        // Order matters: earlier panels take the full edge (top bar spans the window; the tool strip,
        // right panels and left panel run to the bottom; the bottom bar sits between them).
        panels::topbar::show(self, ui);
        panels::strip::show(self, ui);
        if self.ui.right != state::RightPanel::None {
            panels::right::show(self, ui);
        }
        if self.ui.presets {
            panels::presets::show(self, ui);
        }
        if self.ui.left_panel {
            panels::left::show(self, ui);
        }
        panels::bottombar::show(self, ui);
        let t = theme::Tokens::get(&ctx);
        let bg =
            if matches!(self.ui.view, state::ViewMode::Detail | state::ViewMode::Compare | state::ViewMode::Survey) { t.canvas } else { t.grid_bg };
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(bg)).show(ui, |ui| match self.ui.view {
            state::ViewMode::PhotoGrid | state::ViewMode::SquareGrid => panels::grid::show(self, ui),
            state::ViewMode::Detail => panels::detail::show(self, ui),
            state::ViewMode::Compare => panels::compare::show_compare(self, ui),
            state::ViewMode::Survey => panels::compare::show_survey(self, ui),
        });
        panels::dialogs::show(self, &ctx);
        import::progress(self, &ctx);
        export_task::poll(self, &ctx);
        panels::grid::drag_feedback(self, &ctx);
        panels::toast(self, &ctx);
        self.widgets = widgets::take_registry(&ctx);
        self.perf.frame_ms = now_ms() - t0;
    }
}

/// How long a screenshot waits for in-flight renders.
const SCREENSHOT_SETTLE_MS: f64 = 3000.0;
/// How long a windowed screenshot waits for the compositor before falling back to headless.
const SCREENSHOT_FALLBACK_MS: f64 = 2000.0;

/// A `ui.screenshot` request in progress.
struct PendingShot {
    token: u64,
    path: Option<String>,
    reply: Sender<ControlResponse>,
    headless: bool,
    not_before: f64,
    deadline: f64,
    frames: u32,
    /// When the viewport screenshot command went out.
    sent_at: Option<f64>,
}

/// Wall-clock milliseconds since the Unix epoch (`web-time` maps to `Date.now()` on the web).
pub fn now_ms() -> f64 {
    use web_time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64() * 1000.0).unwrap_or(0.0)
}

/// Whether the settings render in black & white.
/// A temporary look for the loupe (hovering a preset or profile).
#[derive(Clone, Debug, PartialEq)]
pub struct HoverPreview {
    /// What is previewed (e.g. "Preset: Warm Glow").
    pub label: String,
    /// The photo's settings with the look applied.
    pub settings: lightcraft_develop::DevelopSettings,
}

pub fn is_bw(d: &lightcraft_develop::DevelopSettings) -> bool {
    d.treatment == lightcraft_develop::Treatment::Bw || d.profile.id == "lc.mono" || d.profile.id.starts_with("lc.bw.")
}

/// Files dropped on the window that are presets rather than photos.
pub fn is_preset_file(path: &str) -> bool {
    let ext = std::path::Path::new(path).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    ["lcpreset", "lrtemplate", "xmp", "zip"].contains(&ext.as_str())
}

#[cfg(test)]
mod drop_tests {
    #[test]
    fn dropped_presets_are_told_apart_from_photos() {
        for p in ["/a/Look.lrtemplate", "/a/b.XMP", "/a/pack.zip", "/a/x.lcpreset"] {
            assert!(super::is_preset_file(p), "{p}");
        }
        for p in ["/a/IMG_1.CR2", "/a/b.dng", "/a/c.jpg", "/a/folder"] {
            assert!(!super::is_preset_file(p), "{p}");
        }
    }
}
