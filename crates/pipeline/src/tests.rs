use lightcraft_develop::{DevelopSettings, controls};
use lightcraft_raster::Rgb32f;

use crate::{RenderRequest, SourceInfo, render};

fn mean_luma(img: &lightcraft_raster::Rgba8) -> f32 {
    img.data.iter().map(|p| 0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32).sum::<f32>() / img.len() as f32
}

fn scene() -> Rgb32f {
    lightcraft_scenes::demo_library()[0].render(240, 160)
}

#[test]
fn default_render_is_sane() {
    let r = render(&scene(), &SourceInfo::default(), &DevelopSettings::default(), &RenderRequest::fit(240, 240));
    assert_eq!((r.image.width, r.image.height), (240, 160));
    let m = mean_luma(&r.image);
    assert!((40.0..220.0).contains(&m), "{m}");
    assert!(r.histogram.total > 0);
}

#[test]
fn grey_ramp_stays_neutral() {
    let src = Rgb32f::from_fn(64, 8, |x, _| [0.002 * 1.12f32.powi(x as i32); 3]);
    let r = render(&src, &SourceInfo::default(), &DevelopSettings::default(), &RenderRequest::fit(64, 8));
    for p in &r.image.data {
        assert!((p[0] as i32 - p[1] as i32).abs() <= 1 && (p[1] as i32 - p[2] as i32).abs() <= 1, "{p:?}");
    }
}

#[test]
fn every_slider_changes_or_keeps_output_without_panicking() {
    let src = scene();
    let base = render(&src, &SourceInfo::default(), &DevelopSettings::default(), &RenderRequest::fit(96, 96)).image;
    for c in controls::CONTROLS {
        for v in [c.min, c.max] {
            let mut s = DevelopSettings::default();
            controls::set(&mut s, c.id, v);
            let r = render(&src, &SourceInfo::default(), &s, &RenderRequest::fit(96, 96));
            assert!(r.image.width > 0, "{}", c.id);
            let _ = &base;
        }
    }
}

#[test]
fn exposure_is_monotone() {
    let src = scene();
    let mut prev = -1.0;
    for ev in [-3.0, -1.5, 0.0, 1.0, 2.5] {
        let mut s = DevelopSettings::default();
        s.light.exposure = ev;
        let m = mean_luma(&render(&src, &SourceInfo::default(), &s, &RenderRequest::fit(96, 96)).image);
        assert!(m > prev, "{ev}: {m} <= {prev}");
        prev = m;
    }
}

#[test]
fn directional_sliders() {
    let src = scene();
    let at = |id: &str, v: f64| {
        let mut s = DevelopSettings::default();
        controls::set(&mut s, id, v);
        mean_luma(&render(&src, &SourceInfo::default(), &s, &RenderRequest::fit(96, 96)).image)
    };
    let base = at("light.exposure", 0.0);
    assert!(at("light.shadows", 100.0) > base);
    assert!(at("light.highlights", -100.0) < base);
    assert!(at("light.whites", 100.0) > base);
    assert!(at("light.blacks", -100.0) < base);
    assert!(at("vignette.amount", -100.0) < base);
    assert!(at("vignette.amount", 100.0) > base);
    assert!(at("effects.dehaze", -100.0) != base);
}

#[test]
fn resolution_independence_of_local_contrast() {
    // Clarity at two preview sizes should give similar results after downscaling.
    let src = lightcraft_scenes::demo_library()[3].render(480, 320);
    let mut s = DevelopSettings::default();
    s.effects.clarity = 80.0;
    s.light.shadows = 60.0;
    let small = render(&src, &SourceInfo::default(), &s, &RenderRequest::fit(120, 120)).image;
    let big = render(&src, &SourceInfo::default(), &s, &RenderRequest::fit(480, 480)).image;
    let bl = big.to_linear();
    let down = lightcraft_raster::resample::resize(&bl, small.width, small.height, lightcraft_raster::resample::Filter::Box).to_srgb8();
    let err: f32 = small.data.iter().zip(&down.data).map(|(a, b)| (a[1] as f32 - b[1] as f32).abs()).sum::<f32>() / small.len() as f32;
    assert!(err < 9.0, "mean abs error {err}");
}

#[test]
fn crop_and_straighten_output_size() {
    let mut s = DevelopSettings::default();
    s.crop.geometry = lightcraft_geom::crop_fit_angle(240.0, 160.0, 8.0, Some(1.0));
    let r = render(&scene(), &SourceInfo::default(), &s, &RenderRequest::fit(200, 200));
    assert_eq!((r.image.width, r.image.height), (200, 200));
}

#[test]
fn mask_brightens_only_inside() {
    use lightcraft_develop::{Mask, MaskComponent, MaskOp, MaskShape};
    use lightcraft_geom::Point;
    let src = Rgb32f::filled(100, 100, [0.1, 0.1, 0.1]);
    let mut s = DevelopSettings::default();
    s.masks.push(Mask {
        components: vec![MaskComponent {
            op: MaskOp::Add,
            invert: false,
            shape: MaskShape::Radial { center: Point::new(0.5, 0.5), rx: 0.2, ry: 0.2, angle: 0.0, feather: 10.0, invert: false },
        }],
        adjust: lightcraft_develop::LocalAdjustments { exposure: 2.0, ..Default::default() },
        ..Default::default()
    });
    let r = render(&src, &SourceInfo::default(), &s, &RenderRequest::fit(100, 100)).image;
    assert!(r.get(50, 50)[0] > r.get(5, 5)[0] + 40);
}

