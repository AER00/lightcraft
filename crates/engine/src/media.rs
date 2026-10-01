//! Source proxies and render jobs.
//!
//! Sources are decoded (or generated) once per level and cached in memory LRUs: `Thumb` (≤ 512 px
//! long edge) for the grid and filmstrip, `Preview` (≤ 2560 px) for the loupe, `Full` (native
//! resolution, the last one only) for exports larger than a preview. A [`RenderJob`] is
//! self-contained and `Send`: frontends run it on a worker thread; if the source wasn't cached yet
//! the job loads it and hands it back in the [`RenderResult`] so the cache can keep it.
//!
//! Rendered thumbnails are cached too ([`lightcraft_preview::PreviewCache`]: memory LRU, plus a
//! disk cache in the library's `thumbs/` folder), keyed by the photo's content hash, the develop
//! settings hash, the size and [`RENDER_CACHE_VERSION`] — so reopening a library shows its grid
//! without decoding a single original, and an edit simply produces a new key.

use std::sync::Arc;

use lightcraft_catalog::{MediaKind, Photo, PhotoId, Source};
use lightcraft_develop::DevelopSettings;
use lightcraft_pipeline::{Quality, RenderRequest, Rendered, SourceInfo, StageCache};
use lightcraft_preview::{Hash128, Hasher128, Lru, PreviewCache};
use lightcraft_raster::{Histogram, Rgb32f};
use serde::{Deserialize, Serialize};

/// Bump when the pipeline's output changes, to invalidate cached thumbnails.
pub const RENDER_CACHE_VERSION: u64 = 3;

/// Thumbnails render at one of these long edges (so window/cell size changes reuse the cache).
pub const THUMB_SIZES: [usize; 4] = [128, 256, 384, 512];

/// Memory budgets.
const THUMB_SOURCE_BYTES: usize = 384 << 20;
const RENDERED_MEM_BYTES: usize = 128 << 20;
/// Disk budget for the thumbnail cache.
pub const DISK_CACHE_BYTES: u64 = 2 << 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceLevel {
    Thumb,
    Preview,
    /// The original at its native resolution (exports larger than a preview).
    Full,
}

impl SourceLevel {
    pub fn max_edge(self) -> usize {
        match self {
            SourceLevel::Thumb => 512,
            SourceLevel::Preview => 2560,
            SourceLevel::Full => usize::MAX,
        }
    }
    /// Smallest level that can serve an output of `long_edge` pixels.
    pub fn for_size(long_edge: usize) -> SourceLevel {
        match long_edge {
            0..=520 => SourceLevel::Thumb,
            521..=2560 => SourceLevel::Preview,
            _ => SourceLevel::Full,
        }
    }
}

/// Decodes a file into a linear Rec.2020 image no larger than `max_edge` (set by the app).
pub type FileLoader = Arc<dyn Fn(&str, usize) -> Result<(Rgb32f, SourceInfo), String> + Send + Sync>;

#[derive(Clone)]
pub enum SourceRef {
    Loaded(Arc<Rgb32f>),
    Demo { scene: Box<lightcraft_scenes::Scene>, max_edge: usize },
    File { path: String, max_edge: usize, loader: Option<FileLoader> },
}

impl SourceRef {
    pub fn load(&self) -> Result<Arc<Rgb32f>, String> {
        match self {
            SourceRef::Loaded(a) => Ok(a.clone()),
            SourceRef::Demo { scene, max_edge } => Ok(Arc::new(scene.render_fit(*max_edge))),
            SourceRef::File { path, max_edge, loader } => match loader {
                Some(l) => l(path, *max_edge).map(|(img, _)| Arc::new(img)),
                None => Err(format!("no decoder available for {path}")),
            },
        }
    }
}

pub struct MediaCache {
    /// Decoded thumbnail-level sources (LRU by bytes).
    thumbs: Lru<PhotoId, Arc<Rgb32f>>,
    previews: Vec<(PhotoId, Arc<Rgb32f>)>,
    /// The last full-resolution original (exports; one at a time: ~300 MB at 24 MP).
    full: Option<(PhotoId, Arc<Rgb32f>)>,
    /// How many previews to keep (LRU).
    pub preview_capacity: usize,
    pub file_loader: Option<FileLoader>,
    pub file_probe: Option<FileProbe>,
    scenes: Vec<lightcraft_scenes::Scene>,
    /// Rendered thumbnails (memory, plus disk once a library is attached).
    pub rendered: Arc<PreviewCache>,
}

