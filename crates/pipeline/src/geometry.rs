//! Output framing: user orientation, crop + straighten, flips — and the mapping between output
//! pixels and normalized source coordinates (used by masks, spots and on-canvas tools).

use lightcraft_develop::DevelopSettings;
use lightcraft_geom::{Affine, CropGeometry, Orientation, Point, Rect};
use lightcraft_raster::resample::{Filter, resize};
use lightcraft_raster::{Rgb32f, par_rows};

/// The geometric frame of a render.
#[derive(Clone, Debug)]
pub struct Frame {
    /// Source (EXIF-oriented) dimensions in pixels.
    pub src_w: usize,
    pub src_h: usize,
    /// User orientation (Rotate Left/Right, Flip) on top of the source.
    pub orient: Orientation,
    /// Oriented dimensions.
    pub ow: f64,
    pub oh: f64,
    pub crop: CropGeometry,
    pub flip_h: bool,
    pub flip_v: bool,
}

impl Frame {
    pub fn new(src_w: usize, src_h: usize, s: &DevelopSettings, apply_crop: bool) -> Frame {
        let orient = s.orientation;
        let (ow, oh) = if orient.swaps_axes() { (src_h as f64, src_w as f64) } else { (src_w as f64, src_h as f64) };
        let crop = if apply_crop { s.crop.geometry } else { CropGeometry::default() };
        Frame { src_w, src_h, orient, ow, oh, crop, flip_h: apply_crop && s.crop.flip_h, flip_v: apply_crop && s.crop.flip_v }
    }

    /// Aspect ratio (w/h) of the output.
    pub fn aspect(&self) -> f64 {
        let r = self.crop.rect_px(self.ow, self.oh);
        r.width() / r.height()
    }

    pub fn fit(&self, max_w: usize, max_h: usize) -> (usize, usize) {
        let a = self.aspect();
        let (mw, mh) = (max_w.max(1) as f64, max_h.max(1) as f64);
        let (w, h) = if mw / mh > a { (mh * a, mh) } else { (mw, mw / a) };
        ((w.round() as usize).max(1), (h.round() as usize).max(1))
    }

    /// Output pixels per unit of the oriented image's long edge.
    pub fn px_per_long(&self, out_w: usize) -> f64 {
        let r = self.crop.rect_px(self.ow, self.oh);
        out_w as f64 / r.width() * self.ow.max(self.oh)
    }

    /// Affine from output pixel coordinates to oriented-image pixel coordinates.
    pub fn out_to_oriented(&self, out_w: usize, out_h: usize) -> Affine {
        let (w, h) = (out_w as f64, out_h as f64);
        let mut flip = Affine::IDENTITY;
        if self.flip_h {
            flip = Affine([-1.0, 0.0, 0.0, 1.0, w, 0.0]) * flip;
        }
        if self.flip_v {
            flip = Affine([1.0, 0.0, 0.0, -1.0, 0.0, h]) * flip;
        }
        self.crop.output_to_source(self.ow, self.oh, w, h) * flip
    }

    /// Map an output pixel to normalized oriented-image coordinates (0..1).
    pub fn out_to_norm(&self, out_w: usize, out_h: usize) -> Affine {
        Affine::scale(1.0 / self.ow, 1.0 / self.oh) * self.out_to_oriented(out_w, out_h)
    }

    /// Normalized oriented coords → output pixel (inverse of `out_to_norm`).
    pub fn norm_to_out(&self, out_w: usize, out_h: usize) -> Affine {
        self.out_to_norm(out_w, out_h).inverse().unwrap_or(Affine::IDENTITY)
    }

    /// Convert normalized oriented coordinates to "long-edge units" (isotropic), used by mask shapes.
    pub fn norm_to_long(&self, p: Point) -> Point {
        let l = self.ow.max(self.oh);
        Point::new(p.x * self.ow / l, p.y * self.oh / l)
    }

