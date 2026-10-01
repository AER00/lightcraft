//! Micro-benchmark for the blur/map primitives (min of N runs, robust to a loaded machine):
//! `RAYON_NUM_THREADS=1 cargo run --release -p lightcraft-raster --example blur_bench`.
use lightcraft_raster::{Plane, Rgb32f, blur::gaussian};
use std::time::Instant;

fn best(n: usize, mut f: impl FnMut()) -> f64 {
    (0..n)
        .map(|_| {
            let t = Instant::now();
            f();
            t.elapsed().as_secs_f64() * 1e3
        })
        .fold(f64::MAX, f64::min)
}

fn main() {
    let (w, h) = (3000, 2400);
    let p = Plane::from_fn(w, h, |x, y| ((x * 7 + y * 13) % 255) as f32 / 255.0);
    let c = Rgb32f::from_fn(w, h, |x, y| [x as f32 / w as f32, y as f32 / h as f32, 0.5]);
    for sigma in [1.0f32, 4.0, 24.0] {
        println!("plane gaussian σ={sigma}: {:.1} ms", best(9, || drop(gaussian(&p, sigma))));
    }
    println!("rgb gaussian σ=4: {:.1} ms", best(9, || drop(gaussian(&c, 4.0))));
}
