//! Keyboard shortcuts: parse `Cmd+Shift+X` style strings and dispatch UI and engine commands.

use egui::{Key, Modifiers};
use serde_json::json;

use crate::LightcraftApp;

/// Secondary key bindings for commands that already exist: `(shortcut, command id, params JSON)`.
/// They complement the primary shortcut declared on the command (Lightroom-desktop keys that our
/// primary keymap assigns elsewhere, see docs/parity.md → Shortcuts). Shown in Help → Keyboard Shortcuts.
pub const ALIASES: &[(&str, &str, &str)] = &[
    ("Cmd+D", "library.selectNone", "{}"),
    ("Shift+E", "dialog.export", "{}"),
    ("Space", "view.zoomToggle", "{}"),
    ("Shift+M", "version.create", "{}"),
    ("Shift+X", "photo.flag", r#"{"flag": "reject", "advance": true}"#),
    ("Shift+U", "photo.flag", r#"{"flag": "none", "advance": true}"#),
];

pub fn parse(s: &str) -> Option<(Modifiers, Key)> {
    let mut m = Modifiers::NONE;
    let mut key = None;
    for part in s.split('+') {
        match part {
            "Cmd" => m.command = true,
            "Shift" => m.shift = true,
            "Alt" => m.alt = true,
            "Ctrl" => m.ctrl = true,
            // "Delete" means the key labelled ⌫ (egui's Backspace); forward-delete also matches, see `matches`.
            "Delete" => key = Some(Key::Backspace),
            k => {
                key = Key::from_name(k).or(match k {
                    "Right" => Some(Key::ArrowRight),
                    "Left" => Some(Key::ArrowLeft),
                    "Up" => Some(Key::ArrowUp),
                    "Down" => Some(Key::ArrowDown),
                    "\\" => Some(Key::Backslash),
                    "/" => Some(Key::Slash),
                    "=" => Some(Key::Equals),
                    "-" => Some(Key::Minus),
                    "[" => Some(Key::OpenBracket),
                    "]" => Some(Key::CloseBracket),
                    _ => None,
                })
            }
        }
    }
    key.map(|k| (m, k))
}

fn matches(i: &egui::InputState, m: Modifiers, k: Key) -> bool {
    i.events.iter().any(|e| match e {
        egui::Event::Key { key, pressed: true, modifiers, .. } => {
            (*key == k || (k == Key::Backspace && *key == Key::Delete))
                && modifiers.command == m.command
                && modifiers.shift == m.shift
                && modifiers.alt == m.alt
        }
        _ => false,
    })
}

pub fn handle(app: &mut LightcraftApp, ctx: &egui::Context) {
    // don't steal keys from text fields
    if ctx.egui_wants_keyboard_input() {
        return;
    }
    let mut fire: Vec<String> = Vec::new();
    let mut aliased: Vec<(&str, serde_json::Value)> = Vec::new();
    ctx.input(|i| {
        let mut ui_keys = Vec::new();
        for (id, _, sc, _) in crate::menus::UI_COMMANDS {
            if let Some((m, k)) = sc.and_then(parse) {
                ui_keys.push((m, k));
                if matches(i, m, k) {
                    fire.push(id.to_string());
                }
            }
        }
        for c in lightcraft_engine::command_specs() {
            // A UI command bound to the same key wraps the engine command (e.g. `W` opens the
            // White Balance Selector tool rather than sampling without a point): the UI one wins.
            if let Some((m, k)) = c.shortcut.and_then(parse)
                && !ui_keys.contains(&(m, k))
                && matches(i, m, k)
            {
                fire.push(c.id.to_string());
            }
        }
        for (sc, id, params) in ALIASES {
            if let Some((m, k)) = parse(sc)
                && matches(i, m, k)
            {
                aliased.push((id, serde_json::from_str(params).unwrap_or_default()));
            }
        }
        // rating 0-5, colour labels 6-9 (with Shift: and advance)
        for (n, key) in [Key::Num0, Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5].iter().enumerate() {
            if matches(i, Modifiers::NONE, *key) {
                fire.push(format!("rate:{n}:0"));
            }
            if matches(i, Modifiers::SHIFT, *key) {
                fire.push(format!("rate:{n}:1"));
            }
        }
        for (label, key) in [("red", Key::Num6), ("yellow", Key::Num7), ("green", Key::Num8), ("blue", Key::Num9)] {
            if matches(i, Modifiers::NONE, key) {
                fire.push(format!("label:{label}"));
            }
        }
    });
    for (id, params) in aliased {
        let _ = app.run(id, params);
    }
    for f in fire {
        if let Some(rest) = f.strip_prefix("rate:") {
            let (n, adv) = rest.split_once(':').unwrap_or(("0", "0"));
            let _ = app.run("photo.rate", json!({"rating": n.parse::<u8>().unwrap_or(0), "advance": adv == "1"}));
            let label = if n == "0" { "Rating cleared".to_string() } else { format!("Rated {}", "★".repeat(n.parse().unwrap_or(0))) };
            app.toast(ctx, label);
        } else if let Some(l) = f.strip_prefix("label:") {
            let _ = app.run("photo.label", json!({"label": l}));
        } else {
            // Delete acts on what's being edited: the active mask in the Masking panel; never the
            // photo while retouching (spots are removed from their own panel).
            if f == "photo.delete" {
                use crate::state::RightPanel as R;
                match app.ui.right {
                    R::Masking => {
                        if app.session.active_mask.is_some() {
                            let _ = app.run("mask.delete", json!({}));
                        }
                        continue;
                    }
                    R::Remove | R::RedEye => continue,
                    _ => {}
                }
            }
            // X is both reject (library) and swap crop aspect (crop tool)
            if f == "photo.reject" && app.ui.right == crate::state::RightPanel::Crop {
                let _ = app.run("crop.rotateAspect", json!({}));
                continue;
            }
            if f == "crop.rotateAspect" && app.ui.right != crate::state::RightPanel::Crop {
                continue;
            }
            let _ = app.run(&f, json!({}));
            match f.as_str() {
                "photo.pick" => app.toast(ctx, "Flagged as Pick"),
                "photo.reject" => app.toast(ctx, "Flagged as Reject"),
                "photo.unflag" => app.toast(ctx, "Unflagged"),
                "edit.undo" => app.toast(ctx, "Undo"),
                "edit.redo" => app.toast(ctx, "Redo"),
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn delete_means_the_backspace_key() {
        assert_eq!(parse("Delete"), Some((Modifiers::NONE, Key::Backspace)));
        assert_eq!(parse("Cmd+Delete"), Some((Modifiers::COMMAND, Key::Backspace)));
    }
    use super::*;

    #[test]
    fn parses_shortcuts() {
        let (m, k) = parse("Cmd+Shift+Z").unwrap();
        assert!(m.command && m.shift && !m.alt);
        assert_eq!(k, Key::Z);
        assert_eq!(parse("Right").unwrap().1, Key::ArrowRight);
        assert_eq!(parse("\\").unwrap().1, Key::Backslash);
        // every declared shortcut parses
        for (id, _, sc, _) in crate::menus::UI_COMMANDS {
            if let Some(sc) = sc {
                assert!(parse(sc).is_some(), "{id}: {sc}");
            }
        }
        for c in lightcraft_engine::command_specs() {
            if let Some(sc) = c.shortcut {
                assert!(parse(sc).is_some(), "{}: {sc}", c.id);
            }
        }
    }

    #[test]
    fn aliases_parse_and_target_existing_commands() {
        let ui: Vec<&str> = crate::menus::UI_COMMANDS.iter().map(|c| c.0).collect();
        for (sc, id, params) in ALIASES {
            assert!(parse(sc).is_some(), "{id}: {sc}");
            assert!(ui.contains(id) || lightcraft_engine::find_command(id).is_some(), "alias {sc} → unknown command {id}");
            assert!(serde_json::from_str::<serde_json::Value>(params).is_ok(), "alias {sc}: bad params");
        }
    }

    /// Engine commands that intentionally share a key and are disambiguated by context in [`handle`].
    const CONTEXTUAL: &[(&str, &str)] = &[("photo.reject", "crop.rotateAspect")];

    /// No key fires two different actions (a UI command may shadow the engine command it wraps).
    #[test]
    fn no_conflicting_bindings() {
        let mut ui: Vec<((Modifiers, Key), String)> = Vec::new();
        for (id, _, sc, _) in crate::menus::UI_COMMANDS {
            if let Some(k) = sc.and_then(parse) {
                ui.push((k, id.to_string()));
            }
        }
        for (sc, id, _) in ALIASES {
            ui.push((parse(sc).unwrap(), format!("alias {id}")));
        }
        let mut engine: Vec<((Modifiers, Key), &str)> = Vec::new();
        for c in lightcraft_engine::command_specs() {
            if let Some(k) = c.shortcut.and_then(parse) {
                engine.push((k, c.id));
            }
        }
        for (sc, id, _) in ALIASES {
            let k = parse(sc).unwrap();
            assert!(!engine.iter().any(|(k2, _)| *k2 == k), "alias {sc} ({id}) shadows an engine shortcut");
        }
        for (i, (k, a)) in ui.iter().enumerate() {
            for (k2, b) in &ui[i + 1..] {
                assert!(k != k2, "{a} and {b} share a key");
            }
        }
        for (i, (k, a)) in engine.iter().enumerate() {
            for (k2, b) in &engine[i + 1..] {
                let contextual = CONTEXTUAL.iter().any(|(x, y)| (x == a && y == b) || (x == b && y == a));
                assert!(k != k2 || contextual, "{a} and {b} share a key");
            }
        }
    }
}
