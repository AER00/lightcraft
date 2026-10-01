//! End-to-end render benchmark (min of N runs, robust to a loaded machine):
//! `cargo run --release -p lightcraft-engine --example render_bench -- corpus/raw/arw-sony-a7m3-compressed.arw`
//! (`N=9` runs per scenario, `RAYON_NUM_THREADS=1` for algorithmic comparisons,
//! `LIGHTCRAFT_PROFILE=1` for per-stage timings).
//!
//! Without a file argument a procedural 6000×4000 source is used. Scenarios: a ~2.5 MP loupe render
//! of the 2560 px preview (cold, and with a warm stage cache while a slider is dragged), a draft,
//! and a full-size render + JPEG encode (export); `ONLY=batch` with several files times a full-size
//! export of each (decode + render + encode). Prints minimum wall-clock and minimum process CPU
//! time: on a shared machine the CPU time shows the work done, wall-clock also the wait for cores.
use std::time::Instant;

use lightcraft_develop::DevelopSettings;
use lightcraft_engine::export::{ExportOptions, encode_image};
use lightcraft_pipeline::{Quality, RenderRequest, SourceInfo, StageCache, render, render_cached};
use lightcraft_raster::Rgb32f;
use lightcraft_raster::resample::{Filter, fit};

/// Process CPU time in ms (all threads). The only `unsafe` is this libc clock read, in a dev-only example.
#[allow(unsafe_code)]
fn cpu_ms() -> f64 {
    #[cfg(unix)]
    {
        let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
        // SAFETY: `clock_gettime` only writes into the timespec we pass.
        unsafe { libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut ts) };
        ts.tv_sec as f64 * 1e3 + ts.tv_nsec as f64 / 1e6
    }
    #[cfg(not(unix))]
    {
        0.0
    }
}

/// Minimum wall-clock and minimum CPU time (ms) over the runs.
struct T(f64, f64);

impl std::fmt::Display for T {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.1} ms wall, {:.1} ms cpu", self.0, self.1)
    }
}

fn best(n: usize, mut f: impl FnMut()) -> T {
    let mut b = T(f64::MAX, f64::MAX);
    for _ in 0..n {
        let (t, c) = (Instant::now(), cpu_ms());
        f();
        b.1 = b.1.min(cpu_ms() - c);
        b.0 = b.0.min(t.elapsed().as_secs_f64() * 1e3);
    }
    b
}

fn typical() -> DevelopSettings {
    let mut s = DevelopSettings::default();
    s.light.exposure = 0.3;
    s.light.highlights = -40.0;
    s.light.shadows = 30.0;
    s.effects.clarity = 15.0;
    s.effects.texture = 10.0;
    s.effects.dehaze = 10.0;
    s.detail.nr_luminance = 30.0;
    s.detail.nr_color = 25.0;
    s.detail.sharpen_amount = 40.0;
    s
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let n: usize = std::env::var("N").ok().and_then(|v| v.parse().ok()).unwrap_or(5);
    let only = std::env::var("ONLY").unwrap_or_default();
    let run = |name: &str| only.is_empty() || only.split(',').any(|o| name.contains(o));
    let (full, info) = match args.first() {
        Some(path) => {
            let bytes = std::fs::read(path).expect("read");
            let t = Instant::now();
            let r = lightcraft_engine::files::load_bytes(&bytes, usize::MAX).expect("decode");
            println!("decode full: {:.0} ms", t.elapsed().as_secs_f64() * 1e3);
            r
        }
        None => (
            Rgb32f::from_fn(6000, 4000, |x, y| {
                let v = ((x as f32 * 0.011).sin() * (y as f32 * 0.007).cos() * 0.5 + 0.5) * 0.4 + ((x * 31 + y * 17) % 13) as f32 * 0.002;
                [v, v * 0.9, v * 0.7]
            }),
            SourceInfo { raw: true, ..Default::default() },
        ),
    };
    println!("source {}×{} ({:.1} MP), {n} runs each", full.width, full.height, (full.width * full.height) as f64 / 1e6);
    let preview = std::sync::Arc::new(fit(&full, 2560, 2560, Filter::Box));
    let s = typical();
    let view = RenderRequest::fit(1920, 1280);
    let draft = RenderRequest { max_w: 1152, max_h: 768, quality: Quality::Draft, apply_crop: true };

    if run("cold") {
        println!("loupe 1920×1280 cold:                 {}", best(n, || drop(render(&preview, &info, &s, &view))));
        println!("loupe draft 1152×768 cold:            {}", best(n, || drop(render(&preview, &info, &s, &draft))));
    }
    let cache = StageCache::default();
    let mut k = 0.0;
    if run("drag") {
        drop(render_cached(&preview, &info, &s, &view, &cache));
        let ms = best(n, || {
            let mut t = s.clone();
            k += 1.0;
            t.light.contrast = k;
            t.light.exposure = 0.3 + k * 0.01;
            drop(render_cached(&preview, &info, &t, &view, &cache));
        });
        println!("loupe 1920×1280 exposure drag (warm): {ms}");
        drop(render_cached(&preview, &info, &s, &draft, &cache));
        let ms = best(n, || {
            let mut t = s.clone();
            k += 1.0;
            t.light.highlights = -40.0 + k;
            drop(render_cached(&preview, &info, &t, &draft, &cache));
        });
        println!("loupe draft highlights drag (warm):   {ms}");
        let ms = best(n, || {
            let mut t = s.clone();
            k += 1.0;
            t.effects.clarity = 15.0 + k;
            drop(render_cached(&preview, &info, &t, &draft, &cache));
        });
        println!("loupe draft clarity drag (warm):      {ms}");
        let ms = best(n, || {
            let mut t = s.clone();
            k += 1.0;
            t.detail.nr_luminance = 30.0 + k;
            drop(render_cached(&preview, &info, &t, &draft, &cache));
        });
        println!("loupe draft NR drag (warm):           {ms}");
    }
    if run("batch") && args.len() > 1 {
        // full-size JPEG export of all the files given (decode + render + encode)
        use lightcraft_engine::export::export_photo;
        let mut session = lightcraft_engine::Session::new().with_fs();
        let r = session.execute("library.import", &serde_json::json!({"paths": args})).expect("import");
        let ids: Vec<_> = r["imported"].as_array().expect("ids").iter().filter_map(|v| v.as_u64()).map(lightcraft_engine::catalog::PhotoId).collect();
        let items: Vec<_> = ids.iter().enumerate().map(|(i, id)| (*id, i + 1)).collect();
        let o = ExportOptions::default();
        let ms = best(n.min(3), || {
            for &(id, seq) in &items {
                drop(export_photo(&mut session, id, &o, seq).expect("export"));
                session.media.forget(id); // as in a batch: every original is decoded once
            }
        });
        println!("batch export of {} files (decode+render+JPEG): {ms}", items.len());
        return;
    }
    if run("export") {
        let big = RenderRequest::fit(full.width, full.height);
        let ne = n.min(3);
        let mut img = None;
        let ms = best(ne, || img = Some(render(&full, &info, &s, &big).image));
        println!("export render {}×{}:            {ms}", full.width, full.height);
        let img = img.expect("rendered");
        println!("export JPEG encode:                   {}", best(ne, || drop(encode_image(&img, &ExportOptions::default()))));
    }
}
