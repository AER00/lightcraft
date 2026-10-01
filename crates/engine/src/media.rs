//! Source proxies and render jobs.
//!
//! Sources are decoded (or generated) once per level and cached: `Thumb` (≤ 512 px long edge) for
//! the grid and filmstrip, `Preview` (≤ 2560 px) for the loupe. A [`RenderJob`] is self-contained
//! and `Send`: frontends run it on a worker thread; if the source wasn't cached yet the job loads it
//! and hands it back in the [`RenderResult`] so the cache can keep it.

use std::collections::HashMap;
use std::sync::Arc;

use lightcraft_catalog::{MediaKind, Photo, PhotoId, Source};
use lightcraft_develop::DevelopSettings;
use lightcraft_pipeline::{RenderRequest, Rendered, SourceInfo};
use lightcraft_raster::Rgb32f;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceLevel {
    Thumb,
    Preview,
}

impl SourceLevel {
    pub fn max_edge(self) -> usize {
        match self {
            SourceLevel::Thumb => 512,
            SourceLevel::Preview => 2560,
        }
    }
    /// Smallest level that can serve an output of `long_edge` pixels.
    pub fn for_size(long_edge: usize) -> SourceLevel {
        if long_edge <= 520 { SourceLevel::Thumb } else { SourceLevel::Preview }
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

#[derive(Default)]
pub struct MediaCache {
    thumbs: HashMap<PhotoId, Arc<Rgb32f>>,
    previews: Vec<(PhotoId, Arc<Rgb32f>)>,
    /// How many previews to keep (LRU).
    pub preview_capacity: usize,
    pub file_loader: Option<FileLoader>,
    pub file_probe: Option<FileProbe>,
    scenes: Vec<lightcraft_scenes::Scene>,
}

impl MediaCache {
    pub fn get(&self, id: PhotoId, level: SourceLevel) -> Option<Arc<Rgb32f>> {
        match level {
            SourceLevel::Thumb => self.thumbs.get(&id).cloned(),
            SourceLevel::Preview => self.previews.iter().find(|(p, _)| *p == id).map(|(_, a)| a.clone()),
        }
    }

    pub fn insert(&mut self, id: PhotoId, level: SourceLevel, img: Arc<Rgb32f>) {
        match level {
            SourceLevel::Thumb => {
                self.thumbs.insert(id, img);
            }
            SourceLevel::Preview => {
                self.previews.retain(|(p, _)| *p != id);
                self.previews.push((id, img));
                let cap = if self.preview_capacity == 0 { 10 } else { self.preview_capacity };
                while self.previews.len() > cap {
                    self.previews.remove(0);
                }
            }
        }
    }

    pub fn source_ref(&mut self, p: &Photo, level: SourceLevel) -> SourceRef {
        if let Some(a) = self.get(p.id, level) {
            return SourceRef::Loaded(a);
        }
        match &p.source {
            Source::Demo { scene } => {
                if self.scenes.is_empty() {
                    self.scenes = lightcraft_scenes::demo_library();
                }
                match self.scenes.iter().find(|s| s.id == *scene) {
                    Some(s) => SourceRef::Demo { scene: Box::new(s.clone()), max_edge: level.max_edge() },
                    None => SourceRef::File { path: format!("demo:{scene}"), max_edge: level.max_edge(), loader: None },
                }
            }
            Source::File { path } => SourceRef::File { path: path.clone(), max_edge: level.max_edge(), loader: self.file_loader.clone() },
        }
    }
}

/// Everything needed to render one photo, detached from the session.
#[derive(Clone)]
pub struct RenderJob {
    pub photo: PhotoId,
    pub level: SourceLevel,
    pub source: SourceRef,
    pub info: SourceInfo,
    pub settings: Arc<DevelopSettings>,
    pub request: RenderRequest,
    /// Identifies the result for caching: hash of settings + request.
    pub key: u64,
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
    pub fn run(self) -> RenderResult {
        let was_loaded = matches!(self.source, SourceRef::Loaded(_));
        match self.source.load() {
            Ok(src) => {
                let rendered = lightcraft_pipeline::render(&src, &self.info, &self.settings, &self.request);
                RenderResult { photo: self.photo, level: self.level, key: self.key, rendered: Ok(rendered), loaded: (!was_loaded).then_some(src) }
            }
            Err(e) => RenderResult { photo: self.photo, level: self.level, key: self.key, rendered: Err(e), loaded: None },
        }
    }
}

pub fn source_info(p: &Photo) -> SourceInfo {
    // Procedural demo scenes are scene-referred HDR (like raw files): use the filmic tone map.
    if matches!(p.source, Source::Demo { .. }) {
        return SourceInfo { raw: true, as_shot_temp: 6500.0, as_shot_tint: 0.0 };
    }
    if p.kind == MediaKind::Raw { SourceInfo { raw: true, as_shot_temp: 5500.0, as_shot_tint: 0.0 } } else { SourceInfo::default() }
}

impl crate::Session {
    /// Build a render job for `id` fitting `max_w × max_h`. `before` renders the unedited look.
    pub fn render_job(&mut self, id: PhotoId, max_w: usize, max_h: usize, before: bool, apply_crop: bool) -> Option<RenderJob> {
        let p = self.catalog.photo(id)?.clone();
        let level = SourceLevel::for_size(max_w.max(max_h));
        let source = self.media.source_ref(&p, level);
        let settings = if before { Arc::new(lightcraft_pipeline::before_settings(&p.develop)) } else { p.develop.clone() };
        let request = RenderRequest { max_w, max_h, quality: lightcraft_pipeline::Quality::Full, apply_crop };
        let key = settings.hash64()
            ^ ((max_w as u64) << 40)
            ^ ((max_h as u64) << 20)
            ^ (apply_crop as u64)
            ^ ((level == SourceLevel::Preview) as u64) << 60;
        Some(RenderJob { photo: id, level, source, info: source_info(&p), settings, request, key })
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
}

pub type FileProbe = Arc<dyn Fn(&str) -> Result<ProbeInfo, String> + Send + Sync>;

/// Register files as photos (duplicates by path are skipped). Returns the new ids.
pub fn import_paths(s: &mut crate::Session, paths: &[String]) -> crate::Result<Vec<PhotoId>> {
    let now = (s.clock)();
    let mut ops = Vec::new();
    let mut ids = Vec::new();
    for path in paths {
        let exists = s.catalog.photos().any(|p| matches!(&p.source, Source::File { path: q } if q == path));
        if exists {
            continue;
        }
        let info = match &s.media.file_probe {
            Some(probe) => match probe(path) {
                Ok(i) => i,
                Err(e) => {
                    log::warn!("import {path}: {e}");
                    continue;
                }
            },
            None => ProbeInfo {
                format: std::path::Path::new(path).extension().map(|e| e.to_string_lossy().to_uppercase()).unwrap_or_default(),
                ..Default::default()
            },
        };
        let id = s.catalog.alloc_photo_id();
        let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.clone());
        let mut p = Photo::new(id, Source::File { path: path.clone() }, &name, &info.format, info.width, info.height, &now);
        p.kind = info.kind;
        p.file_size = info.file_size;
        p.captured = info.captured;
        p.meta = info.meta;
        p.as_shot_wb = info.as_shot_wb;
        if let Some((t, tint)) = info.as_shot_wb {
            p.develop = std::sync::Arc::new(lightcraft_develop::DevelopSettings::for_raw(t, tint));
        }
        ids.push(id);
        ops.push(lightcraft_catalog::Op::AddPhoto { photo: Box::new(p) });
    }
    if !ops.is_empty() {
        s.commit("Add Photos", lightcraft_catalog::Op::Batch { ops })?;
    }
    Ok(ids)
}
