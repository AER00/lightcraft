//! Headless UI rendering: run the app's UI in an offscreen [`egui::Context`] and rasterize it on
//! the CPU ([`crate::softpaint`]) — no window, no GPU, no compositor.
//!
//! Two users:
//! - [`Headless`]: a complete windowless app session (`lightcraft-cli snapshot`, tests). It plays
//!   the role eframe plays for the desktop app: builds [`egui::RawInput`], runs `logic` + `ui`,
//!   keeps a CPU mirror of the textures, executes viewport commands (`Screenshot` is answered with
//!   a CPU-rendered frame, `InnerSize` resizes, `Close` quits). Control-protocol requests go
//!   through the very same handler as the desktop app's control server.
//! - [`HeadlessView`] inside the desktop app: `ui.screenshot {"headless": true}` (and the
//!   automatic fallback when the compositor delivers no frame, e.g. while the display sleeps)
//!   draws the app's UI into a shadow context from `logic`, which keeps ticking when the window
//!   is occluded.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use egui::{Color32, ColorImage, RawInput, TextureId, ViewportCommand, ViewportId};
use serde_json::{Value, json};

use crate::softpaint::{self, CpuTexture, Layered, TextureStore};
use crate::{ControlRequest, LightcraftApp};

/// Background behind the panels (eframe's default clear colour, made opaque).
const CLEAR: Color32 = Color32::from_rgb(12, 12, 12);
/// Simulated frame interval (deterministic animation time).
const FRAME_DT: f64 = 1.0 / 60.0;

/// An offscreen egui context with our fonts and theme, and a CPU mirror of its textures.
pub struct HeadlessView {
    pub ctx: egui::Context,
    pub textures: TextureStore,
    shapes: Vec<egui::epaint::ClippedShape>,
    pixels_per_point: f32,
    size: egui::Vec2,
    frames: u64,
}

impl Default for HeadlessView {
    fn default() -> Self {
        Self::new()
    }
}

impl HeadlessView {
    pub fn new() -> Self {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        crate::theme::apply(&ctx);
        HeadlessView { ctx, textures: TextureStore::default(), shapes: vec![], pixels_per_point: 1.0, size: egui::vec2(1600.0, 1000.0), frames: 0 }
    }

    /// Input for one frame of a `size` (points) viewport at `pixels_per_point`.
    pub fn raw_input(size: egui::Vec2, pixels_per_point: f32, time: f64, events: Vec<egui::Event>) -> RawInput {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        let mut raw = RawInput {
            screen_rect: Some(rect),
            time: Some(time),
            predicted_dt: FRAME_DT as f32,
            focused: true,
            events,
            max_texture_side: Some(16384),
            ..Default::default()
        };
        let info = raw.viewports.entry(ViewportId::ROOT).or_default();
        info.native_pixels_per_point = Some(pixels_per_point);
        info.inner_rect = Some(rect);
        info.outer_rect = Some(rect);
        info.focused = Some(true);
        raw
    }

    /// Run one frame; keeps the shapes for [`Self::paint`]. Returns the root viewport's commands.
    pub fn run(&mut self, raw: RawInput, run_ui: impl FnMut(&mut egui::Ui)) -> Vec<ViewportCommand> {
        if let Some(r) = raw.screen_rect {
            self.size = r.size();
        }
        let mut out = self.ctx.run_ui(raw, run_ui);
        self.frames += 1;
        self.textures.apply(std::mem::take(&mut out.textures_delta));
        self.shapes = std::mem::take(&mut out.shapes);
        self.pixels_per_point = out.pixels_per_point;
        out.viewport_output.remove(&ViewportId::ROOT).map(|v| v.commands).unwrap_or_default()
    }

    /// Frames run so far.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// Size in pixels of the last frame.
    pub fn size_px(&self) -> [usize; 2] {
        [(self.size.x * self.pixels_per_point).round() as usize, (self.size.y * self.pixels_per_point).round() as usize]
    }

    /// Rasterize the last frame. `extra` textures (by id) take precedence over the context's own.
    pub fn paint(&self, extra: &HashMap<TextureId, CpuTexture>) -> ColorImage {
        let prims = self.ctx.tessellate(self.shapes.clone(), self.pixels_per_point);
        softpaint::paint(&prims, &Layered { over: extra, base: &self.textures }, self.size_px(), self.pixels_per_point, CLEAR)
    }
}

/// A windowless app session: the app's `logic` + `ui` driven frame by frame into a
/// [`HeadlessView`], with control-protocol requests answered by the shared handler.
pub struct Headless {
    pub app: LightcraftApp,
    pub view: HeadlessView,
    /// Logical size (points) and scale.
    pub size: egui::Vec2,
    pub pixels_per_point: f32,
    time: f64,
    frames: u64,
    events: Vec<egui::Event>,
    control: Sender<ControlRequest>,
    quit: bool,
}

