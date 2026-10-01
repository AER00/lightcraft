//! Browser host: starts eframe on `<canvas id="lightcraft_canvas">`, wires the in-memory store to
//! the file picker and drag-and-drop, and turns exports into downloads.

use lightcraft_engine::Session;
use lightcraft_ui_egui::{LightcraftApp, Services};
use serde_json::json;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

use crate::bench::Bench;
use crate::store::{MemStore, download_name};

const ACCEPT: &str = ".jpg,.jpeg,.png,.tif,.tiff,.webp,.dng,.cr2,.nef,.arw,.psd,.jxl,.gif,.bmp";

fn window() -> web_sys::Window {
    web_sys::window().expect("no window")
}

/// Milliseconds since navigation start.
fn perf_now() -> f64 {
    window().performance().map(|p| p.now()).unwrap_or(0.0)
}

fn set_status(text: &str) {
    if let Some(el) = window().document().and_then(|d| d.get_element_by_id("lightcraft_status")) {
        el.set_text_content(Some(text));
    }
}

/// Read a browser `File` into the store.
async fn read_file(store: MemStore, file: web_sys::File, ctx: egui::Context) {
    match wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await {
        Ok(buf) => {
            store.add(&file.name(), js_sys::Uint8Array::new(&buf).to_vec());
            ctx.request_repaint();
        }
        Err(e) => log::warn!("reading {}: {e:?}", file.name()),
    }
}

/// Show the browser's open dialog; picked files land in the store asynchronously.
fn open_picker(store: MemStore, ctx: egui::Context) {
    let Some(doc) = window().document() else { return };
    let Ok(input) = doc.create_element("input").map(|e| e.unchecked_into::<web_sys::HtmlInputElement>()) else { return };
    input.set_type("file");
    input.set_multiple(true);
    input.set_accept(ACCEPT);
    let inp = input.clone();
    let on_change = Closure::<dyn FnMut()>::new(move || {
        if let Some(files) = inp.files() {
            for i in 0..files.length() {
                if let Some(f) = files.get(i) {
                    wasm_bindgen_futures::spawn_local(read_file(store.clone(), f, ctx.clone()));
                }
            }
        }
    });
    input.set_onchange(Some(on_change.as_ref().unchecked_ref()));
    on_change.forget();
    input.click();
}

/// Offer `bytes` as a download named after `path`.
fn download(path: &str, bytes: &[u8]) -> Result<(), String> {
    let e = |e: JsValue| format!("download failed: {e:?}");
    let arr = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
    let opts = web_sys::BlobPropertyBag::new();
    let name = download_name(path);
    opts.set_type(crate::store::mime_for(name));
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&arr, &opts).map_err(e)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(e)?;
    let doc = window().document().ok_or("no document")?;
    let a: web_sys::HtmlAnchorElement = doc.create_element("a").map_err(e)?.unchecked_into();
    a.set_href(&url);
    a.set_download(name);
    a.click();
    // revoke once the click has been handled
    let revoke = Closure::once_into_js(move || {
        let _ = web_sys::Url::revoke_object_url(&url);
    });
    let _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(revoke.unchecked_ref(), 5000);
    log::info!("exported {name} ({} bytes)", bytes.len());
    Ok(())
}

fn services(store: MemStore, ctx: egui::Context) -> Services {
    Services {
        pick_files: Some(Box::new(move || {
            open_picker(store.clone(), ctx.clone());
            Vec::new() // files arrive asynchronously and are imported on a later frame
        })),
        // Preset files: browser pickers are asynchronous; not wired on the web yet.
        pick_preset_files: None,
        save_preset_file: None,
        write: Some(Box::new(download)),
        png: Some(Box::new(|img: &lightcraft_raster::Rgba8| {
            lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(img), &lightcraft_codecs::EncodeMeta::default()).unwrap_or_default()
        })),
    }
}

struct WebApp {
    app: LightcraftApp,
    store: MemStore,
    bench: Option<Bench>,
    first_frame_logged: bool,
}

impl WebApp {
    fn new(cc: &eframe::CreationContext<'_>, bench: bool) -> Self {
        let store = MemStore::default();
        let t = perf_now();
        let mut session = Session::with_demo();
        log::info!("lightcraft: wasm started at {:.0} ms; demo library built in {:.0} ms", t, perf_now() - t);
        store.install(&mut session);
        // previews are large (≤ 2560 px, f32): keep few in a 32-bit address space
        session.media.preview_capacity = 3;
        let mut app = LightcraftApp::new(session, services(store.clone(), cc.egui_ctx.clone()));
        app.ui = app.ui.sanitized();
        let origin = lightcraft_ui_egui::now_ms() - perf_now();
        WebApp { app, store, bench: bench.then(|| Bench::new(origin)), first_frame_logged: false }
    }

    fn import_dropped(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for f in dropped {
            let (store, ctx) = (self.store.clone(), ctx.clone());
            wasm_bindgen_futures::spawn_local(async move {
                let name = f.path().to_string_lossy().to_string();
                match f.bytes_async().await {
                    Ok(bytes) => {
                        store.add(&name, bytes);
                        ctx.request_repaint();
                    }
                    Err(e) => log::warn!("reading dropped {name}: {e}"),
                }
            });
        }
        let paths = self.store.take_pending();
        if !paths.is_empty() {
            let n = paths.len();
            match self.app.run("library.import", json!({"paths": paths})) {
                Ok(_) => self.app.toast(ctx, format!("Added {n} photo{}", if n == 1 { "" } else { "s" })),
                Err(e) => log::warn!("import: {e}"),
            }
        }
    }
}

impl eframe::App for WebApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.import_dropped(ctx);
        self.app.logic(ctx);
        if let Some(b) = self.bench.as_mut()
            && let Some(report) = b.step(&mut self.app)
        {
            log::info!("lightcraft-bench {report}");
            set_status(&format!("bench: {report}"));
            self.bench = None;
        }
        if self.bench.is_some() {
            ctx.request_repaint();
        }
    }

    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.app.raw_input_hook(raw);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.app.ui(ui);
        if !self.first_frame_logged && !self.app.widgets.is_empty() {
            self.first_frame_logged = true;
            log::info!("lightcraft first UI frame at {:.0} ms after navigation start", perf_now());
        }
    }
}

#[wasm_bindgen(start)]
pub fn start() {
    eframe::WebLogger::init(log::LevelFilter::Info).ok();
    log::info!("lightcraft: wasm instantiated at {:.0} ms", perf_now());
    let bench = window().location().search().is_ok_and(|q| q.contains("bench"));
    wasm_bindgen_futures::spawn_local(async move {
        let Some(canvas) =
            window().document().and_then(|d| d.get_element_by_id("lightcraft_canvas")).and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
        else {
            log::error!("missing <canvas id=\"lightcraft_canvas\">");
            return;
        };
        let runner = eframe::WebRunner::new();
        let r = runner.start(canvas, eframe::WebOptions::default(), Box::new(move |cc| Ok(Box::new(WebApp::new(cc, bench))))).await;
        match r {
            Ok(()) => {
                if let Some(el) = window().document().and_then(|d| d.get_element_by_id("lightcraft_loading")) {
                    el.remove();
                }
            }
            Err(e) => {
                log::error!("LightCraft failed to start: {e:?}");
                set_status(&format!("LightCraft failed to start: {e:?}"));
            }
        }
    });
}