#[test]
#[ignore]
fn perf_report() {
    let src = lightcraft_scenes::demo_library()[0].render(3000, 2000);
    let mut s = DevelopSettings::default();
    s.light.shadows = 40.0;
    s.effects.clarity = 20.0;
    s.effects.dehaze = 10.0;
    s.color.vibrance = 20.0;
    let t = std::time::Instant::now();
    let n = 5;
    for _ in 0..n {
        let _ = render(&src, &SourceInfo::default(), &s, &RenderRequest::fit(2560, 1440));
    }
    eprintln!("render 2160x1440 from 6 MP: {:.1} ms", t.elapsed().as_secs_f64() * 1000.0 / n as f64);
}

#[test]
fn unedited_rendered_source_is_passthrough() {
    // An sRGB ramp decoded to linear Rec.2020 must render back to (almost) the same 8-bit values.
    let src8 = lightcraft_raster::Rgba8::from_fn(256, 4, |x, y| [x as u8, (x as u8).wrapping_add(y as u8 * 40), 255 - x as u8, 255]);
    let m = lightcraft_color::SRGB.to_space(&lightcraft_color::REC2020);
    let src = src8.to_linear().map(|c| m.apply_f32(c));
    let r = render(&src, &SourceInfo::default(), &DevelopSettings::default(), &RenderRequest::fit(256, 4)).image;
    let mut worst = 0i32;
    for (a, b) in src8.data.iter().zip(&r.data) {
        for c in 0..3 {
            worst = worst.max((a[c] as i32 - b[c] as i32).abs());
        }
    }
    assert!(worst <= 3, "max error {worst}");
}

fn typical_edits() -> DevelopSettings {
    let mut s = DevelopSettings::default();
    s.light.exposure = 0.4;
    s.light.highlights = -40.0;
    s.light.shadows = 30.0;
    s.effects.clarity = 15.0;
    s.effects.texture = 10.0;
    s.effects.dehaze = 12.0;
    s.detail.nr_luminance = 30.0;
    s.detail.nr_color = 25.0;
    s.detail.sharpen_amount = 40.0;
    s
}

fn max_diff(a: &lightcraft_raster::Rgba8, b: &lightcraft_raster::Rgba8) -> i32 {
    assert_eq!((a.width, a.height), (b.width, b.height));
    a.data.iter().zip(&b.data).map(|(p, q)| (0..3).map(|c| (p[c] as i32 - q[c] as i32).abs()).max().unwrap_or(0)).max().unwrap_or(0)
}

#[test]
fn stage_cache_matches_uncached_render_through_a_slider_session() {
    use crate::{Quality, StageCache, render_cached};
    use std::sync::Arc;
    let src = Arc::new(scene());
    let info = SourceInfo { raw: true, ..Default::default() };
    let cache = StageCache::default();
    let full = RenderRequest::fit(200, 200);
    let draft = RenderRequest { max_w: 120, max_h: 120, quality: Quality::Draft, apply_crop: true };
    let mut s = typical_edits();
    // every kind of edit: tone, exposure, spatial amounts, NR, WB, crop, colour — at two sizes
    let steps: Vec<Box<dyn Fn(&mut DevelopSettings)>> = vec![
        Box::new(|s| s.light.contrast = 20.0),
        Box::new(|s| s.light.exposure = -0.7),
        Box::new(|s| s.light.highlights = -80.0),
        Box::new(|s| s.effects.clarity = -30.0),
        Box::new(|s| s.effects.dehaze = 0.0),
        Box::new(|s| s.effects.dehaze = 25.0),
        Box::new(|s| s.detail.nr_luminance = 60.0),
        Box::new(|s| {
            s.wb.mode = lightcraft_develop::WbMode::Custom;
            s.wb.temp = 4000.0;
        }),
        Box::new(|s| s.crop.geometry.rect = lightcraft_geom::Rect::new(0.1, 0.1, 0.9, 0.8)),
        Box::new(|s| s.color.saturation = 30.0),
    ];
    for (i, step) in steps.iter().enumerate() {
        step(&mut s);
        for req in [&full, &draft] {
            let a = render_cached(&src, &info, &s, req, &cache).image;
            let b = render(&src, &info, &s, req).image;
            assert_eq!(a, b, "step {i} {:?}", req.quality);
        }
    }
    assert_eq!(cache.len(), 2);
    // a different source buffer never hits another source's entries
    let other = Arc::new(Rgb32f::filled(src.width, src.height, [0.3, 0.3, 0.3]));
    assert_eq!(render_cached(&other, &info, &s, &full, &cache).image, render(&other, &info, &s, &full).image);
}

#[test]
fn exposure_after_spatial_filters_equals_exposing_the_source() {
    // The pipeline filters the image before exposure and applies exposure per pixel; filters on
    // log luminance are shift-equivariant, so this must equal scaling the source.
    let src = scene();
    let info = SourceInfo { raw: true, ..Default::default() };
    let mut s = typical_edits();
    s.light.exposure = 1.3;
    let a = render(&src, &info, &s, &RenderRequest::fit(200, 200)).image;
    let g = 2f32.powf(1.3);
    let scaled = src.map(|c| c.map(|v| v * g));
    s.light.exposure = 0.0;
    let b = render(&scaled, &info, &s, &RenderRequest::fit(200, 200)).image;
    let d = max_diff(&a, &b);
    assert!(d <= 2, "max difference {d}");
}
