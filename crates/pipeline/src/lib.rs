//! The LightCraft develop pipeline (CPU reference implementation).
//!
//! Input: a scene-referred, linear Rec.2020 source image (already EXIF-oriented) at any resolution
//! (full size or a proxy), plus [`DevelopSettings`]. Output: a display-encoded sRGB image at the
//! requested size, and its histogram.
//!
//! Stage order (see `docs/pipeline.md`):
//! 1. geometry — user orientation, crop + straighten, flips; one resample at output resolution
//! 2. scene-linear — white balance, exposure, dehaze, local tone (highlights/shadows), texture,
//!    clarity, local adjustments (masks)
//! 3. tone map — contrast / whites / blacks filmic curve on luminance, highlight desaturation
//! 4. colour — vibrance, saturation, colour mixer, colour grading, B&W (OkLCh)
//! 5. display — gamut map to sRGB, encode, tone curves (parametric + point), vignette, grain
//!
//! Spatial parameters are specified relative to the image's long edge, so a 400 px preview and a
//! 60 MP export look alike.
//!
//! Exposure is a gain, so the spatial stages run on the un-exposed image and the per-pixel stage
//! applies it (filters on log luminance are shift-equivariant: identical result). With
//! [`render_cached`] each stage's output is reused while its inputs are unchanged ([`StageCache`]):
//! dragging a tone, colour or exposure slider re-runs only the per-pixel stage.
#![forbid(unsafe_code)]

pub mod auto;
mod colorops;
mod finish;
pub mod geometry;
mod local;
pub mod masks;
pub mod profiles;
pub mod spots;
mod tone;

use lightcraft_develop::{DevelopSettings, Treatment};
use std::sync::{Arc, Mutex};

use lightcraft_raster::{Histogram, Plane, Rgb32f, Rgba8, par_rows};

pub use tone::ToneMap;

/// Facts about the source the settings are interpreted against.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceInfo {
    /// Raw sources use absolute Kelvin white balance; rendered sources use a relative scale around
    /// their as-shot white.
    pub raw: bool,
    pub as_shot_temp: f64,
    pub as_shot_tint: f64,
}

