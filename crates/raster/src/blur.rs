//! Gaussian-like blur by three successive box filters (running sums, O(1) per pixel per pass).

use crate::resample::Pixel;
use crate::{Image, par_rows};

/// Box radii approximating a Gaussian of `sigma` with 3 passes (Kovesi / Wells).
fn box_radii(sigma: f32) -> [usize; 3] {
    let n = 3.0f32;
    let w_ideal = (12.0 * sigma * sigma / n + 1.0).sqrt();
    let mut wl = w_ideal.floor() as i32;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wu = wl + 2;
    let m_ideal = (12.0 * sigma * sigma - n * (wl * wl) as f32 - 4.0 * n * wl as f32 - 3.0 * n) / (-4.0 * wl as f32 - 4.0);
    let m = m_ideal.round() as i32;
    std::array::from_fn(|i| (((if (i as i32) < m { wl } else { wu }) - 1) / 2).max(0) as usize)
}

fn box_h<T: Pixel>(src: &Image<T>, r: usize) -> Image<T> {
    let w = src.width;
    let mut out = Image::<T>::new(w, src.height);
    if r == 0 {
        return src.clone();
    }
    let inv = 1.0 / (2 * r + 1) as f32;
    par_rows(&mut out.data, w, |y, row| {
        let s = src.row(y);
        let at = |i: isize| s[i.clamp(0, w as isize - 1) as usize];
        let mut acc = T::zero();
        for i in -(r as isize)..=(r as isize) {
            acc = acc.madd(at(i), 1.0);
        }
        for x in 0..w {
            row[x] = T::zero().madd(acc, inv);
            acc = acc.madd(at(x as isize + r as isize + 1), 1.0).madd(at(x as isize - r as isize), -1.0);
        }
    });
    out
}

fn transpose<T: Pixel>(src: &Image<T>) -> Image<T> {
    let (w, h) = (src.width, src.height);
    let mut out = Image::<T>::new(h, w);
    par_rows(&mut out.data, h, |y, row| {
        for (x, o) in row.iter_mut().enumerate() {
            *o = src.data[x * w + y];
        }
    });
    out
}

/// Blur with Gaussian `sigma` (pixels). `sigma <= 0.3` returns a copy.
pub fn gaussian<T: Pixel>(img: &Image<T>, sigma: f32) -> Image<T> {
    if sigma <= 0.3 || img.is_empty() {
        return img.clone();
    }
    let radii = box_radii(sigma);
    let mut a = img.clone();
    for r in radii {
        a = box_h(&a, r);
    }
    let mut t = transpose(&a);
    for r in radii {
        t = box_h(&t, r);
    }
    transpose(&t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_constant_and_mass() {
        let img = Image::<f32>::filled(50, 30, 0.7);
        let b = gaussian(&img, 4.0);
        assert!(b.data.iter().all(|v| (v - 0.7).abs() < 1e-5));
        let mut imp = Image::<f32>::new(101, 101);
        imp.set(50, 50, 1.0);
        let b = gaussian(&imp, 5.0);
        let sum: f32 = b.data.iter().sum();
        assert!((sum - 1.0).abs() < 1e-3, "{sum}");
        // peak moved to the centre and symmetric
        assert!((b.get(45, 50) - b.get(55, 50)).abs() < 1e-6);
        assert!((b.get(50, 45) - b.get(50, 55)).abs() < 1e-6);
    }

    #[test]
    fn variance_close_to_sigma_squared() {
        let mut imp = Image::<f32>::new(201, 1);
        imp.set(100, 0, 1.0);
        let sigma = 8.0;
        let b = gaussian(&imp, sigma);
        let var: f32 = b.data.iter().enumerate().map(|(i, v)| v * ((i as f32 - 100.0).powi(2))).sum();
        assert!((var.sqrt() - sigma).abs() / sigma < 0.1, "{}", var.sqrt());
    }
}
