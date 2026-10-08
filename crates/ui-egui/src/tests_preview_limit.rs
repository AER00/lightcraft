//! Issue #323: the Detail view must not be softened by a hidden render-size cap. By default the
//! loupe renders at the size it is drawn at (up to the photo's own pixels and a safety ceiling);
//! "Preview size" in Settings only lowers that when the user asks for it.

use crate::state::{AppSettings, LOUPE_EDGE_CEILING, PREVIEW_LIMITS, STANDARD_PREVIEW_EDGE};

/// A texture side no GPU we run on refuses (see the tests of small ones below).
const BIG: usize = 16384;

fn with_limit(preview_limit: u32) -> AppSettings {
    AppSettings { preview_limit, ..Default::default() }
}

// Given a fresh install
#[test]
fn default_is_automatic() {
    assert_eq!(AppSettings::default().preview_limit, 0);
    assert_eq!(PREVIEW_LIMITS[0], 0, "Automatic is the first choice");
}

// Given Automatic, when a 24 MP photo is shown at 1:1, then the render is the photo's own width
#[test]
fn automatic_renders_a_one_to_one_view_at_native_size() {
    assert_eq!(with_limit(0).loupe_edge(6000.0, 6000, BIG), 6000);
}

// Given Automatic, a Retina fit view of 3024 px is not held to the old 2560 px cap
#[test]
fn automatic_does_not_cap_a_fit_view_at_the_old_default() {
    assert_eq!(with_limit(0).loupe_edge(3024.0, 6000, BIG), 3072, "rounded up to the next 64 px step");
}

// Dragging a window edge must not re-render (and re-key the caches) on every pixel
#[test]
fn render_sizes_move_in_steps() {
    let s = with_limit(0);
    assert_eq!(s.loupe_edge(2945.0, 6000, BIG), s.loupe_edge(3008.0, 6000, BIG));
    assert_ne!(s.loupe_edge(3008.0, 6000, BIG), s.loupe_edge(3009.0, 6000, BIG));
}

// Neighbour prefetch and hover / before stand-ins stay at the preview level: a full-size decode
// would evict the open photo's single full-resolution source (review of #323)
#[test]
fn prefetch_and_stand_ins_never_ask_for_the_full_source_level() {
    use lightcraft_engine::SourceLevel;
    let s = with_limit(0);
    for wanted in [1000.0, 2560.0, 3024.0, 6000.0, 48000.0] {
        assert!(s.prefetch_edge(wanted, 6000, BIG) <= SourceLevel::Preview.max_edge(), "{wanted}");
        assert!(s.stand_in_edge(s.loupe_edge(wanted, 6000, BIG), BIG) <= SourceLevel::Preview.max_edge());
    }
    assert_eq!(s.prefetch_edge(1000.0, 6000, BIG), 1024, "a small view is not raised");
}

// Given Automatic, a small photo zoomed far in is not rendered above its own pixels
#[test]
fn never_renders_above_the_photos_own_pixels() {
    assert_eq!(with_limit(0).loupe_edge(8000.0, 1000, BIG), 1000);
}

// Given Automatic, a huge zoom on a huge photo stays under the memory/GPU ceiling
#[test]
fn automatic_is_bounded_by_the_ceiling() {
    assert_eq!(with_limit(0).loupe_edge(48000.0, 12000, BIG), LOUPE_EDGE_CEILING as usize);
}

// Given the user chose 1600 px, the render never exceeds it
#[test]
fn an_explicit_limit_caps_the_render() {
    assert_eq!(with_limit(1600).loupe_edge(6000.0, 6000, BIG), 1600);
    assert_eq!(with_limit(1600).loupe_edge(900.0, 6000, BIG), 960, "and does not raise a small view past its step");
}

