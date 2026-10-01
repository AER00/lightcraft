//! Geometry / optics tests on synthetic images with known distortions.

use lightcraft_develop::{DevelopSettings, EmbeddedLens, EmbeddedVignette, EmbeddedWarp};
use lightcraft_geom::Point;
use lightcraft_raster::resample::{Filter, resize};
use lightcraft_raster::{Rgb32f, Rgba8};

use crate::{RenderRequest, SourceInfo, render};

/// A light grid: dark lines every `step` px (soft, ~3 px wide) on a light background.
pub(crate) fn grid_value(p: Point, step: f64) -> f32 {
    let d = |v: f64| {
        let m = v.rem_euclid(step);
        m.min(step - m)
    };
    let dist = d(p.x).min(d(p.y));
    (0.05 + 0.75 * ((dist - 1.0) / 1.5).clamp(0.0, 1.0)) as f32
}

/// Render a source of `w × h` where each pixel shows `pattern` at `undistort(pixel centre)`.
pub(crate) fn synth(w: usize, h: usize, undistort: impl Fn(Point) -> Point + Sync, pattern: impl Fn(Point) -> [f32; 3] + Sync) -> Rgb32f {
    Rgb32f::from_fn(w, h, |x, y| pattern(undistort(Point::new(x as f64 + 0.5, y as f64 + 0.5))))
}

/// Sub-pixel x of the dark vertical line nearest `x0` on row `y` (green channel darkness centroid).
pub(crate) fn line_x(img: &Rgba8, y: usize, x0: f64, win: f64) -> f64 {
    let (a, b) = ((x0 - win).max(0.0) as usize, ((x0 + win) as usize).min(img.width - 1));
    let row: Vec<f64> = (a..=b).map(|x| img.get(x, y)[1] as f64).collect();
    let max = row.iter().cloned().fold(0.0, f64::max);
    let (mut s, mut sw) = (0.0, 0.0);
    for (i, v) in row.iter().enumerate() {
        let wgt = (max - v).max(0.0).powi(2);
        s += wgt * (a + i) as f64;
        sw += wgt;
    }
    s / sw.max(1e-9) + 0.5
}

/// Sub-pixel y of the dark horizontal line nearest `y0` in column `x`.
pub(crate) fn line_y(img: &Rgba8, x: usize, y0: f64, win: f64) -> f64 {
    let (a, b) = ((y0 - win).max(0.0) as usize, ((y0 + win) as usize).min(img.height - 1));
    let col: Vec<f64> = (a..=b).map(|y| img.get(x, y)[1] as f64).collect();
    let max = col.iter().cloned().fold(0.0, f64::max);
    let (mut s, mut sw) = (0.0, 0.0);
    for (i, v) in col.iter().enumerate() {
        let wgt = (max - v).max(0.0).powi(2);
        s += wgt * (a + i) as f64;
        sw += wgt;
    }
    s / sw.max(1e-9) + 0.5
}

fn spread(v: &[f64]) -> f64 {
    v.iter().cloned().fold(f64::MIN, f64::max) - v.iter().cloned().fold(f64::MAX, f64::min)
}

/// Barrel distortion with coefficient `k` (< 0): distorted radius = r·(1 + k·r²), r in half-diagonal units.
fn barrel_undistort(w: f64, h: f64, k: f64) -> impl Fn(Point) -> Point + Sync {
    let (cx, cy, hd) = (w / 2.0, h / 2.0, w.hypot(h) / 2.0);
    move |p: Point| {
        let (dx, dy) = ((p.x - cx) / hd, (p.y - cy) / hd);
        let rd = dx.hypot(dy);
        if rd < 1e-12 {
            return p;
        }
        // solve r·(1 + k r²) = rd (Newton)
        let mut r = rd;
        for _ in 0..20 {
            let f = r * (1.0 + k * r * r) - rd;
            r -= f / (1.0 + 3.0 * k * r * r);
        }
        let s = r / rd;
        Point::new(cx + dx * s * hd, cy + dy * s * hd)
    }
}

