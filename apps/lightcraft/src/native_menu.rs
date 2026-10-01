//! The native macOS menu bar (via `muda`, pure Rust over AppKit), generated from the same model as
//! the in-window menus ([`lightcraft_ui_egui::menubar`]): the registry's menu paths, shortcuts,
//! enabled and checked state, and live labels ("Undo Exposure").
//!
//! Key handling: a native key equivalent consumes the key press before egui sees it, so
//! - shortcuts installed natively are listed in `LightcraftApp::native_shortcuts` and skipped by
//!   the egui shortcut handler (nothing fires twice);
//! - while a text field has keyboard focus, accelerators without ⌘ (`G`, `1`, `Delete`…) and the
//!   text-editing ones (⌘A/⌘C/⌘V/⌘X/⌘Z) are removed, so typing and text editing work; they come
//!   back when the field loses focus.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, channel};

use lightcraft_ui_egui::LightcraftApp;
use lightcraft_ui_egui::menubar::{MenuNode, menu_bar};
use muda::accelerator::{Accelerator, Code, Modifiers};
use muda::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use serde_json::Value;

const QUIT: &str = "app.quit";
/// Shortcuts text fields need while they have focus.
const TEXT_EDIT: &[&str] = &["Cmd+A", "Cmd+C", "Cmd+V", "Cmd+X", "Cmd+Z", "Cmd+Shift+Z"];

enum Handle {
    Plain(MenuItem),
    Check(CheckMenuItem),
}

struct Item {
    handle: Handle,
    cmd: String,
    params: Value,
    shortcut: Option<String>,
    accel: Option<Accelerator>,
    label: String,
    enabled: bool,
    checked: Option<bool>,
}

pub struct NativeMenu {
    menu: Menu,
    items: HashMap<String, Item>,
    rx: Receiver<String>,
    /// Structure of the installed menu (ids, kinds, submenu names); a change rebuilds it.
    structure: String,
    text_focus: bool,
}

/// `Cmd+Shift+Z` → a muda accelerator (`None` for keys we leave to egui, e.g. Escape).
fn accelerator(sc: &str) -> Option<Accelerator> {
    let mut mods = Modifiers::empty();
    let mut key = None;
    for part in sc.split('+') {
        match part {
            "Cmd" => mods |= Modifiers::META,
            "Shift" => mods |= Modifiers::SHIFT,
            "Alt" => mods |= Modifiers::ALT,
            "Ctrl" => mods |= Modifiers::CONTROL,
            k => key = Some(k),
        }
    }
    let code = match key? {
        k if k.len() == 1 && k.as_bytes()[0].is_ascii_alphabetic() => {
            const LETTERS: [Code; 26] = [
                Code::KeyA,
                Code::KeyB,
                Code::KeyC,
                Code::KeyD,
                Code::KeyE,
                Code::KeyF,
                Code::KeyG,
                Code::KeyH,
                Code::KeyI,
                Code::KeyJ,
                Code::KeyK,
                Code::KeyL,
                Code::KeyM,
                Code::KeyN,
                Code::KeyO,
                Code::KeyP,
                Code::KeyQ,
                Code::KeyR,
                Code::KeyS,
                Code::KeyT,
                Code::KeyU,
                Code::KeyV,
                Code::KeyW,
                Code::KeyX,
                Code::KeyY,
                Code::KeyZ,
            ];
            LETTERS[(k.as_bytes()[0].to_ascii_uppercase() - b'A') as usize]
        }
        k if k.len() == 1 && k.as_bytes()[0].is_ascii_digit() => {
            const DIGITS: [Code; 10] = [
                Code::Digit0,
                Code::Digit1,
                Code::Digit2,
                Code::Digit3,
                Code::Digit4,
                Code::Digit5,
                Code::Digit6,
                Code::Digit7,
                Code::Digit8,
                Code::Digit9,
            ];
            DIGITS[(k.as_bytes()[0] - b'0') as usize]
        }
        "[" => Code::BracketLeft,
        "]" => Code::BracketRight,
        "\\" => Code::Backslash,
        "/" => Code::Slash,
        "=" => Code::Equal,
        "-" => Code::Minus,
        "'" => Code::Quote,
        "Delete" => Code::Backspace,
        _ => return None,
    };
    Some(Accelerator::new(mods, code))
}

/// Accelerators that must step aside while a text field has focus.
fn yields_to_text(sc: &str) -> bool {
    !sc.contains("Cmd") && !sc.contains("Ctrl") || TEXT_EDIT.contains(&sc)
}