impl Default for MediaCache {
    fn default() -> Self {
        MediaCache {
            thumbs: Lru::new(THUMB_SOURCE_BYTES),
            previews: Vec::new(),
            full: None,
            preview_capacity: 0,
            file_loader: None,
            file_probe: None,
            scenes: Vec::new(),
            rendered: Arc::new(PreviewCache::memory(RENDERED_MEM_BYTES)),
        }
    }
}

impl MediaCache {
    /// Keep rendered thumbnails on disk in `dir` as well.
    pub fn attach_disk_cache(&mut self, dir: &std::path::Path) {
        self.rendered = Arc::new(PreviewCache::with_disk(RENDERED_MEM_BYTES, dir, DISK_CACHE_BYTES));
    }

    pub fn get(&mut self, id: PhotoId, level: SourceLevel) -> Option<Arc<Rgb32f>> {
        match level {
            SourceLevel::Thumb => self.thumbs.get(&id).cloned(),
            SourceLevel::Preview => self.previews.iter().find(|(p, _)| *p == id).map(|(_, a)| a.clone()),
            SourceLevel::Full => self.full.as_ref().filter(|(p, _)| *p == id).map(|(_, a)| a.clone()),
        }
    }

    pub fn insert(&mut self, id: PhotoId, level: SourceLevel, img: Arc<Rgb32f>) {
        match level {
            SourceLevel::Thumb => {
                let cost = img.width * img.height * 12 + 64;
                self.thumbs.insert(id, img, cost);
            }
            SourceLevel::Preview => {
                self.previews.retain(|(p, _)| *p != id);
                self.previews.push((id, img));
                let cap = if self.preview_capacity == 0 { 10 } else { self.preview_capacity };
                while self.previews.len() > cap {
                    self.previews.remove(0);
                }
            }
            SourceLevel::Full => self.full = Some((id, img)),
        }
    }

    /// Forget a photo's decoded sources (e.g. after its file changed).
    pub fn forget(&mut self, id: PhotoId) {
        self.thumbs.remove(&id);
        self.previews.retain(|(p, _)| *p != id);
        if self.full.as_ref().is_some_and(|(p, _)| *p == id) {
            self.full = None;
        }
    }

    /// (decoded thumbnail sources, bytes).
    pub fn source_usage(&self) -> (usize, usize) {
        (self.thumbs.len(), self.thumbs.cost())
    }

    pub fn source_ref(&mut self, p: &Photo, level: SourceLevel) -> SourceRef {
        if let Some(a) = self.get(p.id, level) {
            return SourceRef::Loaded(a);
        }
        // Procedural scenes have a nominal size: "full" is that size, not unbounded.
        let max_edge = level.max_edge().min(match (&p.source, level) {
            (Source::Demo { .. }, SourceLevel::Full) => p.width.max(p.height).max(1) as usize,
            _ => usize::MAX,
        });
        self.origin_ref(&p.source, max_edge)
    }

    /// How to load `origin` at most `max_edge` pixels long, ignoring decoded sources in memory (a
    /// render worker in another wasm instance builds its sources from this).
    pub fn origin_ref(&mut self, origin: &Source, max_edge: usize) -> SourceRef {
        match origin {
            Source::Demo { scene } => {
                if self.scenes.is_empty() {
                    self.scenes = lightcraft_scenes::demo_library();
                }
                match self.scenes.iter().find(|s| s.id == *scene) {
                    Some(s) => SourceRef::Demo { scene: Box::new(s.clone()), max_edge },
                    None => SourceRef::File { path: format!("demo:{scene}"), max_edge, loader: None },
                }
            }
            Source::File { path } => SourceRef::File { path: path.clone(), max_edge, loader: self.file_loader.clone() },
        }
    }
}

