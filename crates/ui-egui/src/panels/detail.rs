//! The Detail (loupe) view: the developed photo, before/after, zoom & pan, the filmstrip, and the
//! on-canvas tools (crop, brush/gradient masks, remove spots, white-balance picker).

use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use lightcraft_catalog::PhotoId;
use lightcraft_develop::{DevelopSettings, MaskShape};
use lightcraft_geom::{Affine, Point};
use lightcraft_pipeline::geometry::Frame;
use serde_json::json;

use crate::LightcraftApp;
use crate::render::Slot;
use crate::state::{BeforeAfter, CropOverlay, RightPanel, Zoom};
use crate::theme::Tokens;
use crate::widgets::register;

/// An in-progress on-canvas gesture.
#[derive(Clone, Debug)]
pub enum Gesture {
    Brush {
        points: Vec<Point>,
    },
    Spot {
        points: Vec<Point>,
    },
    CropHandle {
        handle: u8,
        start: lightcraft_geom::Rect,
        angle: f64,
    },
    /// Straighten tool: a line drawn along something that should be level (or plumb).
    StraightenLine {
        a: Pos2,
    },
    CropRotate {
        start_angle: f64,
        a0: f32,
    },
    Pan {
        start: (f32, f32),
        at: Pos2,
    },
    MaskHandle {
        mask: u32,
        handle: u8,
    },
    /// Guided Upright: a guide being drawn from `a` (normalized transformed coordinates).
    Guide {
        a: Point,
    },
}

/// Screen ↔ normalized-image mapping for the displayed frame.
#[derive(Clone, Copy)]
pub struct CanvasMap {
    rect: Rect,
    to_norm: Affine,
    from_norm: Affine,
    k: f32,
}

impl CanvasMap {
    fn new(frame: &Frame, rect: Rect) -> CanvasMap {
        let k = 8.0f32;
        let (w, h) = ((rect.width() * k).round().max(1.0) as usize, (rect.height() * k).round().max(1.0) as usize);
        let to_norm = frame.out_to_norm(w, h);
        CanvasMap { rect, to_norm, from_norm: to_norm.inverse().unwrap_or(Affine::IDENTITY), k }
    }
    pub fn norm(&self, p: Pos2) -> Point {
        self.to_norm.apply(Point::new(((p.x - self.rect.left()) * self.k) as f64, ((p.y - self.rect.top()) * self.k) as f64))
    }
    pub fn screen(&self, n: Point) -> Pos2 {
        let q = self.from_norm.apply(n);
        pos2(self.rect.left() + q.x as f32 / self.k, self.rect.top() + q.y as f32 / self.k)
    }
}

fn fit_rect(area: Rect, aspect: f32, zoom: Zoom, img_px: [usize; 2], ppp: f32, pan: (f32, f32)) -> Rect {
    let (aw, ah) = (area.width(), area.height());
    let (w, h) = match zoom {
        Zoom::Fit => {
            if aw / ah > aspect {
                (ah * aspect, ah)
            } else {
                (aw, aw / aspect)
            }
        }
        Zoom::Fill => {
            if aw / ah > aspect {
                (aw, aw / aspect)
            } else {
                (ah * aspect, ah)
            }
        }
        Zoom::Percent(p) => {
            // full-resolution pixels at p% (the photo's native width)
            let w = img_px[0] as f32 * p as f32 / 100.0 / ppp;
            (w, w / aspect)
        }
    };
    let c = if w <= aw && h <= ah {
        area.center()
    } else {
        // pan: which normalized point of the image sits at the area centre
        pos2(area.center().x - (pan.0 - 0.5) * w, area.center().y - (pan.1 - 0.5) * h)
    };
    Rect::from_center_size(c, vec2(w, h))
}

