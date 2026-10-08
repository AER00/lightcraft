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
pub const MAX_SPAN: usize = if cfg!(target_arch = "wasm32") { 3072 } else { 6144 };

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
    ///
    /// While a slider is dragged (`drafting`) a window made for an earlier value of it still
    /// belongs: drafts follow the drag, and waiting for an exact match would show none.
    pub fn is_current(&self, photo: lightcraft_catalog::PhotoId, full: (usize, usize), settings: u64, drafting: bool) -> bool {
        self.photo == photo && self.full == full && (drafting || self.settings == settings)
    }
}

/// Context to render around the visible pixels of a frame `full_long` pixels along: enough for the
/// wide stages (base, clarity and dehaze planes have sigmas up to 0.02 of it) up to a cap.
pub fn margin_for(full_long: usize) -> usize {
    let want = (full_long as f64 * 0.045).min(MAX_MARGIN as f64) as usize;
    want.div_ceil(SNAP).max(1) * SNAP
}

/// The widest a window may be on a host whose GPU textures are at most `texture_side` px: a
/// window is one texture, and a texture over the limit makes some backends (egui_glow, i.e. the
/// browser build) panic. A multiple of [`SNAP`], at least one grid step, at most [`MAX_SPAN`].
pub fn max_span(texture_side: usize) -> usize {
    (texture_side.min(MAX_SPAN) / SNAP * SNAP).max(SNAP)
}

/// A window render is worth it only when the whole-frame render is this much smaller than the
/// zoomed frame: a little softness (a Retina fit view of 2800 px from the 2560 px preview) is not
/// worth decoding the original for.
pub const WINDOW_RATIO: f32 = 1.25;

/// How big the loupe's view of the photo is, and what the host allows.
#[derive(Clone, Copy, Debug)]
pub struct ViewSizes {
    /// Long edge of the photo as drawn (physical px): the zoomed frame.
    pub drawn_long: f32,
    /// Long edge of the canvas it is drawn in (physical px).
    pub canvas_long: f32,
    /// The photo's own long edge as shown (after crop and rotation).
    pub native_long: usize,
    /// The largest texture the GPU takes.
    pub texture_side: usize,
    /// 1.0, or less while a slider is dragged (a draft of the whole-frame render).
    pub draft_scale: f32,
}

/// What the loupe renders for a view: the whole frame at about canvas size, and, once the view is
/// zoomed past what that holds, the window on screen at no more than 100 %.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoupePlan {
    /// Long edge of the whole-frame render ([`Slot::Main`](crate::render::Slot::Main)): never more
    /// than the canvas shows, the user's preview limit, the texture side or the preview source level.
    pub main_edge: usize,
    /// Long edge of the zoomed frame the window is cut from (never above the photo's own pixels:
    /// beyond 100 % the GPU magnifies the window), or `None`: the whole-frame render is enough.
    pub window_edge: Option<usize>,
}

/// The loupe's render plan. The cost of a render follows the canvas, not the zoom: the whole-frame
/// render is canvas-sized, and the window holds only the pixels on screen at no more than 100 %.
pub fn plan(settings: &crate::state::AppSettings, v: ViewSizes) -> LoupePlan {
    let canvas_long = if v.canvas_long.is_finite() { v.canvas_long.max(8.0) } else { 8.0 };
    let drawn_long = if v.drawn_long.is_finite() { v.drawn_long.max(8.0) } else { canvas_long };
    let scale = if v.draft_scale.is_finite() { v.draft_scale.clamp(0.1, 1.0) } else { 1.0 };
    let main_edge = settings
        .loupe_edge(drawn_long.min(canvas_long) * scale, v.native_long, v.texture_side)
        .min(lightcraft_engine::SourceLevel::Preview.max_edge());
    let frame_edge = settings.window_frame_edge(drawn_long, v.native_long);
    // a user who capped the preview size has chosen speed over sharpness at fit
    let fit = drawn_long <= canvas_long * 1.05;
    let window_edge = (frame_edge as f32 > main_edge as f32 / scale * WINDOW_RATIO && !(fit && settings.preview_limit != 0)).then_some(frame_edge);
    LoupePlan { main_edge, window_edge }
}

/// Whether the picture on screen has the frame's aspect (to the one pixel a texture's whole-pixel
/// size is off by): a window is a part of the frame, so it can only be placed over a picture that
/// is the frame (an unsupported raw's embedded JPEG may be cropped differently).
pub fn same_aspect(shown: f32, frame: f32) -> bool {
    shown.is_finite() && frame.is_finite() && frame > 0.0 && (shown / frame - 1.0).abs() <= 0.02
}

/// Whether a view drawn `drawn_long` pixels along its long edge needs a window render on top of
/// the whole-frame render, which has only `rendered_long` of them.
pub fn needed(drawn_long: f32, rendered_long: usize) -> bool {
    drawn_long.is_finite() && drawn_long > rendered_long as f32 * 1.02 + 1.0
}