// Given a browser whose GPU allows 2048 px textures (egui_glow panics on a bigger one), no render
// the loupe asks for is larger than that: the loupe, its neighbours' prefetch and the stand-ins
#[test]
fn nothing_is_rendered_larger_than_the_gpu_texture_side() {
    let s = with_limit(0);
    for tex in [512, 2048, 4096, 8192] {
        for wanted in [100.0, 2000.0, 6000.0, 48000.0, f32::INFINITY] {
            let e = s.loupe_edge(wanted, 12000, tex);
            assert!(e <= tex, "loupe {e} > {tex}");
            assert!(s.prefetch_edge(wanted, 12000, tex) <= tex);
            assert!(s.stand_in_edge(e, tex) <= tex);
        }
    }
    assert_eq!(s.loupe_edge(6000.0, 6000, 2048), 2048);
}

// A host that reports nothing sensible gets the smallest safe sizes, never a panic or zero
#[test]
fn a_nonsense_texture_side_is_clamped() {
    let s = with_limit(0);
    for tex in [0, 1, 7] {
        let e = s.loupe_edge(1000.0, 6000, tex);
        assert!((8..=512).contains(&e), "{tex}: {e}");
    }
}

// Whatever the inputs (hostile or degenerate), the result is a usable size
#[test]
fn degenerate_inputs_give_a_usable_size() {
    let s = with_limit(0);
    for wanted in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -5.0, 0.0] {
        let e = s.loupe_edge(wanted, 6000, BIG);
        assert!((8..=LOUPE_EDGE_CEILING as usize).contains(&e), "{wanted}: {e}");
    }
    assert!((8..=LOUPE_EDGE_CEILING as usize).contains(&s.loupe_edge(1000.0, 0, BIG)) && s.prefetch_edge(f32::NAN, 0, BIG) >= 8);
    assert!(with_limit(7).loupe_edge(1000.0, 6000, BIG) >= 8, "a corrupt limit is clamped");
}

// Build Standard-Sized Previews needs a number even when the limit is Automatic
#[test]
fn standard_previews_use_the_limit_or_the_standard_size() {
    assert_eq!(with_limit(0).standard_preview_edge(), STANDARD_PREVIEW_EDGE);
    assert_eq!(with_limit(3840).standard_preview_edge(), 3840);
}

