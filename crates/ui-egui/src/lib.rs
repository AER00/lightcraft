//! LightCraft's egui frontend: a Lightroom-style UI over `lightcraft-engine`.
//!
//! The UI is thin: every action goes through [`LightcraftApp::run`], which handles UI commands
//! (views, panels, zoom — see [`menus::UI_COMMANDS`]) and forwards everything else to the engine.
//! The same entry point serves menus, shortcuts, buttons and the control channel ([`control`]).
#![forbid(unsafe_code)]

pub mod control;
pub mod icons;
pub mod menus;
pub mod panels;
pub mod render;
pub mod shortcuts;
pub mod state;
pub mod theme;
pub mod widgets;

use std::sync::mpsc::{Receiver, Sender};

use lightcraft_engine::Session;
use serde_json::Value;

pub use control::{ControlRequest, ControlResponse};
pub use state::UiState;

pub type PickFiles = Box<dyn FnMut() -> Vec<String>>;
/// A save dialog: suggested file name → chosen path (`None` = cancelled).
pub type SaveFile = Box<dyn FnMut(&str) -> Option<String>>;
pub type WriteFn = Box<dyn FnMut(&str, &[u8]) -> Result<(), String>>;
pub type PngEncode = Box<dyn Fn(&lightcraft_raster::Rgba8) -> Vec<u8>>;

/// Platform services injected by the host app (desktop or web).
#[derive(Default)]
pub struct Services {
    /// Show an open dialog for photos; returns paths.
    pub pick_files: Option<PickFiles>,
    /// Open dialog for preset files (`.lcpreset`, `.xmp`).
    pub pick_preset_files: Option<PickFiles>,
    /// Save dialog for an exported `.lcpreset` file.
    pub save_preset_file: Option<SaveFile>,
    pub write: Option<WriteFn>,
    /// PNG encoder (the host links an image encoder; the UI crate stays codec-free).
    pub png: Option<PngEncode>,
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
    /// The host installed a native menu bar.
    pub native_menu: bool,
    control_rx: Option<Receiver<ControlRequest>>,
    pending_screenshots: Vec<(u64, Option<String>, Sender<ControlResponse>)>,
    queued_screenshots: Vec<(u64, f64, u32)>,
    screenshot_token: u64,
    /// Synthetic input events (from the control channel) injected one step per frame.
    pub synthetic: Vec<egui::Event>,
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
}

impl LightcraftApp {
    pub fn new(session: Session, services: Services) -> Self {
        Self {
            session,
            ui: UiState::default(),
            services,
            renderer: render::Renderer::default(),
            perf: Perf::default(),
            integrated_titlebar: false,
            native_menu: false,
            control_rx: None,
            pending_screenshots: vec![],
            queued_screenshots: vec![],
            screenshot_token: 0,
            synthetic: vec![],
            styled: false,
            fonts_ready: false,
            last_time: 0.0,
            canvas_rect: None,
            image_rect: None,
            widgets: vec![],
            gesture: None,
        }
    }

    pub fn with_control(mut self, rx: Receiver<ControlRequest>) -> Self {
        self.control_rx = Some(rx);
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
                control::Outcome::Screenshot { path } => {
                    self.screenshot_token += 1;
                    let token = self.screenshot_token;
                    self.queued_screenshots.push((token, now_ms() + 120.0, 0));
                    self.pending_screenshots.push((token, path, reply));
                }
            }
        }
        self.control_rx = Some(rx);
    }

    fn issue_screenshots(&mut self, ctx: &egui::Context) {
        let now = now_ms();
        let busy = self.renderer.queued() > 0;
        self.queued_screenshots.retain_mut(|(token, at, frames)| {
            *frames += 1;
            // wait for pending renders (up to ~3 s) so screenshots show finished pixels
            if now >= *at && *frames >= 3 && (!busy || *frames > 180) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(*token)));
                false
            } else {
                true
            }
        });
        if !self.queued_screenshots.is_empty() || !self.pending_screenshots.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
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
            if let Some(i) = self.pending_screenshots.iter().position(|(t, _, _)| *t == token) {
                let (_, path, reply) = self.pending_screenshots.remove(i);
                let _ = reply.send(control::save_screenshot(self, &image, path.as_deref()));
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
        if !self.synthetic.is_empty() {
            ctx.request_repaint();
        }
        self.renderer.poll(ctx, &mut self.session);
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
            if !dropped.is_empty() {
                let _ = self.run("library.import", serde_json::json!({"paths": dropped}));
            }
        }
    }

    /// Inject synthetic events (pointer events one per frame; key sequences up to the release).
    pub fn raw_input_hook(&mut self, raw: &mut egui::RawInput) {
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
        let bg = if matches!(self.ui.view, state::ViewMode::Detail | state::ViewMode::Compare) { t.canvas } else { t.grid_bg };
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(bg)).show(ui, |ui| match self.ui.view {
            state::ViewMode::PhotoGrid | state::ViewMode::SquareGrid => panels::grid::show(self, ui),
            state::ViewMode::Detail | state::ViewMode::Compare => panels::detail::show(self, ui),
        });
        panels::dialogs::show(self, &ctx);
        panels::toast(self, &ctx);
        self.widgets = widgets::take_registry(&ctx);
        self.perf.frame_ms = now_ms() - t0;
    }
}

/// Wall-clock milliseconds since the Unix epoch (`web-time` maps to `Date.now()` on the web).
pub fn now_ms() -> f64 {
    use web_time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64() * 1000.0).unwrap_or(0.0)
}

/// Whether the settings render in black & white.
pub fn is_bw(d: &lightcraft_develop::DevelopSettings) -> bool {
    d.treatment == lightcraft_develop::Treatment::Bw || d.profile.id == "lc.mono"
}
