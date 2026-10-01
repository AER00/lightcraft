//! The LightCraft develop pipeline (CPU reference implementation).
//!
//! Input: a scene-referred, linear Rec.2020 source image (already EXIF-oriented) at any resolution
//! (full size or a proxy), plus [`DevelopSettings`]. Output: a display-encoded sRGB image at the
//! requested size, and its histogram.
//!
//! Stage order (see `docs/pipeline.md`):
//! 1. geometry — user orientation, lens corrections (distortion, CA, vignetting), perspective, crop +
//!    straighten, flips; one resample at output resolution; then defringe
//! 2. scene-linear — white balance, exposure, dehaze, local tone (highlights/shadows), texture,
//!    clarity, local adjustments (masks)
//! 3. tone map — contrast / whites / blacks filmic curve on luminance, highlight desaturation
//! 4. colour — vibrance, saturation, colour mixer, colour grading, B&W (OkLCh)
//! 5. display — gamut map to sRGB, encode, tone curves (parametric + point), vignette, grain
//!
//! Spatial parameters are specified relative to the image's long edge, so a 400 px preview and a
//! 60 MP export look alike.
#![forbid(unsafe_code)]

pub mod auto;
mod colorops;
mod finish;
pub mod geometry;
mod local;
pub mod masks;
pub mod optics;
pub mod profiles;
pub mod spots;
mod tone;
pub mod transform;

use lightcraft_develop::{DevelopSettings, Treatment};
use lightcraft_raster::{Histogram, Plane, Rgb32f, Rgba8, par_rows};

pub use tone::ToneMap;

/// Facts about the source the settings are interpreted against.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceInfo {
    /// Lens corrections embedded in the file (DNG opcodes), relative to the EXIF-oriented source.
    pub lens: Option<lightcraft_develop::EmbeddedLens>,
    /// Raw sources use absolute Kelvin white balance; rendered sources use a relative scale around
    /// their as-shot white.
    pub raw: bool,
    pub as_shot_temp: f64,
    pub as_shot_tint: f64,
}

impl Default for SourceInfo {
    fn default() -> Self {
        Self { raw: false, as_shot_temp: 6500.0, as_shot_tint: 0.0, lens: None }
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
pub(crate) struct Prepared {
    pub img: Rgb32f,
    /// Log2 luminance relative to middle grey.
    pub log_l: Plane,
    pub base: Plane,
    pub clarity_blur: Option<Plane>,
    pub texture_blur: Option<Plane>,
    pub dark: Option<Plane>,
    pub masks: Vec<masks::Evaluated>,
    /// Output pixels per unit of the source long edge.
    pub px_per_long: f64,
}

/// Output size for a source of `src_w × src_h` under `s`, fitting `max_w × max_h`.
pub fn output_size(src_w: usize, src_h: usize, s: &DevelopSettings, req: &RenderRequest) -> (usize, usize) {
    geometry::Frame::new(src_w, src_h, s, req.apply_crop).fit(req.max_w, req.max_h)
}

/// The geometric frame `render` uses for `src` (lens data, automatic CA estimate included).
pub fn frame_for(src: &Rgb32f, info: &SourceInfo, s: &DevelopSettings, apply_crop: bool) -> geometry::Frame {
    let mut frame = geometry::Frame::with_lens(src.width, src.height, s, apply_crop, info.lens.as_ref());
    if s.optics.remove_ca && s.section_enabled("optics") {
        frame.add_lateral_ca(optics::estimate_lateral_ca(src));
    }
    frame
}

/// Render `src` with settings `s`.
pub fn render(src: &Rgb32f, info: &SourceInfo, s: &DevelopSettings, req: &RenderRequest) -> Rendered {
    let prof = std::env::var_os("LIGHTCRAFT_PROFILE").is_some();
    // `Instant::now()` panics on wasm32-unknown-unknown: only read the clock when profiling.
    let lap = |what: &str, t: &mut Option<std::time::Instant>| {
        if let Some(t) = t {
            eprintln!("  {what}: {:.1} ms", t.elapsed().as_secs_f64() * 1e3);
            *t = std::time::Instant::now();
        }
    };
    let mut t = prof.then(std::time::Instant::now);
    let s = &*profiles::effective(s);
    let frame = frame_for(src, info, s, req.apply_crop);
    let (w, h) = frame.fit(req.max_w, req.max_h);
    let mut img = frame.sample(src, w, h);
    lap("sample", &mut t);
    let px_per_long = frame.px_per_long(w);

    local::scene_linear_pre(&mut img, info, s);
    optics::defringe(&mut img, s, px_per_long / optics::DEFRINGE_REF_LONG);
    spots::apply(&mut img, &s.spots, &frame, px_per_long);
    local::denoise(&mut img, s, src.width.max(src.height), w.max(h));
    lap("wb/exposure/spots/nr", &mut t);
    let prep = local::prepare(img, s, &frame, px_per_long, req.quality);
    lap("prepare", &mut t);
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
#[cfg(test)]
mod tests_geometry;
