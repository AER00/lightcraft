//! Sharp zoomed views (issue #323). The loupe renders the whole frame in one texture, which can't
//! be as large as a 1:1 or 8:1 view of a big photo. Past that size it also renders just the window
//! of the frame that is on screen, at the zoom scale, and draws it over the whole-frame render.
//!
//! This module is the geometry: when a window is needed and which one. Windows are snapped to a
//! grid and carry a margin, so panning by a few pixels re-uses the render and the spatial stages
//! (clarity, dehaze…) have the context they read around the visible edge.

use lightcraft_engine::pipeline::PixelWindow;

/// Windows start and end on multiples of this many pixels of the zoomed frame.
pub const SNAP: usize = 256;
/// Context rendered beyond the visible pixels (a multiple of [`SNAP`] after snapping).
pub const MARGIN: usize = 256;
/// The most a window may span on either axis: a bound on its texture whatever the window size.
pub const MAX_SPAN: usize = 8192;

/// A window render the loupe asked for: which photo, the size of the zoomed frame it is a window
/// of, and the window. The texture that comes back is drawn at this place of the frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RegionView {
    pub photo: lightcraft_catalog::PhotoId,
    pub key: u64,
    pub full: (usize, usize),
    pub window: PixelWindow,
}

/// Whether a view drawn `drawn_long` pixels along its long edge needs a window render on top of
/// the whole-frame render, which has only `rendered_long` of them.
pub fn needed(drawn_long: f32, rendered_long: usize) -> bool {
    drawn_long.is_finite() && drawn_long > rendered_long as f32 * 1.02 + 1.0
}

/// The window of a `full_w × full_h` frame to render so that `visible` — `(x0, y0, x1, y1)` in the
/// frame's pixels, as much of the frame as is on screen — is covered: expanded by [`MARGIN`],
/// snapped outwards to the [`SNAP`] grid and clamped into the frame. `None`: nothing is visible.
pub fn window_for(full_w: usize, full_h: usize, visible: (f32, f32, f32, f32)) -> Option<PixelWindow> {
    if full_w == 0 || full_h == 0 || [visible.0, visible.1, visible.2, visible.3].iter().any(|v| !v.is_finite()) {
        return None;
    }
    let axis = |lo: f32, hi: f32, full: usize| -> Option<(usize, usize)> {
        let (lo, hi) = (lo.max(0.0), hi.min(full as f32));
        if hi <= lo {
            return None;
        }
        let a = (lo as usize).saturating_sub(MARGIN) / SNAP * SNAP;
        let b = ((hi.ceil() as usize).saturating_add(MARGIN)).div_ceil(SNAP).saturating_mul(SNAP).min(full);
        // a window larger than MAX_SPAN keeps the visible part (centred on it), snapped
        if b - a > MAX_SPAN {
            let mid = ((lo + hi) / 2.0) as usize;
            let a = mid.saturating_sub(MAX_SPAN / 2) / SNAP * SNAP;
            return Some((a, (a + MAX_SPAN).min(full)));
        }
        Some((a, b))
    };
    let (x0, x1) = axis(visible.0, visible.2, full_w)?;
    let (y0, y1) = axis(visible.1, visible.3, full_h)?;
    Some(PixelWindow { x: x0, y: y0, w: x1 - x0, h: y1 - y0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Given a view drawn at 6000 px and a render of 6000, no window is needed; at 6000 vs 2560 it is
    #[test]
    fn a_window_is_needed_only_when_the_whole_frame_render_is_stretched() {
        assert!(!needed(6000.0, 6000));
        assert!(!needed(6100.0, 6000), "a couple of percent is not worth a second render");
        assert!(needed(6000.0, 2560));
        assert!(needed(48000.0, 8192));
        assert!(!needed(f32::NAN, 100) && !needed(f32::INFINITY, 100));
    }

    // Given a visible area in the middle of a big frame, the window covers it with a margin, on the grid
    #[test]
    fn the_window_covers_the_visible_area_with_a_margin_on_the_grid() {
        let w = window_for(48_000, 32_000, (20_000.0, 10_000.0, 21_400.0, 10_900.0)).unwrap();
        assert!(w.x <= 20_000 - MARGIN && w.x + w.w >= 21_400 + MARGIN, "{w:?}");
        assert!(w.y <= 10_000 - MARGIN && w.y + w.h >= 10_900 + MARGIN, "{w:?}");
        assert_eq!((w.x % SNAP, w.y % SNAP, w.w % SNAP, w.h % SNAP), (0, 0, 0, 0));
    }

    // Given a small pan, the same window is kept (no re-render); a big pan changes it
    #[test]
    fn small_pans_keep_the_window() {
        let a = window_for(48_000, 32_000, (20_000.0, 10_000.0, 21_400.0, 10_900.0)).unwrap();
        let b = window_for(48_000, 32_000, (20_040.0, 10_030.0, 21_440.0, 10_930.0)).unwrap();
        assert_eq!(a, b);
        let c = window_for(48_000, 32_000, (26_000.0, 10_000.0, 27_400.0, 10_900.0)).unwrap();
        assert_ne!(a, c);
    }

    // Given a frame smaller than the margin, or a view at its edge, the window is clamped into it
    #[test]
    fn the_window_stays_inside_the_frame() {
        let w = window_for(3000, 2000, (-500.0, -500.0, 900.0, 700.0)).unwrap();
        assert_eq!((w.x, w.y), (0, 0));
        let w = window_for(3000, 2000, (2500.0, 1500.0, 9000.0, 9000.0)).unwrap();
        assert!(w.x + w.w <= 3000 && w.y + w.h <= 2000, "{w:?}");
        assert_eq!((w.x + w.w, w.y + w.h), (3000, 2000));
    }

    // Given nothing visible, or hostile numbers, there is no window (never a panic)
    #[test]
    fn nothing_visible_or_hostile_input_is_no_window() {
        assert_eq!(window_for(1000, 1000, (2000.0, 0.0, 3000.0, 100.0)), None);
        assert_eq!(window_for(1000, 1000, (10.0, 10.0, 10.0, 90.0)), None);
        assert_eq!(window_for(0, 1000, (0.0, 0.0, 10.0, 10.0)), None);
        for v in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(window_for(1000, 1000, (v, 0.0, 10.0, 10.0)), None);
        }
        let w = window_for(usize::MAX / 2, usize::MAX / 2, (0.0, 0.0, 1e30, 1e30)).unwrap();
        assert!(w.w <= MAX_SPAN && w.h <= MAX_SPAN, "{w:?}");
    }

    // Given a view of an enormous area, the window is bounded
    #[test]
    fn a_window_is_bounded() {
        let w = window_for(100_000, 100_000, (0.0, 0.0, 90_000.0, 90_000.0)).unwrap();
        assert!(w.w <= MAX_SPAN && w.h <= MAX_SPAN);
    }
}
