//! Background rendering: engine [`RenderJob`]s run on a [`JobPool`]; results become textures.
//!
//! Each visible thing that needs pixels is a *slot* (a grid thumbnail, the loupe, the "before"
//! image…). Requests are deduplicated per slot (a newer request replaces a queued older one) and
//! prioritised (loupe first, then on-screen thumbnails, then prefetch); thumbnails scrolled far
//! out of view are dropped from the queue. Thumbnail jobs hit the engine's preview cache (memory +
//! disk) before rendering. On wasm the jobs run inline, one per frame.

use std::collections::HashMap;

use lightcraft_catalog::PhotoId;
use lightcraft_engine::Session;
use lightcraft_engine::media::{RenderJob, RenderResult};
use lightcraft_preview::JobPool;
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

pub struct Renderer {
    pool: JobPool<Slot, RenderResult>,
    /// Slot → (key, priority) of the request in flight (queued or running).
    pending: HashMap<Slot, (u64, u32)>,
    pub textures: HashMap<Slot, Tex>,
    pub last_main_ms: f64,
    /// Jobs finished since start (for inspect/perf).
    pub completed: u64,
}

impl Default for Renderer {
    fn default() -> Self {
        let threads = if cfg!(target_arch = "wasm32") { 0 } else { JobPool::<Slot, RenderResult>::default_threads().min(6) };
        Renderer { pool: JobPool::new(threads), pending: HashMap::new(), textures: HashMap::new(), last_main_ms: 0.0, completed: 0 }
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
        self.pool.submit(slot, key, priority, Box::new(move || job.run()));
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
        self.textures.keys().filter(|s| matches!(s, Slot::Thumb(_))).count()
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
            let (slot, r, ms) = (done.slot, done.result, if done.ms > 0.0 { done.ms } else { inline_ms });
            session.accept(&r);
            self.completed += 1;
            if self.pending.get(&slot).is_some_and(|p| p.0 == r.key) {
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

    /// Drop thumbnails that aren't in `keep` (bounded memory for huge libraries), and queued
    /// thumbnail jobs for photos that scrolled out of `keep`.
    pub fn evict_thumbs(&mut self, keep: &std::collections::HashSet<PhotoId>, max: usize) {
        let dropped = self.pool.reprioritize(|s, p| match s {
            Slot::Thumb(id) if !keep.contains(id) && p <= 10 => None,
            _ => Some(p),
        });
        for s in dropped {
            self.pending.remove(&s);
        }
        let thumbs = self.textures.keys().filter(|s| matches!(s, Slot::Thumb(_))).count();
        if thumbs <= max {
            return;
        }
        self.textures.retain(|s, _| !matches!(s, Slot::Thumb(id) if !keep.contains(id)));
    }
}
