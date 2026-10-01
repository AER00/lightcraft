//! Lightroom-style widgets: sliders with gradient tracks and hollow thumbs, section headers,
//! flyout rows, bordered buttons. Every interactive widget registers an automation id + rect
//! ([`register`]) so the control channel can find and click it by name.

use egui::{Align2, Color32, CornerRadius, Rect, Response, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use lightcraft_develop::{ControlSpec, Track};

use crate::icons::{Icon, paint};
use crate::theme::Tokens;

// ------------------------------------------------------------------ automation registry

#[derive(Clone, Default)]
pub struct Registry(pub Vec<(String, Rect)>);

/// Record a widget's automation id and screen rect for this frame.
pub fn register(ctx: &egui::Context, id: impl Into<String>, rect: Rect) {
    let id = id.into();
    ctx.data_mut(|d| d.get_temp_mut_or_default::<Registry>(egui::Id::new("lc-registry")).0.push((id, rect)));
}

pub fn take_registry(ctx: &egui::Context) -> Vec<(String, Rect)> {
    ctx.data_mut(|d| std::mem::take(&mut d.get_temp_mut_or_default::<Registry>(egui::Id::new("lc-registry")).0))
}

// ------------------------------------------------------------------ colour helpers

pub fn hex(s: &str) -> Color32 {
    let v = u32::from_str_radix(s.trim_start_matches('#'), 16).unwrap_or(0x808080);
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

pub fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()))
}

/// Band colours for the 8 colour-mixer bands (swatches and tracks).
pub const BAND_COLORS: [&str; 8] = ["#aa0000", "#bb6600", "#aaaa00", "#009900", "#00aaaa", "#0044cc", "#7722aa", "#aa0077"];

fn track_stops(track: &Track) -> Option<Vec<Color32>> {
    Some(match track {
        Track::Plain | Track::Centered => return None,
        Track::Temp => ["#193961", "#47566a", "#60666d", "#70705b", "#737339", "#75742a"].iter().map(|h| hex(h)).collect(),
        Track::Tint => ["#246123", "#2c552c", "#394739", "#4a3d4c", "#6b336b", "#872c87"].iter().map(|h| hex(h)).collect(),
        Track::Gradient { from, to } => vec![hex(from), hex(to)],
        Track::Hue { band } => {
            let i = *band as usize;
            let prev = hex(BAND_COLORS[(i + 7) % 8]);
            let next = hex(BAND_COLORS[(i + 1) % 8]);
            let me = hex(BAND_COLORS[i]);
            vec![prev.gamma_multiply(0.7), lerp(prev, me, 0.5).gamma_multiply(0.7), me.gamma_multiply(0.7), lerp(me, next, 0.5).gamma_multiply(0.7), next.gamma_multiply(0.7)]
        }
        Track::Sat { band } => vec![hex("#454343"), lerp(hex("#454343"), hex(BAND_COLORS[*band as usize]), 0.6), hex(BAND_COLORS[*band as usize])],
        Track::Lum { band } => {
            let c = hex(BAND_COLORS[*band as usize]);
            vec![c.gamma_multiply(0.2), c.gamma_multiply(0.55), lerp(c, Color32::WHITE, 0.35), lerp(c, Color32::WHITE, 0.6)]
        }
        Track::Rainbow => ["#aa0000", "#aaaa00", "#009900", "#00aaaa", "#0044cc", "#aa0077", "#aa0000"].iter().map(|h| hex(h)).collect(),
    })
}

