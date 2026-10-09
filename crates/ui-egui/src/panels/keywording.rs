//! The Keywording box (the top of the right panel's Keywords): the keywords of the selected photos,
//! not only the active one's, as Lightroom Classic shows them. A keyword only some of the selected
//! photos have is marked as such.

use lightcraft_catalog::{Catalog, PhotoId};
use serde_json::json;

use crate::LightcraftApp;
use crate::theme::Tokens;
use crate::widgets::register;

/// The Keywording box's view: the keywords (chips), or what exported files will carry.
pub fn view_switch(app: &mut LightcraftApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        for (export, id, label) in [(false, "keywords", "Keywords"), (true, "willExport", "Will Export")] {
            let r = ui.selectable_label(app.ui.keywording_will_export == export, crate::i18n::tr(label));
            register(ui.ctx(), format!("keywordView:{id}"), r.rect);
            if r.clicked() {
                app.ui.keywording_will_export = export;
            }
        }
    });
}

/// What exported files will carry for the selection, read only: one name only some of the photos
/// carry is marked with an asterisk.
pub fn export_row(app: &mut LightcraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let selection = app.session.selection.ids.clone();
    let names = app.caches.keyword_export(&app.session.catalog, &selection);
    if names.is_empty() {
        ui.label(egui::RichText::new(crate::i18n::tr("No keywords are exported")).color(t.text_dim));
        return;
    }
    ui.horizontal_wrapped(|ui| {
        for n in names.iter() {
            let (body, _) = chip(ui, &n.path, !n.on_all(), false);
            register(ui.ctx(), format!("keywordExport:{}", n.path), body.rect);
            if !n.on_all() {
                body.on_hover_text(crate::i18n::tr_format!("On {have} of {of} selected photos", have = n.have, of = n.of));
            }
        }
    });
}

/// One chip, measured before it is placed so that a wrapping row moves it whole to the next line:
/// the name (cut short to fit the row) and, with `cross`, a × at its end. A `partial` one (only
/// some of the selected photos) is dimmed and marked with an asterisk. Returns the name's response
/// and the ×'s.
fn chip(ui: &mut egui::Ui, path: &str, partial: bool, cross: bool) -> (egui::Response, Option<egui::Response>) {
    use egui::{Align2, Rect, Sense, pos2, vec2};
    let t = Tokens::get(ui.ctx());
    let font = t.font(13.0);
    let color = if partial { t.text_dim } else { t.text_label };
    let name = format!("{}{}", path.replace('|', " › "), if partial { " *" } else { "" });
    let cross_w = if cross { 18.0 } else { 0.0 };
    // at most the row's width
    let room = (ui.max_rect().width() - 16.0 - cross_w).max(24.0);
    let measure = |s: &str| ui.painter().layout_no_wrap(s.to_string(), font.clone(), color).size().x;
    let shown = if measure(&name) <= room { name.clone() } else { crate::widgets::elide_head(&name, room, measure) };
    let w = measure(&shown) + 16.0 + cross_w;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 22.0), Sense::click());
    ui.painter().rect_stroke(rect, 10.0, egui::Stroke::new(1.0, t.field_border), egui::StrokeKind::Inside);
    if resp.hovered() {
        ui.painter().rect_filled(rect.shrink(1.0), 10.0, t.hover.gamma_multiply(0.5));
    }
    ui.painter().text(pos2(rect.left() + 8.0, rect.center().y), Align2::LEFT_CENTER, &shown, font.clone(), color);
    let body = Rect::from_min_max(rect.min, pos2(rect.right() - cross_w, rect.bottom()));
    let resp = if shown != name { resp.on_hover_text(path.replace('|', " › ")) } else { resp };
    let x = cross.then(|| {
        let xr = Rect::from_center_size(pos2(rect.right() - 11.0, rect.center().y), vec2(14.0, 14.0));
        let xresp = ui.interact(xr, resp.id.with("remove"), Sense::click());
        ui.painter().text(xr.center(), Align2::CENTER_CENTER, "×", t.font(13.0), if xresp.hovered() { t.text } else { t.text_dim });
        xresp
    });
    let mut body_resp = resp;
    body_resp.rect = body;
    (body_resp, x)
}

