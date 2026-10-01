//! Background rendering: a small worker pool runs engine [`RenderJob`]s; results become textures.
//!
//! Each visible thing that needs pixels is a *slot* (a grid thumbnail, the loupe, the "before"
//! image…). Requests are deduplicated per slot (a newer request replaces a queued older one) and
//! prioritised (loupe first, then visible thumbnails). On wasm the jobs run inline, one per frame.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex};

use lightcraft_catalog::PhotoId;
use lightcraft_engine::Session;
use lightcraft_engine::media::{RenderJob, RenderResult};
use lightcraft_raster::Histogram;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Slot {
    Thumb(PhotoId),
    Main,
    Before,
    Compare(u8),
}

pub struct Tex {
    pub key: u64,
    pub photo: PhotoId,
    pub tex: egui::TextureHandle,
    pub size: [usize; 2],
    pub histogram: Option<Histogram>,
    pub ms: f64,
}

type Queue = Arc<(Mutex<Vec<(Slot, u32, RenderJob)>>, Condvar)>;

pub struct Renderer {
    queue: Queue,
    results_tx: Sender<(Slot, RenderResult, f64)>,
    results: Receiver<(Slot, RenderResult, f64)>,
    pending: HashMap<Slot, u64>,
    pub textures: HashMap<Slot, Tex>,
    started: bool,
    pub last_main_ms: f64,
    /// Jobs finished since start (for inspect/perf).
    pub completed: u64,
}

impl Default for Renderer {
    fn default() -> Self {
        let (tx, rx) = channel();
        Renderer {
            queue: Arc::new((Mutex::new(Vec::new()), Condvar::new())),
            results_tx: tx,
            results: rx,
            pending: HashMap::new(),
            textures: HashMap::new(),
            started: false,
            last_main_ms: 0.0,
            completed: 0,
        }
    }
}

fn now() -> f64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64() * 1000.0).unwrap_or(0.0)
    }
    #[cfg(target_arch = "wasm32")]
    {
        0.0
    }
}

impl Renderer {
    fn start(&mut self) {
        if self.started {
            return;
        }
        self.started = true;
        #[cfg(not(target_arch = "wasm32"))]
        {
            let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 6);
            for i in 0..n {
                let q = self.queue.clone();
                let tx = self.results_tx.clone();
                let _ = std::thread::Builder::new().name(format!("lc-render-{i}")).spawn(move || {
                    loop {
                        let job = {
                            let (lock, cv) = &*q;
                            let mut g = lock.lock().unwrap_or_else(|e| e.into_inner());
                            while g.is_empty() {
                                g = cv.wait(g).unwrap_or_else(|e| e.into_inner());
                            }
                            // highest priority, newest first
                            let best = g.iter().enumerate().max_by_key(|(i, (_, p, _))| (*p, *i)).map(|(i, _)| i).unwrap_or(0);
                            g.remove(best)
                        };
                        let t0 = now();
                        let (slot, _, job) = job;
                        let r = job.run();
                        if tx.send((slot, r, now() - t0)).is_err() {
                            break;
                        }
                    }
                });
            }
        }
    }

    /// Is the slot's current texture (or pending request) already for `key`?
    pub fn is_current(&self, slot: Slot, key: u64) -> bool {
        self.textures.get(&slot).is_some_and(|t| t.key == key) || self.pending.get(&slot) == Some(&key)
    }

    /// Request a render for `slot` (no-op if already current or pending).
    pub fn request(&mut self, slot: Slot, job: RenderJob, priority: u32) {
        if self.is_current(slot, job.key) {
            return;
        }
        self.start();
        self.pending.insert(slot, job.key);
        let (lock, cv) = &*self.queue;
        let mut g = lock.lock().unwrap_or_else(|e| e.into_inner());
        g.retain(|(s, _, _)| *s != slot);
        g.push((slot, priority, job));
        cv.notify_one();
    }

    pub fn queued(&self) -> usize {
        self.queue.0.lock().map(|g| g.len()).unwrap_or(0)
    }

    /// Collect finished jobs into textures. Returns true if anything changed.
    pub fn poll(&mut self, ctx: &egui::Context, session: &mut Session) -> bool {
        #[cfg(target_arch = "wasm32")]
        {
            // inline: run one job per frame
            let job = {
                let mut g = self.queue.0.lock().unwrap_or_else(|e| e.into_inner());
                let best = g.iter().enumerate().max_by_key(|(i, (_, p, _))| (*p, *i)).map(|(i, _)| i);
                best.map(|b| g.remove(b))
            };
            if let Some((slot, _, job)) = job {
                let r = job.run();
                let _ = self.results_tx.send((slot, r, 0.0));
            }
        }
        let mut changed = false;
        while let Ok((slot, r, ms)) = self.results.try_recv() {
            session.accept(&r);
            self.completed += 1;
            if self.pending.get(&slot) == Some(&r.key) {
                self.pending.remove(&slot);
            }
            let Ok(rendered) = r.rendered else {
                continue;
            };
            let img = &rendered.image;
            let color = egui::ColorImage::from_rgba_unmultiplied([img.width, img.height], &img.as_bytes());
            let name = format!("{slot:?}");
            match self.textures.get_mut(&slot) {
                Some(t) => {
                    t.tex.set(color, egui::TextureOptions::LINEAR);
                    t.key = r.key;
                    t.photo = r.photo;
                    t.size = [img.width, img.height];
                    t.histogram = Some(rendered.histogram);
                    t.ms = ms;
                }
                None => {
                    let tex = ctx.load_texture(name, color, egui::TextureOptions::LINEAR);
                    self.textures.insert(
                        slot,
                        Tex { key: r.key, photo: r.photo, tex, size: [img.width, img.height], histogram: Some(rendered.histogram), ms },
                    );
                }
            }
            if slot == Slot::Main {
                self.last_main_ms = ms;
            }
            changed = true;
        }
        if !self.pending.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
        changed
    }

    /// Drop thumbnails that aren't in `keep` (bounded memory for huge libraries).
    pub fn evict_thumbs(&mut self, keep: &std::collections::HashSet<PhotoId>, max: usize) {
        let thumbs = self.textures.keys().filter(|s| matches!(s, Slot::Thumb(_))).count();
        if thumbs <= max {
            return;
        }
        self.textures.retain(|s, _| !matches!(s, Slot::Thumb(id) if !keep.contains(id)));
    }
}
