//! Presentation-only localization. Command ids, user names and catalog data remain unchanged.
use std::{cell::Cell, collections::BTreeMap, sync::OnceLock};

use serde::{Deserialize, Serialize};

include!(concat!(env!("OUT_DIR"), "/formats.rs"));

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    #[default]
    #[serde(rename = "en")]
    En,
    #[serde(rename = "ja")]
    Ja,
    #[serde(rename = "zh-tw", alias = "zh-TW", alias = "zh-Hant", alias = "zh-hant")]
    ZhTw,
}

impl Language {
    pub const ALL: [Self; 3] = [Self::En, Self::Ja, Self::ZhTw];
    pub fn name(self) -> &'static str {
        match self {
            Self::En => "English",
            Self::Ja => "日本語",
            Self::ZhTw => "繁體中文（台灣）",
        }
    }
    pub fn parse(code: &str) -> Option<Self> {
        match code.to_ascii_lowercase().as_str() {
            "en" => Some(Self::En),
            "ja" => Some(Self::Ja),
            "zh-tw" | "zh-hant" => Some(Self::ZhTw),
            _ => None,
        }
    }
    pub fn tr(self, source: &str) -> &str {
        match self {
            Self::En => source,
            Self::Ja => japanese().get(source).map(String::as_str).unwrap_or(source),
            Self::ZhTw => traditional_chinese().get(source).map(String::as_str).unwrap_or(source),
        }
    }
}

thread_local! {
    static LANGUAGE: Cell<Language> = const { Cell::new(Language::En) };
}

pub fn default_language() -> Language {
    std::env::var("LIGHTCRAFT_LANGUAGE").ok().and_then(|code| Language::parse(&code)).unwrap_or_default()
}

pub fn set_language(language: Language) {
    LANGUAGE.with(|value| value.set(language));
}

pub fn language() -> Language {
    LANGUAGE.with(Cell::get)
}

/// Kept for callers of the original two-language API.
pub fn is_japanese() -> bool {
    language() == Language::Ja
}

fn japanese() -> &'static BTreeMap<String, String> {
    static MESSAGES: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    MESSAGES.get_or_init(|| {
        serde_json::from_str(include_str!("../locales/ja.json")).unwrap_or_else(|error| {
            log::error!("Invalid Japanese message catalog: {error}");
            BTreeMap::new()
        })
    })
}

fn traditional_chinese() -> &'static BTreeMap<String, String> {
    static MESSAGES: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    MESSAGES.get_or_init(|| {
        serde_json::from_str(include_str!("../locales/zh-tw.json")).unwrap_or_else(|error| {
            log::error!("Invalid Traditional Chinese message catalog: {error}");
            BTreeMap::new()
        })
    })
}

/// Translate a built-in display label, preserving unknown labels verbatim.
/// Never call this on editable user text, filenames or command identifiers.
pub fn tr(source: &str) -> &str {
    language().tr(source)
}

/// Display names from stock presets/profiles; imported and editable names remain verbatim.
pub fn builtin_label(source: &str, builtin: bool) -> &str {
    if builtin { tr(source) } else { source }
}

/// Display-only history labels. Preserve custom preset names within generated steps.
pub fn history_label<'a>(source: &'a str, presets: &'a [lightcraft_develop::Preset]) -> String {
    if let Some(name) = source.strip_prefix("Preset: ") {
        let builtin = presets.iter().any(|p| p.builtin && p.name == name) && !presets.iter().any(|p| !p.builtin && p.name == name);
        return tr_format!("Preset: {name}", name = builtin_label(name, builtin));
    }
    if let Some(name) = source.strip_prefix("Reset ") {
        return tr_format!("Reset {name}", name = tr(name));
    }
    if let Some(name) = source.strip_prefix("Quick Develop: ") {
        return tr_format!("Quick Develop: {name}", name = tr(name));
    }
    tr(source).to_string()
}

/// Built-in library headings, without translating an album's editable name.
pub fn source_label(source: lightcraft_engine::LibrarySource, catalog: &lightcraft_catalog::Catalog) -> String {
    let label = source.label(catalog);
    if matches!(source, lightcraft_engine::LibrarySource::Album(id) if catalog.album(id).is_some()) { label } else { tr(&label).to_string() }
}

