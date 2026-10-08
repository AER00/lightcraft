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
/// The most context rendered beyond the visible pixels (a multiple of [`SNAP`]).
pub const MAX_MARGIN: usize = 768;
/// The most a window may span on either axis: a bound on its texture whatever the window size.
pub const MAX_SPAN: usize = 6144;

/// A window render the loupe asked for: which photo, the size of the zoomed frame it is a window
/// of, and the window. The texture that comes back is drawn at this place of the frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RegionView {
    pub photo: lightcraft_catalog::PhotoId,
    pub key: u64,
    pub full: (usize, usize),
    pub window: PixelWindow,
    /// Hash of the develop settings it was rendered with.
    pub settings: u64,
}

impl RegionView {
    /// Whether this window's pixels still belong in a loupe showing `photo` with `settings` in a
    /// frame of `full` pixels (they are drawn until the next window arrives, if so).
    pub fn is_current(&self, photo: lightcraft_catalog::PhotoId, full: (usize, usize), settings: u64) -> bool {
        self.photo == photo && self.full == full && self.settings == settings
    }
}

/// Whether windows can be rendered for these settings. Spot removal reads its source patch from
/// outside the spot, and an automatic one is chosen from the pixels at hand: neither is the same
/// in a window as in the whole frame, so those photos stay on the whole-frame render.
pub fn windows_allowed(s: &lightcraft_develop::DevelopSettings) -> bool {
    s.spots.is_empty()
}

/// Context to render around the visible pixels of a frame `full_long` pixels along: enough for the
/// wide stages (base, clarity and dehaze planes have sigmas up to 0.02 of it) up to a cap.
pub fn margin_for(full_long: usize) -> usize {
    let want = (full_long as f64 * 0.045).min(MAX_MARGIN as f64) as usize;
    want.div_ceil(SNAP).max(1) * SNAP
}

/// Whether a view drawn `drawn_long` pixels along its long edge needs a window render on top of
/// the whole-frame render, which has only `rendered_long` of them.
pub fn needed(drawn_long: f32, rendered_long: usize) -> bool {
    drawn_long.is_finite() && drawn_long > rendered_long as f32 * 1.02 + 1.0
}

/// The window of a `full_w × full_h` frame to render so that `visible` — `(x0, y0, x1, y1)` in the
/// frame's pixels, as much of the frame as is on screen — is covered: expanded by a margin,
/// snapped outwards to the [`SNAP`] grid (margin: see [`margin_for`]) and clamped into the frame. `None`: nothing is visible.
pub fn window_for(full_w: usize, full_h: usize, visible: (f32, f32, f32, f32)) -> Option<PixelWindow> {
    if full_w == 0 || full_h == 0 || [visible.0, visible.1, visible.2, visible.3].iter().any(|v| !v.is_finite()) {
        return None;
    }
    let margin = margin_for(full_w.max(full_h));
    let axis = |lo: f32, hi: f32, full: usize| -> Option<(usize, usize)> {
        let (lo, hi) = (lo.max(0.0), hi.min(full as f32));
        if hi <= lo {
            return None;
        }
        let a = (lo as usize).saturating_sub(margin) / SNAP * SNAP;
        let b = ((hi.ceil() as usize).saturating_add(margin)).div_ceil(SNAP).saturating_mul(SNAP).min(full);
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
        let m = margin_for(48_000);
        assert!(w.x + m <= 20_000 && w.x + w.w >= 21_400 + m, "{w:?}");
        assert!(w.y + m <= 10_000 && w.y + w.h >= 10_900 + m, "{w:?}");
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

    // Given a deeper zoom, the margin grows with the wide stages' kernels (3σ of the base, clarity
    // and dehaze planes are 0.045 of the frame), up to a cap that bounds the window
    #[test]
    fn the_margin_grows_with_the_zoom_up_to_a_cap() {
        assert_eq!(margin_for(1000), SNAP, "never less than one grid step");
        assert!(margin_for(8000) > margin_for(3000));
        assert_eq!(margin_for(48_000), MAX_MARGIN);
        assert_eq!(margin_for(0), SNAP);
        assert!(margin_for(usize::MAX) <= MAX_MARGIN);
        for l in [100, 3000, 8000, 48_000, 400_000] {
            assert_eq!(margin_for(l) % SNAP, 0);
        }
    }

    // Given a tile rendered for other settings, another photo or another zoom, it is not drawn
    #[test]
    fn a_tile_is_drawn_only_while_it_shows_what_the_loupe_shows() {
        use lightcraft_catalog::PhotoId;
        let v = RegionView { photo: PhotoId(1), key: 7, full: (6000, 4000), window: PixelWindow { x: 0, y: 0, w: 256, h: 256 }, settings: 11 };
        assert!(v.is_current(PhotoId(1), (6000, 4000), 11));
        assert!(!v.is_current(PhotoId(2), (6000, 4000), 11), "another photo");
        assert!(!v.is_current(PhotoId(1), (6000, 4001), 11), "another zoom");
        assert!(!v.is_current(PhotoId(1), (6000, 4000), 12), "another look (an edit, an undo)");
    }

    // Given a photo with spot removal, no window render: a spot reads pixels beyond the window
    #[test]
    fn photos_with_spots_stay_on_the_whole_frame_render() {
        use lightcraft_develop::{DevelopSettings, Spot};
        let mut s = DevelopSettings::default();
        assert!(windows_allowed(&s));
        s.spots.push(Spot::default());
        assert!(!windows_allowed(&s));
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
