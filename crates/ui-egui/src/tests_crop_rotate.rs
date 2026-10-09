//! Rotating in the crop tool (issue #534): outside the crop box the pointer shows that a drag
//! rotates, a drag shows the angle as it changes, and the Straighten value takes an exact angle.

use std::time::Duration;

use serde_json::json;

use crate::headless::Headless;
use crate::{LightcraftApp, Services};

const T: Duration = Duration::from_secs(20);
const SETTLE: Duration = Duration::from_secs(120);

fn crop_tool() -> Headless {
    let app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services { png: None, ..Default::default() });
    let mut h = Headless::new(app, [1200.0, 800.0], 1.0);
    for (method, params) in [("ui.set", json!({"view": "detail"})), ("engine.execute", json!({"command": "panel.crop"}))] {
        let r = h.request(method, params, T);
        assert_eq!(r["ok"], true, "{r}");
    }
    h.settle(SETTLE);
    h
}

fn has(h: &Headless, id: &str) -> bool {
    h.app.widgets.iter().any(|(w, _)| w == id)
}

fn angle(h: &Headless) -> f64 {
    let id = h.app.session.active().expect("a photo");
    h.app.session.catalog.photo(id).expect("the photo").develop.crop.geometry.angle
}

fn hover(h: &mut Headless, p: egui::Pos2) {
    let r = h.request("ui.move", json!({"x": p.x, "y": p.y}), T);
    assert_eq!(r["ok"], true, "{r}");
    h.settle(SETTLE);
}

/// Outside the crop box, where a drag rotates, the pointer becomes a curved double arrow drawn at
/// the pointer (no system cursor has one); inside it is the move cursor, with no rotation glyph.
#[test]
fn outside_the_crop_box_the_pointer_shows_rotation() {
    let mut h = crop_tool();
    let img = h.app.image_rect.expect("the photo on screen");
    hover(&mut h, egui::pos2(img.left() - 24.0, img.center().y));
    assert_eq!(h.last_cursor, egui::CursorIcon::None, "the system cursor gives way to the glyph");
    assert!(has(&h, "cropRotateCursor"), "the rotation glyph is drawn at the pointer");
    hover(&mut h, img.center());
    assert_eq!(h.last_cursor, egui::CursorIcon::Move);
    assert!(!has(&h, "cropRotateCursor"));
}

/// Dragging outside the box rotates, and the angle is shown next to the pointer while it moves.
#[test]
fn rotating_shows_the_angle() {
    let mut h = crop_tool();
    let r = h.request(
        "ui.pointer",
        json!({"events": [{"kind": "down", "x": -0.05, "y": 0.2}, {"kind": "drag", "x": -0.05, "y": 0.25}, {"kind": "drag", "x": -0.05, "y": 0.3}]}),
        T,
    );
    assert_eq!(r["ok"], true, "{r}");
    h.settle(SETTLE);
    assert!(angle(&h) != 0.0, "the drag rotated the crop");
    assert!(has(&h, "cropAngleReadout"), "the angle is shown while rotating");
    let r = h.request("ui.pointer", json!({"events": [{"kind": "up", "x": -0.05, "y": 0.3}]}), T);
    assert_eq!(r["ok"], true, "{r}");
    h.settle(SETTLE);
    assert!(!has(&h, "cropAngleReadout"), "and gone once released");
}

/// The Straighten value takes an exact angle: click it, type, Enter.
#[test]
fn the_straighten_value_takes_an_exact_angle() {
    let mut h = crop_tool();
    let r = h.request("ui.clickWidget", json!({"id": "sliderValue:crop.angle"}), T);
    assert_eq!(r["ok"], true, "{r}");
    h.settle(SETTLE);
    for key in ["a", "Backspace"] {
        // select what is there and clear it
        let r = h.request("ui.key", json!({"key": key, "cmd": key == "a"}), T);
        assert_eq!(r["ok"], true, "{r}");
    }
    let r = h.request("ui.text", json!({"text": "2.5"}), T);
    assert_eq!(r["ok"], true, "{r}");
    let r = h.request("ui.key", json!({"key": "Enter"}), T);
    assert_eq!(r["ok"], true, "{r}");
    h.settle(SETTLE);
    assert!((angle(&h) - 2.5).abs() < 1e-9, "typed 2.5°, got {}", angle(&h));
}

/// The angle next to the pointer reads like the Straighten value, in degrees.
#[test]
fn the_angle_reads_like_the_straighten_value() {
    use crate::panels::detail::crop_angle_label;
    assert_eq!(crop_angle_label(0.0), "0.00°", "as the slider shows it at rest");
    assert_eq!(crop_angle_label(2.5), "+2.50°");
    assert_eq!(crop_angle_label(-12.254), "-12.25°");
    assert_eq!(crop_angle_label(-0.001), "0.00°", "no minus zero, and every zero the same");
}

/// Hovering the Straighten value says that a value can be typed there.
#[test]
fn the_straighten_value_says_it_can_be_typed() {
    let mut h = crop_tool();
    let r = h.request("ui.hoverWidget", json!({"id": "sliderValue:crop.angle"}), T);
    assert_eq!(r["ok"], true, "{r}");
    // tooltips wait for the pointer to rest
    let _ = h.step_until(Duration::from_secs(5), |h| h.painted_text().iter().any(|t| t.contains("type")));
    assert!(
        h.painted_text().iter().any(|t| t == "Click to type a value"),
        "{:?}",
        h.painted_text().iter().filter(|t| t.contains("lick")).collect::<Vec<_>>()
    );
}