/// Everything needed to render one photo, detached from the session.
#[derive(Clone)]
pub struct RenderJob {
    pub photo: PhotoId,
    pub level: SourceLevel,
    pub source: SourceRef,
    /// Where the photo's pixels come from (for executors that can't share `source`, e.g. a web
    /// worker).
    pub origin: Source,
    pub info: SourceInfo,
    pub settings: Arc<DevelopSettings>,
    pub request: RenderRequest,
    /// Identifies the result for caching: hash of settings + request.
    pub key: u64,
    /// Rendered-thumbnail cache and this job's key in it.
    pub cache: Option<(Arc<PreviewCache>, Hash128)>,
    /// Intermediate results of this view's previous renders (set by the frontend for the loupe):
    /// slider drags then only redo the stages the changed setting feeds.
    pub stages: Option<Arc<StageCache>>,
}

pub struct RenderResult {
    pub photo: PhotoId,
    pub level: SourceLevel,
    pub key: u64,
    pub rendered: Result<Rendered, String>,
    /// A source that was loaded by this job (to be inserted into the cache).
    pub loaded: Option<Arc<Rgb32f>>,
}

impl RenderJob {
    /// Render at draft quality (interactive drags). Gets its own result key.
    pub fn draft(mut self) -> Self {
        if self.request.quality != Quality::Draft {
            self.request.quality = Quality::Draft;
            self.key ^= 0x9e37_79b9_7f4a_7c15;
        }
        self
    }

    /// Reuse `stages` across this view's renders.
    pub fn with_stages(mut self, stages: Arc<StageCache>) -> Self {
        self.stages = Some(stages);
        self
    }

    pub fn run(self) -> RenderResult {
        if let Some((cache, key)) = &self.cache
            && let Some(img) = cache.get(*key)
        {
            let image = Arc::unwrap_or_clone(img);
            let histogram = Histogram::of_srgb8(&image);
            return RenderResult { photo: self.photo, level: self.level, key: self.key, rendered: Ok(Rendered { image, histogram }), loaded: None };
        }
        let was_loaded = matches!(self.source, SourceRef::Loaded(_));
        match self.source.load() {
            Ok(src) => {
                let rendered = match &self.stages {
                    Some(st) => lightcraft_pipeline::render_cached(&src, &self.info, &self.settings, &self.request, st),
                    None => lightcraft_pipeline::render(&src, &self.info, &self.settings, &self.request),
                };
                if let Some((cache, key)) = &self.cache {
                    cache.put(*key, Arc::new(rendered.image.clone()));
                }
                RenderResult { photo: self.photo, level: self.level, key: self.key, rendered: Ok(rendered), loaded: (!was_loaded).then_some(src) }
            }
            Err(e) => RenderResult { photo: self.photo, level: self.level, key: self.key, rendered: Err(e), loaded: None },
        }
    }
}

/// What identifies a photo's pixels for caching: its content hash, else its source.
pub fn content_key(p: &Photo) -> String {
    match (&p.content_hash, &p.source) {
        (Some(h), _) => h.clone(),
        (None, Source::Demo { scene }) => format!("demo:{scene}"),
        (None, Source::File { path }) => format!("file:{path}:{}", p.file_size),
    }
}

pub fn source_info(p: &Photo) -> SourceInfo {
    // Procedural demo scenes are scene-referred HDR (like raw files): use the filmic tone map.
    if matches!(p.source, Source::Demo { .. }) {
        return SourceInfo { raw: true, as_shot_temp: 6500.0, as_shot_tint: 0.0, lens: None };
    }
    if p.kind == MediaKind::Raw {
        SourceInfo { raw: true, as_shot_temp: 5500.0, as_shot_tint: 0.0, lens: p.embedded_lens }
    } else {
        SourceInfo::default()
    }
}

impl crate::Session {
    /// Build a render job for `id` fitting `max_w × max_h`. `before` renders the unedited look.
    pub fn render_job(&mut self, id: PhotoId, max_w: usize, max_h: usize, before: bool, apply_crop: bool) -> Option<RenderJob> {
        self.build_job(id, max_w, max_h, before, apply_crop, None)
    }

    /// A grid/filmstrip thumbnail job: the long edge is rounded up to one of [`THUMB_SIZES`]
    /// (≥ `long_edge`) and the result comes from / goes to the thumbnail cache (memory + disk).
    pub fn thumb_job(&mut self, id: PhotoId, long_edge: usize) -> Option<RenderJob> {
        let b = THUMB_SIZES.iter().copied().find(|s| *s >= long_edge).unwrap_or(THUMB_SIZES[THUMB_SIZES.len() - 1]);
        self.build_job(id, b, b, false, true, Some(b))
    }

