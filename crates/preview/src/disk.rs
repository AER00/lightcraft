//! Rendered thumbnails on disk: `<dir>/<2 hex>/<32 hex>.jpg`, bounded in bytes.
//!
//! The cache is disposable: files are written via temp + rename (readers never see partial
//! files) but not fsynced; unreadable files are deleted and re-rendered. When the total size
//! exceeds the budget, the least recently used files (by modification time, refreshed on read)
//! are removed down to 80 % of the budget.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use lightcraft_raster::Rgba8;

use crate::Hash128;

const QUALITY: u8 = 90;

pub struct DiskCache {
    dir: PathBuf,
    budget: u64,
    /// Bytes on disk (`None` until first scanned).
    total: Mutex<Option<u64>>,
    pub hits: AtomicU64,
    pub misses: AtomicU64,
    pub writes: AtomicU64,
}

impl DiskCache {
    pub fn new(dir: &Path, budget: u64) -> DiskCache {
        DiskCache {
            dir: dir.to_path_buf(),
            budget,
            total: Mutex::new(None),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            writes: AtomicU64::new(0),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn path(&self, key: Hash128) -> PathBuf {
        let hex = key.to_string();
        self.dir.join(&hex[..2]).join(format!("{hex}.jpg"))
    }

    pub fn get(&self, key: Hash128) -> Option<Rgba8> {
        let p = self.path(key);
        let Ok(bytes) = std::fs::read(&p) else {
            self.misses.fetch_add(1, Ordering::Relaxed);
            return None;
        };
        match decode(&bytes) {
            Some(img) => {
                self.hits.fetch_add(1, Ordering::Relaxed);
                // refresh recency for pruning
                if let Ok(f) = std::fs::File::options().write(true).open(&p) {
                    let _ = f.set_modified(std::time::SystemTime::now());
                }
                Some(img)
            }
            None => {
                log::warn!("preview cache: dropping unreadable {}", p.display());
                let _ = std::fs::remove_file(&p);
                self.misses.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    pub fn put(&self, key: Hash128, img: &Rgba8) {
        let Some(bytes) = encode(img) else { return };
        let p = self.path(key);
        let Some(parent) = p.parent() else { return };
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
        // know the current total before adding (the first scan must not count this file twice)
        let _ = self.size();
        let tmp = p.with_extension(format!("tmp{:?}", std::thread::current().id()).replace(['(', ')'], ""));
        if std::fs::write(&tmp, &bytes).is_err() || std::fs::rename(&tmp, &p).is_err() {
            let _ = std::fs::remove_file(&tmp);
            return;
        }
        self.writes.fetch_add(1, Ordering::Relaxed);
        let over = {
            let mut t = self.total.lock().unwrap_or_else(|e| e.into_inner());
            let total = t.get_or_insert(0);
            *total += bytes.len() as u64;
            *total > self.budget
        };
        if over {
            self.prune();
        }
    }

    /// All cache files: (path, size, modified).
    fn scan(&self) -> Vec<(PathBuf, u64, std::time::SystemTime)> {
        let mut out = Vec::new();
        let Ok(rd) = std::fs::read_dir(&self.dir) else { return out };
        for sub in rd.flatten() {
            let Ok(files) = std::fs::read_dir(sub.path()) else { continue };
            for f in files.flatten() {
                if let Ok(m) = f.metadata()
                    && m.is_file()
                {
                    out.push((f.path(), m.len(), m.modified().unwrap_or(std::time::UNIX_EPOCH)));
                }
            }
        }
        out
    }

    /// Remove least recently used files until the cache is at most 80 % of its budget.
    pub fn prune(&self) {
        let mut files = self.scan();
        let mut total: u64 = files.iter().map(|f| f.1).sum();
        let target = self.budget / 5 * 4;
        files.sort_by_key(|f| f.2);
        for (p, len, _) in files {
            if total <= target {
                break;
            }
            if std::fs::remove_file(&p).is_ok() {
                total -= len;
            }
        }
        *self.total.lock().unwrap_or_else(|e| e.into_inner()) = Some(total);
    }

    /// Bytes on disk (scans if not yet known).
    pub fn size(&self) -> u64 {
        let mut t = self.total.lock().unwrap_or_else(|e| e.into_inner());
        *t.get_or_insert_with(|| self.scan().iter().map(|f| f.1).sum())
    }

    pub fn clear(&self) {
        for (p, _, _) in self.scan() {
            let _ = std::fs::remove_file(p);
        }
        *self.total.lock().unwrap_or_else(|e| e.into_inner()) = Some(0);
    }
}

fn encode(img: &Rgba8) -> Option<Vec<u8>> {
    if img.width == 0 || img.height == 0 || img.width > u16::MAX as usize || img.height > u16::MAX as usize {
        return None;
    }
    let mut rgb = Vec::with_capacity(img.width * img.height * 3);
    for p in &img.data {
        rgb.extend_from_slice(&p[..3]);
    }
    let mut out = Vec::new();
    let mut enc = jpeg_encoder::Encoder::new(&mut out, QUALITY);
    enc.set_sampling_factor(jpeg_encoder::SamplingFactor::R_4_2_0);
    enc.encode(&rgb, img.width as u16, img.height as u16, jpeg_encoder::ColorType::Rgb).ok()?;
    Some(out)
}

fn decode(bytes: &[u8]) -> Option<Rgba8> {
    use zune_core::bytestream::ZCursor;
    use zune_core::colorspace::ColorSpace;
    use zune_core::options::DecoderOptions;
    let opts = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGB);
    let mut d = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(bytes), opts);
    let px = d.decode().ok()?;
    let info = d.info()?;
    let (w, h) = (info.width as usize, info.height as usize);
    if px.len() < w * h * 3 {
        return None;
    }
    let data = px.chunks_exact(3).take(w * h).map(|c| [c[0], c[1], c[2], 255]).collect();
    Some(Rgba8 { width: w, height: h, data })
}