/// A chip's menu: it acts on the selected photos (deleting a keyword from the whole library is the
/// Keyword List's).
fn menu(app: &mut LightcraftApp, ui: &mut egui::Ui, chip: &Chip) {
    let item = |ui: &mut egui::Ui, id: &str, label: &str| {
        let r = ui.button(label);
        register(ui.ctx(), format!("keywordChipMenu:{id}"), r.rect);
        r.clicked()
    };
    if !chip.on_all() && item(ui, "add", crate::i18n::tr("Add to All Selected Photos")) {
        let _ = app.run("photo.setMeta", json!({"addKeywords": [chip.path]}));
    }
    if item(ui, "remove", crate::i18n::tr("Remove from Selected Photos")) {
        let _ = app.run("photo.setMeta", json!({"removeKeywords": [chip.path]}));
    }
    if item(ui, "show", crate::i18n::tr("Show Photos with Keyword")) {
        super::left::browse_all_photos(app, true);
        let _ = app.run("library.filter", json!({"keyword": chip.path}));
    }
    ui.separator();
    if item(ui, "edit", crate::i18n::tr("Edit Keyword Tag…")) {
        app.ui.dialog = Some(super::keyword_list::edit_dialog(app, &chip.path));
    }
}

/// The selection's keywords as chips: the name (right-click for its menu), and × to take it off
/// every selected photo. One only some of them have is marked with an asterisk.
pub fn chip_row(app: &mut LightcraftApp, ui: &mut egui::Ui) {
    let selection = app.session.selection.ids.clone();
    let chips = app.caches.keyword_chips(&app.session.catalog, &selection);
    ui.horizontal_wrapped(|ui| {
        for c in chips.iter() {
            let (body, x) = chip(ui, &c.path, !c.on_all(), true);
            register(ui.ctx(), format!("keywordChip:{}", c.path), body.rect);
            if let Some(x) = x {
                register(ui.ctx(), format!("keywordChipRemove:{}", c.path), x.rect);
                if x.on_hover_text(crate::i18n::tr("Remove from Selected Photos")).clicked() {
                    let _ = app.run("photo.setMeta", json!({"removeKeywords": [c.path]}));
                }
            }
            let body = if c.on_all() {
                body
            } else {
                register(ui.ctx(), format!("keywordChipPartial:{}", c.path), body.rect);
                body.on_hover_text(crate::i18n::tr_format!("On {have} of {of} selected photos", have = c.have, of = c.of))
            };
            let c = c.clone();
            body.context_menu(|ui| menu(app, ui, &c));
        }
    });
}

/// A keyword of the selection, as a chip shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Chip {
    /// The keyword, spelled as the library spells it.
    pub path: String,
    /// Selected photos that have it.
    pub have: usize,
    /// Selected photos.
    pub of: usize,
}

impl Chip {
    /// Every selected photo has it.
    pub fn on_all(&self) -> bool {
        self.have >= self.of
    }
}

/// "Will Export" (Lightroom Classic's Keyword Tags ▸ Will Export): the names exported files carry
/// for the selected `photos`, as the keyword tag options say, each with how many of them carry it;
/// by name.
pub(crate) fn will_export(catalog: &Catalog, photos: &[PhotoId]) -> Vec<Chip> {
    let mut have: std::collections::BTreeMap<String, (String, usize)> = Default::default();
    for id in photos {
        let Some(p) = catalog.photo(*id) else { continue };
        for name in catalog.export_keywords(&p.meta.keywords).flat {
            have.entry(name.to_lowercase()).or_insert_with(|| (name.clone(), 0)).1 += 1;
        }
    }
    have.into_values().map(|(path, have)| Chip { path, have, of: photos.len() }).collect()
}

