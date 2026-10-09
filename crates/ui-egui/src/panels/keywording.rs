//! The Keywording box (the top of the right panel's Keywords): the keywords of the selected photos,
//! not only the active one's, as Lightroom Classic shows them. A keyword only some of the selected
//! photos have is marked as such.

use lightcraft_catalog::{Catalog, PhotoId};
use serde_json::json;

use crate::LightcraftApp;
use crate::theme::Tokens;
use crate::widgets::register;

/// The selection's keywords as chips: the name (right-click for its menu), and × to take it off
/// every selected photo. One only some of them have is marked with an asterisk.
pub fn chip_row(app: &mut LightcraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let selection = app.session.selection.ids.clone();
    let chips = app.caches.keyword_chips(&app.session.catalog, &selection);
    ui.horizontal_wrapped(|ui| {
        for chip in chips.iter() {
            let frame = egui::Frame::NONE.stroke(egui::Stroke::new(1.0, t.field_border)).corner_radius(10.0).inner_margin(egui::Margin {
                left: 8,
                right: 4,
                top: 2,
                bottom: 2,
            });
            let shown = frame.show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let partial = !chip.on_all();
                let name = format!("{}{}", chip.path.replace('|', " › "), if partial { " *" } else { "" });
                let color = if partial { t.text_dim } else { t.text_label };
                let label = ui.add(egui::Label::new(egui::RichText::new(name).color(color)).sense(egui::Sense::click()));
                register(ui.ctx(), format!("keywordChip:{}", chip.path), label.rect);
                let label = if partial {
                    register(ui.ctx(), format!("keywordChipPartial:{}", chip.path), label.rect);
                    label.on_hover_text(crate::i18n::tr_format!("On {have} of {of} selected photos", have = chip.have, of = chip.of))
                } else {
                    label.on_hover_text(chip.path.replace('|', " › "))
                };
                let x = ui.add(egui::Button::new(egui::RichText::new("×").color(t.text_dim)).frame(false));
                register(ui.ctx(), format!("keywordChipRemove:{}", chip.path), x.rect);
                if x.on_hover_text(crate::i18n::tr("Remove from Selected Photos")).clicked() {
                    let _ = app.run("photo.setMeta", json!({"removeKeywords": [chip.path]}));
                }
                label
            });
            let k = chip.path.clone();
            shown.inner.context_menu(|ui| {
                if ui.button(crate::i18n::tr("Remove from Photo")).clicked() {
                    let _ = app.run("photo.setMeta", json!({"removeKeywords": [k]}));
                }
                if ui.button(crate::i18n::tr("Show Photos with Keyword")).clicked() {
                    let _ = app.run("library.filter", json!({"keyword": k}));
                }
                ui.separator();
                if ui.button(crate::i18n::tr("Rename Keyword…")).clicked() {
                    app.ui.dialog = Some(crate::state::Dialog::RenameKeyword { from: k.clone(), to: k.clone() });
                }
                if ui.button(crate::i18n::tr("Delete Keyword")).clicked() {
                    let _ = app.run("keyword.delete", json!({"keyword": k}));
                }
            });
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
}
