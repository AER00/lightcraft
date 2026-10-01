//! Background rendering: engine [`RenderJob`]s run on a [`JobPool`]; results become textures.
//!
//! Each visible thing that needs pixels is a *slot* (a grid thumbnail, the loupe, the "before"
//! image…). Requests are deduplicated per slot (a newer request replaces a queued older one) and
//! prioritised (loupe first, then on-screen thumbnails, then prefetch); thumbnails scrolled far
//! out of view are dropped from the queue. Thumbnail jobs hit the engine's preview cache (memory +
//! disk) before rendering. On wasm the jobs run inline, one per frame.
//!
//! Stand-ins ([`QuickJob`]s, once per photo and settings): [`Slot::Preview`] holds what the loupe
//! shows until [`Slot::Main`] has the photo's render (cached view render, embedded camera JPEG or
//! a thumbnail render); [`Slot::ThumbQuick`] holds a raw's embedded preview in the grid until its
//! rendered thumbnail arrives (a cached thumbnail found by the quick job becomes the
//! [`Slot::Thumb`] texture directly).

use std::collections::HashMap;

use lightcraft_catalog::PhotoId;
use lightcraft_engine::Session;
use lightcraft_engine::media::{QuickJob, QuickSource, RenderJob, RenderResult};
use lightcraft_engine::pipeline::StageCache;
use lightcraft_preview::JobPool;
use lightcraft_raster::Histogram;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Slot {
    Thumb(PhotoId),
    /// A thumbnail's stand-in (embedded preview of an unedited raw).
    ThumbQuick(PhotoId),
    Main,
    /// The loupe's stand-in until `Main` has the photo.
    Preview,
    Before,
    Compare(u8),
    /// Background preparation of a neighbouring photo (no texture: its decoded source and view
    /// render are cached by the engine). 0 = next, 1 = previous.
    Prefetch(u8),
}

pub struct Tex {
    pub key: u64,
    pub photo: PhotoId,
    pub tex: egui::TextureHandle,
    pub size: [usize; 2],
    pub histogram: Option<Histogram>,
    pub ms: f64,
    /// Set for stand-ins: where the image came from.
    pub quick: Option<QuickSource>,
    /// CPU copy of the pixels (only with [`Renderer::keep_pixels`]; for headless screenshots).
    pub pixels: Option<std::sync::Arc<egui::ColorImage>>,
}

pub struct Renderer {
    pool: JobPool<Slot, RenderResult>,
    /// Slot → (key, priority) of the request in flight (queued or running).
    pending: HashMap<Slot, (u64, u32)>,
    pub textures: HashMap<Slot, Tex>,
    pub last_main_ms: f64,
    /// Jobs finished since start (for inspect/perf).
    pub completed: u64,
    /// Keep a CPU copy of every texture so the UI can be rasterized headlessly (set when the app
    /// is driven by the control channel).
    pub keep_pixels: bool,
    /// Per-view intermediate results (loupe and "before"), so slider drags redo only what changed.
    stages: HashMap<Slot, Arc<StageCache>>,
    /// Slot → key of the last quick job requested for it (each is tried once).
    quick_tried: HashMap<Slot, u64>,
    /// Prefetch slot → key of the last job submitted (each runs once).
    prefetched: HashMap<Slot, u64>,
}

impl Default for Renderer {
    fn default() -> Self {
        let threads = if cfg!(target_arch = "wasm32") { 0 } else { JobPool::<Slot, RenderResult>::default_threads().min(6) };
        Renderer {
            pool: JobPool::new(threads),
            pending: HashMap::new(),
            textures: HashMap::new(),
            last_main_ms: 0.0,
            completed: 0,
            keep_pixels: false,
            stages: HashMap::new(),
            quick_tried: HashMap::new(),
            prefetched: HashMap::new(),
        }
    }
}

impl Renderer {
    /// Is the slot's current texture (or pending request) already for `key`?
    pub fn is_current(&self, slot: Slot, key: u64) -> bool {
        self.textures.get(&slot).is_some_and(|t| t.key == key) || self.pending.get(&slot).is_some_and(|p| p.0 == key)
    }

    /// Request a render for `slot` (no-op if already current or pending at the same priority).
    pub fn request(&mut self, slot: Slot, job: RenderJob, priority: u32) {
        if self.textures.get(&slot).is_some_and(|t| t.key == job.key) {
            return;
        }
        if let Some(&(key, prio)) = self.pending.get(&slot)
            && key == job.key
            && (prio == priority || !self.pool.is_queued(slot))
        {
            return;
        }
        self.pending.insert(slot, (job.key, priority));
        let key = job.key;
        let job = if matches!(slot, Slot::Main | Slot::Before) { job.with_stages(self.stages.entry(slot).or_default().clone()) } else { job };
        self.pool.submit(slot, key, priority, Box::new(move || job.run()));
    }

    /// Request a stand-in for `slot` (once per job key).
    pub fn request_quick(&mut self, slot: Slot, job: QuickJob, priority: u32) {
        if self.quick_tried.get(&slot) == Some(&job.key) {
            return;
        }
        self.quick_tried.insert(slot, job.key);
        self.pending.insert(slot, (job.key, priority));
        let key = job.key;
        self.pool.submit(slot, key, priority, Box::new(move || job.run()));
    }

