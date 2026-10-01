//! Sampling commands: Point Color, the targeted adjustment tool, red eye.

use serde_json::json;

use crate::Session;

fn demo() -> Session {
    Session::with_demo()
}

fn active_dev(s: &Session) -> lightcraft_develop::DevelopSettings {
    (*s.develop_of(s.active().unwrap()).unwrap()).clone()
}

/// Spread of the channels (a saturation proxy) at normalized `(x, y)`.
fn chroma_at(img: &lightcraft_raster::Rgba8, x: f64, y: f64) -> i32 {
    let p = img.data[(img.height as f64 * y) as usize * img.width + (img.width as f64 * x) as usize];
    p[0].max(p[1]).max(p[2]) as i32 - p[0].min(p[1]).min(p[2]) as i32
}

#[test]
fn point_color_pick_and_adjust() {
    let mut s = demo();
    let id = s.active().unwrap();
    let before = s.render_now(id, 384, 384).unwrap().image;
    let r = s.execute("pointColor.pick", &json!({"x": 0.5, "y": 0.08})).unwrap();
    assert_eq!(r["index"], 0);
    let d = active_dev(&s);
    assert_eq!(d.point_colors.len(), 1);
    assert!(d.point_colors[0].chroma > 0.01, "the sky has colour: {r}");
    // the sample's sliders are develop controls (and listed as such)
    s.execute("develop.set", &json!({"control": "pointColor.0.satShift", "value": -100})).unwrap();
    assert_eq!(active_dev(&s).point_colors[0].sat_shift, -100.0);
    let ctl = s.execute("develop.controls", &json!({"section": "pointColor"})).unwrap();
    assert!(ctl.as_array().unwrap().iter().any(|c| c["id"] == "pointColor.0.satShift" && c["value"] == -100.0), "{ctl}");
    // the picked colour is desaturated in the render
    let after = s.render_now(id, 384, 384).unwrap().image;
    assert!(
        chroma_at(&after, 0.5, 0.08) + 8 < chroma_at(&before, 0.5, 0.08),
        "{} vs {}",
        chroma_at(&after, 0.5, 0.08),
        chroma_at(&before, 0.5, 0.08)
    );
    // at most 8 samples; delete
    for _ in 0..7 {
        s.execute("pointColor.pick", &json!({"x": 0.3, "y": 0.5})).unwrap();
    }
    assert!(s.execute("pointColor.pick", &json!({"x": 0.3, "y": 0.5})).is_err());
    s.execute("pointColor.delete", &json!({"index": 3})).unwrap();
    assert_eq!(active_dev(&s).point_colors.len(), 7);
    // survives the settings JSON (what the catalog and XMP sidecars store)
    let d = active_dev(&s);
    assert_eq!(lightcraft_develop::DevelopSettings::from_json(&d.to_json()).unwrap(), d);
}