/// Calendar headings for the grid and the narrower date sidebar. ISO keys stay unchanged.
pub fn date_group_label(key: &str, short: bool) -> String {
    let label = lightcraft_catalog::dates::group_label(key);
    if language() != Language::ZhTw {
        return if short {
            match key.len() {
                7 => label.split(' ').next().unwrap_or(&label).to_string(),
                10 => label.rsplitn(3, ' ').nth(2).unwrap_or(&label).to_string(),
                _ => label,
            }
        } else {
            label
        };
    }
    let Some(year) = key.get(..4).and_then(|y| y.parse::<u32>().ok()) else { return tr(&label).to_string() };
    if key.len() == 4 {
        return tr_format!("{year}", year = year);
    }
    let Some(month) = key.get(5..7).and_then(|m| m.parse::<u32>().ok()).filter(|m| (1..=12).contains(m)) else {
        return tr(&label).to_string();
    };
    if key.len() == 7 {
        return if short { tr_format!("{month}", month = month) } else { tr_format!("{month} {year}", month = month, year = year) };
    }
    if key.len() == 10
        && let Some(weekday) = lightcraft_catalog::dates::weekday(key)
        && let Some(day) = key.get(8..10).and_then(|d| d.parse::<u32>().ok())
    {
        let weekday = tr(weekday);
        return if short {
            tr_format!("{weekday}, {day}", weekday = weekday, day = day)
        } else {
            tr_format!("{weekday}, {day} {month} {year}", weekday = weekday, day = day, month = month, year = year)
        };
    }
    tr(&label).to_string()
}