    /// Prepare a photo in the background (a [`Slot::Prefetch`] job runs once per key; a newer one
    /// replaces it while queued). Memory stays bounded by the engine's source caches.
    pub fn prefetch(&mut self, slot: Slot, job: RenderJob, priority: u32) {
        if self.prefetched.get(&slot) == Some(&job.key) {
            return;
        }
        self.prefetched.insert(slot, job.key);
        self.pending.insert(slot, (job.key, priority));
        let key = job.key;
        self.pool.submit(slot, key, priority, Box::new(move || job.run()));
    }

    /// Is a request for `slot` queued or running?
    pub fn is_pending(&self, slot: Slot) -> bool {
        self.pending.contains_key(&slot)
    }

    /// The texture to show for a grid/filmstrip thumbnail: the rendered one, else its stand-in.
    pub fn thumb(&self, id: PhotoId) -> Option<&Tex> {
        self.textures.get(&Slot::Thumb(id)).or_else(|| self.textures.get(&Slot::ThumbQuick(id)))
    }

    pub fn queued(&self) -> usize {
        self.pool.queued()
    }

    /// Requests queued or running.
    pub fn in_flight(&self) -> usize {
        self.pending.len()
    }

    /// Thumbnail textures currently loaded.
    pub fn thumb_textures(&self) -> usize {
        self.textures.keys().filter(|s| matches!(s, Slot::Thumb(_) | Slot::ThumbQuick(_))).count()
    }

    /// Collect finished jobs into textures. Returns true if anything changed.
    pub fn poll(&mut self, ctx: &egui::Context, session: &mut Session) -> bool {
        // wasm: run one job per frame on this thread, timed with the host clock
        #[cfg(target_arch = "wasm32")]
        let inline_ms = {
            let t0 = crate::now_ms();
            self.pool.run_inline(1);
            crate::now_ms() - t0
        };
        #[cfg(not(target_arch = "wasm32"))]
        let inline_ms = 0.0;
        let mut changed = false;
        while let Some(done) = self.pool.try_recv() {
            let (mut slot, r, ms) = (done.slot, done.result, if done.ms > 0.0 { done.ms } else { inline_ms });
            session.accept(&r);
            self.completed += 1;
            if self.pending.get(&slot).is_some_and(|p| p.0 == r.key) {
                self.pending.remove(&slot);
            }
            let Ok(rendered) = r.rendered else {
                continue;
            };
            if matches!(slot, Slot::Prefetch(_)) {
                continue;
            }
            if let Slot::ThumbQuick(id) = slot {
                if self.textures.contains_key(&Slot::Thumb(id)) {
                    continue; // the real thumbnail won the race
                }
                if r.quick == Some(QuickSource::Cached) {
                    // the photo's own cached thumbnail: final
                    slot = Slot::Thumb(id);
                }
            }
            if let Slot::Thumb(id) = slot {
                self.textures.remove(&Slot::ThumbQuick(id));
            }
            let img = &rendered.image;
            let color = std::sync::Arc::new(egui::ColorImage::from_rgba_unmultiplied([img.width, img.height], &img.as_bytes()));
            let pixels = self.keep_pixels.then(|| color.clone());
            let name = format!("{slot:?}");
            match self.textures.get_mut(&slot) {
                Some(t) => {
                    t.tex.set(color, egui::TextureOptions::LINEAR);
                    t.pixels = pixels;
                    t.key = r.key;
                    t.photo = r.photo;
                    t.size = [img.width, img.height];
                    t.histogram = Some(rendered.histogram);
                    t.ms = ms;
                    t.quick = r.quick;
                }
                None => {
                    let tex = ctx.load_texture(name, color, egui::TextureOptions::LINEAR);
                    self.textures.insert(
                        slot,
                        Tex {
                            key: r.key,
                            photo: r.photo,
                            tex,
                            size: [img.width, img.height],
                            histogram: Some(rendered.histogram),
                            ms,
                            quick: r.quick,
                            pixels,
                        },
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

    /// CPU copies of the current textures by id (see [`Self::keep_pixels`]).
    pub fn cpu_textures(&self) -> HashMap<egui::TextureId, crate::softpaint::CpuTexture> {
        self.textures.values().filter_map(|t| Some((t.tex.id(), crate::softpaint::CpuTexture::linear(t.pixels.clone()?)))).collect()
    }

    /// Drop thumbnails that aren't in `keep` (bounded memory for huge libraries), and queued
    /// thumbnail jobs for photos that scrolled out of `keep`.
    pub fn evict_thumbs(&mut self, keep: &std::collections::HashSet<PhotoId>, max: usize) {
        let dropped = self.pool.reprioritize(|s, p| match s {
            Slot::Thumb(id) | Slot::ThumbQuick(id) if !keep.contains(id) && p <= 11 => None,
            _ => Some(p),
        });
        for s in dropped {
            self.pending.remove(&s);
            self.quick_tried.remove(&s);
        }
        let thumbs = self.thumb_textures();
        if thumbs <= max {
            return;
        }
        self.textures.retain(|s, _| !matches!(s, Slot::Thumb(id) | Slot::ThumbQuick(id) if !keep.contains(id)));
        self.quick_tried.retain(|s, _| !matches!(s, Slot::ThumbQuick(id) if !keep.contains(id)));
    }
}
