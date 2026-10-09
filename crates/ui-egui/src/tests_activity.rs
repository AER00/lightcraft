//! Issue #345: the activity stack — one place, top-left, for every long-running task, with ✕ to cancel.

use std::time::Duration;

use lightcraft_engine::activity::{Cancel, TaskInfo, Unit};
use serde_json::json;

use crate::headless::Headless;
use crate::panels::activity::{WIDTH, count_text};
use crate::{LightcraftApp, Services};

const T: Duration = Duration::from_secs(20);

fn demo() -> Headless {
    let app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services { png: None, ..Default::default() });
    let mut h = Headless::new(app, [1200.0, 800.0], 1.0);
    h.step();
    h
}

fn has(h: &Headless, id: &str) -> bool {
    h.app.widgets.iter().any(|(w, _)| w == id)
}

fn rows(h: &Headless) -> usize {
    h.app.widgets.iter().filter(|(w, _)| w.starts_with("activity:row:")).count()
}

fn rect(h: &Headless, id: &str) -> egui::Rect {
    h.app.widgets.iter().find(|(w, _)| w == id).map(|(_, r)| *r).unwrap_or_else(|| panic!("no widget {id}"))
}

/// Rows show once their task is half a second old.
fn wait_visible(h: &mut Headless) {
    std::thread::sleep(Duration::from_millis(600));
    h.step();
    h.step();
}

/// Click a widget and let the synthetic events (move, press, release) play out.
fn click(h: &mut Headless, id: &str) {
    h.step();
    h.step();
    assert_eq!(h.request("ui.clickWidget", json!({"id": id}), T)["ok"], true, "{id}");
    for _ in 0..4 {
        h.step();
    }
}

#[test]
fn rows_appear_after_half_a_second_and_cancel_by_click() {
    let mut h = demo();
    let g = h.app.session.activity.start("export", "Exporting", Cancel::Yes);
    g.progress(2, 5);
    h.step();
    assert!(!has(&h, &format!("activity:row:{}", g.id())), "not before 0.5 s");
    wait_visible(&mut h);
    assert!(has(&h, &format!("activity:row:{}", g.id())));
    click(&mut h, &format!("activity:cancel:{}", g.id()));
    assert!(g.is_cancelled());
    let inspect = h.request("ui.inspect", json!({}), T);
    assert_eq!(inspect["result"]["activity"][0]["cancelling"], true, "{}", inspect["result"]["activity"]);
}

#[test]
fn more_than_three_rows_overflow_and_expand() {
    let mut h = demo();
    let guards: Vec<_> = (0..5).map(|i| h.app.session.activity.start("export", &format!("Task {i}"), Cancel::Yes)).collect();
    wait_visible(&mut h);
    assert_eq!(rows(&h), 3);
    assert!(has(&h, "activity:more"));
    click(&mut h, "activity:more");
    assert_eq!(rows(&h), 5);
    drop(guards);
    h.step();
    assert_eq!(rows(&h), 0);
}

#[test]
fn not_cancellable_rows_have_no_cross() {
    let mut h = demo();
    let g = h.app.session.activity.start("faces", "Finding faces", Cancel::No);
    wait_visible(&mut h);
    assert!(has(&h, &format!("activity:row:{}", g.id())));
    assert!(!has(&h, &format!("activity:cancel:{}", g.id())));
}

#[test]
fn count_text_by_unit() {
    let mut t = TaskInfo {
        id: 1,
        kind: "export",
        label: "Exporting".into(),
        done: 3,
        total: 25,
        unit: Unit::Count,
        detail: String::new(),
        cancellable: true,
        cancelling: false,
        age_ms: 600,
    };
    assert_eq!(count_text(&t), "3 of 25");
    (t.done, t.total, t.unit) = (12 * 1_048_576, 340 * 1_048_576, Unit::Bytes);
    assert_eq!(count_text(&t), "12 of 340 MB");
    (t.done, t.total, t.unit) = (400, 1000, Unit::Percent);
    assert_eq!(count_text(&t), "40 %");
    t.total = 0;
    assert_eq!(count_text(&t), "");
}