pub fn show(app: &mut LightcraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let full = ui.max_rect();
    let film_h = if app.ui.filmstrip { t.film_h } else { 0.0 };
    let canvas = Rect::from_min_max(full.min, pos2(full.right(), full.bottom() - film_h));
    app.canvas_rect = Some(canvas);
    if app.ui.filmstrip {
        filmstrip(app, ui, Rect::from_min_max(pos2(full.left(), canvas.bottom()), full.max));
    }
    let Some(id) = app.session.active() else {
        super::empty_message(ui, canvas, "No photo selected", "Choose a photo in the grid or filmstrip");
        return;
    };
    let Some(photo) = app.session.catalog.photo(id).cloned() else { return };
    let d = (*photo.develop).clone();
    let crop_tool = app.ui.right == RightPanel::Crop;
    let frame = Frame::with_lens(photo.width.max(1) as usize, photo.height.max(1) as usize, &d, !crop_tool, photo.embedded_lens.as_ref());
    let aspect = frame.aspect() as f32;
    let ppp = ui.ctx().pixels_per_point();
    let area = canvas.shrink(if crop_tool { 48.0 } else { 24.0 });
    let native = [photo.width.max(1) as usize, photo.height.max(1) as usize];
    // two views (before, after): side by side or stacked
    let split = matches!(app.ui.before_after, BeforeAfter::SideBySide | BeforeAfter::TopBottom);
    let split_view = matches!(app.ui.before_after, BeforeAfter::Split | BeforeAfter::SplitTopBottom);
    let areas: Vec<Rect> = match app.ui.before_after {
        BeforeAfter::SideBySide => {
            let half = area.width() / 2.0 - 6.0;
            vec![
                Rect::from_min_size(area.min, vec2(half, area.height())),
                Rect::from_min_size(pos2(area.center().x + 6.0, area.top()), vec2(half, area.height())),
            ]
        }
        BeforeAfter::TopBottom => {
            let half = area.height() / 2.0 - 14.0;
            vec![
                Rect::from_min_size(area.min, vec2(area.width(), half)),
                Rect::from_min_size(pos2(area.left(), area.center().y + 14.0), vec2(area.width(), half)),
            ]
        }
        _ => vec![area],
    };
    let main_area = *areas.last().unwrap_or(&area);
    let img_rect = fit_rect(main_area, aspect, app.ui.zoom, native, ppp, app.ui.pan);
    app.image_rect = Some(img_rect);
    // request renders: the loupe at display resolution (drafts during drags)
    let interacting = app.session.interaction.is_some();
    let scale = if interacting { 0.6 } else { 1.0 };
    let want = (img_rect.width().max(img_rect.height()) * ppp * scale).min(2560.0) as usize;
    let (rw, rh) = if aspect >= 1.0 { (want, (want as f32 / aspect) as usize) } else { ((want as f32 * aspect) as usize, want) };
    if let Some(job) = app.session.loupe_job(id, rw.max(8), rh.max(8), !crop_tool) {
        let job = if interacting { job.draft() } else { job };
        app.renderer.request(Slot::Main, job, 100);
    }
    // once this photo is on screen: prepare its neighbours in filmstrip order (source decoded and
    // kept, view render cached) so stepping to them is instant
    if !interacting && !app.renderer.is_pending(Slot::Main) && app.renderer.textures.get(&Slot::Main).is_some_and(|t| t.photo == id) {
        let ids = app.session.visible_cloned();
        if let Some(i) = ids.iter().position(|p| *p == id) {
            let next = ids.get(i + 1).copied();
            let prev = i.checked_sub(1).and_then(|j| ids.get(j)).copied();
            for (n, nid) in [next, prev].into_iter().enumerate() {
                let Some(nid) = nid else { continue };
                let Some(np) = app.session.catalog.photo(nid).cloned() else { continue };
                let nf = Frame::with_lens(np.width.max(1) as usize, np.height.max(1) as usize, &np.develop, !crop_tool, np.embedded_lens.as_ref());
                let na = nf.aspect() as f32;
                let nr = fit_rect(main_area, na, app.ui.zoom, [np.width.max(1) as usize, np.height.max(1) as usize], ppp, app.ui.pan);
                let nw = (nr.width().max(nr.height()) * ppp).min(2560.0) as usize;
                let (w, h) = if na >= 1.0 { (nw, (nw as f32 / na) as usize) } else { ((nw as f32 * na) as usize, nw) };
                if let Some(job) = app.session.loupe_job(nid, w.max(8), h.max(8), !crop_tool) {
                    app.renderer.prefetch(Slot::Prefetch(n as u8), job, PREFETCH_PRIORITY);
                }
            }
        }
    }
    // until the loupe has this photo: show its cached render / embedded preview / a thumbnail
    if app.renderer.textures.get(&Slot::Main).is_none_or(|t| t.photo != id)
        && let Some(q) = app.session.quick_view_job(id, want.max(8), !crop_tool)
    {
        app.renderer.request_quick(Slot::Preview, q, 110);
    }
    let show_before = app.ui.before_after == BeforeAfter::Original || ui.input(|i| i.key_down(egui::Key::Backslash));
    if (split || show_before || split_view)
        && let Some(job) = app.session.render_job(id, rw.max(8), rh.max(8), true, !crop_tool)
    {
        app.renderer.request(Slot::Before, job, 90);
    }
    let p = ui.painter_at(canvas);
    // what a view slot shows: its own render of this photo, else the stand-ins (no blank frame
    // between photos, and the full render replaces them in place)
    let draw = |slot: Slot, r: Rect| -> &'static str {
        let mine = |s: Slot| app.renderer.textures.get(&s).filter(|t| t.photo == id);
        let (tex, what) = if let Some(t) = mine(slot) {
            (t, "render")
        } else if let Some(t) = mine(Slot::Preview) {
            (t, t.quick.map(quick_name).unwrap_or("preview"))
        } else if let Some(t) = app.renderer.thumb(id) {
            (t, "thumb")
        } else {
            return "none";
        };
        p.image(tex.tex.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        what
    };
    let shown;
    if split {
        let br = fit_rect(areas[0], aspect, app.ui.zoom, native, ppp, app.ui.pan);
        draw(Slot::Before, br);
        shown = draw(Slot::Main, img_rect);
        p.text(pos2(br.left(), br.bottom() + 14.0), Align2::LEFT_CENTER, "Before", t.font(12.0), t.text_dim);
        p.text(pos2(img_rect.left(), img_rect.bottom() + 14.0), Align2::LEFT_CENTER, "After", t.font(12.0), t.text_dim);
    } else if show_before {
        shown = draw(Slot::Before, img_rect);
        p.text(pos2(img_rect.left() + 8.0, img_rect.top() + 14.0), Align2::LEFT_CENTER, "Before", t.font(12.0), t.text);
    } else {
        shown = draw(Slot::Main, img_rect);
        if shown == "none" {
            p.text(canvas.center(), Align2::CENTER_CENTER, "Rendering…", t.font(13.0), t.text_dim);
        }
    }
    app.loupe_shown = Some((id, shown));
    if app.ui.before_after == BeforeAfter::Split {
        let mid = img_rect.center().x;
        if let Some(tex) = app.renderer.textures.get(&Slot::Before).filter(|t| t.photo == id) {
            let left = Rect::from_min_max(img_rect.min, pos2(mid, img_rect.bottom()));
            p.image(tex.tex.id(), left, Rect::from_min_max(pos2(0.0, 0.0), pos2(0.5, 1.0)), Color32::WHITE);
        }
        p.line_segment([pos2(mid, img_rect.top()), pos2(mid, img_rect.bottom())], Stroke::new(1.5, Color32::WHITE));
    }
    if app.ui.before_after == BeforeAfter::SplitTopBottom {
        let mid = img_rect.center().y;
        if let Some(tex) = app.renderer.textures.get(&Slot::Before).filter(|t| t.photo == id) {
            let top = Rect::from_min_max(img_rect.min, pos2(img_rect.right(), mid));
            p.image(tex.tex.id(), top, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 0.5)), Color32::WHITE);
        }
        p.line_segment([pos2(img_rect.left(), mid), pos2(img_rect.right(), mid)], Stroke::new(1.5, Color32::WHITE));
    }
    if app.ui.show_clipping && !show_before {
        clipping_overlay(app, &p, img_rect);
    }
    register(ui.ctx(), "canvas:image", img_rect);
    let map = CanvasMap::new(&frame, img_rect);
    let resp = ui.interact(canvas, egui::Id::new("loupe"), Sense::click_and_drag());
    match app.ui.right {
        RightPanel::Crop => crop_overlay(app, ui, &resp, &map, &frame, &d, id),
        RightPanel::Masking => mask_overlay(app, ui, &resp, &map, &d),
        RightPanel::Remove => remove_overlay(app, ui, &resp, &map, &d),
        _ => general_interaction(app, ui, &resp, &map, img_rect, canvas, native, aspect),
    }
    resp.context_menu(|ui| super::grid::context_menu(app, ui, id));
}

