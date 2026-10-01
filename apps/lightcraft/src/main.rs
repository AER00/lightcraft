//! LightCraft desktop app.
//!
//! Usage: `lightcraft [--control <port>] [--demo] [files or folders…]`
//!
//! `--control <port>` (or `LIGHTCRAFT_CONTROL_PORT`) starts a localhost JSON-lines control server:
//! `{"id":1,"method":"ui.inspect","params":{}}` → `{"id":1,"ok":true,"result":…}`.
//! See `lightcraft_ui_egui::control` for the methods.

mod control_server;

use lightcraft_engine::Session;
use lightcraft_ui_egui::{LightcraftApp, Services, UiState};

struct App(LightcraftApp);

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.0.logic(ctx);
    }
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.0.raw_input_hook(raw);
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.0.ui(ui);
    }
    fn on_exit(&mut self) {
        save_prefs(&self.0);
    }
}

fn config_dir() -> Option<std::path::PathBuf> {
    if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join("Library/Application Support/LightCraft"))
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(|a| std::path::PathBuf::from(a).join("LightCraft"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
            .map(|c| c.join("lightcraft"))
    }
}

fn load_prefs(app: &mut LightcraftApp) {
    if std::env::var_os("LIGHTCRAFT_NO_PREFS").is_some() {
        return;
    }
    if let Some(p) = config_dir().map(|d| d.join("ui.json"))
        && let Ok(bytes) = std::fs::read(&p)
        && let Ok(ui) = serde_json::from_slice::<UiState>(&bytes)
    {
        app.ui = ui.sanitized();
    }
}

fn save_prefs(app: &LightcraftApp) {
    if std::env::var_os("LIGHTCRAFT_NO_PREFS").is_some() {
        return;
    }
    if let Some(d) = config_dir() {
        let _ = std::fs::create_dir_all(&d);
        if let Ok(bytes) = serde_json::to_vec_pretty(&app.ui) {
            let _ = std::fs::write(d.join("ui.json"), bytes);
        }
    }
}

fn services() -> Services {
    Services {
        pick_files: Some(Box::new(|| {
            rfd::FileDialog::new()
                .add_filter("Photos", &["jpg", "jpeg", "png", "tif", "tiff", "webp", "dng", "cr2", "nef", "arw", "psd", "jxl", "gif", "bmp"])
                .pick_files()
                .unwrap_or_default()
                .into_iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect()
        })),
        write: Some(Box::new(|p: &str, b: &[u8]| {
            if let Some(dir) = std::path::Path::new(p).parent().filter(|d| !d.as_os_str().is_empty()) {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            std::fs::write(p, b).map_err(|e| e.to_string())
        })),
        png: Some(Box::new(|img: &lightcraft_raster::Rgba8| {
            lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(img), &lightcraft_codecs::EncodeMeta::default()).unwrap_or_default()
        })),
    }
}

/// Expand folders (recursively) into photo files.
fn collect(path: &std::path::Path, out: &mut Vec<String>) {
    if path.is_dir() {
        if let Ok(rd) = std::fs::read_dir(path) {
            let mut v: Vec<_> = rd.flatten().map(|e| e.path()).collect();
            v.sort();
            for p in v {
                collect(&p, out);
            }
        }
    } else if path.extension().is_some_and(|e| {
        matches!(
            e.to_string_lossy().to_lowercase().as_str(),
            "jpg" | "jpeg" | "png" | "tif" | "tiff" | "webp" | "dng" | "cr2" | "nef" | "arw" | "psd" | "jxl" | "gif" | "bmp"
        )
    }) {
        out.push(path.to_string_lossy().to_string());
    }
}

fn main() -> eframe::Result {
    let mut control_port: Option<u16> = std::env::var("LIGHTCRAFT_CONTROL_PORT").ok().and_then(|p| p.parse().ok());
    let mut files = Vec::new();
    let mut demo = true;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--control" => control_port = args.next().and_then(|p| p.parse().ok()),
            "--no-demo" => demo = false,
            "--version" => {
                println!("lightcraft {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            _ => collect(std::path::Path::new(&a), &mut files),
        }
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("LightCraft")
            .with_inner_size([1600.0, 1000.0])
            .with_min_inner_size([900.0, 560.0])
            .with_drag_and_drop(true)
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false),
        ..Default::default()
    };
    eframe::run_native(
        "LightCraft",
        options,
        Box::new(move |cc| {
            let session = if demo && files.is_empty() { Session::with_demo() } else { Session::new() }.with_fs();
            let mut app = LightcraftApp::new(session, services());
            load_prefs(&mut app);
            app.integrated_titlebar = cfg!(target_os = "macos");
            if let Some(port) = control_port {
                let rx = control_server::start(port, cc.egui_ctx.clone());
                app = app.with_control(rx);
            }
            if !files.is_empty() {
                let _ = app.run("library.import", serde_json::json!({"paths": files}));
                app.ui.view = lightcraft_ui_egui::state::ViewMode::PhotoGrid;
            }
            Ok(Box::new(App(app)))
        }),
    )
}