/// The keywords of the selected `photos`, each once whatever its case, with how many of them have
/// it; by name.
pub(crate) fn chips(catalog: &Catalog, photos: &[PhotoId]) -> Vec<Chip> {
    use lightcraft_catalog::keywords::clean;
    // by lower-case keyword: the photos with it (each once)
    let mut have: std::collections::BTreeMap<String, (String, usize)> = Default::default();
    let mut seen = std::collections::HashSet::new();
    for id in photos {
        let Some(p) = catalog.photo(*id) else { continue };
        seen.clear();
        for k in &p.meta.keywords {
            let k = clean(k);
            if k.is_empty() || !seen.insert(k.to_lowercase()) {
                continue;
            }
            have.entry(k.to_lowercase()).or_insert_with(|| (k.clone(), 0)).1 += 1;
        }
    }
    have.into_values().map(|(k, n)| Chip { path: catalog.keyword_path(&k).unwrap_or(k), have: n, of: photos.len() }).collect()
}

#[cfg(test)]
mod tests {
    use lightcraft_catalog::{Op, Photo, Source};

    use super::*;

    fn library(keywords: &[&[&str]]) -> (Catalog, Vec<PhotoId>) {
        let mut c = Catalog::new();
        let mut ids = Vec::new();
        for k in keywords {
            let id = c.alloc_photo_id();
            let mut p = Photo::new(id, Source::Demo { scene: 1 }, "a.jpg", "JPEG", 3, 2, "2026-01-01");
            p.meta.keywords = k.iter().map(|s| s.to_string()).collect();
            c.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
            ids.push(id);
        }
        (c, ids)
    }

    fn shown(chips: &[Chip]) -> Vec<(&str, usize, usize)> {
        chips.iter().map(|c| (c.path.as_str(), c.have, c.of)).collect()
    }

    /// The chips are the keywords of every selected photo, by name, each saying how many of the
    /// selected photos have it.
    #[test]
    fn the_chips_are_the_selections_keywords() {
        let (c, ids) = library(&[&["travel|Italy", "beach"], &["beach", "Weddings"], &["sea"]]);
        assert_eq!(shown(&chips(&c, &ids[..2])), [("beach", 2, 2), ("travel|Italy", 1, 2), ("Weddings", 1, 2)]);
        assert!(chips(&c, &ids[..2])[0].on_all() && !chips(&c, &ids[..2])[1].on_all());
        assert_eq!(shown(&chips(&c, &ids[2..])), [("sea", 1, 1)]);
        assert!(chips(&c, &[]).is_empty());
    }

    /// A keyword two photos spell differently is one chip, spelled as the library spells it.
    #[test]
    fn a_keyword_spelled_two_ways_is_one_chip() {
        let (c, ids) = library(&[&["Beach"], &["beach"], &["BEACH", "beach"]]);
        assert_eq!(shown(&chips(&c, &ids)), [("Beach", 3, 3)]);
    }

    /// "Will Export": the names exported files carry for the selection, as the keyword tag options
    /// say (a keyword left out goes, its parents and synonyms come), each with how many of the
    /// selected photos carry it.
    #[test]
    fn will_export_shows_what_exported_files_carry() {
        use lightcraft_catalog::keywords::KeywordInfo;
        let (mut c, ids) = library(&[&["Places|Lisbon", "draft"], &["Places|Lisbon"]]);
        let out = KeywordInfo { include_on_export: false, ..KeywordInfo::default() };
        c.apply(Op::SetKeyword { path: "Places".into(), info: Some(out.clone()) }).unwrap();
        c.apply(Op::SetKeyword { path: "draft".into(), info: Some(out) }).unwrap();
        c.apply(Op::SetKeyword {
            path: "Places|Lisbon".into(),
            info: Some(KeywordInfo { synonyms: vec!["Lisboa".into()], ..KeywordInfo::default() }),
        })
        .unwrap();
        assert_eq!(shown(&will_export(&c, &ids)), [("Lisboa", 2, 2), ("Lisbon", 2, 2)]);
        assert!(will_export(&c, &[]).is_empty());
    }
}