    /// Sample the source into a `w × h` output buffer: orientation, crop, rotation, flips — one
    /// bilinear resample from a pre-filtered (area-downscaled) copy, so minification never aliases.
    pub fn sample(&self, src: &Rgb32f, w: usize, h: usize) -> Rgb32f {
        let oriented = if self.orient == Orientation::Normal { None } else { Some(src.oriented(self.orient)) };
        let o = oriented.as_ref().unwrap_or(src);
        let crop_px = self.crop.rect_px(self.ow, self.oh);
        let k = crop_px.width() / w as f64;
        let (base, s) = if k > 1.25 {
            let nw = ((o.width as f64 / k).round() as usize).max(1);
            let nh = ((o.height as f64 / k).round() as usize).max(1);
            (resize(o, nw, nh, Filter::Mitchell), nw as f64 / o.width as f64)
        } else {
            (o.clone(), 1.0)
        };
        let xf = Affine::scale(s, base.height as f64 / o.height as f64) * self.out_to_oriented(w, h);
        let identity_like = self.crop.is_identity() && !self.flip_h && !self.flip_v && base.width == w && base.height == h;
        if identity_like {
            return base;
        }
        let mut out = Rgb32f::new(w, h);
        par_rows(&mut out.data, w, |y, row| {
            for (x, px) in row.iter_mut().enumerate() {
                let p = xf.apply(Point::new(x as f64 + 0.5, y as f64 + 0.5));
                *px = base.sample_bilinear(p.x as f32, p.y as f32);
            }
        });
        out
    }

    /// The crop rectangle as a quad in normalized oriented coordinates (for overlays).
    pub fn crop_quad_norm(&self) -> [Point; 4] {
        let r = self.crop.rect_px(self.ow, self.oh);
        let back = Affine::rotate_about(-self.crop.angle.to_radians(), Point::new(self.ow / 2.0, self.oh / 2.0));
        r.corners().map(|p| {
            let q = back.apply(p);
            Point::new(q.x / self.ow, q.y / self.oh)
        })
    }
}

/// Whole-image rectangle in normalized coordinates.
pub const FULL: Rect = Rect::UNIT;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_frame_is_identity() {
        let s = DevelopSettings::default();
        let f = Frame::new(300, 200, &s, true);
        assert_eq!(f.fit(3000, 3000), (3000, 2000));
        let m = f.out_to_norm(300, 200);
        let p = m.apply(Point::new(150.0, 100.0));
        assert!((p.x - 0.5).abs() < 1e-9 && (p.y - 0.5).abs() < 1e-9);
        assert!((f.px_per_long(300) - 300.0).abs() < 1e-9);
    }

    #[test]
    fn rotation_swaps_aspect() {
        let s = DevelopSettings { orientation: Orientation::Rotate90, ..Default::default() };
        let f = Frame::new(300, 200, &s, true);
        assert_eq!(f.fit(1000, 1000), (667, 1000));
    }

    #[test]
    fn crop_changes_aspect_and_mapping() {
        let mut s = DevelopSettings::default();
        s.crop.geometry.rect = Rect::new(0.5, 0.0, 1.0, 1.0);
        let f = Frame::new(400, 200, &s, true);
        assert_eq!(f.fit(1000, 1000), (1000, 1000));
        let p = f.out_to_norm(100, 100).apply(Point::new(0.0, 0.0));
        assert!((p.x - 0.5).abs() < 1e-9 && p.y.abs() < 1e-9);
        let back = f.norm_to_out(100, 100).apply(Point::new(0.75, 0.5));
        assert!((back.x - 50.0).abs() < 1e-6 && (back.y - 50.0).abs() < 1e-6);
    }

    #[test]
    fn flip_mirrors() {
        let mut s = DevelopSettings::default();
        s.crop.flip_h = true;
        let src = Rgb32f::from_fn(4, 1, |x, _| [x as f32, 0.0, 0.0]);
        let f = Frame::new(4, 1, &s, true);
        let out = f.sample(&src, 4, 1);
        assert!((out.get(0, 0)[0] - 3.0).abs() < 1e-5);
        assert!((out.get(3, 0)[0] - 0.0).abs() < 1e-5);
    }
}