/// Neighbour prefetch: below on-screen thumbnails, above background thumbnail refreshes.
const PREFETCH_PRIORITY: u32 = 4;

fn quick_name(q: lightcraft_engine::media::QuickSource) -> &'static str {
    use lightcraft_engine::media::QuickSource;
    match q {
        QuickSource::Cached => "cached",
        QuickSource::Embedded => "embedded",
        QuickSource::Small => "small",
    }
}

fn clipping_overlay(app: &LightcraftApp, p: &egui::Painter, r: Rect) {
    // Highlight clipped regions using the histogram's extremes isn't spatial; show a subtle frame hint.
    if let Some(h) = app.renderer.textures.get(&Slot::Main).and_then(|t| t.histogram.as_ref()) {
        let (lo, hi) = h.clipping();
        if hi > 0.003 {
            p.rect_stroke(r, 0.0, Stroke::new(2.0, Color32::from_rgb(255, 60, 60)), StrokeKind::Outside);
        }
        if lo > 0.003 {
            p.rect_stroke(r.expand(3.0), 0.0, Stroke::new(2.0, Color32::from_rgb(60, 120, 255)), StrokeKind::Outside);
        }
    }
}

fn general_interaction(
    app: &mut LightcraftApp,
    ui: &mut egui::Ui,
    resp: &egui::Response,
    map: &CanvasMap,
    img: Rect,
    canvas: Rect,
    native: [usize; 2],
    aspect: f32,
) {
    if app.ui.tool == "wbPicker" {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        if resp.clicked()
            && let Some(q) = resp.interact_pointer_pos()
        {
            let n = map.norm(q);
            let _ = app.run("develop.wbPick", json!({"x": n.x, "y": n.y}));
            app.ui.tool.clear();
        }
        return;
    }
    // click toggles Fit ↔ 100 % at the clicked point; drag pans when zoomed
    let zoomed = img.width() > canvas.width() + 1.0 || img.height() > canvas.height() + 1.0;
    if resp.double_clicked() || (resp.clicked() && !zoomed) {
        if let Some(q) = resp.interact_pointer_pos() {
            let u = ((q.x - img.left()) / img.width()).clamp(0.0, 1.0);
            let v = ((q.y - img.top()) / img.height()).clamp(0.0, 1.0);
            app.ui.pan = (u, v);
        }
        app.ui.zoom = if matches!(app.ui.zoom, Zoom::Fit) { Zoom::Percent(100) } else { Zoom::Fit };
    } else if resp.clicked() && zoomed {
        app.ui.zoom = Zoom::Fit;
    }
    if zoomed {
        ui.ctx().set_cursor_icon(if resp.dragged() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
        if resp.dragged() {
            let dlt = resp.drag_delta();
            app.ui.pan.0 = (app.ui.pan.0 - dlt.x / img.width()).clamp(0.0, 1.0);
            app.ui.pan.1 = (app.ui.pan.1 - dlt.y / img.height()).clamp(0.0, 1.0);
        }
    }
    let _ = (native, aspect);
}

// ------------------------------------------------------------------------ crop

fn crop_overlay(app: &mut LightcraftApp, ui: &mut egui::Ui, resp: &egui::Response, map: &CanvasMap, frame: &Frame, d: &DevelopSettings, id: PhotoId) {
    if app.ui.tool == "guidedUpright" {
        guided_overlay(app, ui, resp, map, frame, d);
        return;
    }
    if app.ui.tool == "straighten" {
        straighten_overlay(app, ui, resp);
        return;
    }
    let quad = frame_crop_quad(d, frame);
    let pts: Vec<Pos2> = quad.iter().map(|q| map.screen(*q)).collect();
    let p = ui.painter();
    // darken outside the crop
    let img = map.rect;
    let shade = Color32::from_black_alpha(150);
    let mut mesh = egui::epaint::Mesh::default();
    let outer = [img.left_top(), img.right_top(), img.right_bottom(), img.left_bottom()];
    for i in 0..4 {
        let a = outer[i];
        let b = outer[(i + 1) % 4];
        let c = pts[(i + 1) % 4];
        let dd = pts[i];
        let base = mesh.vertices.len() as u32;
        for v in [a, b, c, dd] {
            mesh.vertices.push(egui::epaint::Vertex { pos: v, uv: Pos2::ZERO, color: shade });
        }
        mesh.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    p.add(mesh);
    p.add(egui::Shape::closed_line(pts.clone(), Stroke::new(1.0, Color32::WHITE)));
    // overlay guides
    let lerp = |a: Pos2, b: Pos2, t: f32| a + (b - a) * t;
    let guide = Stroke::new(0.8, Color32::from_white_alpha(150));
    let fracs: Vec<f32> = match app.ui.crop_overlay {
        CropOverlay::Thirds => vec![1.0 / 3.0, 2.0 / 3.0],
        CropOverlay::Grid => (1..8).map(|i| i as f32 / 8.0).collect(),
        CropOverlay::Golden => vec![0.382, 0.618],
        CropOverlay::Diagonal | CropOverlay::None => vec![],
    };
    for f in fracs {
        p.line_segment([lerp(pts[0], pts[1], f), lerp(pts[3], pts[2], f)], guide);
        p.line_segment([lerp(pts[0], pts[3], f), lerp(pts[1], pts[2], f)], guide);
    }
    // handles: 0..3 corners, 4..7 edges (top, right, bottom, left)
    let handles: Vec<Pos2> = (0..8).map(|i| if i < 4 { pts[i] } else { lerp(pts[i - 4], pts[(i - 3) % 4], 0.5) }).collect();
    for (i, h) in handles.iter().enumerate() {
        register(ui.ctx(), format!("cropHandle:{i}"), Rect::from_center_size(*h, vec2(14.0, 14.0)));
        let s = if i < 4 { 12.0 } else { 9.0 };
        p.rect_filled(Rect::from_center_size(*h, vec2(s, 3.0)), 0.0, Color32::WHITE);
        p.rect_filled(Rect::from_center_size(*h, vec2(3.0, s)), 0.0, Color32::WHITE);
    }
    let inside = |q: Pos2| {
        let n = map.norm(q);
        let s = to_straight(n, d.crop.geometry.angle, frame);
        d.crop.geometry.rect.contains(Point::new(s.x.clamp(-1.0, 2.0), s.y))
    };
    if let Some(hq) = resp.hover_pos() {
        let near = handles.iter().position(|h| h.distance(hq) < 12.0);
        ui.ctx().set_cursor_icon(match near {
            Some(0 | 2) => egui::CursorIcon::ResizeNwSe,
            Some(1 | 3) => egui::CursorIcon::ResizeNeSw,
            Some(4 | 6) => egui::CursorIcon::ResizeVertical,
            Some(_) => egui::CursorIcon::ResizeHorizontal,
            None if inside(hq) => egui::CursorIcon::Move,
            None => egui::CursorIcon::Alias,
        });
    }
    if resp.drag_started()
        && let Some(q) = resp.interact_pointer_pos()
    {
        let _ = app.run("develop.beginInteraction", json!({"label": "Crop"}));
        app.gesture = Some(match handles.iter().position(|h| h.distance(q) < 12.0) {
            Some(h) => Gesture::CropHandle { handle: h as u8, start: d.crop.geometry.rect, angle: d.crop.geometry.angle },
            None if inside(q) => Gesture::CropHandle { handle: 8, start: d.crop.geometry.rect, angle: d.crop.geometry.angle },
            None => {
                let c = map.screen(Point::new(0.5, 0.5));
                Gesture::CropRotate { start_angle: d.crop.geometry.angle, a0: (q - c).angle() }
            }
        });
    }
    if resp.dragged()
        && let Some(q) = resp.interact_pointer_pos()
    {
        match app.gesture.clone() {
            Some(Gesture::CropHandle { handle, start, angle }) => {
                let n = to_straight(map.norm(q), angle, frame);
                let o = resp.interact_pointer_pos().map(|q0| q0 - resp.drag_delta()).unwrap_or(q);
                let _ = o;
                let mut r = start;
                let orig = ui.input(|i| i.pointer.press_origin()).map(|q0| to_straight(map.norm(q0), angle, frame)).unwrap_or(n);
                let (dx, dy) = (n.x - orig.x, n.y - orig.y);
                match handle {
                    0 => (r.x0, r.y0) = (start.x0 + dx, start.y0 + dy),
                    1 => (r.x1, r.y0) = (start.x1 + dx, start.y0 + dy),
                    2 => (r.x1, r.y1) = (start.x1 + dx, start.y1 + dy),
                    3 => (r.x0, r.y1) = (start.x0 + dx, start.y1 + dy),
                    4 => r.y0 = start.y0 + dy,
                    5 => r.x1 = start.x1 + dx,
                    6 => r.y1 = start.y1 + dy,
                    7 => r.x0 = start.x0 + dx,
                    _ => r = start.translate(lightcraft_geom::Vec2::new(dx, dy)),
                }
                // aspect lock
                if let Some((aw, ah)) = d.crop.aspect
                    && handle < 4
                {
                    let (iw, ih) = (frame.ow, frame.oh);
                    let a = if (iw >= ih) == (aw >= ah) { aw as f64 / ah as f64 } else { ah as f64 / aw as f64 };
                    let w_px = (r.x1 - r.x0).abs() * iw;
                    let h_px = w_px / a;
                    let hn = h_px / ih;
                    if handle == 0 || handle == 1 {
                        r.y0 = r.y1 - hn;
                    } else {
                        r.y1 = r.y0 + hn;
                    }
                }
                let rr = lightcraft_geom::Rect::new(r.x0.min(r.x1), r.y0.min(r.y1), r.x0.max(r.x1), r.y0.max(r.y1));
                if rr.width() > 0.02 && rr.height() > 0.02 {
                    let _ = app.run("crop.set", json!({"rect": [rr.x0, rr.y0, rr.x1, rr.y1]}));
                }
            }
            Some(Gesture::CropRotate { start_angle, a0 }) => {
                let c = map.screen(Point::new(0.5, 0.5));
                let a = (q - c).angle();
                let ang = (start_angle + (a - a0).to_degrees() as f64).clamp(-45.0, 45.0);
                let _ = app.run("crop.straighten", json!({"angle": (ang * 100.0).round() / 100.0}));
            }
            _ => {}
        }
    }
    if resp.drag_stopped() {
        app.gesture = None;
        let _ = app.run("develop.endInteraction", json!({}));
    }
    let _ = id;
}

/// Guided Upright: draw up to four guides along lines that should be vertical or horizontal. Guides are
/// stored in lens-corrected (pre-perspective) coordinates, so they stay attached to the image as it warps.
fn guided_overlay(app: &mut LightcraftApp, ui: &mut egui::Ui, resp: &egui::Response, map: &CanvasMap, frame: &Frame, d: &DevelopSettings) {
    let p = ui.painter_at(map.rect.expand(8.0));
    let col = Color32::from_rgb(255, 196, 40);
    let draw = |a: Pos2, b: Pos2| {
        p.line_segment([a, b], Stroke::new(3.0, Color32::from_black_alpha(120)));
        p.line_segment([a, b], Stroke::new(1.5, col));
        for q in [a, b] {
            p.circle_filled(q, 4.0, col);
            p.circle_stroke(q, 4.0, Stroke::new(1.0, Color32::BLACK));
        }
    };
    for (i, (a, b)) in d.geometry.guides.iter().enumerate() {
        let (sa, sb) = (map.screen(frame.corrected_to_transformed(*a)), map.screen(frame.corrected_to_transformed(*b)));
        register(ui.ctx(), format!("uprightGuide:{i}"), Rect::from_two_pos(sa, sb));
        draw(sa, sb);
    }
    ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    if resp.drag_started()
        && let Some(q) = resp.interact_pointer_pos()
    {
        app.gesture = Some(Gesture::Guide { a: map.norm(q) });
    }
    if let (Some(Gesture::Guide { a }), Some(q)) = (&app.gesture, resp.interact_pointer_pos()) {
        draw(map.screen(*a), q);
    }
    if resp.drag_stopped()
        && let (Some(Gesture::Guide { a }), Some(q)) = (app.gesture.clone(), resp.interact_pointer_pos())
    {
        app.gesture = None;
        let b = map.norm(q);
        if a.dist(b) > 0.02 {
            let (ca, cb) = (frame.transformed_to_corrected(a), frame.transformed_to_corrected(b));
            let _ = app.run("geometry.guides", json!({"guides": [[ca.x, ca.y, cb.x, cb.y]], "add": true}));
        }
    }
}

/// Normalized oriented coords → the straightened (rotated) frame the crop rect lives in.
fn to_straight(n: Point, angle: f64, frame: &Frame) -> Point {
    let (w, h) = (frame.ow, frame.oh);
    let px = Point::new(n.x * w, n.y * h);
    let r = Affine::rotate_about(angle.to_radians(), Point::new(w / 2.0, h / 2.0)).apply(px);
    Point::new(r.x / w, r.y / h)
}

fn frame_crop_quad(d: &DevelopSettings, frame: &Frame) -> [Point; 4] {
    let f = Frame { crop: d.crop.geometry, ..frame.clone() };
    f.crop_quad_norm()
}

// ------------------------------------------------------------------------ masks

fn mask_overlay(app: &mut LightcraftApp, ui: &mut egui::Ui, resp: &egui::Response, map: &CanvasMap, d: &DevelopSettings) {
    let red = Color32::from_rgba_unmultiplied(230, 30, 40, 90);
    let long = (map.rect.width().max(map.rect.height())) as f64;
    let active = app.session.active_mask;
    let clip = app.canvas_rect.unwrap_or(map.rect);
    let p = &ui.painter_at(clip);
    for m in &d.masks {
        let sel = Some(m.id) == active;
        for c in &m.components {
            match &c.shape {
                MaskShape::Brush { strokes } if sel && app.ui.mask_overlay => {
                    for s in strokes {
                        let r = (s.size * long) as f32;
                        for q in &s.points {
                            p.circle_filled(map.screen(*q), r, if s.erase { Color32::from_black_alpha(60) } else { red });
                        }
                    }
                }
                MaskShape::Radial { center, rx, ry, angle, .. } => {
                    let c0 = map.screen(*center);
                    let pts: Vec<Pos2> = (0..64)
                        .map(|i| {
                            let a = i as f64 / 64.0 * std::f64::consts::TAU;
                            let (s, co) = angle.to_radians().sin_cos();
                            let (x, y) = (rx * a.cos(), ry * a.sin());
                            let l = frame_long_norm(map);
                            map.screen(Point::new(center.x + (x * co - y * s) * l.0, center.y + (x * s + y * co) * l.1))
                        })
                        .collect();
                    if sel && app.ui.mask_overlay {
                        p.add(egui::Shape::convex_polygon(pts.clone(), red, Stroke::NONE));
                    }
                    p.add(egui::Shape::closed_line(
                        pts,
                        Stroke::new(if sel { 1.5 } else { 1.0 }, Color32::from_white_alpha(if sel { 230 } else { 120 })),
                    ));
                    pin(p, c0, sel);
                    if sel {
                        register(ui.ctx(), format!("maskPin:{}", m.id), Rect::from_center_size(c0, vec2(14.0, 14.0)));
                    }
                }
                MaskShape::Linear { start, end } => {
                    let (a, b) = (map.screen(*start), map.screen(*end));
                    let dir = (b - a).normalized();
                    let perp = vec2(-dir.y, dir.x) * 3000.0;
                    for (q, alpha) in [(a, 230), (b, 110)] {
                        p.line_segment([q - perp, q + perp], Stroke::new(1.0, Color32::from_white_alpha(if sel { alpha } else { 80 })));
                    }
                    pin(p, a + (b - a) * 0.5, sel);
                    if sel {
                        for (i, q) in [a, b].iter().enumerate() {
                            p.circle_stroke(*q, 5.0, Stroke::new(1.5, Color32::WHITE));
                            register(ui.ctx(), format!("maskHandle:{}:{i}", m.id), Rect::from_center_size(*q, vec2(14.0, 14.0)));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    // brush tool
    if app.ui.tool == "brush" {
        let r = (app.ui.brush_size as f64 * long) as f32;
        if let Some(h) = resp.hover_pos() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::None);
            p.circle_stroke(h, r, Stroke::new(1.0, Color32::WHITE));
            p.circle_stroke(h, r * (1.0 - app.ui.brush_feather / 100.0).max(0.05), Stroke::new(1.0, Color32::from_white_alpha(120)));
        }
        if let Some(q) = resp.interact_pointer_pos()
            && (resp.drag_started() || resp.dragged() || resp.clicked())
        {
            let n = map.norm(q);
            match &mut app.gesture {
                Some(Gesture::Brush { points }) => points.push(n),
                _ => app.gesture = Some(Gesture::Brush { points: vec![n] }),
            }
        }
        if let Some(Gesture::Brush { points }) = &app.gesture {
            for q in points {
                p.circle_filled(map.screen(*q), r, red);
            }
        }
        if (resp.drag_stopped() || resp.clicked())
            && let Some(Gesture::Brush { points }) = app.gesture.take()
        {
            let pts: Vec<[f64; 2]> = points.iter().map(|q| [q.x, q.y]).collect();
            let _ = app.run(
                "mask.brushStroke",
                json!({"points": pts, "size": app.ui.brush_size, "feather": app.ui.brush_feather, "flow": app.ui.brush_flow, "erase": app.ui.brush_erase}),
            );
        }
        return;
    }
    // drag pins/handles of the active mask
    let Some(mid) = active else { return };
    let Some(m) = d.masks.iter().find(|m| m.id == mid) else { return };
    let Some(comp) = m.components.first() else { return };
    if resp.drag_started()
        && let Some(q) = resp.interact_pointer_pos()
    {
        let handle = match &comp.shape {
            MaskShape::Radial { center, .. } => (map.screen(*center).distance(q) < 14.0).then_some(0),
            MaskShape::Linear { start, end } => {
                if map.screen(*start).distance(q) < 14.0 {
                    Some(1)
                } else if map.screen(*end).distance(q) < 14.0 {
                    Some(2)
                } else {
                    Some(0)
                }
            }
            _ => None,
        };
        if let Some(h) = handle {
            let _ = app.run("develop.beginInteraction", json!({"label": "Edit Mask"}));
            app.gesture = Some(Gesture::MaskHandle { mask: mid, handle: h });
        }
    }
    if resp.dragged()
        && let (Some(Gesture::MaskHandle { mask, handle }), Some(q)) = (app.gesture.clone(), resp.interact_pointer_pos())
    {
        let n = map.norm(q);
        let dn = {
            let q0 = q - resp.drag_delta();
            let n0 = map.norm(q0);
            Point::new(n.x - n0.x, n.y - n0.y)
        };
        let new_shape = match comp.shape.clone() {
            MaskShape::Radial { center, rx, ry, angle, feather, invert } => {
                MaskShape::Radial { center: Point::new(center.x + dn.x, center.y + dn.y), rx, ry, angle, feather, invert }
            }
            MaskShape::Linear { start, end } => match handle {
                1 => MaskShape::Linear { start: n, end },
                2 => MaskShape::Linear { start, end: n },
                _ => MaskShape::Linear { start: Point::new(start.x + dn.x, start.y + dn.y), end: Point::new(end.x + dn.x, end.y + dn.y) },
            },
            s => s,
        };
        let _ = app.run("mask.update", json!({"id": mask, "shape": new_shape}));
    }
    if resp.drag_stopped() && matches!(app.gesture, Some(Gesture::MaskHandle { .. })) {
        app.gesture = None;
        let _ = app.run("develop.endInteraction", json!({}));
    }
}

/// Long-edge units → normalized units for x and y.
fn frame_long_norm(map: &CanvasMap) -> (f64, f64) {
    // derive from the mapping: 1 normalized unit in x and y in screen points
    let o = map.screen(Point::new(0.0, 0.0));
    let x = map.screen(Point::new(1.0, 0.0)).distance(o) as f64;
    let y = map.screen(Point::new(0.0, 1.0)).distance(o) as f64;
    let l = x.max(y);
    (l / x.max(1e-9), l / y.max(1e-9))
}

fn pin(p: &egui::Painter, c: Pos2, sel: bool) {
    p.circle_filled(c, 6.0, if sel { Color32::from_rgb(1, 101, 221) } else { Color32::from_gray(200) });
    p.circle_stroke(c, 6.0, Stroke::new(1.5, Color32::WHITE));
}

// ------------------------------------------------------------------------ remove

fn remove_overlay(app: &mut LightcraftApp, ui: &mut egui::Ui, resp: &egui::Response, map: &CanvasMap, d: &DevelopSettings) {
    let long = (map.rect.width().max(map.rect.height())) as f64;
    let p = ui.painter();
    for s in &d.spots {
        let r = (s.size * long) as f32;
        for q in &s.points {
            p.circle_stroke(map.screen(*q), r, Stroke::new(1.0, Color32::from_white_alpha(200)));
        }
    }
    let r = (app.ui.remove_size as f64 * long) as f32;
    if let Some(h) = resp.hover_pos() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::None);
        p.circle_stroke(h, r, Stroke::new(1.0, Color32::WHITE));
    }
    if let Some(q) = resp.interact_pointer_pos()
        && (resp.drag_started() || resp.dragged() || resp.clicked())
    {
        let n = map.norm(q);
        match &mut app.gesture {
            Some(Gesture::Spot { points }) => points.push(n),
            _ => app.gesture = Some(Gesture::Spot { points: vec![n] }),
        }
    }
    if let Some(Gesture::Spot { points }) = &app.gesture {
        for q in points {
            p.circle_filled(map.screen(*q), r, Color32::from_white_alpha(60));
        }
    }
    if (resp.drag_stopped() || resp.clicked())
        && let Some(Gesture::Spot { points }) = app.gesture.take()
    {
        let mode = match app.ui.tool.as_str() {
            "heal" => "heal",
            "clone" => "clone",
            _ => "remove",
        };
        let pts: Vec<[f64; 2]> = points.iter().map(|q| [q.x, q.y]).collect();
        let _ = app.run("spot.add", json!({"mode": mode, "points": pts, "size": app.ui.remove_size}));
    }
}

// ------------------------------------------------------------------------ filmstrip

fn filmstrip(app: &mut LightcraftApp, ui: &mut egui::Ui, r: Rect) {
    let t = Tokens::get(ui.ctx());
    ui.painter().rect_filled(r, 0.0, t.canvas);
    ui.painter().rect_filled(Rect::from_min_size(r.min, vec2(r.width(), 4.0)), 0.0, Color32::from_gray(0x20));
    let ids = app.session.visible_cloned();
    let active = app.session.selection.active;
    let cell_w = 120.0;
    let ppp = ui.ctx().pixels_per_point();
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(r.shrink2(vec2(0.0, 4.0))));
    let scroll_key = egui::Id::new("film-last-active");
    let last: Option<PhotoId> = child.data(|d| d.get_temp(scroll_key));
    child.data_mut(|d| d.insert_temp(scroll_key, active));
    egui::ScrollArea::horizontal().id_salt("filmstrip").auto_shrink([false, false]).show_viewport(&mut child, |ui, vp| {
        let (area, _) = ui.allocate_exact_size(vec2(ids.len() as f32 * cell_w, r.height() - 16.0), Sense::hover());
        for (i, id) in ids.iter().enumerate() {
            let cr = Rect::from_min_size(pos2(area.left() + i as f32 * cell_w, area.top()), vec2(cell_w, area.height()));
            if Some(*id) == active && last != active {
                ui.scroll_to_rect(cr, Some(egui::Align::Center));
            }
            let local = Rect::from_min_size(pos2(i as f32 * cell_w, 0.0), cr.size());
            if !local.intersects(vp.expand2(vec2(cell_w * 4.0, 0.0))) {
                continue;
            }
            let resp = ui.interact(cr, egui::Id::new(("film", id.0)), Sense::click());
            register(ui.ctx(), format!("film:{}", id.0), cr);
            let sel = Some(*id) == active;
            let p = ui.painter();
            if sel {
                p.rect_filled(cr, 0.0, t.cell_selected);
            } else if resp.hovered() {
                p.rect_filled(cr, 0.0, t.cell_selected.gamma_multiply(0.6));
            }
            if let Some(ph) = app.session.catalog.photo(*id) {
                let name = ph.file_name.rsplit_once('.').map(|(n, _)| n).unwrap_or(&ph.file_name);
                let short: String = if name.len() > 14 { format!("{}…", &name[..13]) } else { name.to_string() };
                p.text(pos2(cr.left() + 8.0, cr.top() + 10.0), Align2::LEFT_CENTER, short, t.font(10.0), t.text_dim);
                p.text(pos2(cr.right() - 8.0, cr.top() + 10.0), Align2::RIGHT_CENTER, &ph.format, t.semibold(8.5), t.text_dim);
            }
            let img_area = Rect::from_min_max(cr.min + vec2(10.0, 22.0), cr.max - vec2(10.0, 8.0));
            super::grid::request_thumb(app, *id, (256.0 * ppp.min(2.0) / 2.0) as usize * 2, 8);
            if let Some(tex) = app.renderer.thumb(*id) {
                let [tw, th] = tex.size;
                let s = (img_area.width() / tw as f32).min(img_area.height() / th as f32);
                let fr = Rect::from_center_size(img_area.center(), vec2(tw as f32 * s, th as f32 * s));
                p.image(tex.tex.id(), fr, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                if sel {
                    p.rect_stroke(fr, 0.0, Stroke::new(1.5, Color32::WHITE), StrokeKind::Outside);
                }
            }
            if resp.clicked() {
                let m = ui.input(|i| i.modifiers);
                let mode = if m.command {
                    "toggle"
                } else if m.shift {
                    "range"
                } else {
                    "replace"
                };
                let _ = app.run("library.select", json!({"ids": [id.0], "mode": mode}));
            }
        }
    });
}

/// Straighten tool: drag along a horizon (or a vertical) to set the crop angle; double-click = Auto.
/// The image is shown unrotated in the crop view, so the line's on-screen angle is its image angle.
fn straighten_overlay(app: &mut LightcraftApp, ui: &mut egui::Ui, resp: &egui::Response) {
    ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    if resp.double_clicked() {
        let _ = app.run("crop.autoStraighten", json!({}));
        app.ui.tool.clear();
        app.gesture = None;
        return;
    }
    if resp.drag_started()
        && let Some(q) = ui.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos())
    {
        app.gesture = Some(Gesture::StraightenLine { a: q });
    }
    let Some(Gesture::StraightenLine { a }) = app.gesture.clone() else { return };
    let Some(b) = resp.interact_pointer_pos().or(resp.hover_pos()) else { return };
    let p = ui.painter();
    p.line_segment([a, b], Stroke::new(3.0, Color32::from_black_alpha(140)));
    p.line_segment([a, b], Stroke::new(1.5, Color32::WHITE));
    if resp.drag_stopped() {
        app.gesture = None;
        let v = b - a;
        if v.length() > 8.0 {
            let mut deg = v.y.atan2(v.x).to_degrees() as f64;
            // fold to the nearest axis: a line closer to vertical straightens plumb lines
            while deg > 45.0 {
                deg -= 90.0;
            }
            while deg < -45.0 {
                deg += 90.0;
            }
            let _ = app.run("crop.straighten", json!({"angle": (-deg * 100.0).round() / 100.0}));
        }
        app.ui.tool.clear();
    }
}