impl Default for SourceInfo {
    fn default() -> Self {
        Self { raw: false, as_shot_temp: 6500.0, as_shot_tint: 0.0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Quality {
    /// Interactive drags: skips the most expensive refinements.
    Draft,
    #[default]
    Full,
}

#[derive(Clone, Copy, Debug)]
pub struct RenderRequest {
    /// Output must fit in `max_w × max_h` (aspect preserved).
    pub max_w: usize,
    pub max_h: usize,
    pub quality: Quality,
    /// Show the crop frame's content only (true) or the whole uncropped image (crop tool active).
    pub apply_crop: bool,
}

impl RenderRequest {
    pub fn fit(max_w: usize, max_h: usize) -> Self {
        Self { max_w, max_h, quality: Quality::Full, apply_crop: true }
    }
}

pub struct Rendered {
    pub image: Rgba8,
    pub histogram: Histogram,
}

/// Everything the per-pixel stage needs, precomputed at output resolution.
///
/// The image and planes are computed *before exposure* (so they can be reused while exposure is
/// dragged): the per-pixel stage multiplies the image by `gain` and shifts log planes by `ev`.
/// Spatial filters on log luminance are shift-equivariant, so this equals filtering after exposure.
pub(crate) struct Prepared {
    pub img: Arc<Rgb32f>,
    /// Log2 luminance relative to middle grey (before exposure: add `ev`).
    pub log_l: Arc<Plane>,
    pub base: Arc<Plane>,
    pub clarity_blur: Option<Arc<Plane>>,
    pub texture_blur: Option<Arc<Plane>>,
    pub dark: Option<Arc<Plane>>,
    /// Airlight of `dark` (before exposure).
    pub air: f32,
    pub masks: Vec<masks::Evaluated>,
    /// Exposure in EV and as a linear gain.
    pub ev: f32,
    pub gain: f32,
    /// Output pixels per unit of the source long edge.
    pub px_per_long: f64,
}

/// Output size for a source of `src_w × src_h` under `s`, fitting `max_w × max_h`.
pub fn output_size(src_w: usize, src_h: usize, s: &DevelopSettings, req: &RenderRequest) -> (usize, usize) {
    geometry::Frame::new(src_w, src_h, s, req.apply_crop).fit(req.max_w, req.max_h)
}

/// Intermediate results of recent renders of one view, reused by [`render_cached`].
///
/// Each stage is keyed by exactly the inputs it depends on: the resampled source by the source
/// buffer, framing and output size; the white-balanced, retouched, denoised image by those plus
/// white balance, spots and noise reduction; each spatial plane by that plus its own radius. So a
/// tone or colour slider drag (or exposure) re-runs only the per-pixel stage, a clarity drag skips
/// resampling and noise reduction, and so on. Holds the last few output sizes (a draft-size and a
/// full-size render of the same view both stay warm).
pub struct StageCache {
    entries: Mutex<Vec<CacheEntry>>,
    capacity: usize,
}

impl Default for StageCache {
    fn default() -> Self {
        StageCache { entries: Mutex::new(Vec::new()), capacity: 2 }
    }
}

impl StageCache {
    /// Drop everything (e.g. when memory is needed).
    pub fn clear(&self) {
        self.lock().clear();
    }

    /// Number of cached output sizes.
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<CacheEntry>> {
        self.entries.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn get(&self, src: &Arc<Rgb32f>, geo: u64) -> Option<CacheEntry> {
        self.lock().iter().find(|e| e.geo == geo && Arc::ptr_eq(&e.src, src)).cloned()
    }

    fn put(&self, e: CacheEntry) {
        let mut v = self.lock();
        v.retain(|o| !(o.geo == e.geo && Arc::ptr_eq(&o.src, &e.src)));
        v.push(e);
        while v.len() > self.capacity {
            v.remove(0);
        }
    }
}

#[derive(Clone)]
struct CacheEntry {
    src: Arc<Rgb32f>,
    geo: u64,
    sampled: Arc<Rgb32f>,
    lin: Option<(u64, Arc<Rgb32f>)>,
    planes: local::Planes,
}

fn hash_of(parts: impl std::hash::Hash) -> u64 {
    use std::hash::{BuildHasher, BuildHasherDefault, DefaultHasher};
    BuildHasherDefault::<DefaultHasher>::default().hash_one(parts)
}

/// Render `src` with settings `s`.
pub fn render(src: &Rgb32f, info: &SourceInfo, s: &DevelopSettings, req: &RenderRequest) -> Rendered {
    render_impl(Src::Borrowed(src), info, s, req, None)
}

/// [`render`], reusing (and refreshing) the intermediate results in `cache`. The output is
/// identical to [`render`]'s.
pub fn render_cached(src: &Arc<Rgb32f>, info: &SourceInfo, s: &DevelopSettings, req: &RenderRequest, cache: &StageCache) -> Rendered {
    render_impl(Src::Shared(src), info, s, req, Some(cache))
}

enum Src<'a> {
    Borrowed(&'a Rgb32f),
    Shared(&'a Arc<Rgb32f>),
}

fn render_impl(src: Src<'_>, info: &SourceInfo, s: &DevelopSettings, req: &RenderRequest, cache: Option<&StageCache>) -> Rendered {
    // `Instant::now()` panics on wasm32-unknown-unknown: only read the clock when profiling.
    let lap = |what: &str, t: &mut Option<std::time::Instant>| {
        if let Some(t) = t {
            eprintln!("  {what}: {:.1} ms", t.elapsed().as_secs_f64() * 1e3);
            *t = std::time::Instant::now();
        }
    };
    let mut t = profiling().then(std::time::Instant::now);
    let s = &*profiles::effective(s);
    let src_img: &Rgb32f = match &src {
        Src::Borrowed(r) => r,
        Src::Shared(a) => a,
    };
    let frame = geometry::Frame::new(src_img.width, src_img.height, s, req.apply_crop);
    let (w, h) = frame.fit(req.max_w, req.max_h);
    let px_per_long = frame.px_per_long(w);
    let src_long = src_img.width.max(src_img.height);

    let shared = match (&src, cache) {
        (Src::Shared(a), Some(c)) => Some((*a, c)),
        _ => None,
    };
    let geo = hash_of((format!("{frame:?}"), w, h));
    let cached = shared.and_then(|(a, c)| c.get(a, geo));
    let sampled = match &cached {
        Some(e) => e.sampled.clone(),
        None => Arc::new(frame.sample(src_img, w, h)),
    };
    lap("sample", &mut t);

    let (wb_t, wb_tint) = local::effective_wb(info, s);
    let d = &s.detail;
    let lin_key = hash_of((
        geo,
        [wb_t, wb_tint, info.as_shot_temp, info.as_shot_tint].map(f64::to_bits),
        format!("{:?}", s.spots),
        [d.nr_luminance, d.nr_detail, d.nr_color, d.nr_color_detail, d.nr_color_smoothness].map(f64::to_bits),
        src_long,
    ));
    let lin = match cached.as_ref().and_then(|e| e.lin.clone()).filter(|(k, _)| *k == lin_key) {
        Some((_, img)) => img,
        None => {
            // Without a cache the resampled buffer is ours: work on it in place.
            let mut img = if shared.is_some() { (*sampled).clone() } else { Arc::unwrap_or_clone(sampled.clone()) };
            local::white_balance(&mut img, info, s);
            spots::apply(&mut img, &s.spots, &frame, px_per_long);
            local::denoise(&mut img, s, src_long, w.max(h));
            Arc::new(img)
        }
    };
    lap("wb/spots/nr", &mut t);
    let mut planes = match cached.map(|e| e.planes) {
        Some(p) if p.key == lin_key => p,
        _ => local::Planes { key: lin_key, ..Default::default() },
    };
    let prep = local::prepare(lin.clone(), s, &frame, px_per_long, req.quality, &mut planes);
    lap("prepare", &mut t);
    if let Some((a, c)) = shared {
        c.put(CacheEntry { src: a.clone(), geo, sampled, lin: Some((lin_key, lin)), planes });
    }
    let image = finish::finish(&prep, s, &frame, info);
    lap("finish", &mut t);
    let histogram = Histogram::of_srgb8(&image);
    lap("histogram", &mut t);
    Rendered { image, histogram }
}

/// Convenience: render a before/after pair side by side is up to the UI; this renders "before"
/// (default look, keeping the crop so framing matches).
pub fn before_settings(s: &DevelopSettings) -> DevelopSettings {
    let mut b = DevelopSettings { crop: s.crop, orientation: s.orientation, ..DevelopSettings::default() };
    b.wb = lightcraft_develop::WhiteBalance { mode: lightcraft_develop::WbMode::AsShot, ..b.wb };
    b
}

pub(crate) fn is_bw(s: &DevelopSettings) -> bool {
    s.treatment == Treatment::Bw || s.profile.id == "lc.mono"
}

/// Parallel map over output rows with index.
/// `LIGHTCRAFT_PROFILE` is set: print per-stage timings to stderr.
pub(crate) fn profiling() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("LIGHTCRAFT_PROFILE").is_some())
}

/// Run `f`, printing its duration under `LIGHTCRAFT_PROFILE`.
pub(crate) fn timed<R>(what: &str, f: impl FnOnce() -> R) -> R {
    if !profiling() {
        return f();
    }
    let t = std::time::Instant::now();
    let r = f();
    eprintln!("    {what}: {:.1} ms", t.elapsed().as_secs_f64() * 1e3);
    r
}

pub(crate) fn for_rows<T: Send>(data: &mut [T], w: usize, f: impl Fn(usize, &mut [T]) + Sync + Send) {
    par_rows(data, w, f)
}

#[cfg(test)]
mod tests;