/// Localized capture times for display; the source metadata is never rewritten.
pub fn display_time(iso: &str) -> String {
    if language() != Language::ZhTw || lightcraft_catalog::dates::normalize_iso(iso).is_none() {
        return lightcraft_catalog::dates::display_time(iso);
    }
    let date = date_group_label(iso.get(..10).unwrap_or(iso), false);
    match iso.get(11..).filter(|time| !time.is_empty()) {
        Some(time) => format!("{date} {time}"),
        None => date,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traditional_chinese_catalog_covers_commands_and_japanese_messages() {
        let messages = traditional_chinese();
        for key in japanese().keys() {
            assert!(messages.get(key).is_some_and(|value| !value.is_empty()), "missing: {key}");
        }
        for preset in lightcraft_engine::presets::builtin() {
            for key in [&preset.name, &preset.group] {
                assert!(messages.contains_key(key), "missing built-in preset label: {key}");
            }
        }
        for profile in lightcraft_engine::presets::PROFILES {
            for key in [profile.name, profile.group] {
                assert!(messages.contains_key(key), "missing profile label: {key}");
            }
        }
        for command in lightcraft_engine::command_specs() {
            assert!(messages.contains_key(command.label), "missing command: {}", command.label);
        }
        for (_, label, _, _) in crate::menus::UI_COMMANDS {
            assert!(messages.contains_key(*label), "missing UI command: {label}");
        }
    }

    #[test]
    fn traditional_chinese_switches_through_menu_and_control_and_persists() {
        use crate::control::{ControlRequest, Outcome};
        let mut app = crate::LightcraftApp::new(lightcraft_engine::Session::with_demo(), Default::default());
        app.run("app.language.traditionalChinese", serde_json::json!({})).unwrap();
        assert_eq!(app.ui.language, Language::ZhTw);
        assert_eq!(tr("File"), "檔案");
        assert_eq!(source_label(lightcraft_engine::LibrarySource::All, &app.session.catalog), "所有照片");
        assert_eq!(date_group_label("2026-09-20", false), "2026年9月20日 星期日");
        assert_eq!(date_group_label("2026-09-20", true), "20日 星期日");
        assert_eq!(date_group_label("2026-09", false), "2026年9月");
        assert_eq!(date_group_label("2026-09", true), "9月");
        assert_eq!(date_group_label("2026", false), "2026年");
        assert_eq!(date_group_label("", false), "未知日期");
        assert_eq!(display_time("2026-09-20T16:04:05"), "2026年9月20日 星期日 16:04:05");
        assert_eq!(display_time("a-user-value"), "a-user-value");
        assert_eq!(crate::menubar::checked(&app, "app.language.traditionalChinese"), Some(true));
        assert_eq!(crate::menubar::checked(&app, "app.language.english"), Some(false));
        assert_eq!(tr("my-photo.jpg"), "my-photo.jpg");
        assert_eq!(tr("develop.set"), "develop.set");
        for id in ["album.addPhotos", "metadata.applyPreset", "label.applySet"] {
            assert_eq!(crate::menubar::display_item_label(id, &serde_json::json!({"id": 1}), "Color"), "Color");
        }
        assert_eq!(crate::menubar::display_item_label("view.photoGrid", &serde_json::Value::Null, "Color"), "色彩");
        assert_eq!(tr_format!("{n} photo{}", "s", n = 12), "12 張照片");
        assert_eq!(tr_format!("Exported {ok} of {total} photo{}", "s", ok = 4, total = 12), "已匯出 4／12 張照片");
        let saved = serde_json::to_string(&app.ui).unwrap();
        assert_eq!(serde_json::from_str::<crate::UiState>(&saved).unwrap().language, Language::ZhTw);
        let ctx = egui::Context::default();
        for code in ["zh-tw", "zh-TW", "zh-hant", "zh-Hant"] {
            assert_eq!(Language::parse(code), Some(Language::ZhTw));
            let (req, _) = ControlRequest::new("ui.set", serde_json::json!({"language": code}));
            let Outcome::Done(reply) = crate::control::handle(&mut app, &ctx, &req) else { panic!("expected reply") };
            assert_eq!(reply["ok"], true);
            assert_eq!(app.ui.language, Language::ZhTw);
        }
        let (req, _) = ControlRequest::new("ui.set", serde_json::json!({"language": "xx"}));
        let Outcome::Done(reply) = crate::control::handle(&mut app, &ctx, &req) else { panic!("expected reply") };
        assert_eq!(reply["ok"], false);
        assert_eq!(app.ui.language, Language::ZhTw);
        let ids = |app: &crate::LightcraftApp| crate::menus::menu_entries(app).into_iter().map(|entry| entry.id).collect::<Vec<_>>();
        let chinese_ids = ids(&app);
        app.run("app.language.english", serde_json::json!({})).unwrap();
        assert_eq!(tr("File"), "File");
        assert_eq!(chinese_ids, ids(&app));
    }

    #[test]
    fn traditional_chinese_presets_and_panel_headers_are_painted() {
        let mut app = crate::LightcraftApp::new(lightcraft_engine::Session::with_demo(), Default::default());
        app.ui.language = Language::ZhTw;
        app.ui.presets = true;
        let builtin = app.session.presets.iter().find(|p| p.name == "Warm Glow").unwrap().clone();
        app.session.presets.push(lightcraft_develop::Preset { id: "user.test".into(), builtin: false, ..builtin });
        for (panel, title) in [(crate::state::RightPanel::Activity, "歷史紀錄"), (crate::state::RightPanel::Versions, "版本")] {
            app.ui.right = panel;
            let text = painted_text(&mut app);
            assert!(text.contains(title), "{text}");
            assert!(!text.contains("History"), "{text}");
            assert!(!text.contains("Versions"), "{text}");
            assert!(text.contains("暖光"), "built-in preset: {text}");
            assert!(text.contains("Warm Glow"), "user preset: {text}");
            assert!(text.contains("色彩"), "built-in group: {text}");
            assert!(text.contains("Color"), "user group: {text}");
        }
        assert_eq!(history_label("Exposure", &app.session.presets), "曝光");
        assert_eq!(history_label("Preset: Warm Film", &app.session.presets), "預設集：暖調底片");
        assert_eq!(history_label("Preset: Warm Glow", &app.session.presets), "預設集：Warm Glow");
        set_language(Language::En);
    }

    fn painted_text(app: &mut crate::LightcraftApp) -> String {
        fn collect(shape: &egui::epaint::Shape, text: &mut String) {
            match shape {
                egui::epaint::Shape::Text(shape) => {
                    text.push_str(&shape.galley.job.text);
                    text.push('\n');
                }
                egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, text)),
                _ => {}
            }
        }
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut text = String::new();
        for frame in 0..4 {
            let input = crate::headless::HeadlessView::raw_input(egui::vec2(1600.0, 1000.0), 1.0, frame as f64 / 60.0, vec![]);
            let mut out = ctx.run_ui(input, |ui| {
                app.logic(ui.ctx());
                app.ui(ui);
            });
            out.textures_delta.clear();
            text.clear();
            for shape in out.shapes {
                collect(&shape.shape, &mut text);
            }
        }
        text
    }

    #[test]
    fn traditional_chinese_fonts_cover_static_and_dynamic_catalogs() {
        if lightcraft_engine::fonts::japanese(lightcraft_engine::CRAFT_FONTS).next().is_none() {
            eprintln!("skipped glyph coverage: build with CRAFT_FONTS_DIR");
            return;
        }
        let formats: BTreeMap<String, String> = serde_json::from_str(include_str!("../locales/zh-tw-formats.json")).unwrap();
        let ctx = fonts_ctx(lightcraft_engine::CRAFT_FONTS);
        ctx.fonts_mut(|fonts| {
            for family in ui_families() {
                let font = egui::FontId::new(13.0, family);
                let mut missing = std::collections::BTreeSet::new();
                for message in traditional_chinese().values().chain(formats.values()).map(String::as_str).chain([Language::ZhTw.name()]) {
                    // egui reports false for glyphs of Monospace's first (replacement)
                    // face, Hack. Check its CJK fallback; proportional/semibold check all text.
                    for ch in message.chars().filter(|ch| !ch.is_whitespace()) {
                        if font.family == egui::FontFamily::Monospace && !('\u{4e00}'..='\u{9fff}').contains(&ch) {
                            continue;
                        }
                        if !fonts.has_glyph(&font, ch) {
                            missing.insert(ch);
                        }
                    }
                }
                assert!(missing.is_empty(), "Missing glyphs in {font:?}: {missing:?}");
            }
        });
    }

    #[test]
    fn catalog_is_valid_and_contains_core_workflows() {
        let messages: BTreeMap<String, String> = serde_json::from_str(include_str!("../locales/ja.json")).unwrap();
        for key in ["Import Photos…", "Export…", "Exposure", "White Balance", "Settings", "Language"] {
            assert!(messages.get(key).is_some_and(|value| !value.is_empty() && value != key), "{key}");
        }
    }

    #[test]
    fn language_switches_and_unknown_text_survives() {
        set_language(Language::Ja);
        assert_eq!(tr("Exposure"), "露出");
        assert_eq!(tr("my-photo.jpg"), "my-photo.jpg");
        assert_eq!(tr("develop.set"), "develop.set");
        assert_eq!(crate::menubar::display_item_label("album.addPhotos", &serde_json::json!({"id": 1}), "Color"), "Color");
        assert_eq!(crate::menubar::display_item_label("app.export", &serde_json::json!({"preset": "Color"}), "Color"), "Color");
        assert_eq!(crate::menubar::display_item_label("view.photoGrid", &serde_json::Value::Null, "Color"), "カラー");
        set_language(Language::En);
        assert_eq!(tr("Exposure"), "Exposure");
    }

    #[test]
    fn translated_formats_preserve_counts_and_remove_english_plural_suffixes() {
        set_language(Language::Ja);
        assert_eq!(tr_format!("{n} photo{}", "s", n = 12), "12枚");
        assert_eq!(tr_format!("Exported {ok} of {total} photo{}", "s", ok = 4, total = 12), "12枚中4枚を書き出しました");
        set_language(Language::En);
        assert_eq!(tr_format!("{n} photo{}", "s", n = 12), "12 photos");
    }

    #[test]
    fn preferences_round_trip_and_old_settings_remain_readable() {
        let old: crate::state::UiState = serde_json::from_str("{}").unwrap();
        assert_eq!(old.language, Language::En);
        let settings = crate::state::UiState { language: Language::Ja, ..old };
        let saved = serde_json::to_string(&settings).unwrap();
        let restored: crate::state::UiState = serde_json::from_str(&saved).unwrap();
        assert_eq!(restored.language, Language::Ja);
    }

    #[test]
    fn japanese_is_painted_and_both_font_weights_cover_the_catalog() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut app = crate::LightcraftApp::new(lightcraft_engine::Session::with_demo(), Default::default());
        app.ui.language = Language::Ja;
        app.ui.left_panel = true;
        let mut text = String::new();
        fn collect(shape: &egui::epaint::Shape, text: &mut String) {
            match shape {
                egui::epaint::Shape::Text(shape) => {
                    text.push_str(&shape.galley.job.text);
                    text.push('\n');
                }
                egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, text)),
                _ => {}
            }
        }
        for frame in 0..4 {
            let input = crate::headless::HeadlessView::raw_input(egui::vec2(1600.0, 1000.0), 1.0, frame as f64 / 60.0, vec![]);
            let mut out = ctx.run_ui(input, |ui| {
                app.logic(ui.ctx());
                app.ui(ui);
            });
            // This assertion inspects shapes without a renderer; discard texture uploads explicitly.
            out.textures_delta.clear();
            text.clear();
            for shape in out.shapes {
                collect(&shape.shape, &mut text);
            }
        }
        assert!(text.contains("マイフォト"), "{text}");
        assert!(text.contains("すべての写真"), "{text}");
        if lightcraft_engine::fonts::japanese(lightcraft_engine::CRAFT_FONTS).next().is_none() {
            eprintln!("skipped glyph coverage: built without CRAFT_FONTS_DIR, so there is no Japanese UI font");
        } else {
            ctx.fonts_mut(|fonts| {
                for family in [egui::FontFamily::Proportional, egui::FontFamily::Name(crate::theme::FONT_SEMIBOLD.into())] {
                    let font = egui::FontId::new(13.0, family);
                    for message in japanese().values() {
                        for ch in message.chars().filter(|ch| !ch.is_whitespace()) {
                            assert!(fonts.has_glyph(&font, ch), "Missing glyph {ch} in {message}");
                        }
                    }
                }
            });
        }
        // Locale affects presentation only: command ids remain the same.
        let ids = |app: &crate::LightcraftApp| crate::menus::menu_entries(app).into_iter().map(|entry| entry.id).collect::<Vec<_>>();
        let japanese_ids = ids(&app);
        set_language(Language::En);
        assert_eq!(japanese_ids, ids(&app));
    }
    /// The UI families, after one frame so the font definitions are loaded.
    fn fonts_ctx(craft: &'static [lightcraft_engine::CraftFont]) -> egui::Context {
        let ctx = egui::Context::default();
        ctx.set_fonts(crate::theme::font_definitions(craft));
        let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
        out.textures_delta.clear();
        ctx
    }

    fn ui_families() -> [egui::FontFamily; 3] {
        [egui::FontFamily::Proportional, egui::FontFamily::Name(crate::theme::FONT_SEMIBOLD.into()), egui::FontFamily::Monospace]
    }

    /// Built with craft-fonts, Japanese renders with real glyphs (no tofu) in every UI family.
    #[test]
    fn craft_fonts_render_japanese_in_the_ui() {
        if lightcraft_engine::fonts::japanese(lightcraft_engine::CRAFT_FONTS).next().is_none() {
            eprintln!("skipped: built without CRAFT_FONTS_DIR, so there is no Japanese UI font");
            return;
        }
        let ctx = fonts_ctx(lightcraft_engine::CRAFT_FONTS);
        ctx.fonts_mut(|fonts| {
            for family in ui_families() {
                let font = egui::FontId::new(13.0, family);
                for ch in "日本語の文字".chars() {
                    assert!(fonts.has_glyph(&font, ch), "{ch} in {font:?}");
                }
                let galley = fonts.layout_no_wrap("日本語の文字".into(), font.clone(), egui::Color32::WHITE);
                assert!(galley.size().x > 13.0 * 5.0, "{font:?}: six full-width glyphs, {:?}", galley.size());
            }
        });
    }

    /// Built without craft-fonts, the UI (in Japanese, too) still installs its fonts and runs;
    /// Latin text keeps Inter.
    #[test]
    fn the_ui_works_without_craft_fonts() {
        let ctx = fonts_ctx(&[]);
        ctx.fonts_mut(|fonts| {
            // (Not Monospace: egui's `has_glyph` reports false for glyphs of the family's
            // replacement-glyph face, which there is Hack, the face that draws Latin.)
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Name(crate::theme::FONT_SEMIBOLD.into())] {
                let font = egui::FontId::new(13.0, family);
                assert!("LightCraft".chars().all(|ch| fonts.has_glyph(&font, ch)), "{font:?}");
            }
        });
        let mut app = crate::LightcraftApp::new(lightcraft_engine::Session::with_demo(), Default::default());
        app.ui.language = Language::Ja;
        for frame in 0..3 {
            let input = crate::headless::HeadlessView::raw_input(egui::vec2(1200.0, 800.0), 1.0, frame as f64 / 60.0, vec![]);
            let mut out = ctx.run_ui(input, |ui| {
                app.logic(ui.ctx());
                app.ui(ui);
            });
            out.textures_delta.clear();
        }
        set_language(Language::En);
    }
}
