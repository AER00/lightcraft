//! The GPU renderer: the CPU pipeline's stages, evaluated on the device where a kernel exists and
//! on the CPU otherwise (per-stage hybrid). Stage results stay on the device and are cached per
//! view exactly like [`lightcraft_pipeline::StageCache`] (same keys, see [`lightcraft_pipeline::Plan`]).

use std::sync::{Arc, Mutex};

use lightcraft_develop::DevelopSettings;
use lightcraft_pipeline::finish::{FinishParams, mask_terms};
use lightcraft_pipeline::{Plan, RenderRequest, Rendered, SourceInfo, local};
use lightcraft_raster::{Histogram, Plane, Rgb32f, Rgba8};

use crate::ctx::{Buf, Gpu, groups2};
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

/// CPU copies made along the way (for stages that still run on the CPU).
#[derive(Default)]
struct Host {
    lin: Option<Arc<Rgb32f>>,
    log_l: Option<Arc<Plane>>,
}

fn rgb_words(img: &Rgb32f) -> &[f32] {
    bytemuck::cast_slice(&img.data)
}

pub(crate) fn download_rgb(gpu: &Gpu, b: &Buf, w: usize, h: usize) -> Rgb32f {
    let v: Vec<[f32; 3]> = gpu.finish_and_read(gpu.encoder(), b, w * h * 3);
    Rgb32f { width: w, height: h, data: v }
}

