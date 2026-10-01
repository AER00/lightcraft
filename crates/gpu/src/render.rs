//! The GPU renderer: the CPU pipeline's stages, evaluated on the device where a kernel exists and
//! on the CPU otherwise (per-stage hybrid). Stage results stay on the device and are cached per
//! view exactly like [`lightcraft_pipeline::StageCache`] (same keys, see [`lightcraft_pipeline::Plan`]).

use std::sync::{Arc, Mutex};

use lightcraft_develop::DevelopSettings;
use lightcraft_pipeline::finish::{FinishParams, MASK_TERMS, mask_terms};
use lightcraft_pipeline::{Plan, RenderRequest, Rendered, SourceInfo, local};
use lightcraft_raster::resample::Filter;
use lightcraft_raster::{Histogram, Plane, Rgb32f, Rgba8};

use crate::ctx::{Buf, Gpu, groups1, groups2};
use crate::params::{Present, finish_block};

/// Device-resident stages of one view, kept next to the CPU [`lightcraft_pipeline::StageCache`]
/// (as its extension). Holds the last two output sizes, like the CPU cache.
#[derive(Default)]
pub struct GpuStages {
    entries: Mutex<Vec<Entry>>,
}

const CAPACITY: usize = 2;

#[derive(Clone)]
struct Entry {
    src: Arc<Rgb32f>,
    geo: u64,
    sampled: Arc<Buf>,
    lin: Option<(u64, Arc<Buf>)>,
    planes: Planes,
}

/// Spatial planes of one linear image (`key` = its `lin_key`), each tagged with its radius.
#[derive(Clone, Default)]
struct Planes {
    key: u64,
    log_l: Option<Arc<Buf>>,
    base: Option<(u32, Arc<Buf>)>,
    clarity: Option<(u32, Arc<Buf>)>,
    texture: Option<(u32, Arc<Buf>)>,
    dark: Option<(u32, Arc<Buf>, f32)>,
}

impl GpuStages {
    fn get(&self, src: &Arc<Rgb32f>, geo: u64) -> Option<Entry> {
        self.lock().iter().find(|e| e.geo == geo && Arc::ptr_eq(&e.src, src)).cloned()
    }

