//! Issue #323: the Detail view must not be softened by a hidden render-size cap. By default the
//! loupe renders at the size it is drawn at (up to the photo's own pixels and a safety ceiling);
//! "Preview size" in Settings only lowers that when the user asks for it.

use crate::state::{AppSettings, LOUPE_EDGE_CEILING, PREVIEW_LIMITS, STANDARD_PREVIEW_EDGE};

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
    assert_eq!(with_limit(0).loupe_edge(6000.0, 6000), 6000);
}

// Given Automatic, a Retina fit view of 3024 px is not held to the old 2560 px cap
#[test]
fn automatic_does_not_cap_a_fit_view_at_the_old_default() {
    assert_eq!(with_limit(0).loupe_edge(3024.0, 6000), 3072, "rounded up to the next 64 px step");
}

// Dragging a window edge must not re-render (and re-key the caches) on every pixel
#[test]
fn render_sizes_move_in_steps() {
    let s = with_limit(0);
    assert_eq!(s.loupe_edge(2945.0, 6000), s.loupe_edge(3008.0, 6000));
    assert_ne!(s.loupe_edge(3008.0, 6000), s.loupe_edge(3009.0, 6000));
}

// Neighbour prefetch and hover / before stand-ins stay at the preview level: a full-size decode
// would evict the open photo's single full-resolution source (review of #323)
#[test]
fn prefetch_and_stand_ins_never_ask_for_the_full_source_level() {
    use lightcraft_engine::SourceLevel;
    let s = with_limit(0);
    for wanted in [1000.0, 2560.0, 3024.0, 6000.0, 48000.0] {
        assert!(s.prefetch_edge(wanted, 6000) <= SourceLevel::Preview.max_edge(), "{wanted}");
        assert!(s.stand_in_edge(s.loupe_edge(wanted, 6000)) <= SourceLevel::Preview.max_edge());
    }
    assert_eq!(s.prefetch_edge(1000.0, 6000), 1024, "a small view is not raised");
}

// Given Automatic, a small photo zoomed far in is not rendered above its own pixels
#[test]
fn never_renders_above_the_photos_own_pixels() {
    assert_eq!(with_limit(0).loupe_edge(8000.0, 1000), 1000);
}

// Given Automatic, a huge zoom on a huge photo stays under the memory/GPU ceiling
#[test]
fn automatic_is_bounded_by_the_ceiling() {
    assert_eq!(with_limit(0).loupe_edge(48000.0, 12000), LOUPE_EDGE_CEILING as usize);
}

// Given the user chose 1600 px, the render never exceeds it
#[test]
fn an_explicit_limit_caps_the_render() {
    assert_eq!(with_limit(1600).loupe_edge(6000.0, 6000), 1600);
    assert_eq!(with_limit(1600).loupe_edge(900.0, 6000), 960, "and does not raise a small view past its step");
}

// Whatever the inputs (hostile or degenerate), the result is a usable size
#[test]
fn degenerate_inputs_give_a_usable_size() {
    let s = with_limit(0);
    for wanted in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -5.0, 0.0] {
        let e = s.loupe_edge(wanted, 6000);
        assert!((8..=LOUPE_EDGE_CEILING as usize).contains(&e), "{wanted}: {e}");
    }
    assert!((8..=LOUPE_EDGE_CEILING as usize).contains(&s.loupe_edge(1000.0, 0)) && s.prefetch_edge(f32::NAN, 0) >= 8);
    assert!(with_limit(7).loupe_edge(1000.0, 6000) >= 8, "a corrupt limit is clamped");
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

    // Given a photo larger than the old 2560 px cap, when I zoom to 1:1, then the loupe holds its pixels
    #[test]
    fn one_to_one_is_rendered_at_the_photos_own_size() {
        let (mut h, native) = detail();
        h.request("engine.execute", json!({"command": "view.zoom100"}), T);
        h.settle(SETTLE);
        let want = native.min(crate::state::LOUPE_EDGE_CEILING as usize);
        let got = rendered_long_edge(&h);
        assert!(got.abs_diff(want) <= 2, "native {native}: rendered {got}, wanted {want}");
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

    // Given a photo that fits the whole-frame render at 1:1, then no second render is made
    #[test]
    fn no_window_render_while_the_whole_frame_render_is_sharp_enough() {
        let (mut h, _) = detail();
        h.request("engine.execute", json!({"command": "view.zoom100"}), T);
        h.settle(SETTLE);
        assert_eq!(region_tile(&h), None);
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
}