#[test]
fn manual_distortion_straightens_barrel_grid() {
    let (w, h) = (480usize, 320usize);
    // the Distortion slider at +60 corrects exactly this barrel
    let k = -0.2 * 0.6;
    let src = synth(w, h, barrel_undistort(w as f64, h as f64, k), |p| [grid_value(p, 40.0); 3]);
    let rows = [12usize, 150, 308];
    let measure = |s: &DevelopSettings| {
        let img = render(&src, &SourceInfo::default(), s, &RenderRequest::fit(w, h)).image;
        let xs: Vec<f64> = rows.iter().map(|&y| line_x(&img, y, 40.0, 15.0)).collect();
        let ys: Vec<f64> = [12usize, 220, 468].iter().map(|&x| line_y(&img, x, 40.0, 15.0)).collect();
        (spread(&xs), spread(&ys))
    };
    let (bx, by) = measure(&DevelopSettings::default());
    assert!(bx > 4.0 && by > 3.0, "the synthetic barrel should bend lines: {bx} {by}");
    let mut s = DevelopSettings::default();
    s.optics.distortion = 60.0;
    let (cx, cy) = measure(&s);
    assert!(cx < 1.0 && cy < 1.0, "corrected lines should be straight: {cx} {cy}");
}

/// CA: red plane magnified by `ar`, blue by `ab`, on a neutral disc grid.
fn ca_source(w: usize, h: usize, ar: f64, ab: f64) -> Rgb32f {
    let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
    let disc = move |p: Point| -> f32 {
        let cell = 40.0;
        let (fx, fy) = ((p.x / cell).rem_euclid(1.0) - 0.5, (p.y / cell).rem_euclid(1.0) - 0.5);
        let d = fx.hypot(fy) * cell - 11.0;
        (0.04 + 0.8 * (d / 1.2 + 0.5).clamp(0.0, 1.0)) as f32
    };
    Rgb32f::from_fn(w, h, |x, y| {
        let p = Point::new(x as f64 + 0.5, y as f64 + 0.5);
        let at = |a: f64| disc(Point::new(cx + (p.x - cx) / (1.0 + a), cy + (p.y - cy) / (1.0 + a)));
        [at(ar), at(0.0), at(ab)]
    })
}

fn fringe(img: &Rgba8) -> f64 {
    img.data.iter().map(|p| (p[0] as f64 - p[1] as f64).abs() + (p[2] as f64 - p[1] as f64).abs()).sum::<f64>() / img.len() as f64
}

#[test]
fn remove_ca_aligns_colour_planes() {
    let (w, h) = (640usize, 420usize);
    let src = ca_source(w, h, 0.005, -0.004);
    let before = render(&src, &SourceInfo::default(), &DevelopSettings::default(), &RenderRequest::fit(w, h)).image;
    let mut s = DevelopSettings::default();
    s.optics.remove_ca = true;
    let after = render(&src, &SourceInfo::default(), &s, &RenderRequest::fit(w, h)).image;
    let (fb, fa) = (fringe(&before), fringe(&after));
    assert!(fa < fb * 0.35, "fringing {fb:.2} → {fa:.2}");
    // manual sliders can do the same: +ca_red shrinks... the red plane is sampled further out
    let mut m = DevelopSettings::default();
    m.optics.ca_red = 0.005 / 0.003 * 100.0;
    m.optics.ca_blue = -0.004 / 0.003 * 100.0;
    let manual = render(&src, &SourceInfo::default(), &m, &RenderRequest::fit(w, h)).image;
    assert!(fringe(&manual) < fb * 0.35, "manual {:.2}", fringe(&manual));
}

