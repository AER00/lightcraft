//! Headless tests of the side panels: dragging their inner edge resizes them within limits, and
//! the chosen widths survive switching panels and a save/load of the UI state (issue #20).

use std::time::Duration;

use serde_json::json;

use crate::headless::Headless;
use crate::state::{LEFT_WIDTH, MIN_PHOTO_WIDTH, RIGHT_WIDTH};
use crate::{LightcraftApp, Services};

const T: Duration = Duration::from_secs(20);
const SETTLE: Duration = Duration::from_secs(120);

fn demo(size: [f32; 2], ui: serde_json::Value) -> Headless {
    let services = Services { png: None, ..Default::default() };
    let app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), services);
    let mut h = Headless::new(app, size, 1.0);
    let r = h.request("ui.set", ui, T);
    assert_eq!(r["ok"], true, "{r}");
    h.settle(SETTLE);
    h
}

fn widget(h: &Headless, id: &str) -> egui::Rect {
    h.app.widgets.iter().find(|(w, _)| w == id).map(|(_, r)| *r).unwrap_or_else(|| panic!("no widget {id}"))
}

/// Drag horizontally from `x` by `dx` (within the 1400 pt window) at mid height.
fn drag(h: &mut Headless, x: f32, dx: f32) {
    let to = (x + dx).clamp(1.0, 1399.0);
    let r = h.request("ui.drag", json!({"x": x, "y": 500.0, "toX": to, "toY": 500.0, "steps": 12}), T);
    assert_eq!(r["ok"], true, "{r}");
    h.step();
}

#[test]
fn side_panels_resize_by_their_inner_edge_and_remember_it() {
    let mut h = demo([1400.0, 900.0], json!({"view": "detail", "right": "edit", "leftPanel": true}));
    let right = widget(&h, "panel:right_panel");
    let left = widget(&h, "panel:left_panel");
    assert_eq!((left.width(), right.width()), (LEFT_WIDTH.default, RIGHT_WIDTH.default));
    // the right panel's left edge: 120 pt wider
    drag(&mut h, right.left() + 2.0, -120.0);
    assert!((h.app.ui.right_width - (RIGHT_WIDTH.default + 120.0)).abs() <= 2.0, "{}", h.app.ui.right_width);
    assert!((widget(&h, "panel:right_panel").width() - h.app.ui.right_width).abs() <= 1.0);
    // the left sidebar's right edge: 70 pt wider
    drag(&mut h, left.right() - 2.0, 70.0);
    assert!((h.app.ui.left_width - (LEFT_WIDTH.default + 70.0)).abs() <= 2.0, "{}", h.app.ui.left_width);
    // kept across panel switches and view changes
    let (lw, rw) = (h.app.ui.left_width, h.app.ui.right_width);
    for set in [json!({"right": "info"}), json!({"view": "photoGrid"}), json!({"right": "masking", "view": "detail"})] {
        h.request("ui.set", set, T);
        h.step();
        assert_eq!((widget(&h, "panel:left_panel").width(), widget(&h, "panel:right_panel").width()), (lw, rw));
    }
    // and across a save/load of the UI state (ui.json)
    let saved = serde_json::to_string(&h.app.ui).unwrap();
    let back = serde_json::from_str::<crate::UiState>(&saved).unwrap().sanitized();
    assert_eq!((back.left_width, back.right_width), (lw, rw));
    // limits: the photo area stays usable however far the edges are dragged…
    let right = widget(&h, "panel:right_panel");
    drag(&mut h, right.left() + 2.0, -1400.0);
    let left = widget(&h, "panel:left_panel");
    drag(&mut h, left.right() - 2.0, 1400.0);
    h.step();
    let (left, right) = (widget(&h, "panel:left_panel"), widget(&h, "panel:right_panel"));
    assert!(right.width() <= RIGHT_WIDTH.max && left.width() <= LEFT_WIDTH.max, "{left:?} {right:?}");
    assert!(right.left() - left.right() >= MIN_PHOTO_WIDTH - 1.0, "photo area {} wide", right.left() - left.right());
    // …and neither panel gets narrower than its minimum
    drag(&mut h, right.left() + 2.0, 1400.0);
    let left = widget(&h, "panel:left_panel");
    drag(&mut h, left.right() - 2.0, -1400.0);
    assert_eq!((h.app.ui.left_width, h.app.ui.right_width), (LEFT_WIDTH.min, RIGHT_WIDTH.min));
    // out-of-range saved widths are clamped on load
    let mut u = crate::UiState { left_width: 5.0, right_width: f32::NAN, ..Default::default() }.sanitized();
    assert_eq!((u.left_width, u.right_width), (LEFT_WIDTH.min, RIGHT_WIDTH.default));
    u.right_width = 9000.0;
    assert_eq!(u.sanitized().right_width, RIGHT_WIDTH.max);
}

#[test]
fn a_narrow_window_shrinks_the_panels_without_forgetting_their_width() {
    let mut h = demo([1400.0, 900.0], json!({"view": "detail", "right": "edit", "leftPanel": true, "rightWidth": 480.0, "leftWidth": 400.0}));
    assert_eq!(widget(&h, "panel:right_panel").width(), 480.0);
    h.request("ui.resize", json!({"width": 1000.0, "height": 800.0}), T);
    h.settle(SETTLE);
    let (left, right) = (widget(&h, "panel:left_panel"), widget(&h, "panel:right_panel"));
    assert!(right.left() - left.right() >= MIN_PHOTO_WIDTH - 1.0, "{left:?} {right:?}");
    // the chosen widths come back when the window does
    assert_eq!((h.app.ui.left_width, h.app.ui.right_width), (400.0, 480.0));
    h.request("ui.resize", json!({"width": 1400.0, "height": 900.0}), T);
    h.settle(SETTLE);
    assert_eq!(widget(&h, "panel:right_panel").width(), 480.0);
}