impl Headless {
    /// Wrap `app` (its control channel is replaced by the driver's).
    pub fn new(app: LightcraftApp, size: [f32; 2], pixels_per_point: f32) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut app = app.with_control(rx);
        app.headless_host = true;
        Headless {
            app,
            view: HeadlessView::new(),
            size: egui::vec2(size[0], size[1]),
            pixels_per_point,
            time: 0.0,
            frames: 0,
            events: vec![],
            control: tx,
            quit: false,
        }
    }

    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// `app.quit` was requested.
    pub fn quit_requested(&self) -> bool {
        self.quit
    }

    /// Run one frame.
    pub fn step(&mut self) {
        let mut raw = HeadlessView::raw_input(self.size, self.pixels_per_point, self.time, std::mem::take(&mut self.events));
        self.app.raw_input_hook(&mut raw);
        let app = &mut self.app;
        let commands = self.view.run(raw, |ui| {
            app.logic(ui.ctx());
            app.ui(ui);
        });
        self.time += FRAME_DT;
        self.frames += 1;
        for c in commands {
            match c {
                ViewportCommand::Screenshot(user_data) => {
                    let image = Arc::new(self.paint());
                    self.events.push(egui::Event::Screenshot { viewport_id: ViewportId::ROOT, user_data, image });
                }
                ViewportCommand::InnerSize(s) if s.x >= 1.0 && s.y >= 1.0 => self.size = s,
                ViewportCommand::Close => self.quit = true,
                _ => {}
            }
        }
    }

    /// Is anything still in progress (renders, queued input)?
    pub fn busy(&self) -> bool {
        self.app.renderer.in_flight() > 0 || self.app.merge.busy() || !self.app.synthetic.is_empty() || !self.events.is_empty()
    }

    /// Run frames until nothing is pending (renders finished, input consumed) for a few frames in
    /// a row, or `timeout` passes. Returns whether it settled.
    pub fn settle(&mut self, timeout: Duration) -> bool {
        let t0 = Instant::now();
        let mut quiet = 0;
        loop {
            self.step();
            if self.busy() {
                quiet = 0;
                std::thread::sleep(Duration::from_millis(1));
            } else {
                quiet += 1;
            }
            // a few quiet frames let layout and short UI animations finish
            if quiet >= 12 {
                return true;
            }
            if t0.elapsed() > timeout {
                return false;
            }
        }
    }

    /// Rasterize the last frame (with the photo textures).
    pub fn paint(&self) -> ColorImage {
        self.view.paint(&HashMap::new())
    }

    /// Settle, then render the current UI.
    pub fn snapshot(&mut self, timeout: Duration) -> ColorImage {
        self.settle(timeout);
        self.paint()
    }

    /// Send a control-protocol request (see [`crate::control`]) and run frames until it is
    /// answered, then until its input has been consumed. Returns `{"ok": …, "result"|"error": …}`.
    pub fn request(&mut self, method: &str, params: Value, timeout: Duration) -> Value {
        let (req, rx) = ControlRequest::new(method, params);
        if self.control.send(req).is_err() {
            return json!({"ok": false, "error": "control channel closed"});
        }
        let t0 = Instant::now();
        let reply = loop {
            self.step();
            if let Ok(v) = rx.try_recv() {
                break v;
            }
            if t0.elapsed() > timeout {
                return json!({"ok": false, "error": "timeout"});
            }
            if self.busy() {
                std::thread::sleep(Duration::from_millis(1));
            }
        };
        // let injected input (clicks, keys, drags) play out before the next request
        let t1 = Instant::now();
        while (!self.app.synthetic.is_empty() || !self.events.is_empty()) && t1.elapsed() < timeout {
            self.step();
        }
        self.step();
        reply
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo(size: [f32; 2]) -> Headless {
        let services = crate::Services { png: None, ..Default::default() };
        let mut app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), services);
        app.ui.view = crate::state::ViewMode::PhotoGrid;
        Headless::new(app, size, 1.0)
    }

    #[test]
    fn demo_grid_snapshot_has_ui_pixels() {
        let t0 = Instant::now();
        let mut h = demo([1200.0, 760.0]);
        let img = h.snapshot(Duration::from_secs(20));
        eprintln!("headless snapshot: {:?} in {:?} ({} frames)", img.size, t0.elapsed(), h.frames());
        assert_eq!(img.size, [1200, 760]);
        // not blank: many distinct colours
        let mut colours: Vec<u32> = img.pixels.iter().map(|c| u32::from_le_bytes(c.to_array())).collect();
        colours.sort_unstable();
        colours.dedup();
        assert!(colours.len() > 200, "only {} colours", colours.len());
        // text in the top bar: bright pixels on the dark chrome
        let top_h = crate::theme::Tokens::default().top_bar_h as usize;
        let bright = img.pixels[..top_h * 1200].iter().filter(|c| c.r() > 150 && c.g() > 150 && c.b() > 150).count();
        assert!(bright > 30, "no text in the top bar ({bright} bright px)");
        // thumbnails arrived (photo textures were drawn)
        assert!(h.app.renderer.thumb_textures() > 0);
    }

    #[test]
    fn snapshots_are_deterministic_and_control_requests_work() {
        let shot = || {
            let mut h = demo([900.0, 600.0]);
            let r = h.request("ui.set", json!({"view": "detail"}), Duration::from_secs(10));
            assert_eq!(r["ok"], true, "{r}");
            h.snapshot(Duration::from_secs(20))
        };
        let (a, b) = (shot(), shot());
        assert_eq!(a.size, b.size);
        let diff = a.pixels.iter().zip(&b.pixels).filter(|(x, y)| x != y).count();
        assert_eq!(diff, 0, "{diff} pixels differ between two runs");
    }

    #[test]
    fn screenshot_request_is_answered_without_a_window() {
        let mut h = demo([800.0, 500.0]);
        let r = h.request("ui.screenshot", json!({}), Duration::from_secs(20));
        assert_eq!(r["ok"], true, "{r}");
        assert_eq!(r["result"]["width"], 800);
        assert_eq!(r["result"]["height"], 500);
        let r = h.request("ui.resize", json!({"width": 640, "height": 400}), Duration::from_secs(5));
        assert_eq!(r["ok"], true);
        let r = h.request("ui.screenshot", json!({"headless": true}), Duration::from_secs(20));
        assert_eq!(r["result"]["width"], 640, "{r}");
    }
}
