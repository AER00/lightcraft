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