#[test]
fn profile_corrections_use_embedded_lens_only_when_enabled() {
    let (w, h) = (300usize, 200usize);
    let src = synth(w, h, |p| p, |p| [grid_value(p, 30.0); 3]);
    let lens = EmbeddedLens {
        warp: Some(EmbeddedWarp { planes: [[1.0, -0.08, 0.0, 0.0, 0.0, 0.0]; 3], center: Point::new(0.5, 0.5), radius: 0.6 }),
        vignette: Some(EmbeddedVignette { k: [0.5, 0.0, 0.0, 0.0, 0.0], center: Point::new(0.5, 0.5), radius: 0.6 }),
    };
    let info = SourceInfo { lens: Some(lens), ..SourceInfo::default() };
    let mut s = DevelopSettings::default();
    let off = render(&src, &info, &s, &RenderRequest::fit(w, h)).image;
    let plain = render(&src, &SourceInfo::default(), &s, &RenderRequest::fit(w, h)).image;
    assert_eq!(off.data, plain.data, "profile corrections off: embedded data ignored");
    s.optics.lens_profile = true;
    let on = render(&src, &info, &s, &RenderRequest::fit(w, h)).image;
    assert_ne!(on.data, off.data);
    // corners brightened by the embedded vignette correction
    assert!(on.get(2, 2)[1] as i32 >= off.get(2, 2)[1] as i32);
    s.optics.profile_distortion = 0.0;
    s.optics.profile_vignetting = 0.0;
    let zero = render(&src, &info, &s, &RenderRequest::fit(w, h)).image;
    assert_eq!(zero.data, off.data, "0 % = no correction");
}

fn mean_abs_diff_downscaled(small: &Rgba8, big: &Rgba8) -> f32 {
    let down = resize(&big.to_linear(), small.width, small.height, Filter::Box).to_srgb8();
    small.data.iter().zip(&down.data).map(|(a, b)| (0..3).map(|i| (a[i] as f32 - b[i] as f32).abs()).sum::<f32>() / 3.0).sum::<f32>()
        / small.len() as f32
}

#[test]
fn optics_preview_matches_export() {
    let src = lightcraft_scenes::demo_library()[2].render(960, 640);
    let mut s = DevelopSettings::default();
    s.optics.distortion = 40.0;
    s.optics.vignetting = 60.0;
    s.optics.vignetting_midpoint = 30.0;
    s.optics.ca_red = 30.0;
    s.optics.defringe_purple_amount = 8.0;
    let small = render(&src, &SourceInfo::default(), &s, &RenderRequest::fit(240, 240)).image;
    let big = render(&src, &SourceInfo::default(), &s, &RenderRequest::fit(960, 960)).image;
    let err = mean_abs_diff_downscaled(&small, &big);
    assert!(err < 4.0, "mean abs error {err}");
}

// ------------------------------------------------------------------------------------------ M4.3 transforms

/// Source whose content is a straight grid after the forward transform `h` (centred coordinates of a `w × h`
/// image): each source pixel shows the grid at its transformed position.
fn keystoned_grid(w: usize, h: usize, fwd: lightcraft_geom::Homography, step: f64) -> Rgb32f {
    let l = w.max(h) as f64 / 2.0;
    let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
    synth(
        w,
        h,
        move |p| {
            let q = fwd.apply(Point::new((p.x - cx) / l, (p.y - cy) / l));
            Point::new(cx + q.x * l, cy + q.y * l)
        },
        move |p| [grid_value(p, step); 3],
    )
}

#[test]
fn manual_vertical_keystone_widens_the_top() {
    let g = lightcraft_develop::Geometry { vertical: -50.0, ..Default::default() };
    let hm = crate::transform::manual(&g);
    let (tl, tr) = (hm.apply(Point::new(-1.0, -0.66)), hm.apply(Point::new(1.0, -0.66)));
    let (bl, br) = (hm.apply(Point::new(-1.0, 0.66)), hm.apply(Point::new(1.0, 0.66)));
    assert!(tr.x - tl.x > (br.x - bl.x) * 1.15, "top {} bottom {}", tr.x - tl.x, br.x - bl.x);
    let c = hm.apply(Point::new(0.0, 0.0));
    assert!(c.dist(Point::new(0.0, 0.0)) < 1e-9, "centre stays put");
    let g = lightcraft_develop::Geometry { horizontal: -50.0, ..Default::default() };
    let hm = crate::transform::manual(&g);
    let (lt, lb) = (hm.apply(Point::new(-0.9, -0.6)), hm.apply(Point::new(-0.9, 0.6)));
    let (rt, rb) = (hm.apply(Point::new(0.9, -0.6)), hm.apply(Point::new(0.9, 0.6)));
    assert!(lb.y - lt.y > rb.y - rt.y, "left side enlarged");
}