fn paint_track(ui: &Ui, rect: Rect, track: &Track, t: &Tokens, thumb_x: f32, ring: f32) {
    let p = ui.painter();
    let y = rect.center().y;
    let h = 1.0;
    if let Some(stops) = track_stops(track) {
        // gradient: segments between stops
        let n = stops.len().max(2) - 1;
        let steps = 48;
        for i in 0..steps {
            let (a, b) = (i as f32 / steps as f32, (i + 1) as f32 / steps as f32);
            let seg = (a * n as f32).floor() as usize;
            let local = a * n as f32 - seg as f32;
            let c = lerp(stops[seg.min(n)], stops[(seg + 1).min(n)], local);
            let x0 = rect.left() + rect.width() * a;
            let x1 = rect.left() + rect.width() * b;
            p.rect_filled(Rect::from_min_max(pos2(x0, y - h - 0.5), pos2(x1 + 0.5, y + h + 0.5)), 0.0, c);
        }
    } else {
        p.rect_filled(Rect::from_min_max(pos2(rect.left(), y - h), pos2(rect.right(), y + h)), 1.0, t.track);
    }
    // gap around the hollow thumb
    p.rect_filled(Rect::from_center_size(pos2(thumb_x, y), vec2(ring * 2.0 + 4.0, 6.0)), 0.0, t.chrome);
}

/// Result of a slider interaction this frame.
#[derive(Default, Debug, Clone, Copy)]
pub struct SliderOut {
    pub value: Option<f64>,
    pub drag_started: bool,
    pub drag_stopped: bool,
    pub reset: bool,
}

/// A Lightroom slider row (label + value above a track with a hollow ring thumb).
/// Double-click the label or thumb to reset. Shift-drag = fine adjustment.
pub fn slider(ui: &mut Ui, spec: &ControlSpec, value: f64, enabled: bool, label_override: Option<&str>) -> SliderOut {
    let t = Tokens::get(ui.ctx());
    let w = ui.available_width();
    let (row, _) = ui.allocate_exact_size(vec2(w, t.slider_row_h), Sense::hover());
    let pad_l = 24.0;
    let pad_r = 22.0;
    let label_rect = Rect::from_min_size(pos2(row.left() + pad_l, row.top() + 4.0), vec2(w - pad_l - pad_r, 18.0));
    let track_rect = Rect::from_min_max(pos2(row.left() + pad_l, row.top() + 22.0), pos2(row.right() - pad_r, row.top() + 40.0));
    let id = ui.id().with(spec.id);
    let resp = ui.interact(track_rect.expand2(vec2(8.0, 2.0)), id, if enabled { Sense::click_and_drag() } else { Sense::hover() });
    let label_resp = ui.interact(label_rect, id.with("label"), Sense::click());
    register(ui.ctx(), format!("slider:{}", spec.id), track_rect);
    let mut out = SliderOut::default();
    let span = (spec.max - spec.min).max(1e-9);
    let to_x = |v: f64| track_rect.left() + ((v - spec.min) / span).clamp(0.0, 1.0) as f32 * track_rect.width();
    let from_x = |x: f32| spec.min + ((x - track_rect.left()) / track_rect.width()).clamp(0.0, 1.0) as f64 * span;
    let mut v = value;
    if resp.double_clicked() || label_resp.double_clicked() {
        out.reset = true;
        out.value = Some(spec.default);
        v = spec.default;
    } else {
        if resp.drag_started() {
            out.drag_started = true;
        }
        if (resp.dragged() || resp.drag_started())
            && let Some(p) = resp.interact_pointer_pos()
        {
            let fine = ui.input(|i| i.modifiers.shift);
            let nv = if fine {
                let dx = ui.input(|i| i.pointer.delta().x);
                value + dx as f64 * span / track_rect.width() as f64 * 0.1
            } else {
                from_x(p.x)
            };
            let step = spec.step.max(1e-9);
            let nv = ((nv / step).round() * step).clamp(spec.min, spec.max);
            if (nv - value).abs() > 1e-12 {
                out.value = Some(nv);
                v = nv;
            }
        } else if resp.clicked()
            && let Some(p) = resp.interact_pointer_pos()
        {
            let step = spec.step.max(1e-9);
            let nv = ((from_x(p.x) / step).round() * step).clamp(spec.min, spec.max);
            out.value = Some(nv);
            v = nv;
            out.drag_started = true;
            out.drag_stopped = true;
        }
        if resp.drag_stopped() {
            out.drag_stopped = true;
        }
    }
    // paint
    let hovered = resp.hovered() || resp.dragged();
    let text_c = if enabled { t.text_label } else { t.text_disabled };
    let p = ui.painter();
    p.text(label_rect.left_center(), Align2::LEFT_CENTER, label_override.unwrap_or(spec.label), t.font(12.5), text_c);
    let shown = if spec.id == "wb.temp" { format!("{v:.0}") } else { spec.format(v).replace("+0.00", "0").replace("-0.00", "0") };
    let shown = if shown == "+0" || shown == "-0" { "0".to_string() } else { shown };
    p.text(label_rect.right_center(), Align2::RIGHT_CENTER, shown, t.font(12.5), text_c);
    let ring = 7.0;
    let tx = to_x(v);
    paint_track(ui, track_rect, &spec.track, &t, tx, ring);
    let p = ui.painter();
    let ring_c = if !enabled {
        t.text_disabled
    } else if hovered {
        t.thumb_hover
    } else {
        t.thumb
    };
    p.circle_filled(pos2(tx, track_rect.center().y), ring, t.chrome);
    p.circle_stroke(pos2(tx, track_rect.center().y), ring, Stroke::new(2.0, ring_c));
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    out
}