/// The window of a `full_w × full_h` frame to render so that `visible` — `(x0, y0, x1, y1)` in the
/// frame's pixels, as much of the frame as is on screen — is covered: expanded by a margin,
/// snapped outwards to the [`SNAP`] grid (margin: see [`margin_for`]) and clamped into the frame,
/// at most `max_span` ([`max_span`]) wide or high. `None`: nothing is visible.
pub fn window_for(full_w: usize, full_h: usize, visible: (f32, f32, f32, f32), max_span: usize) -> Option<PixelWindow> {
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
        if b - a > max_span {
            let mid = ((lo + hi) / 2.0) as usize;
            let a = mid.saturating_sub(max_span / 2) / SNAP * SNAP;
            return Some((a, (a + max_span).min(full)));
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
    use crate::state::AppSettings;

    const BIG: usize = 16384;

    fn sizes(drawn: f32, canvas: f32, native: usize) -> ViewSizes {
        ViewSizes { drawn_long: drawn, canvas_long: canvas, native_long: native, texture_side: BIG, draft_scale: 1.0 }
    }

    fn auto() -> AppSettings {
        AppSettings::default()
    }

    // Given a fit view on a Retina Mac (about 2800 px), the preview is enough: no original decoded
    #[test]
    fn a_retina_fit_view_needs_no_window() {
        let p = plan(&auto(), sizes(2800.0, 2800.0, 6000));
        assert_eq!(p.main_edge, 2560, "the preview source level");
        assert_eq!(p.window_edge, None);
    }

    // Given a 4K display, the fit view is 1.5x the preview: the window makes it sharp
    #[test]
    fn a_large_fit_view_gets_a_window() {
        let p = plan(&auto(), sizes(3840.0, 3840.0, 6000));
        assert_eq!((p.main_edge, p.window_edge), (2560, Some(3840)));
    }

    // Given 1:1 on a 24 MP photo in a 2800 px canvas, the whole frame stays canvas-sized and the
    // window is cut from the photo's own pixels
    #[test]
    fn one_to_one_renders_the_canvas_and_a_native_window() {
        let p = plan(&auto(), sizes(6000.0, 2800.0, 6000));
        assert_eq!((p.main_edge, p.window_edge), (2560, Some(6000)));
    }

    // Given 800 %, the window is still at 100 % (the GPU magnifies it): the cost does not grow
    #[test]
    fn beyond_one_to_one_the_window_stays_at_native_resolution() {
        let p = plan(&auto(), sizes(48000.0, 2800.0, 6000));
        assert_eq!((p.main_edge, p.window_edge), (2560, Some(6000)));
        let q = plan(&auto(), sizes(480.0 * 100.0, 2800.0, 6000));
        assert_eq!(q.window_edge, Some(6000));
    }

    // Given a small photo zoomed in, the whole-frame render already holds all its pixels
    #[test]
    fn a_small_photo_needs_no_window() {
        let p = plan(&auto(), sizes(8000.0, 1400.0, 1000));
        assert_eq!((p.main_edge, p.window_edge), (1000, None));
    }

    // Given a slider drag at 400 % (drafts at 0.6 scale), the window plan does not change: it is
    // the whole-frame draft that gets cheaper
    #[test]
    fn a_drag_shrinks_the_whole_frame_draft_not_the_window() {
        let still = plan(&auto(), sizes(24000.0, 2800.0, 6000));
        let drag = plan(&auto(), ViewSizes { draft_scale: 0.6, ..sizes(24000.0, 2800.0, 6000) });
        assert!(drag.main_edge < still.main_edge, "{drag:?} vs {still:?}");
        assert_eq!(drag.window_edge, still.window_edge);
    }

    // Given the user capped the preview size, the whole-frame render obeys it, and a fit view
    // stays on it (no original decoded); a zoomed view still gets its sharp window
    #[test]
    fn a_preview_limit_caps_the_whole_frame_render_and_spares_the_fit_view() {
        let s = AppSettings { preview_limit: 1600, ..Default::default() };
        let fit = plan(&s, sizes(2800.0, 2800.0, 6000));
        assert_eq!((fit.main_edge, fit.window_edge), (1600, None));
        let zoomed = plan(&s, sizes(6000.0, 2800.0, 6000));
        assert_eq!((zoomed.main_edge, zoomed.window_edge), (1600, Some(6000)));
    }

    // Given a GPU with 2048 px textures, the whole-frame render obeys it, the window frame need not
    // (a window is cut to the texture side separately)
    #[test]
    fn the_texture_side_caps_the_whole_frame_render_only() {
        let p = plan(&auto(), ViewSizes { texture_side: 2048, ..sizes(6000.0, 2800.0, 6000) });
        assert_eq!((p.main_edge, p.window_edge), (2048, Some(6000)));
    }

    // Given a picture that is not the frame (an embedded JPEG cropped differently), no window
    #[test]
    fn a_window_needs_a_picture_with_the_frames_aspect() {
        assert!(same_aspect(1.5, 1.5) && same_aspect(1.51, 1.5));
        assert!(!same_aspect(1.5, 1.0) && !same_aspect(f32::NAN, 1.0) && !same_aspect(1.0, 0.0));
    }

    // Given a drag at fit on a Retina Mac, the window does not switch on (it would flicker with the drag)
    #[test]
    fn a_drag_at_fit_does_not_switch_the_window_on() {
        let p = plan(&auto(), ViewSizes { draft_scale: 0.6, ..sizes(2800.0, 2800.0, 6000) });
        assert_eq!(p.window_edge, None);
    }

    // Hostile numbers give a plan, never a panic
    #[test]
    fn hostile_sizes_give_a_usable_plan() {
        for v in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -3.0, 0.0] {
            let p = plan(&auto(), ViewSizes { drawn_long: v, canvas_long: v, draft_scale: v, ..sizes(1.0, 1.0, 6000) });
            assert!((8..=2560).contains(&p.main_edge), "{v}: {p:?}");
        }
        let p = plan(&auto(), sizes(1000.0, 1000.0, 0));
        assert!(p.main_edge >= 8);
    }

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
        let w = window_for(48_000, 32_000, (20_000.0, 10_000.0, 21_400.0, 10_900.0), MAX_SPAN).unwrap();
        let m = margin_for(48_000);
        assert!(w.x + m <= 20_000 && w.x + w.w >= 21_400 + m, "{w:?}");
        assert!(w.y + m <= 10_000 && w.y + w.h >= 10_900 + m, "{w:?}");
        assert_eq!((w.x % SNAP, w.y % SNAP, w.w % SNAP, w.h % SNAP), (0, 0, 0, 0));
    }

    // Given a small pan, the same window is kept (no re-render); a big pan changes it
    #[test]
    fn small_pans_keep_the_window() {
        let a = window_for(48_000, 32_000, (20_000.0, 10_000.0, 21_400.0, 10_900.0), MAX_SPAN).unwrap();
        let b = window_for(48_000, 32_000, (20_040.0, 10_030.0, 21_440.0, 10_930.0), MAX_SPAN).unwrap();
        assert_eq!(a, b);
        let c = window_for(48_000, 32_000, (26_000.0, 10_000.0, 27_400.0, 10_900.0), MAX_SPAN).unwrap();
        assert_ne!(a, c);
    }

    // Given a frame smaller than the margin, or a view at its edge, the window is clamped into it
    #[test]
    fn the_window_stays_inside_the_frame() {
        let w = window_for(3000, 2000, (-500.0, -500.0, 900.0, 700.0), MAX_SPAN).unwrap();
        assert_eq!((w.x, w.y), (0, 0));
        let w = window_for(3000, 2000, (2500.0, 1500.0, 9000.0, 9000.0), MAX_SPAN).unwrap();
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
        assert!(v.is_current(PhotoId(1), (6000, 4000), 11, false));
        assert!(!v.is_current(PhotoId(2), (6000, 4000), 11, false), "another photo");
        assert!(!v.is_current(PhotoId(1), (6000, 4001), 11, false), "another zoom");
        assert!(!v.is_current(PhotoId(1), (6000, 4000), 12, false), "another look (an edit, an undo)");
        assert!(v.is_current(PhotoId(1), (6000, 4000), 12, true), "a drag's drafts follow the value");
        assert!(!v.is_current(PhotoId(2), (6000, 4000), 12, true), "…of this photo only");
    }

    // Given a GPU that allows 2048 px textures, no window is wider than that
    #[test]
    fn a_window_fits_the_texture_side() {
        for tex in [512, 2048, 4096] {
            let span = max_span(tex);
            assert!(span <= tex.max(SNAP) && span.is_multiple_of(SNAP), "{tex}: {span}");
            let w = window_for(48_000, 32_000, (20_000.0, 10_000.0, 21_400.0, 10_900.0), span).unwrap();
            assert!(w.w <= span && w.h <= span, "{tex}: {w:?}");
        }
        assert_eq!(max_span(100_000), MAX_SPAN);
        assert_eq!(max_span(0), SNAP);
    }

    // Given nothing visible, or hostile numbers, there is no window (never a panic)
    #[test]
    fn nothing_visible_or_hostile_input_is_no_window() {
        assert_eq!(window_for(1000, 1000, (2000.0, 0.0, 3000.0, 100.0), MAX_SPAN), None);
        assert_eq!(window_for(1000, 1000, (10.0, 10.0, 10.0, 90.0), MAX_SPAN), None);
        assert_eq!(window_for(0, 1000, (0.0, 0.0, 10.0, 10.0), MAX_SPAN), None);
        for v in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(window_for(1000, 1000, (v, 0.0, 10.0, 10.0), MAX_SPAN), None);
        }
        let w = window_for(usize::MAX / 2, usize::MAX / 2, (0.0, 0.0, 1e30, 1e30), MAX_SPAN).unwrap();
        assert!(w.w <= MAX_SPAN && w.h <= MAX_SPAN, "{w:?}");
    }

    // Given a view of an enormous area, the window is bounded
    #[test]
    fn a_window_is_bounded() {
        let w = window_for(100_000, 100_000, (0.0, 0.0, 90_000.0, 90_000.0), MAX_SPAN).unwrap();
        assert!(w.w <= MAX_SPAN && w.h <= MAX_SPAN);
    }
}