// Given a ui.json from before this setting (key previewEdge), when it loads, then the user is on
// Automatic: the old default of 2560 was the bug, and the key no longer means anything
#[test]
fn an_old_saved_preview_edge_does_not_pin_the_cap() {
    let s: AppSettings = serde_json::from_str(r#"{"previewEdge": 2560}"#).unwrap();
    assert_eq!(s.preview_limit, 0);
}

// A saved limit is kept; an unknown one (hand-edited file) falls back to Automatic
#[test]
fn saved_limits_round_trip_and_unknown_ones_are_reset() {
    let mut ui = crate::UiState::default();
    ui.settings.preview_limit = 3840;
    let back = serde_json::from_str::<crate::UiState>(&serde_json::to_string(&ui).unwrap()).unwrap().sanitized();
    assert_eq!(back.settings.preview_limit, 3840);
    ui.settings.preview_limit = 123;
    assert_eq!(ui.sanitized().settings.preview_limit, 0);
}

// ---- Behaviour in the running UI -------------------------------------------------------------

mod in_the_loupe {
    use std::time::Duration;

    use serde_json::json;

    use crate::headless::Headless;
    use crate::render::Slot;
    use crate::{LightcraftApp, Services};

    const T: Duration = Duration::from_secs(20);
    const SETTLE: Duration = Duration::from_secs(120);

    /// The demo library in a 1400×900 window, the first photo open in Detail.
    fn detail() -> (Headless, usize) {
        let services = Services { png: None, ..Default::default() };
        let app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), services);
        let mut h = Headless::new(app, [1400.0, 900.0], 1.0);
        let r = h.request("ui.set", json!({"view": "detail", "right": "none", "filmstrip": false}), T);
        assert_eq!(r["ok"], true, "{r}");
        h.settle(SETTLE);
        let native = {
            let id = h.app.session.active().expect("a photo is open");
            let p = h.app.session.catalog.photo(id).expect("it is in the catalog");
            p.width.max(p.height) as usize
        };
        (h, native)
    }

    fn rendered_long_edge(h: &Headless) -> usize {
        let t = h.app.renderer.textures.get(&Slot::Main).expect("the loupe rendered");
        t.size[0].max(t.size[1])
    }

    // Given a photo larger than the canvas, when I zoom to 1:1, then the whole frame stays about
    // canvas-sized (a drag redoes that, not 24 MP) and the pixels on screen come from a window
    // cut from the photo's own pixels
    #[test]
    fn one_to_one_is_a_canvas_sized_frame_plus_a_native_window() {
        let (mut h, native) = detail();
        h.request("engine.execute", json!({"command": "view.zoom100"}), T);
        h.settle(SETTLE);
        let main = rendered_long_edge(&h);
        assert!(main <= 2560 && main < native, "native {native}: the whole-frame render is {main}");
        let region = h.app.region_view.expect("a window render");
        assert_eq!(region.full.0.max(region.full.1), native, "the window is cut from the photo's own pixels");
        let (w, hh) = region_tile(&h).expect("its texture");
        assert!(w >= 1300 && hh >= 700, "it covers the canvas: {w}×{hh}");
    }

    // Given the user capped the preview at 1600 px, then 1:1 is rendered no larger than that
    #[test]
    fn an_explicit_limit_still_caps_one_to_one() {
        let (mut h, native) = detail();
        assert!(native > 1600, "the demo photo must be larger than the cap for this to mean anything (is {native})");
        h.app.ui.settings.preview_limit = 1600;
        h.request("engine.execute", json!({"command": "view.zoom100"}), T);
        h.settle(SETTLE);
        assert!(rendered_long_edge(&h) <= 1600, "rendered {}", rendered_long_edge(&h));
    }

    fn region_tile(h: &Headless) -> Option<(usize, usize)> {
        h.app.renderer.textures.get(&Slot::Region).map(|t| (t.size[0], t.size[1]))
    }

    // Given a fit view, the whole-frame render is all there is: no second render, no original decoded
    #[test]
    fn a_fit_view_makes_no_window_render() {
        let (h, _) = detail();
        assert_eq!(region_tile(&h), None);
        assert_eq!(h.app.region_view, None);
    }

    // Given the user capped the preview at 1600 px, when I zoom to 1:1, then the visible part is
    // rendered on its own at the zoom scale and drawn over the whole-frame render
    #[test]
    fn a_capped_one_to_one_view_gets_a_sharp_window() {
        let (mut h, native) = detail();
        h.app.ui.settings.preview_limit = 1600;
        h.request("engine.execute", json!({"command": "view.zoom100"}), T);
        h.settle(SETTLE);
        let (w, hh) = region_tile(&h).expect("a window render");
        // it covers what is on screen (the 1400×900 window less its chrome), and is not the frame
        assert!(w >= 1300 && hh >= 700, "{w}×{hh}");
        assert!(w < native && hh < native, "{w}×{hh} of a {native} px frame");
        let region = h.app.region_view.expect("the window is described for the inspector");
        assert_eq!(region.full.0.max(region.full.1), native, "1:1 means the frame is the photo's own size");
    }

    // Given 800 % zoom (a frame of 48000 px), the window is rendered and bounded
    #[test]
    fn eight_to_one_renders_only_what_is_visible() {
        let (mut h, native) = detail();
        h.app.ui.zoom = crate::state::Zoom::Percent(800.0);
        h.settle(SETTLE);
        let (w, hh) = region_tile(&h).expect("a window render");
        assert!(w.max(hh) <= crate::region::MAX_SPAN && w < native * 8 / 2, "{w}×{hh}");
        // …and panning far away asks for another window
        let before = h.app.region_view.unwrap().window;
        h.app.ui.pan = (0.9, 0.9);
        h.settle(SETTLE);
        assert_ne!(h.app.region_view.unwrap().window, before);
    }

    // The window is drawn where it belongs: its pixels agree with the whole-frame render at the
    // same place of the frame (a shift, a flip or a wrong scale would not)
    #[test]
    fn the_window_lines_up_with_the_whole_frame_render() {
        let (mut h, _) = detail();
        h.app.renderer.keep_pixels = true;
        h.app.ui.settings.preview_limit = 1600;
        h.request("engine.execute", json!({"command": "view.zoom100"}), T);
        h.settle(SETTLE);
        let view = h.app.region_view.expect("a window render");
        let tile = h.app.renderer.textures.get(&Slot::Region).and_then(|t| t.pixels.clone()).expect("tile pixels");
        let main = h.app.renderer.textures.get(&Slot::Main).and_then(|t| t.pixels.clone()).expect("main pixels");
        let (mut sum, mut n) = (0.0f32, 0.0f32);
        for j in (8..tile.size[1] - 8).step_by(37) {
            for i in (8..tile.size[0] - 8).step_by(37) {
                // the same point of the frame in the whole-frame render
                let u = (view.window.x + i) as f32 / view.full.0 as f32;
                let v = (view.window.y + j) as f32 / view.full.1 as f32;
                let (mx, my) =
                    (((u * main.size[0] as f32) as usize).min(main.size[0] - 1), ((v * main.size[1] as f32) as usize).min(main.size[1] - 1));
                let (a, b) = (tile.pixels[j * tile.size[0] + i], main.pixels[my * main.size[0] + mx]);
                sum += (0..3).map(|k| (a.to_array()[k] as f32 - b.to_array()[k] as f32).abs()).sum::<f32>() / 3.0;
                n += 1.0;
            }
        }
        assert!(sum / n < 6.0, "mean difference {:.2} / 255 between the window and the frame render", sum / n);
    }

    // Given a Fit view, a slider drag (whose Main draft is at 0.6 scale) does not start window renders
    #[test]
    fn a_slider_drag_at_fit_makes_no_window_render() {
        let (mut h, _) = detail();
        h.app.session.begin_interaction("Exposure").unwrap();
        for _ in 0..5 {
            h.step();
        }
        assert_eq!(h.app.region_view, None);
        assert_eq!(region_tile(&h), None);
    }

    // Given a zoomed view with a window render, when I zoom back to Fit, then the window's texture is freed
    #[test]
    fn the_window_texture_is_freed_when_it_is_not_needed() {
        let (mut h, _) = detail();
        h.app.ui.settings.preview_limit = 1600;
        h.request("engine.execute", json!({"command": "view.zoom100"}), T);
        h.settle(SETTLE);
        assert!(region_tile(&h).is_some());
        h.request("engine.execute", json!({"command": "view.zoomFit"}), T);
        h.settle(SETTLE);
        assert_eq!(region_tile(&h), None);
        assert_eq!(h.app.region_view, None);
    }

    // Given a zoomed view, when the photo is edited, then the old window (made with the old look)
    // is no longer current and is replaced by one for the new look
    #[test]
    fn an_edit_replaces_the_window() {
        let (mut h, _) = detail();
        h.app.ui.settings.preview_limit = 1600;
        h.request("engine.execute", json!({"command": "view.zoom100"}), T);
        h.settle(SETTLE);
        let first = h.app.region_view.expect("a window");
        h.request("engine.execute", json!({"command": "develop.set", "params": {"control": "light.exposure", "value": 1.0}}), T);
        h.settle(SETTLE);
        let second = h.app.region_view.expect("a window");
        assert_ne!(first.settings, second.settings);
        assert_ne!(first.key, second.key);
        assert_eq!(h.app.renderer.textures.get(&Slot::Region).map(|t| t.key), Some(second.key), "the new window arrived");
    }

    // Given a photo with spot removal, a zoomed view still gets its sharp window
    #[test]
    fn a_photo_with_spots_gets_a_window_too() {
        let (mut h, _) = detail();
        let id = h.app.session.active().unwrap();
        let mut s = (*h.app.session.develop_of(id).unwrap()).clone();
        s.spots.push(lightcraft_develop::Spot {
            points: vec![lightcraft_geom::Point::new(0.5, 0.5)],
            size: 0.01,
            source_offset: Some(lightcraft_geom::Point::new(0.05, 0.0)),
            ..Default::default()
        });
        h.app.session.set_develop(id, s, "Spot").unwrap();
        h.app.ui.settings.preview_limit = 1600;
        h.request("engine.execute", json!({"command": "view.zoom100"}), T);
        h.settle(SETTLE);
        assert!(region_tile(&h).is_some(), "a window render");
    }

    // Given a browser GPU that takes only 2048 px textures (egui_glow panics on a bigger one), when
    // I zoom to 1:1 and 8:1, then no texture the loupe holds is larger than that
    #[test]
    fn no_loupe_texture_exceeds_the_gpu_limit() {
        let (mut h, _) = detail();
        h.max_texture_side = 2048;
        h.request("engine.execute", json!({"command": "view.zoom100"}), T);
        h.settle(SETTLE);
        for zoom in [crate::state::Zoom::Fit, crate::state::Zoom::Percent(100.0), crate::state::Zoom::Percent(800.0)] {
            h.app.ui.zoom = zoom;
            h.settle(SETTLE);
            for (slot, t) in &h.app.renderer.textures {
                assert!(t.size[0] <= 2048 && t.size[1] <= 2048, "{zoom:?} {slot:?}: {}×{}", t.size[0], t.size[1]);
            }
            let main = h.app.renderer.textures.get(&Slot::Main).expect("loupe");
            assert!(main.size[0].max(main.size[1]) >= 700, "still a real render, not a thumbnail: {:?}", main.size);
        }
    }

    // Given 400 % on a big photo, when a slider is dragged, then the work per frame follows the
    // canvas, not the zoom: a draft of the whole frame at canvas scale and a draft of the window
    #[test]
    fn a_slider_drag_when_zoomed_in_renders_canvas_sized_drafts() {
        let (mut h, native) = detail();
        h.app.ui.zoom = crate::state::Zoom::Percent(400.0);
        h.settle(SETTLE);
        h.app.session.begin_interaction("Exposure").unwrap();
        h.request("engine.execute", json!({"command": "develop.set", "params": {"control": "light.exposure", "value": 0.5}}), T);
        h.settle(SETTLE);
        let main = rendered_long_edge(&h);
        assert!(main <= 1000 && main * 4 < native, "the whole-frame draft is {main} px for a {native} px photo");
        let region = h.app.region_view.expect("the window is drafted too");
        assert_eq!(region.full.0.max(region.full.1), native, "at 100 %, magnified by the GPU");
        let (w, hh) = region_tile(&h).expect("a window draft");
        assert!(w * hh <= 3_000_000, "{w}×{hh}: the window holds what is on screen, not the frame");
    }

    fn tile(h: &Headless, slot: Slot) -> Option<(usize, usize)> {
        h.app.renderer.textures.get(&slot).map(|t| (t.size[0], t.size[1]))
    }

    // Given Before/After side by side at 1:1, both sides are sharp windows of the same frame: the
    // Before is not a 2560 px stand-in beside a full resolution After
    #[test]
    fn side_by_side_at_one_to_one_compares_like_with_like() {
        let (mut h, native) = detail();
        h.app.ui.before_after = crate::state::BeforeAfter::SideBySide;
        h.request("engine.execute", json!({"command": "view.zoom100"}), T);
        h.settle(SETTLE);
        let (after, before) = (h.app.region_view.expect("After window"), h.app.region_before_view.expect("Before window"));
        assert_eq!(after.full, before.full, "the same frame");
        assert_eq!(after.full.0.max(after.full.1), native);
        let (a, b) = (tile(&h, Slot::Region).expect("After tile"), tile(&h, Slot::RegionBefore).expect("Before tile"));
        assert_eq!(a, b, "the same window of each");
        let main = h.app.renderer.textures.get(&Slot::Main).unwrap();
        let before_main = h.app.renderer.textures.get(&Slot::Before).unwrap();
        assert_eq!(main.size, before_main.size, "and the same whole-frame size under them");
    }

    // Given the Before view alone (\), only the Before gets a window
    #[test]
    fn the_original_view_windows_only_the_before() {
        let (mut h, _) = detail();
        h.app.ui.before_after = crate::state::BeforeAfter::Original;
        h.request("engine.execute", json!({"command": "view.zoom100"}), T);
        h.settle(SETTLE);
        assert!(h.app.region_before_view.is_some());
        assert_eq!(h.app.region_view, None);
        assert!(tile(&h, Slot::RegionBefore).is_some() && tile(&h, Slot::Region).is_none());
    }

    // Given a wipe (Split), both sides are windows, drawn over the same rect
    #[test]
    fn a_wipe_windows_both_sides() {
        let (mut h, _) = detail();
        h.app.ui.before_after = crate::state::BeforeAfter::Split;
        h.request("engine.execute", json!({"command": "view.zoom100"}), T);
        h.settle(SETTLE);
        assert_eq!(h.app.region_view.map(|v| v.window), h.app.region_before_view.map(|v| v.window));
        assert!(tile(&h, Slot::Region).is_some() && tile(&h, Slot::RegionBefore).is_some());
    }

    // Given the Before view turned off, its window is freed
    #[test]
    fn leaving_before_after_frees_the_before_window() {
        let (mut h, _) = detail();
        h.app.ui.before_after = crate::state::BeforeAfter::SideBySide;
        h.request("engine.execute", json!({"command": "view.zoom100"}), T);
        h.settle(SETTLE);
        assert!(tile(&h, Slot::RegionBefore).is_some());
        h.app.ui.before_after = crate::state::BeforeAfter::Off;
        h.settle(SETTLE);
        assert_eq!(tile(&h, Slot::RegionBefore), None);
        assert_eq!(h.app.region_before_view, None);
        assert!(tile(&h, Slot::Region).is_some());
    }

    // Given a trackpad pinch from fit to well past 100 %, the render sizes are held while it runs:
    // no stream of renders of every size in between, and the sharp window arrives once it stops
    #[test]
    fn a_pinch_holds_the_render_sizes_until_it_stops() {
        let (mut h, native) = detail();
        let main_at_fit = rendered_long_edge(&h);
        let at = h.app.canvas_rect.unwrap().center();
        h.request("ui.move", json!({"x": at.x, "y": at.y}), T);
        for _ in 0..40 {
            h.request("ui.zoom", json!({"factor": 1.08}), T);
            assert_eq!(h.app.region_view, None, "no window while the pinch runs");
        }
        assert!(matches!(h.app.ui.zoom, crate::state::Zoom::Percent(p) if p > 100.0), "{:?}", h.app.ui.zoom);
        assert_eq!(rendered_long_edge(&h), main_at_fit, "the whole-frame render keeps its size");
        assert!(!h.app.renderer.is_pending(Slot::Region), "no window was asked for during the pinch");
        for _ in 0..60 {
            h.step();
        }
        h.settle(SETTLE);
        let region = h.app.region_view.expect("the window arrives when the pinch is over");
        assert_eq!(region.full.0.max(region.full.1), native);
    }

    // Given a picture on screen that is not the frame (an unsupported raw's embedded JPEG cropped
    // differently, #280), no window is rendered: its coordinates are the frame's
    #[test]
    fn no_window_over_a_picture_that_is_not_the_frame() {
        let (mut h, _) = detail();
        h.request("engine.execute", json!({"command": "view.zoom100"}), T);
        h.settle(SETTLE);
        assert!(h.app.region_view.is_some());
        let t = h.app.renderer.textures.get_mut(&Slot::Main).unwrap();
        t.size = [t.size[0], t.size[1] / 2];
        h.step();
        assert_eq!(h.app.region_view, None);
        assert_eq!(region_tile(&h), None);
    }

    /// `cargo test -p lightcraft-ui-egui --release profile_slider_drag -- --ignored --nocapture`:
    /// milliseconds from a slider tick to the loupe showing it, at 100 % and 400 % of the demo
    /// library's 24 MP photos (what the maintainers asked for in the review of #351).
    #[test]
    #[ignore = "a measurement, not a check"]
    fn profile_slider_drag() {
        for percent in [0.0, 100.0, 400.0] {
            let (mut h, native) = detail();
            h.app.ui.zoom = if percent == 0.0 { crate::state::Zoom::Fit } else { crate::state::Zoom::Percent(percent) };
            h.settle(SETTLE);
            h.app.session.begin_interaction("Exposure").unwrap();
            let mut ms = Vec::new();
            for tick in 0..20 {
                let t0 = std::time::Instant::now();
                let r = h.request(
                    "engine.execute",
                    json!({"command": "develop.set", "params": {"control": "light.exposure", "value": tick as f64 * 0.05}}),
                    T,
                );
                assert_eq!(r["ok"], true);
                let mut frames = 0;
                while (h.app.renderer.is_pending(Slot::Main) || h.app.renderer.is_pending(Slot::Region)) && frames < 2000 {
                    h.step();
                    std::thread::sleep(std::time::Duration::from_micros(200));
                    frames += 1;
                }
                ms.push(t0.elapsed().as_secs_f64() * 1e3);
            }
            ms.sort_by(|a, b| a.total_cmp(b));
            let (w, hh) = region_tile(&h).unwrap_or((0, 0));
            eprintln!(
                "PROFILE {percent:>4}% (0 = fit) of {native} px: tick→shown median {:.1} ms, p90 {:.1} ms, max {:.1} ms; whole-frame render {} px, window tile {w}×{hh}",
                ms[ms.len() / 2],
                ms[ms.len() * 9 / 10],
                ms[ms.len() - 1],
                rendered_long_edge(&h)
            );
            eprintln!("        {}", h.app.renderer.memory());
            let job_ms = |slot| h.app.renderer.textures.get(&slot).map_or(0.0, |t| t.ms);
            eprintln!("        last job times: whole frame {:.1} ms, window {:.1} ms", job_ms(Slot::Main), job_ms(Slot::Region));
        }
    }

    // Given a slider drag that outpaces the renders (each tick supersedes the last), the original
    // the window was cut from is kept once it is decoded: not decoded again for every tick
    #[test]
    fn a_drag_keeps_the_decoded_original_even_when_every_result_is_superseded() {
        let (mut h, _) = detail();
        assert_eq!(h.app.session.memory_report().full_source.bytes, 0, "fit needed no original");
        h.app.ui.zoom = crate::state::Zoom::Percent(400.0);
        h.app.session.begin_interaction("Exposure").unwrap();
        let mut ticks = 0;
        while !h.app.renderer.textures.contains_key(&Slot::Region) && ticks < 3000 {
            let v = (ticks % 40) as f64 * 0.02;
            h.request("engine.execute", json!({"command": "develop.set", "params": {"control": "light.exposure", "value": v}}), T);
            h.step();
            ticks += 1;
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(h.app.renderer.textures.contains_key(&Slot::Region), "a window arrived");
        assert!(h.app.session.memory_report().full_source.bytes > 0, "the original stayed in the cache while the drag went on");
    }

    // Given a wipe at 1:1, the Before window is drawn after (over) the low-resolution Before
    // stand-in on its side of the line, never under it
    #[test]
    fn a_wipe_draws_the_before_window_over_its_stand_in() {
        for mode in [crate::state::BeforeAfter::Split, crate::state::BeforeAfter::SplitTopBottom] {
            let (mut h, _) = detail();
            h.app.ui.before_after = mode;
            h.request("engine.execute", json!({"command": "view.zoom100"}), T);
            h.settle(SETTLE);
            h.step();
            let order = &h.app.draw_order;
            let pos = |what: &str| order.iter().position(|d| *d == what);
            let (stand_in, before, after) = (pos("wipe stand-in"), pos("window RegionBefore"), pos("window Region"));
            assert!(stand_in.is_some() && before.is_some() && after.is_some(), "{mode:?}: {order:?}");
            assert!(before > stand_in, "{mode:?}: the Before window is under its stand-in: {order:?}");
        }
    }

    // Given a Before and an After with the same look (an unedited photo), both windows are kept
    // apart: the job key is the same, and one tile must not replace the other
    #[test]
    fn identical_before_and_after_keep_a_tile_each() {
        let (mut h, _) = detail();
        h.app.ui.before_after = crate::state::BeforeAfter::SideBySide;
        h.request("engine.execute", json!({"command": "view.zoom100"}), T);
        h.settle(SETTLE);
        assert_eq!(h.app.region_tiles.len(), 2, "{:?}", h.app.region_tiles.keys().collect::<Vec<_>>());
    }
}