/// Collapsible section header ("› Light"). Returns the response (click toggles).
pub fn section_header(ui: &mut Ui, id: &str, title: &str, open: bool, enabled: Option<bool>) -> (Response, Option<bool>) {
    let t = Tokens::get(ui.ctx());
    let w = ui.available_width();
    let (r, resp) = ui.allocate_exact_size(vec2(w, 52.0), Sense::click());
    register(ui.ctx(), format!("section:{id}"), r);
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(r, 0.0, t.chrome.gamma_multiply(1.06));
    }
    let chev = Rect::from_center_size(pos2(r.left() + 30.0, r.center().y), vec2(14.0, 14.0));
    paint(p, chev, if open { Icon::ChevronDown } else { Icon::ChevronRight }, t.text_label);
    p.text(pos2(r.left() + 44.0, r.center().y), Align2::LEFT_CENTER, title, t.semibold(14.0), t.text);
    let mut toggled = None;
    if let Some(on) = enabled
        && (resp.hovered() || !on)
    {
        let eye = Rect::from_center_size(pos2(r.right() - 34.0, r.center().y), vec2(18.0, 18.0));
        let er = ui.interact(eye, ui.id().with(("eye", id)), Sense::click());
        register(ui.ctx(), format!("sectionEye:{id}"), eye);
        paint(ui.painter(), eye, Icon::Eye, if on { t.icon } else { t.text_disabled });
        if !on {
            ui.painter().line_segment([eye.left_bottom(), eye.right_top()], Stroke::new(1.5, t.text_disabled));
        }
        if er.clicked() {
            toggled = Some(!on);
        }
    }
    (resp, toggled)
}

/// Thin divider between groups.
pub fn divider(ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(vec2(w, 1.0), Sense::hover());
    ui.painter().rect_filled(r, 0.0, t.divider);
}