    fn put(&self, e: Entry) {
        let mut v = self.lock();
        v.retain(|o| !(o.geo == e.geo && Arc::ptr_eq(&o.src, &e.src)));
        v.push(e);
        while v.len() > CAPACITY {
            v.remove(0);
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Entry>> {
        self.entries.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Number of cached output sizes.
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A render in progress: the device and the commands recorded so far. Reading a result back
/// submits everything recorded before it.
pub(crate) struct Cx<'a> {
    pub gpu: &'a Gpu,
    enc: Option<wgpu::CommandEncoder>,
}

impl<'a> Cx<'a> {
    pub fn new(gpu: &'a Gpu) -> Cx<'a> {
        Cx { gpu, enc: Some(gpu.encoder()) }
    }

    /// Record kernel `name` (see [`Gpu::run`]).
    pub fn run(&mut self, name: &str, p: &[u32], bufs: &[Option<&Buf>], groups: [u32; 3]) {
        let enc = self.enc.get_or_insert_with(|| self.gpu.encoder());
        self.gpu.run(enc, name, p, bufs, groups);
    }

    /// Submit what is recorded and read back `len` values of `b`.
    pub fn read<T: bytemuck::Pod>(&mut self, b: &Buf, len: usize) -> Vec<T> {
        let enc = self.enc.take().unwrap_or_else(|| self.gpu.encoder());
        self.gpu.finish_and_read(enc, b, len)
    }

    /// A copy of `b` (recorded).
    pub fn copy(&mut self, b: &Buf) -> Buf {
        let out = self.gpu.buffer(b.len);
        let enc = self.enc.get_or_insert_with(|| self.gpu.encoder());
        enc.copy_buffer_to_buffer(&b.buf, 0, &out.buf, 0, (b.len * 4) as u64);
        out
    }

    pub fn read_rgb(&mut self, b: &Buf, w: usize, h: usize) -> Rgb32f {
        Rgb32f { width: w, height: h, data: self.read(b, w * h * 3) }
    }

    pub fn read_plane(&mut self, b: &Buf, w: usize, h: usize) -> Plane {
        Plane { width: w, height: h, data: self.read(b, w * h) }
    }
}

pub(crate) fn rgb_words(img: &Rgb32f) -> &[f32] {
    bytemuck::cast_slice(&img.data)
}

// ---------------------------------------------------------------------------------------------
// Filters (twins of `lightcraft_raster::blur` / `resample` and `lightcraft_pipeline::local`)

/// Pixels each blur thread slides its running sum over.
const CHUNK: usize = 16;

/// Gaussian blur of a `w × h` image of `nc` interleaved channels: three box passes each way
/// (`lightcraft_raster::blur::gaussian`).
pub(crate) fn gaussian(cx: &mut Cx<'_>, src: &Buf, w: usize, h: usize, nc: usize, sigma: f32) -> Buf {
    if sigma <= 0.3 || w * h == 0 {
        return cx.copy(src);
    }
    let radii = lightcraft_raster::blur::box_radii(sigma);
    let mut cur: Option<Buf> = None;
    let passes = radii.iter().map(|r| ("box_h", *r)).chain(radii.iter().map(|r| ("box_v", *r)));
    for (k, r) in passes {
        if r == 0 {
            continue;
        }
        let out = cx.gpu.buffer(w * h * nc);
        let groups = if k == "box_h" { groups2(w.div_ceil(CHUNK), h, [64, 4]) } else { groups2(w, h.div_ceil(CHUNK), [64, 4]) };
        let p = [w as u32, h as u32, nc as u32, r as u32, CHUNK as u32];
        cx.run(k, &p, &[Some(cur.as_ref().unwrap_or(src)), Some(&out)], groups);
        cur = Some(out);
    }
    cur.unwrap_or_else(|| cx.copy(src))
}

/// Resample taps of `lightcraft_raster::resample::resize`, packed for the `resize_*` kernels.
fn taps(src: usize, dst: usize, filter: Filter) -> Vec<u32> {
    let w = lightcraft_raster::resample::weights(src, dst, filter);
    let mut t = Vec::with_capacity(dst * 3);
    let mut weights = Vec::new();
    let base = dst * 3;
    for (lo, ws) in &w {
        t.extend_from_slice(&[*lo as u32, ws.len() as u32, (base + weights.len()) as u32]);
        weights.extend(ws.iter().map(|v| v.to_bits()));
    }
    t.extend(weights);
    t
}

/// `lightcraft_raster::resample::resize` of an `nc`-channel image.
pub(crate) fn resize(cx: &mut Cx<'_>, src: &Buf, (sw, sh): (usize, usize), (dw, dh): (usize, usize), nc: usize, filter: Filter) -> Buf {
    let (dw, dh) = (dw.max(1), dh.max(1));
    if (sw, sh) == (dw, dh) {
        return cx.copy(src);
    }
    let tx = cx.gpu.upload(&taps(sw, dw, filter));
    let tmp = cx.gpu.buffer(dw * sh * nc);
    cx.run("resize_h", &[sw as u32, sh as u32, dw as u32, nc as u32], &[Some(src), Some(&tx), Some(&tmp)], groups2(dw, sh, [16, 16]));
    let ty = cx.gpu.upload(&taps(sh, dh, filter));
    let out = cx.gpu.buffer(dw * dh * nc);
    cx.run("resize_v", &[dw as u32, sh as u32, dh as u32, nc as u32], &[Some(&tmp), Some(&ty), Some(&out)], groups2(dw, dh, [16, 16]));
    out
}

/// An element-wise kernel of the `map` module over `n` items.
pub(crate) fn map(cx: &mut Cx<'_>, k: &str, n: usize, extra: &[u32], ins: [Option<&Buf>; 3], out: &Buf) {
    let mut p = vec![n as u32];
    p.extend_from_slice(extra);
    cx.run(k, &p, &[ins[0], ins[1], ins[2], Some(out)], groups1(n));
}

/// Self-guided filter of a plane (`local::guided`).
fn guided(cx: &mut Cx<'_>, p: &Buf, w: usize, h: usize, sigma: f32, eps: f32) -> Buf {
    let ab = guided_coeffs(cx, p, w, h, sigma, eps);
    let q = cx.gpu.buffer(w * h);
    map(cx, "guided_apply", w * h, &[], [Some(p), Some(&ab), None], &q);
    q
}

/// Blurred (a, b) coefficients, interleaved.
fn guided_coeffs(cx: &mut Cx<'_>, p: &Buf, w: usize, h: usize, sigma: f32, eps: f32) -> Buf {
    let n = w * h;
    let pp = cx.gpu.buffer(2 * n);
    map(cx, "guided_pre", n, &[], [Some(p), None, None], &pp);
    let m = gaussian(cx, &pp, w, h, 2, sigma);
    let ab = cx.gpu.buffer(2 * n);
    map(cx, "guided_ab", n, &[eps.to_bits()], [Some(&m), None, None], &ab);
    gaussian(cx, &ab, w, h, 2, sigma)
}

/// Fast guided filter (`local::guided_fast`): coefficients on a subsampled grid.
fn guided_fast(cx: &mut Cx<'_>, p: &Buf, w: usize, h: usize, sigma: f32, eps: f32) -> Buf {
    let s = local::guided_fast_step(sigma);
    if s <= 1 {
        return guided(cx, p, w, h, sigma, eps);
    }
    let (lw, lh) = (w.div_ceil(s).max(1), h.div_ceil(s).max(1));
    let lo = resize(cx, p, (w, h), (lw, lh), 1, Filter::Box);
    let ab = guided_coeffs(cx, &lo, lw, lh, sigma / s as f32, eps);
    let up = resize(cx, &ab, (lw, lh), (w, h), 2, Filter::Bilinear);
    let q = cx.gpu.buffer(w * h);
    map(cx, "guided_apply", w * h, &[], [Some(p), Some(&up), None], &q);
    q
}

// ---------------------------------------------------------------------------------------------
// The render

/// CPU copies made along the way (for stages that still run on the CPU).
#[derive(Default)]
struct Host {
    sampled: Option<Rgb32f>,
    lin: Option<Arc<Rgb32f>>,
    log_l: Option<Arc<Plane>>,
}

fn profiling() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("LIGHTCRAFT_PROFILE").is_some())
}

/// Render on `gpu`, reusing `stages` (if given). `None` when the render does not fit the device.
pub fn render(
    gpu: &Gpu,
    src: &Arc<Rgb32f>,
    info: &SourceInfo,
    s: &DevelopSettings,
    req: &RenderRequest,
    stages: Option<&GpuStages>,
) -> Option<Rendered> {
    let mut t = profiling().then(std::time::Instant::now);
    let lap = |what: &str, t: &mut Option<std::time::Instant>| {
        if let Some(t) = t {
            eprintln!("  gpu {what}: {:.1} ms (recorded)", t.elapsed().as_secs_f64() * 1e3);
            *t = std::time::Instant::now();
        }
    };
    let plan = lightcraft_pipeline::plan(src, info, s, req);
    let (w, h) = (plan.w, plan.h);
    let n = w * h;
    if !gpu.fits(n * 3) || !gpu.fits(src.data.len() * 3) {
        return None;
    }
    let s = &*plan.settings;
    let cached = stages.and_then(|c| c.get(src, plan.geo));
    let mut host = Host::default();
    let mut cx = Cx::new(gpu);

    // 1. geometry
    let sampled = match &cached {
        Some(e) => e.sampled.clone(),
        None => Arc::new(sample(&mut cx, src, &plan, &mut host)),
    };
    lap("sample", &mut t);

    // 2. white balance, defringe, spots, noise reduction
    let lin = match cached.as_ref().and_then(|e| e.lin.clone()).filter(|(k, _)| *k == plan.lin_key) {
        Some((_, b)) => b,
        None => Arc::new(linear(&mut cx, &sampled, info, &plan, &mut host)),
    };
    lap("wb/nr", &mut t);

    // 3. spatial planes
    let mut planes = match cached.map(|e| e.planes) {
        Some(p) if p.key == plan.lin_key => p,
        _ => Planes { key: plan.lin_key, ..Default::default() },
    };
    let prep = prepare(&mut cx, &lin, &plan, req, &mut planes);
    lap("planes", &mut t);
    if let Some(c) = stages {
        c.put(Entry { src: src.clone(), geo: plan.geo, sampled, lin: Some((plan.lin_key, lin.clone())), planes });
    }

    // 4. masks
    let (masks, terms) = masks(&mut cx, &lin, &prep, &plan, &mut host);
    lap("masks", &mut t);

    // 5. per-pixel stage
    let fp = FinishParams::new(s, &plan.frame, info, w, h, plan.px_per_long, prep.air);
    let present = Present { clarity: prep.clarity.is_some(), texture: prep.texture.is_some(), dark: prep.dark.is_some() };
    let (p, aux) = finish_block(&fp, &terms, &present);
    let aux = gpu.upload(&aux);
    let out = gpu.buffer(n);
    cx.run(
        "main",
        &p,
        &[
            Some(&lin),
            Some(&prep.log_l),
            Some(&prep.base),
            prep.clarity.as_deref(),
            prep.texture.as_deref(),
            prep.dark.as_deref(),
            masks.as_ref(),
            Some(&aux),
            Some(&out),
        ],
        groups2(w, h, [16, 16]),
    );
    let data: Vec<[u8; 4]> = cx.read(&out, n);
    let image = Rgba8 { width: w, height: h, data };
    lap("finish + readback", &mut t);
    let histogram = Histogram::of_srgb8(&image);
    Some(Rendered { image, histogram })
}

/// Resample the source into the output frame.
fn sample(cx: &mut Cx<'_>, src: &Arc<Rgb32f>, plan: &Plan<'_>, host: &mut Host) -> Buf {
    let img = plan.frame.sample(src, plan.w, plan.h);
    let b = cx.gpu.upload(rgb_words(&img));
    host.sampled = Some(img);
    b
}

/// The white-balanced, retouched, denoised image.
fn linear(cx: &mut Cx<'_>, sampled: &Buf, info: &SourceInfo, plan: &Plan<'_>, host: &mut Host) -> Buf {
    let (w, h) = (plan.w, plan.h);
    let n = w * h;
    let s = &*plan.settings;
    let img = if lightcraft_pipeline::lin_needs_cpu(s) {
        // defringe / spot removal: CPU
        let mut img = match host.sampled.take() {
            Some(i) => i,
            None => cx.read_rgb(sampled, w, h),
        };
        lightcraft_pipeline::lin_cpu(&mut img, info, plan);
        cx.gpu.upload(rgb_words(&img))
    } else {
        let m = local::wb_matrix_for(info, s);
        let mut p = vec![m.is_some() as u32];
        p.extend(m.unwrap_or_default().iter().flatten().map(|v| v.to_bits()));
        let out = cx.gpu.buffer(n * 3);
        map(cx, "wb_k", n, &p, [Some(sampled), None, None], &out);
        out
    };
    denoise(cx, img, plan)
}

/// Noise reduction (`local::denoise`).
fn denoise(cx: &mut Cx<'_>, img: Buf, plan: &Plan<'_>) -> Buf {
    let (w, h) = (plan.w, plan.h);
    let n = w * h;
    let (lum, col) = local::nr_params(&plan.settings, plan.src_long, w.max(h));
    let mut img = img;
    if let Some(nr) = lum {
        let l = cx.gpu.buffer(n);
        map(cx, "log_lum_k", n, &[], [Some(&img), None, None], &l);
        let f = guided(cx, &l, w, h, nr.sigma, nr.eps);
        let out = cx.gpu.buffer(n * 3);
        map(cx, "nr_lum", n, &[nr.k.to_bits()], [Some(&img), Some(&l), Some(&f)], &out);
        img = out;
    }
    if let Some(nr) = col {
        let chroma = cx.gpu.buffer(n * 3);
        map(cx, "chroma_k", n, &[], [Some(&img), None, None], &chroma);
        let b = gaussian(cx, &chroma, w, h, 3, nr.sigma);
        let out = cx.gpu.buffer(n * 3);
        map(cx, "nr_col", n, &[nr.t.to_bits()], [Some(&img), Some(&chroma), Some(&b)], &out);
        img = out;
    }
    img
}

/// The planes the per-pixel stage reads.
struct Prep {
    log_l: Arc<Buf>,
    base: Arc<Buf>,
    clarity: Option<Arc<Buf>>,
    texture: Option<Arc<Buf>>,
    dark: Option<Arc<Buf>>,
    air: f32,
}

fn plane_at(slot: &mut Option<(u32, Arc<Buf>)>, sigma: f32, f: impl FnOnce() -> Buf) -> Arc<Buf> {
    match slot {
        Some((k, p)) if *k == sigma.to_bits() => p.clone(),
        _ => {
            let p = Arc::new(f());
            *slot = Some((sigma.to_bits(), p.clone()));
            p
        }
    }
}

/// The spatial planes (`local::prepare`), reusing those in `planes`.
fn prepare(cx: &mut Cx<'_>, lin: &Buf, plan: &Plan<'_>, req: &RenderRequest, planes: &mut Planes) -> Prep {
    let (w, h) = (plan.w, plan.h);
    let n = w * h;
    let sig = local::plane_sigmas(&plan.settings, plan.px_per_long, req.quality);
    let log_l = match &planes.log_l {
        Some(b) => b.clone(),
        None => {
            let l = cx.gpu.buffer(n);
            map(cx, "log_lum_k", n, &[], [Some(lin), None, None], &l);
            let b = Arc::new(l);
            planes.log_l = Some(b.clone());
            b
        }
    };
    let base = match sig.base {
        Some(sg) => plane_at(&mut planes.base, sg, || guided_fast(cx, &log_l, w, h, sg, local::BASE_EPS)),
        None => log_l.clone(),
    };
    let clarity = sig.clarity.map(|sg| plane_at(&mut planes.clarity, sg, || guided_fast(cx, &log_l, w, h, sg, local::CLARITY_EPS)));
    let texture = sig.texture.map(|sg| plane_at(&mut planes.texture, sg, || gaussian(cx, &log_l, w, h, 1, sg)));
    let (dark, air) = match sig.dark {
        None => (None, 1.0),
        Some(sg) => match &planes.dark {
            Some((k, d, air)) if *k == sg.to_bits() => (Some(d.clone()), *air),
            _ => {
                let d0 = cx.gpu.buffer(n);
                map(cx, "dark_k", n, &[], [Some(lin), None, None], &d0);
                let d = gaussian(cx, &d0, w, h, 1, sg);
                // airlight: a percentile of every 7th value, as on the CPU
                let m = n.div_ceil(local::AIRLIGHT_STEP);
                let sub = cx.gpu.buffer(m);
                map(cx, "subsample", m, &[local::AIRLIGHT_STEP as u32], [Some(&d), None, None], &sub);
                let air = local::airlight_of(cx.read(&sub, m));
                let b = Arc::new(d);
                planes.dark = Some((sg.to_bits(), b.clone(), air));
                (Some(b), air)
            }
        },
    };
    Prep { log_l, base, clarity, texture, dark, air }
}

/// Evaluate the masks: their alpha planes (concatenated) and their adjustment terms.
fn masks(cx: &mut Cx<'_>, lin: &Buf, prep: &Prep, plan: &Plan<'_>, host: &mut Host) -> (Option<Buf>, Vec<[f32; MASK_TERMS]>) {
    let s = &*plan.settings;
    if !s.masks.iter().any(|m| m.visible && !m.components.is_empty()) {
        return (None, Vec::new());
    }
    let (w, h) = (plan.w, plan.h);
    let img = host.lin.get_or_insert_with(|| Arc::new(cx.read_rgb(lin, w, h))).clone();
    let l = host.log_l.get_or_insert_with(|| Arc::new(cx.read_plane(&prep.log_l, w, h))).clone();
    let ev = s.light.exposure as f32;
    let ev = lightcraft_pipeline::masks::evaluate(&s.masks, &plan.frame, w, h, &img, &l, ev);
    let terms = ev.iter().map(|m| mask_terms(&m.adjust)).collect();
    let mut all = Vec::with_capacity(ev.len() * w * h);
    for m in &ev {
        all.extend_from_slice(&m.alpha.data);
    }
    (Some(cx.gpu.upload(&all)), terms)
}
