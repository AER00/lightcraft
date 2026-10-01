//! An in-process [`Session`] that answers the control-channel methods the MCP tools use, so the
//! MCP server (and the CLI) work without a window.

use std::path::Path;

use lightcraft_engine::Session;
use lightcraft_engine::catalog::PhotoId;
use lightcraft_raster::Rgba8;
use serde_json::{Value, json};

use crate::backend::Backend;

/// File extensions recognised as photos when expanding folders.
pub const PHOTO_EXTENSIONS: &[&str] =
    &["jpg", "jpeg", "png", "tif", "tiff", "webp", "dng", "cr2", "cr3", "nef", "arw", "raf", "orf", "rw2", "pef", "psd", "jxl", "gif", "bmp", "avif"];

/// Headless backend: a [`Session`] with filesystem hooks.
pub struct Headless {
    pub session: Session,
}

impl Default for Headless {
    fn default() -> Self {
        Self::new(Session::new().with_fs())
    }
}

impl Headless {
    pub fn new(session: Session) -> Self {
        Self { session }
    }

    /// A headless session with the procedurally generated demo library.
    pub fn demo() -> Self {
        Self::new(Session::with_demo().with_fs())
    }

    fn photo_or_active(&self, p: &Value) -> Result<PhotoId, String> {
        p.get("id").and_then(Value::as_u64).map(PhotoId).or(self.session.active()).ok_or_else(|| "no photo (import one or pass `id`)".to_string())
    }

    /// Render `id` (or the active photo) so its long edge is at most `size` pixels.
    pub fn render(&mut self, p: &Value, default_size: u64) -> Result<Rgba8, String> {
        let id = self.photo_or_active(p)?;
        let size = p.get("size").or(p.get("longEdge")).and_then(Value::as_u64).unwrap_or(default_size).clamp(16, 16384) as usize;
        Ok(self.session.render_now(id, size, size)?.image)
    }

    /// The UI command `app.export`, emulated: render and write PNG/JPEG/TIFF/WebP by extension.
    fn export(&mut self, p: &Value) -> Result<Value, String> {
        let id = self.photo_or_active(p)?;
        let img = self.render(&json!({"id": id.0, "size": p.get("longEdge").and_then(Value::as_u64).unwrap_or(3000)}), 3000)?;
        let path = match p.get("path").and_then(Value::as_str) {
            Some(path) => path.to_string(),
            None => {
                let name = self.session.catalog.photo(id).map(|p| p.file_name.clone()).unwrap_or_else(|| "export".into());
                let stem = name.rsplit_once('.').map(|(a, _)| a.to_string()).unwrap_or(name);
                format!("{stem}-lightcraft.png")
            }
        };
        let quality = p.get("quality").and_then(Value::as_u64).unwrap_or(90).clamp(1, 100) as u8;
        write_image(Path::new(&path), &img, quality)?;
        Ok(json!({"path": path, "width": img.width, "height": img.height}))
    }
}

impl Backend for Headless {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let p = if params.is_null() { json!({}) } else { params };
        match method {
            "engine.execute" | "ui.menu.invoke" | "command" => {
                let id = p.get("command").or(p.get("id")).and_then(Value::as_str).ok_or("missing `command`")?;
                let params = p.get("params").cloned().filter(|v| !v.is_null()).unwrap_or(json!({}));
                if id == "app.export" {
                    return self.export(&params);
                }
                self.session.execute(id, &params).map_err(|e| e.to_string())
            }
            "engine.commands" => {
                let mut v: Vec<Value> = self.session.commands().into_iter().map(|c| serde_json::to_value(c).unwrap_or_default()).collect();
                v.push(json!({"id": "app.export", "label": "Export Now", "menu": [], "shortcut": null,
                    "params": "{path?: output file (.png/.jpg/.tif/.webp), longEdge?: pixels (3000), quality?: 1..100 (JPEG)}",
                    "enabled": self.session.active().is_some()}));
                Ok(Value::Array(v))
            }
            "ui.render" => {
                let img = self.render(&p, 1600)?;
                match p.get("path").and_then(Value::as_str) {
                    Some(path) => {
                        write_image(Path::new(path), &img, 92)?;
                        Ok(json!({"path": path, "width": img.width, "height": img.height}))
                    }
                    None => Ok(json!({"width": img.width, "height": img.height})),
                }
            }
            "app.export" => self.export(&p),
            m if m.starts_with("ui.") || m == "app.quit" => {
                Err(format!("`{m}` needs the desktop app: start `lightcraft --control 7980` and run the MCP server with `--connect`"))
            }
            other => Err(format!("unknown method `{other}`")),
        }
    }

    fn has_ui(&self) -> bool {
        false
    }

    fn describe(&self) -> String {
        "headless".into()
    }
}

/// Expand folders (recursively, sorted) into photo files and make paths absolute.
pub fn expand_paths(paths: &[String]) -> Vec<String> {
    fn walk(p: &Path, out: &mut Vec<String>) {
        if p.is_dir() {
            if let Ok(rd) = std::fs::read_dir(p) {
                let mut v: Vec<_> = rd.flatten().map(|e| e.path()).collect();
                v.sort();
                for c in v {
                    if c.file_name().is_some_and(|n| !n.to_string_lossy().starts_with('.')) {
                        walk(&c, out);
                    }
                }
            }
        } else if p.extension().is_some_and(|e| PHOTO_EXTENSIONS.contains(&e.to_string_lossy().to_lowercase().as_str())) {
            out.push(std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf()).to_string_lossy().to_string());
        }
    }
    let mut out = Vec::new();
    for p in paths {
        let path = Path::new(p);
        if path.is_dir() {
            walk(path, &mut out);
        } else {
            // Explicit files are kept even with unknown extensions (the probe decides).
            out.push(std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf()).to_string_lossy().to_string());
        }
    }
    out
}

/// Encode by extension: `.png` (default), `.jpg`/`.jpeg` (quality), `.tif`/`.tiff`, `.webp` (lossless).
pub fn encode_image(ext: &str, img: &Rgba8, quality: u8) -> Result<Vec<u8>, String> {
    use lightcraft_codecs::{ChromaSubsampling, EncodeImage, EncodeMeta, TiffCompression};
    let e = EncodeImage::rgba8(img);
    let m = EncodeMeta::default();
    match ext.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => lightcraft_codecs::encode_jpeg(&e, quality, ChromaSubsampling::S444, &m),
        "tif" | "tiff" => lightcraft_codecs::encode_tiff(&e, TiffCompression::Deflate, &m),
        "webp" => lightcraft_codecs::encode_webp_lossless(&e, &m),
        _ => lightcraft_codecs::encode_png(&e, &m),
    }
    .map_err(|e| e.to_string())
}

/// Encode by the path's extension and write the file.
pub fn write_image(path: &Path, img: &Rgba8, quality: u8) -> Result<(), String> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
    let bytes = encode_image(&ext, img, quality)?;
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}
