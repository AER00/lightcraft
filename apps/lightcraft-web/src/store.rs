//! In-memory "filesystem" for the web build: files picked or dropped in the browser are kept as
//! bytes under synthetic paths (`mem/<n>/<name>`), and the engine reads them through the same
//! [`FileLoader`]/[`FileProbe`] hooks the desktop app backs with `std::fs`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use lightcraft_engine::files::{load_bytes, probe_bytes};
use lightcraft_engine::media::{FileLoader, FileProbe};

#[derive(Default)]
struct Inner {
    files: HashMap<String, Arc<[u8]>>,
    /// Paths added since the last [`MemStore::take_pending`] (to import on the next frame).
    pending: Vec<String>,
    next: u64,
}

/// Shared, cloneable byte store. File reads in the browser are async, so additions arrive from
/// promise callbacks and are picked up by the app on its next frame.
#[derive(Clone, Default)]
pub struct MemStore(Arc<Mutex<Inner>>);

impl MemStore {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Store a file's bytes; returns its synthetic path and queues it for import.
    pub fn add(&self, name: &str, bytes: Vec<u8>) -> String {
        let mut g = self.lock();
        g.next += 1;
        let name = name.rsplit(['/', '\\']).next().filter(|n| !n.is_empty()).unwrap_or("photo");
        let path = format!("mem/{}/{name}", g.next);
        g.files.insert(path.clone(), bytes.into());
        g.pending.push(path.clone());
        path
    }

    pub fn get(&self, path: &str) -> Option<Arc<[u8]>> {
        self.lock().files.get(path).cloned()
    }

    /// Paths added since the last call.
    pub fn take_pending(&self) -> Vec<String> {
        std::mem::take(&mut self.lock().pending)
    }

    pub fn len(&self) -> usize {
        self.lock().files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Engine hooks that decode/probe from this store.
    pub fn hooks(&self) -> (FileLoader, FileProbe) {
        let s = self.clone();
        let loader: FileLoader = Arc::new(move |path: &str, max_edge: usize| {
            let bytes = s.get(path).ok_or_else(|| format!("{path}: not in memory"))?;
            load_bytes(&bytes, max_edge)
        });
        let s = self.clone();
        let probe: FileProbe = Arc::new(move |path: &str| {
            let bytes = s.get(path).ok_or_else(|| format!("{path}: not in memory"))?;
            probe_bytes(path, &bytes)
        });
        (loader, probe)
    }

    /// Install the hooks into a session.
    pub fn install(&self, session: &mut lightcraft_engine::Session) {
        let (l, p) = self.hooks();
        session.media.file_loader = Some(l);
        session.media.file_probe = Some(p);
    }
}

/// File name part of an export path (the browser download name).
pub fn download_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().filter(|n| !n.is_empty()).unwrap_or("export.png")
}

/// MIME type for a download, by extension.
pub fn mime_for(name: &str) -> &'static str {
    match name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).as_deref() {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("tif" | "tiff") => "image/tiff",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn png_bytes(w: usize, h: usize) -> Vec<u8> {
        let bytes: Vec<u8> = (0..w * h).flat_map(|i| [(i * 7) as u8, 128, 200, 255]).collect();
        let img = lightcraft_raster::Rgba8::from_bytes(w, h, &bytes).unwrap();
        lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&img), &lightcraft_codecs::EncodeMeta::default()).unwrap()
    }

    #[test]
    fn add_take_get() {
        let s = MemStore::default();
        let a = s.add("C:\\x\\a.png", vec![1, 2]);
        let b = s.add("a.png", vec![3]);
        assert_ne!(a, b, "same name twice gets distinct paths");
        assert!(a.ends_with("/a.png"));
        assert_eq!(s.take_pending(), vec![a.clone(), b]);
        assert!(s.take_pending().is_empty());
        assert_eq!(&*s.get(&a).unwrap(), &[1, 2]);
        assert_eq!(s.len(), 2);
    }

    #[test]
    fn import_and_render_from_memory() {
        let store = MemStore::default();
        let mut session = lightcraft_engine::Session::new();
        store.install(&mut session);
        let path = store.add("tiny.png", png_bytes(40, 30));
        let r = session.execute("library.import", &json!({"paths": [path]})).unwrap();
        let id = lightcraft_engine::catalog::PhotoId(r["imported"][0].as_u64().unwrap());
        let p = session.catalog.photo(id).unwrap();
        assert_eq!((p.width, p.height), (40, 30));
        assert_eq!(p.file_name, "tiny.png");
        let out = session.render_now(id, 20, 20).unwrap();
        assert_eq!(out.image.width, 20);
    }

    #[test]
    fn download_names() {
        assert_eq!(download_name("dir/x-lightcraft.png"), "x-lightcraft.png");
        assert_eq!(download_name("y.png"), "y.png");
        assert_eq!(mime_for("a.JPG"), "image/jpeg");
        assert_eq!(mime_for("a"), "application/octet-stream");
        assert_eq!(download_name(""), "export.png");
    }
}