    fn build_job(
        &mut self,
        id: PhotoId,
        max_w: usize,
        max_h: usize,
        before: bool,
        apply_crop: bool,
        thumb_bucket: Option<usize>,
    ) -> Option<RenderJob> {
        let p = self.catalog.photo(id)?.clone();
        let level = SourceLevel::for_size(max_w.max(max_h));
        let source = self.media.source_ref(&p, level);
        let settings = if before { Arc::new(lightcraft_pipeline::before_settings(&p.develop)) } else { p.develop.clone() };
        let request = RenderRequest { max_w, max_h, quality: Quality::Full, apply_crop };
        let key = settings.hash64() ^ ((max_w as u64) << 40) ^ ((max_h as u64) << 20) ^ (apply_crop as u64) ^ (level as u64) << 60;
        let cache = thumb_bucket.map(|b| {
            let k = Hasher128::new().str(&content_key(&p)).u64(settings.hash64()).u64(b as u64).u64(RENDER_CACHE_VERSION).finish();
            (self.media.rendered.clone(), k)
        });
        Some(RenderJob { photo: id, level, source, origin: p.source.clone(), info: source_info(&p), settings, request, key, cache, stages: None })
    }

    /// Accept a finished job's loaded source into the cache.
    pub fn accept(&mut self, r: &RenderResult) {
        if let Some(src) = &r.loaded {
            self.media.insert(r.photo, r.level, src.clone());
        }
    }

    /// Synchronous render (CLI, MCP, tests).
    pub fn render_now(&mut self, id: PhotoId, max_w: usize, max_h: usize) -> Result<Rendered, String> {
        let job = self.render_job(id, max_w, max_h, false, true).ok_or("no such photo")?;
        let r = job.run();
        self.accept(&r);
        r.rendered
    }

    /// The source proxy for pixel-statistics commands (auto tone/WB), loading synchronously.
    pub fn source_now(&mut self, id: PhotoId, level: SourceLevel) -> Result<Arc<Rgb32f>, String> {
        let p = self.catalog.photo(id).ok_or("no such photo")?.clone();
        let r = self.media.source_ref(&p, level).load()?;
        self.media.insert(id, level, r.clone());
        Ok(r)
    }
}

/// What an import learns from a file header (set by the app from `lightcraft-codecs`/`-raw`).
#[derive(Clone, Debug, Default)]
pub struct ProbeInfo {
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub kind: MediaKind,
    pub file_size: u64,
    pub captured: Option<String>,
    pub meta: lightcraft_catalog::Meta,
    pub as_shot_wb: Option<(f64, f64)>,
    /// Hash of the file's bytes (hex), for duplicate detection.
    pub content_hash: Option<String>,
    /// Lens corrections embedded in the file (DNG opcodes).
    pub embedded_lens: Option<lightcraft_develop::EmbeddedLens>,
    /// The file's embedded XMP packet (raw/DNG files), for develop settings stored inside the file.
    pub xmp: Option<String>,
}

pub type FileProbe = Arc<dyn Fn(&str) -> Result<ProbeInfo, String> + Send + Sync>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_cover_sizes_and_large_exports_use_the_original() {
        assert_eq!(SourceLevel::for_size(256), SourceLevel::Thumb);
        assert_eq!(SourceLevel::for_size(2560), SourceLevel::Preview);
        assert_eq!(SourceLevel::for_size(2561), SourceLevel::Full);
        let mut s = crate::Session::with_demo();
        let p = s.catalog.photos().next().unwrap().clone();
        let long = p.width.max(p.height) as usize;
        assert!(long > 2560, "demo photos are camera-sized");
        // a full-size export reads the source at its native size, not an upscaled preview
        let job = s.render_job(p.id, long, long, false, true).unwrap();
        assert_eq!(job.level, SourceLevel::Full);
        match &job.source {
            SourceRef::Demo { max_edge, .. } => assert_eq!(*max_edge, long),
            _ => panic!("demo source expected"),
        }
        let loupe = s.render_job(p.id, 1600, 1600, false, true).unwrap();
        assert_eq!(loupe.level, SourceLevel::Preview);
        assert_ne!(loupe.key, s.render_job(p.id, 1600, 1600, false, true).unwrap().draft().key);
    }
}
