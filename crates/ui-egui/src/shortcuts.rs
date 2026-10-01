//! Keyboard shortcuts: parse `Cmd+Shift+X` style strings and dispatch UI and engine commands.

use egui::{Key, Modifiers};
use serde_json::json;

use crate::LightcraftApp;

pub fn parse(s: &str) -> Option<(Modifiers, Key)> {
    let mut m = Modifiers::NONE;
    let mut key = None;
    for part in s.split('+') {
        match part {
            "Cmd" => m.command = true,
            "Shift" => m.shift = true,
            "Alt" => m.alt = true,
            "Ctrl" => m.ctrl = true,
            k => {
                key = Key::from_name(k).or(match k {
                    "Right" => Some(Key::ArrowRight),
                    "Left" => Some(Key::ArrowLeft),
                    "Up" => Some(Key::ArrowUp),
                    "Down" => Some(Key::ArrowDown),
                    "Delete" => Some(Key::Backspace),
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
            *key == k && modifiers.command == m.command && modifiers.shift == m.shift && modifiers.alt == m.alt
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
    ctx.input(|i| {
        for (id, _, sc, _) in crate::menus::UI_COMMANDS {
            if let Some((m, k)) = sc.and_then(parse)
                && matches(i, m, k)
            {
                fire.push(id.to_string());
            }
        }
        for c in lightcraft_engine::command_specs() {
            if let Some((m, k)) = c.shortcut.and_then(parse)
                && matches(i, m, k)
            {
                fire.push(c.id.to_string());
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
    for f in fire {
        if let Some(rest) = f.strip_prefix("rate:") {
            let (n, adv) = rest.split_once(':').unwrap_or(("0", "0"));
            let _ = app.run("photo.rate", json!({"rating": n.parse::<u8>().unwrap_or(0), "advance": adv == "1"}));
            let label = if n == "0" { "Rating cleared".to_string() } else { format!("Rated {}", "★".repeat(n.parse().unwrap_or(0))) };
            app.toast(ctx, label);
        } else if let Some(l) = f.strip_prefix("label:") {
            let _ = app.run("photo.label", json!({"label": l}));
        } else {
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
}