#[test]
fn manual_transforms_straighten_known_keystone() {
    let (w, h) = (480usize, 320usize);
    for g in [
        lightcraft_develop::Geometry { vertical: -40.0, ..Default::default() },
        lightcraft_develop::Geometry { horizontal: 35.0, rotate: 3.0, ..Default::default() },
        lightcraft_develop::Geometry { vertical: 25.0, aspect: 30.0, scale: 110.0, offset_x: 6.0, offset_y: -4.0, ..Default::default() },
    ] {
        // the source was "shot" with the inverse transform; correcting with `g` must give the straight grid back
        let fwd = crate::transform::manual(&g);
        let src = keystoned_grid(w, h, fwd, 40.0);
        let s = DevelopSettings { geometry: g.clone(), ..Default::default() };
        let img = render(&src, &SourceInfo::default(), &s, &RenderRequest::fit(w, h)).image;
        let xs: Vec<f64> = [60usize, 150, 260].iter().map(|&y| line_x(&img, y, 200.0, 15.0)).collect();
        let ys: Vec<f64> = [60usize, 220, 420].iter().map(|&x| line_y(&img, x, 200.0, 15.0)).collect();
        assert!(spread(&xs) < 1.0 && (xs[0] - 200.0).abs() < 1.0, "{g:?}: vertical line at {xs:?}");
        assert!(spread(&ys) < 1.0 && (ys[0] - 200.0).abs() < 1.0, "{g:?}: horizontal line at {ys:?}");
        // and without the correction the lines are not straight/where they belong
        let raw = render(&src, &SourceInfo::default(), &DevelopSettings::default(), &RenderRequest::fit(w, h)).image;
        let rx: Vec<f64> = [60usize, 150, 260].iter().map(|&y| line_x(&raw, y, 200.0, 15.0)).collect();
        let ry: Vec<f64> = [60usize, 220, 420].iter().map(|&x| line_y(&raw, x, 200.0, 15.0)).collect();
        assert!(spread(&rx) > 1.5 || spread(&ry) > 1.5 || (rx[0] - 200.0).abs() > 1.5, "{g:?}: source should be distorted");
    }
}

fn blank_fraction(img: &Rgba8) -> f64 {
    let blank =
        crate::render(&Rgb32f::filled(4, 4, crate::geometry::BLANK), &SourceInfo::default(), &DevelopSettings::default(), &RenderRequest::fit(4, 4))
            .image
            .get(1, 1);
    img.data.iter().filter(|p| (0..3).all(|i| (p[i] as i32 - blank[i] as i32).abs() <= 1)).count() as f64 / img.len() as f64
}

#[test]
fn constrain_crop_removes_blank_areas() {
    let src = Rgb32f::from_fn(300, 200, |x, y| [0.05 + 0.002 * x as f32, 0.1 + 0.002 * y as f32, 0.3]);
    let mut s = DevelopSettings::default();
    s.geometry.vertical = -60.0;
    s.geometry.rotate = 4.0;
    let free = render(&src, &SourceInfo::default(), &s, &RenderRequest::fit(300, 300)).image;
    assert!(blank_fraction(&free) > 0.03, "keystone leaves blank corners: {}", blank_fraction(&free));
    s.geometry.constrain_crop = true;
    let c = render(&src, &SourceInfo::default(), &s, &RenderRequest::fit(300, 300)).image;
    assert_eq!((c.width, c.height), (free.width, free.height), "constrain keeps the aspect");
    assert!(blank_fraction(&c) < 0.002, "constrained: {}", blank_fraction(&c));
}

#[test]
fn transforms_preview_matches_export() {
    let src = lightcraft_scenes::demo_library()[4].render(960, 640);
    let mut s = DevelopSettings::default();
    s.geometry.vertical = -30.0;
    s.geometry.horizontal = 15.0;
    s.geometry.rotate = 2.5;
    s.geometry.scale = 105.0;
    s.geometry.constrain_crop = true;
    let small = render(&src, &SourceInfo::default(), &s, &RenderRequest::fit(240, 240)).image;
    let big = render(&src, &SourceInfo::default(), &s, &RenderRequest::fit(960, 960)).image;
    assert_eq!(small.width * 4, big.width);
    let err = mean_abs_diff_downscaled(&small, &big);
    assert!(err < 4.0, "mean abs error {err}");
}