#[test]
fn stale_cancel_is_harmless() {
    let mut h = demo();
    let g = h.app.session.activity.start("export", "Exporting", Cancel::Yes);
    wait_visible(&mut h);
    let id = g.id();
    drop(g);
    let r = h.request("engine.execute", json!({"command": "activity.cancel", "params": {"id": id}}), T);
    assert_eq!(r["ok"], false, "an error result, not a panic: {r}");
    h.step();
    assert!(h.app.ui.toast.is_none(), "{:?}", h.app.ui.toast);
}

#[test]
fn long_detail_stays_inside_the_stack() {
    let mut h = demo();
    let g = h.app.session.activity.start("export", "Exportieren", Cancel::Yes);
    g.detail(&"IMG_".repeat(75));
    wait_visible(&mut h);
    let row = rect(&h, &format!("activity:row:{}", g.id()));
    assert!(row.width() <= WIDTH + 1.0, "{row:?}");
}

#[test]
fn cross_sits_at_the_right_edge_of_its_row() {
    let mut h = demo();
    let g = h.app.session.activity.start("export", "Exporting", Cancel::Yes);
    wait_visible(&mut h);
    let row = rect(&h, &format!("activity:row:{}", g.id()));
    let cross = rect(&h, &format!("activity:cancel:{}", g.id()));
    assert!(cross.right() >= row.right() - 2.0, "the cross is at the right, not after the label: {cross:?} in {row:?}");
}

#[test]
fn quit_with_a_running_task_asks_and_quit_anyway_cancels() {
    let mut h = demo();
    let g = h.app.session.activity.start("export", "Exporting", Cancel::Yes);
    wait_visible(&mut h);
    assert!(!crate::panels::notices::may_close(&mut h.app));
    assert!(matches!(h.app.quit_prompt, Some(crate::QuitPrompt::Tasks(_))), "{:?}", h.app.quit_prompt);
    h.step();
    assert!(!has(&h, "button:quitRetry"));
    click(&mut h, "button:quitAnyway");
    assert!(g.is_cancelled() && h.quit_requested());
}

#[test]
fn quit_cancel_keeps_the_task_running() {
    let mut h = demo();
    let g = h.app.session.activity.start("import", "Importing", Cancel::Yes);
    assert!(!crate::panels::notices::may_close(&mut h.app));
    click(&mut h, "button:quitCancel");
    assert!(!g.is_cancelled() && h.app.quit_prompt.is_none() && !h.app.quit_confirmed);
}

#[test]
fn cancelling_task_does_not_block_quit() {
    let mut h = demo();
    let g = h.app.session.activity.start("export", "Exporting", Cancel::Yes);
    h.app.session.activity.cancel(g.id()).unwrap();
    let _f = h.app.session.activity.start("faces", "Finding faces", Cancel::No);
    assert!(crate::panels::notices::may_close(&mut h.app));
}

/// A slow disk: every file takes `ms` to write (the UI must not wait for it).
fn slow_writer(ms: u64) -> crate::SharedWrite {
    std::sync::Arc::new(move |_: &str, _: &[u8]| {
        std::thread::sleep(Duration::from_millis(ms));
        Ok(())
    })
}

/// Export the first `n` photos in view through the Export dialog (small JPEGs, a made-up folder).
fn start_export(h: &mut Headless, n: usize) {
    let ids: Vec<u64> = h.app.session.visible_cloned().iter().take(n).map(|p| p.0).collect();
    assert_eq!(ids.len(), n, "the demo library has {n} photos");
    h.request("engine.execute", json!({"command": "library.select", "params": {"ids": ids}}), T);
    h.request("engine.execute", json!({"command": "dialog.export", "params": {}}), T);
    h.step();
    if let Some(crate::state::Dialog::Export { full_size, resize, dir, .. }) = &mut h.app.ui.dialog {
        *full_size = false;
        *resize = lightcraft_engine::export::Resize::long_edge(64);
        *dir = "/lc-test-out".into();
    }
    let r = h.request("ui.dialog.confirm", json!({}), T);
    assert_eq!(r["ok"], true, "{r}");
}

