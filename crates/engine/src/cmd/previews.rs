//! Build previews ahead of time: each photo's grid thumbnail and its loupe view (standard size,
//! or 1:1), rendered into the memory + disk preview cache so browsing and the loupe are instant.
//! Runs on a background thread (the app keeps working; progress / cancel by command), or inline
//! with `wait` (CLI, MCP, tests).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use serde_json::{Value, json};

use super::{CommandSpec, always, bad, cmd, str_param};
use crate::media::{RenderJob, THUMB_SIZES};
use crate::{Result, Session};

/// The long edge of a standard-sized preview.
pub const STANDARD_EDGE: usize = 2048;

/// A preview build in progress.
#[derive(Debug, Default)]
pub struct PreviewBuild {
    pub total: usize,
    pub done: AtomicUsize,
    pub failed: AtomicUsize,
    pub cancel: AtomicBool,
    pub finished: AtomicBool,
}

impl PreviewBuild {
    pub fn json(&self) -> Value {
        json!({
            "total": self.total,
            "done": self.done.load(Ordering::Relaxed),
            "failed": self.failed.load(Ordering::Relaxed),
            "running": !self.finished.load(Ordering::Relaxed),
            "cancelled": self.cancel.load(Ordering::Relaxed),
        })
    }
}

/// The jobs that build `id`'s previews: the largest grid thumbnail, then the view render at
/// `edge` (`None` = 1:1, the photo's full size).
fn jobs_for(s: &mut Session, id: lightcraft_catalog::PhotoId, edge: Option<usize>) -> Vec<RenderJob> {
    let Some(p) = s.catalog.photo(id) else { return Vec::new() };
    let full = p.width.max(p.height) as usize;
    let e = edge.unwrap_or(full).min(full.max(1)).max(1);
    let mut v = Vec::new();
    v.extend(s.thumb_job(id, THUMB_SIZES[THUMB_SIZES.len() - 1]));
    v.extend(s.loupe_job(id, e, e, true));
    v
}

fn run_all(jobs: Vec<Vec<RenderJob>>, state: &PreviewBuild) {
    for photo_jobs in jobs {
        if state.cancel.load(Ordering::Relaxed) {
            break;
        }
        let ok = photo_jobs.into_iter().all(|j| j.run().rendered.is_ok());
        if !ok {
            state.failed.fetch_add(1, Ordering::Relaxed);
        }
        state.done.fetch_add(1, Ordering::Relaxed);
    }
    state.finished.store(true, Ordering::Relaxed);
}

fn build(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "library.buildPreviews";
    let edge = match str_param(p, "size").unwrap_or("standard") {
        "standard" => Some(p.get("edge").and_then(Value::as_u64).map_or(STANDARD_EDGE, |e| e.clamp(256, 8192) as usize)),
        "full" | "1:1" => None,
        other => return Err(bad(C, format!("unknown size `{other}` (standard|full)"))),
    };
    if s.preview_build.as_ref().is_some_and(|b| !b.finished.load(Ordering::Relaxed)) {
        return Err(bad(C, "a preview build is already running"));
    }
    // explicit ids, else the selection, else everything in view
    let ids = if p.get("ids").is_some() || p.get("id").is_some() || !s.selection.ids.is_empty() { s.targets(p) } else { s.visible_cloned() };
    let jobs: Vec<Vec<RenderJob>> = ids.iter().map(|id| jobs_for(s, *id, edge)).filter(|j| !j.is_empty()).collect();
    let state = Arc::new(PreviewBuild { total: jobs.len(), ..Default::default() });
    s.preview_build = Some(state.clone());
    let wait = p.get("wait").and_then(Value::as_bool).unwrap_or(false) || cfg!(target_arch = "wasm32");
    if wait {
        run_all(jobs, &state);
        return Ok(state.json());
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let st = state.clone();
        std::thread::Builder::new()
            .name("lc-build-previews".into())
            .spawn(move || run_all(jobs, &st))
            .map_err(|e| bad(C, format!("could not start: {e}")))?;
    }
    Ok(state.json())
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "library.buildPreviews",
            "Build Previews",
            [],
            None,
            "{size?: standard (2048 px, or `edge`) | full (1:1), ids?, wait?: bool} — render the grid thumbnail and loupe view of the selected photos (else all in view) into the preview cache, in the background unless `wait` → {total, done, failed, running}",
            always,
            build
        ),
        cmd!(query "library.previewProgress", "Preview Build Progress", [], None, "{} → {total, done, failed, running, cancelled} | null", always, |s, _| {
            Ok(s.preview_build.as_ref().map_or(Value::Null, |b| b.json()))
        }),
        cmd!("library.cancelPreviews", "Cancel Preview Build", [], None, "{}", always, |s, _| {
            if let Some(b) = &s.preview_build {
                b.cancel.store(true, Ordering::Relaxed);
            }
            Ok(s.preview_build.as_ref().map_or(Value::Null, |b| b.json()))
        }),
    ]
}