/// A flyout row inside a section (Curve, Color Mixer, Color Grading…): inset dark box with icon tile.
pub fn flyout_row(ui: &mut Ui, id: &str, title: &str, icon: Icon, open: bool) -> Response {
    let t = Tokens::get(ui.ctx());
    let w = ui.available_width();
    let (outer, _) = ui.allocate_exact_size(vec2(w, 40.0), Sense::hover());
    let r = Rect::from_min_max(pos2(outer.left() + 16.0, outer.top() + 4.0), pos2(outer.right() - 16.0, outer.bottom() - 4.0));
    let resp = ui.interact(r, ui.id().with(("flyout", id)), Sense::click());
    register(ui.ctx(), format!("flyout:{id}"), r);
    let p = ui.painter();
    p.rect_filled(r, 4.0, if resp.hovered() { t.hover.gamma_multiply(0.8) } else { t.inset });
    let tile = Rect::from_min_size(pos2(r.left() + 4.0, r.top() + 4.0), vec2(r.height() - 8.0, r.height() - 8.0));
    p.rect_filled(tile, 3.0, t.chrome.gamma_multiply(1.15));
    paint(p, tile.shrink(3.0), icon, t.text_label);
    p.text(pos2(tile.right() + 10.0, r.center().y), Align2::LEFT_CENTER, title, t.semibold(13.0), t.text_label);
    paint(p, Rect::from_center_size(pos2(r.right() - 14.0, r.center().y), vec2(12.0, 12.0)), if open { Icon::ChevronDown } else { Icon::TriangleLeft }, t.text_label);
    resp
}

/// A small bordered text button (Auto, B&W, HDR…).
pub fn text_button(ui: &mut Ui, id: &str, label: &str, active: bool) -> Response {
    let t = Tokens::get(ui.ctx());
    let font = t.semibold(11.5);
    let galley = ui.painter().layout_no_wrap(label.to_string(), font, t.text);
    let size = vec2((galley.size().x + 18.0).max(37.0), 24.0);
    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
    register(ui.ctx(), format!("button:{id}"), r);
    let fill = if active {
        t.pressed
    } else if resp.hovered() {
        t.hover
    } else {
        t.button
    };
    let p = ui.painter();
    p.rect(r, CornerRadius::same(4), fill, Stroke::new(1.0, t.button_border), StrokeKind::Inside);
    p.galley(r.center() - galley.size() / 2.0, galley, t.text);
    resp
}

/// An icon-only button. `active` draws the selected background (tool strip).
pub fn icon_button(ui: &mut Ui, id: &str, icon: Icon, size: egui::Vec2, active: bool, enabled: bool, tooltip: &str) -> Response {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    register(ui.ctx(), format!("icon:{id}"), r);
    let p = ui.painter();
    let sq = Rect::from_center_size(r.center(), vec2(32.0f32.min(size.x), 32.0f32.min(size.y)));
    if active {
        p.rect_filled(sq, 4.0, t.tool_active);
    } else if resp.hovered() && enabled {
        p.rect_filled(sq, 4.0, t.hover.gamma_multiply(0.7));
    }
    let c = if !enabled {
        t.text_disabled
    } else if active {
        t.pick
    } else if resp.hovered() {
        t.text
    } else {
        t.icon
    };
    let glyph = (size.x.min(size.y) * 0.62).min(22.0);
    paint(p, Rect::from_center_size(r.center(), vec2(glyph, glyph)), icon, c);
    if !tooltip.is_empty() { resp.on_hover_text(tooltip) } else { resp }
}

/// Star rating row (5 stars); returns a new rating when clicked (clicking the current rating clears).
pub fn stars(ui: &mut Ui, id: &str, rating: u8, size: f32) -> Option<u8> {
    let t = Tokens::get(ui.ctx());
    let mut out = None;
    let hover_n = {
        let mut h = None;
        for i in 0..5u8 {
            let (r, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
            register(ui.ctx(), format!("star:{id}:{}", i + 1), r);
            if resp.hovered() {
                h = Some(i + 1);
            }
            if resp.clicked() {
                out = Some(if rating == i + 1 { 0 } else { i + 1 });
            }
            ui.painter().text(r.center(), Align2::CENTER_CENTER, "", t.font(1.0), t.star);
            let filled = i < rating;
            paint(ui.painter(), r.shrink(size * 0.12), if filled { Icon::StarFilled } else { Icon::Star }, if filled { t.star } else { t.icon });
        }
        h
    };
    let _ = hover_n;
    out
}