#[test]
fn export_shows_a_row_and_stops_on_activity_cancel() {
    let mut h = demo();
    h.app.services.write_shared = Some(slow_writer(200));
    start_export(&mut h, 5);
    assert!(h.app.export.is_some(), "running in the background");
    let tasks = h.request("ui.inspect", json!({}), T)["result"]["activity"].clone();
    assert_eq!(tasks[0]["kind"], "export", "{tasks}");
    assert_eq!(tasks[0]["total"], 5, "{tasks}");
    let id = tasks[0]["id"].as_u64().unwrap();
    assert_eq!(h.request("engine.execute", json!({"command": "activity.cancel", "params": {"id": id}}), T)["ok"], true);
    assert!(h.step_until(Duration::from_secs(60), |h| h.app.export.is_none()));
    assert!(h.app.session.activity.list().is_empty());
    assert!(h.app.last_export_result.as_ref().is_some_and(|r| r["cancelled"] == true), "{:?}", h.app.last_export_result);
    assert!(!has(&h, "button:exportCancel"), "the old window is gone");
}

#[test]
fn dead_export_worker_leaves_no_row() {
    let mut h = demo();
    h.app.services.write_shared = Some(std::sync::Arc::new(|_: &str, _: &[u8]| -> Result<(), String> { panic!("synthetic writer panic") }));
    start_export(&mut h, 2);
    assert!(h.step_until(Duration::from_secs(60), |h| h.app.export.is_none()));
    assert!(h.app.session.activity.list().is_empty());
}

/// A folder of `n` stand-in JPEGs and an app whose file probe takes 100 ms each (a slow drive).
fn slow_import(tag: &str, n: usize) -> (Headless, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("lc-activity-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for i in 0..n {
        std::fs::write(dir.join(format!("IMG_{i:03}.jpg")), format!("not really a jpeg {i}")).unwrap();
    }
    let mut s = lightcraft_engine::Session::new();
    s.media.file_probe = Some(std::sync::Arc::new(|p: &str| {
        std::thread::sleep(Duration::from_millis(100));
        Ok(lightcraft_engine::media::ProbeInfo {
            width: 60,
            height: 40,
            format: "JPEG".into(),
            content_hash: Some(p.to_string()),
            ..Default::default()
        })
    }));
    let mut h = Headless::new(LightcraftApp::new(s, Services { png: None, ..Default::default() }), [1200.0, 800.0], 1.0);
    h.step();
    (h, dir)
}

#[test]
fn import_cancelled_by_command_reports_cancelled() {
    let n = 40;
    let (mut h, dir) = slow_import("cancel", n);
    let undo0 = h.app.session.undo.len();
    crate::import::start_paths(&mut h.app, vec![dir.to_string_lossy().to_string()]).unwrap();
    assert!(h.step_until(Duration::from_secs(60), |h| h.app.import.as_ref().is_some_and(|t| t.imported >= 2)));
    let tasks = h.app.session.activity.list();
    assert_eq!(tasks.first().map(|t| t.kind), Some("import"), "{tasks:?}");
    h.app.session.activity.cancel(tasks[0].id).unwrap();
    assert!(h.step_until(Duration::from_secs(60), |h| h.app.import.is_none()));
    assert!(h.app.session.activity.list().is_empty());
    assert!(h.app.ui.toast.as_ref().is_some_and(|t| t.0.starts_with("Import cancelled")), "{:?}", h.app.ui.toast);
    assert!(h.app.session.catalog.len() < n, "stopped part-way");
    assert_eq!(h.app.session.undo.len(), undo0 + 1, "one undo step");
    let _ = std::fs::remove_dir_all(&dir);
}