/// `&` marks a mnemonic in muda labels.
fn label_text(s: &str) -> String {
    s.replace('&', "&&")
}

fn structure_of(bar: &[(String, Vec<MenuNode>)]) -> String {
    fn walk(n: &[MenuNode], out: &mut String) {
        for x in n {
            match x {
                MenuNode::Item { id, params, checked, shortcut, .. } => {
                    out.push_str(&MenuNode::key(id, params));
                    out.push(if checked.is_some() { 'c' } else { 'i' });
                    out.push_str(shortcut.as_deref().unwrap_or(""));
                }
                MenuNode::Separator => out.push('-'),
                MenuNode::Submenu { label, children } => {
                    out.push('[');
                    out.push_str(label);
                    walk(children, out);
                    out.push(']');
                }
            }
            out.push(';');
        }
    }
    let mut s = String::new();
    for (t, items) in bar {
        s.push_str(t);
        walk(items, &mut s);
    }
    s
}

impl NativeMenu {
    /// Build the menu bar and install it as the application menu (main thread, after launch).
    pub fn install(app: &mut LightcraftApp, ctx: &egui::Context) -> NativeMenu {
        let (tx, rx) = channel::<String>();
        let repaint = ctx.clone();
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            let _ = tx.send(e.id.0);
            repaint.request_repaint();
        }));
        let mut m = NativeMenu { menu: Menu::new(), items: HashMap::new(), rx, structure: String::new(), text_focus: false };
        m.rebuild(app);
        app.native_menu = true;
        m
    }

    fn rebuild(&mut self, app: &mut LightcraftApp) {
        let bar = menu_bar(app);
        self.structure = structure_of(&bar);
        self.menu = Menu::new();
        self.items.clear();

        // the application menu
        let app_menu = Submenu::new("LightCraft", true);
        let about = MenuItem::with_id("app.about", "About LightCraft", true, None);
        let quit = MenuItem::with_id(QUIT, "Quit LightCraft", true, accelerator("Cmd+Q"));
        let _ = app_menu.append_items(&[
            &about,
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::services(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::hide(Some("Hide LightCraft")),
            &PredefinedMenuItem::hide_others(None),
            &PredefinedMenuItem::show_all(None),
            &PredefinedMenuItem::separator(),
            &quit,
        ]);
        let _ = self.menu.append(&app_menu);

        for (title, nodes) in &bar {
            let sub = Submenu::new(title, true);
            if title == "Window" {
                let _ = sub.append_items(&[
                    &PredefinedMenuItem::minimize(None),
                    &PredefinedMenuItem::maximize(Some("Zoom")),
                    &PredefinedMenuItem::separator(),
                ]);
            }
            // About lives in the app menu on macOS
            let nodes: Vec<MenuNode> = nodes.iter().filter(|n| !matches!(n, MenuNode::Item { id, .. } if id == "app.about")).cloned().collect();
            self.append_nodes(&sub, &nodes);
            if title == "Window" {
                let _ = sub.append_items(&[&PredefinedMenuItem::separator(), &PredefinedMenuItem::bring_all_to_front(None)]);
                sub.set_as_windows_menu_for_nsapp();
            }
            if title == "Help" {
                sub.set_as_help_menu_for_nsapp();
            }
            let _ = self.menu.append(&sub);
        }
        self.menu.init_for_nsapp();
        self.text_focus = false;
        self.publish_shortcuts(app);
    }

    fn append_nodes(&mut self, sub: &Submenu, nodes: &[MenuNode]) {
        for n in nodes {
            match n {
                MenuNode::Separator => {
                    let _ = sub.append(&PredefinedMenuItem::separator());
                }
                MenuNode::Submenu { label, children } => {
                    let s = Submenu::new(label_text(label), true);
                    self.append_nodes(&s, children);
                    let _ = sub.append(&s);
                }
                MenuNode::Item { id, params, label, shortcut, enabled, checked } => {
                    let key = MenuNode::key(id, params);
                    if self.items.contains_key(&key) {
                        continue; // muda ids must be unique
                    }
                    let accel = shortcut.as_deref().and_then(accelerator);
                    let handle = match checked {
                        Some(c) => {
                            let it = CheckMenuItem::with_id(key.clone(), label_text(label), *enabled, *c, accel);
                            let _ = sub.append(&it);
                            Handle::Check(it)
                        }
                        None => {
                            let it = MenuItem::with_id(key.clone(), label_text(label), *enabled, accel);
                            let _ = sub.append(&it);
                            Handle::Plain(it)
                        }
                    };
                    self.items.insert(
                        key,
                        Item {
                            handle,
                            cmd: id.clone(),
                            params: params.clone(),
                            shortcut: shortcut.clone(),
                            accel,
                            label: label.clone(),
                            enabled: *enabled,
                            checked: *checked,
                        },
                    );
                }
            }
        }
    }

    /// Tell the egui shortcut handler which shortcuts the menu bar currently owns.
    fn publish_shortcuts(&self, app: &mut LightcraftApp) {
        app.native_shortcuts = self
            .items
            .values()
            .filter(|i| i.accel.is_some())
            .filter_map(|i| i.shortcut.clone())
            .filter(|sc| !(self.text_focus && yields_to_text(sc)))
            .collect::<HashSet<_>>();
    }

    /// Per frame: run chosen items, then sync labels / enabled / checked and the text-focus
    /// accelerators with the app state.
    pub fn update(&mut self, app: &mut LightcraftApp, ctx: &egui::Context) {
        while let Ok(key) = self.rx.try_recv() {
            if key == QUIT {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                continue;
            }
            if key == "app.about" {
                let _ = app.run("app.about", serde_json::json!({}));
                continue;
            }
            if let Some(it) = self.items.get(&key) {
                let (cmd, params) = (it.cmd.clone(), it.params.clone());
                let _ = lightcraft_ui_egui::menubar::run_item(app, &cmd, params);
            }
        }
        let bar = menu_bar(app);
        if structure_of(&bar) != self.structure {
            self.rebuild(app);
            return;
        }
        fn walk(nodes: &[MenuNode], items: &mut HashMap<String, Item>) {
            for n in nodes {
                match n {
                    MenuNode::Submenu { children, .. } => walk(children, items),
                    MenuNode::Item { id, params, label, enabled, checked, .. } => {
                        let Some(it) = items.get_mut(&MenuNode::key(id, params)) else { continue };
                        if it.label != *label {
                            match &it.handle {
                                Handle::Plain(h) => h.set_text(label_text(label)),
                                Handle::Check(h) => h.set_text(label_text(label)),
                            }
                            it.label = label.clone();
                        }
                        if it.enabled != *enabled {
                            match &it.handle {
                                Handle::Plain(h) => h.set_enabled(*enabled),
                                Handle::Check(h) => h.set_enabled(*enabled),
                            }
                            it.enabled = *enabled;
                        }
                        if let (Handle::Check(h), Some(c)) = (&it.handle, checked) {
                            // also resyncs after AppKit toggled it on click
                            if h.is_checked() != *c {
                                h.set_checked(*c);
                            }
                            it.checked = Some(*c);
                        }
                    }
                    MenuNode::Separator => {}
                }
            }
        }
        walk(&bar.iter().flat_map(|(_, v)| v.clone()).collect::<Vec<_>>(), &mut self.items);

        let focus = ctx.egui_wants_keyboard_input();
        if focus != self.text_focus {
            self.text_focus = focus;
            for it in self.items.values() {
                let Some(sc) = it.shortcut.as_deref() else { continue };
                if it.accel.is_none() || !yields_to_text(sc) {
                    continue;
                }
                let accel = if focus { None } else { it.accel };
                let _ = match &it.handle {
                    Handle::Plain(h) => h.set_accelerator(accel),
                    Handle::Check(h) => h.set_accelerator(accel),
                };
            }
            self.publish_shortcuts(app);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_map_to_accelerators() {
        assert_eq!(accelerator("Cmd+Shift+Z"), Some(Accelerator::new(Modifiers::META | Modifiers::SHIFT, Code::KeyZ)));
        assert_eq!(accelerator("G"), Some(Accelerator::new(Modifiers::empty(), Code::KeyG)));
        assert_eq!(accelerator("Cmd+'"), Some(Accelerator::new(Modifiers::META, Code::Quote)));
        assert_eq!(accelerator("Escape"), None);
        assert!(yields_to_text("G") && yields_to_text("Delete") && yields_to_text("Cmd+V"));
        assert!(!yields_to_text("Cmd+Shift+E"));
        // every registry shortcut that a menu shows maps (or is deliberately left to egui)
        let app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Default::default());
        fn all(n: &[MenuNode], out: &mut Vec<String>) {
            for x in n {
                match x {
                    MenuNode::Item { shortcut: Some(s), .. } => out.push(s.clone()),
                    MenuNode::Submenu { children, .. } => all(children, out),
                    _ => {}
                }
            }
        }
        let mut scs = Vec::new();
        for (_, v) in menu_bar(&app) {
            all(&v, &mut scs);
        }
        for sc in scs {
            assert!(accelerator(&sc).is_some(), "{sc}");
        }
    }
}