pub(crate) fn download_plane(gpu: &Gpu, b: &Buf, w: usize, h: usize) -> Plane {
    let v: Vec<f32> = gpu.finish_and_read(gpu.encoder(), b, w * h);
    Plane { width: w, height: h, data: v }
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
    let prof = lightcraft_pipeline_profiling();
    let mut t = prof.then(std::time::Instant::now);
    let lap = |what: &str, t: &mut Option<std::time::Instant>| {
        if let Some(t) = t {
            eprintln!("  gpu {what}: {:.1} ms", t.elapsed().as_secs_f64() * 1e3);
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

    // 1. geometry
    let sampled = match &cached {
        Some(e) => e.sampled.clone(),
        None => Arc::new(sample(gpu, src, &plan)),
    };
    lap("sample", &mut t);

    // 2. white balance, defringe, spots, noise reduction
    let lin = match cached.as_ref().and_then(|e| e.lin.clone()).filter(|(k, _)| *k == plan.lin_key) {
        Some((_, b)) => b,
        None => Arc::new(linear(gpu, &sampled, info, &plan, &mut host)),
    };
    lap("wb/nr", &mut t);

    // 3. spatial planes
    let mut planes = match cached.map(|e| e.planes) {
        Some(p) if p.key == plan.lin_key => p,
        _ => Planes { key: plan.lin_key, ..Default::default() },
    };
    let prep = prepare(gpu, &lin, &plan, req, &mut planes, &mut host);
    lap("planes", &mut t);
    if let Some(c) = stages {
        c.put(Entry { src: src.clone(), geo: plan.geo, sampled, lin: Some((plan.lin_key, lin.clone())), planes });
    }

    // 4. masks
    let (masks, terms) = masks(gpu, &lin, &prep, &plan, &mut host);
    lap("masks", &mut t);

    // 5. per-pixel stage
    let fp = FinishParams::new(s, &plan.frame, info, w, h, plan.px_per_long, prep.air);
    let present = Present { clarity: prep.clarity.is_some(), texture: prep.texture.is_some(), dark: prep.dark.is_some() };
    let (p, aux) = finish_block(&fp, &terms, &present);
    let aux = gpu.upload(&aux);
    let out = gpu.buffer(n);
    let mut enc = gpu.encoder();
    gpu.run(
        &mut enc,
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
    let data: Vec<[u8; 4]> = gpu.finish_and_read(enc, &out, n);
    let image = Rgba8 { width: w, height: h, data };
    lap("finish", &mut t);
    let histogram = Histogram::of_srgb8(&image);
    Some(Rendered { image, histogram })
}

fn lightcraft_pipeline_profiling() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("LIGHTCRAFT_PROFILE").is_some())
}

/// Resample the source into the output frame.
fn sample(gpu: &Gpu, src: &Arc<Rgb32f>, plan: &Plan<'_>) -> Buf {
    let img = plan.frame.sample(src, plan.w, plan.h);
    gpu.upload(rgb_words(&img))
}

/// The white-balanced, retouched, denoised image.
fn linear(gpu: &Gpu, sampled: &Buf, info: &SourceInfo, plan: &Plan<'_>, host: &mut Host) -> Buf {
    let mut img = download_rgb(gpu, sampled, plan.w, plan.h);
    lightcraft_pipeline::lin_cpu(&mut img, info, plan);
    local::denoise(&mut img, &plan.settings, plan.src_long, plan.w.max(plan.h));
    let b = gpu.upload(rgb_words(&img));
    host.lin = Some(Arc::new(img));
    b
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

fn host_lin(gpu: &Gpu, lin: &Buf, plan: &Plan<'_>, host: &mut Host) -> Arc<Rgb32f> {
    host.lin.get_or_insert_with(|| Arc::new(download_rgb(gpu, lin, plan.w, plan.h))).clone()
}

fn host_log_l(gpu: &Gpu, prep_log_l: &Buf, plan: &Plan<'_>, host: &mut Host) -> Arc<Plane> {
    host.log_l.get_or_insert_with(|| Arc::new(download_plane(gpu, prep_log_l, plan.w, plan.h))).clone()
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

fn prepare(gpu: &Gpu, lin: &Buf, plan: &Plan<'_>, req: &RenderRequest, planes: &mut Planes, host: &mut Host) -> Prep {
    let s = &*plan.settings;
    let sig = local::plane_sigmas(s, plan.px_per_long, req.quality);
    let log_l = match &planes.log_l {
        Some(b) => b.clone(),
        None => {
            let img = host_lin(gpu, lin, plan, host);
            let l = img.map(local::log_lum);
            let b = Arc::new(gpu.upload(&l.data));
            host.log_l = Some(Arc::new(l));
            planes.log_l = Some(b.clone());
            b
        }
    };
    let mut cpu_plane = |f: &dyn Fn(&Plane, &Rgb32f) -> Plane| {
        let l = host_log_l(gpu, &log_l, plan, host);
        let img = host_lin(gpu, lin, plan, host);
        gpu.upload(&f(&l, &img).data)
    };
    let base = match sig.base {
        Some(sg) => plane_at(&mut planes.base, sg, || cpu_plane(&|l, _| local::guided_fast(l, sg, local::BASE_EPS))),
        None => log_l.clone(),
    };
    let clarity = sig.clarity.map(|sg| plane_at(&mut planes.clarity, sg, || cpu_plane(&|l, _| local::guided_fast(l, sg, local::CLARITY_EPS))));
    let texture = sig.texture.map(|sg| plane_at(&mut planes.texture, sg, || cpu_plane(&|l, _| lightcraft_raster::blur::gaussian(l, sg))));
    let (dark, air) = match sig.dark {
        None => (None, 1.0),
        Some(sg) => match &planes.dark {
            Some((k, d, air)) if *k == sg.to_bits() => (Some(d.clone()), *air),
            _ => {
                let img = host_lin(gpu, lin, plan, host);
                let d = lightcraft_raster::blur::gaussian(&img.map(local::dark_of), sg);
                let air = local::airlight(&d);
                let b = Arc::new(gpu.upload(&d.data));
                planes.dark = Some((sg.to_bits(), b.clone(), air));
                (Some(b), air)
            }
        },
    };
    Prep { log_l, base, clarity, texture, dark, air }
}

/// Evaluate the masks: their alpha planes (concatenated) and their adjustment terms.
fn masks(gpu: &Gpu, lin: &Buf, prep: &Prep, plan: &Plan<'_>, host: &mut Host) -> (Option<Buf>, Vec<[f32; lightcraft_pipeline::finish::MASK_TERMS]>) {
    let s = &*plan.settings;
    if !s.masks.iter().any(|m| m.visible && !m.components.is_empty()) {
        return (None, Vec::new());
    }
    let img = host_lin(gpu, lin, plan, host);
    let l = host_log_l(gpu, &prep.log_l, plan, host);
    let ev = s.light.exposure as f32;
    let ev = lightcraft_pipeline::masks::evaluate(&s.masks, &plan.frame, plan.w, plan.h, &img, &l, ev);
    let terms = ev.iter().map(|m| mask_terms(&m.adjust)).collect();
    let mut all = Vec::with_capacity(ev.len() * plan.w * plan.h);
    for m in &ev {
        all.extend_from_slice(&m.alpha.data);
    }
    (Some(gpu.upload(&all)), terms)
}
