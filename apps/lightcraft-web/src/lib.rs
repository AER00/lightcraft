//! LightCraft in the browser: the same egui UI as the desktop app, compiled to WASM.
//!
//! Build with `cargo xtask web` (→ `target/web/`), then serve that folder over HTTP.
//! Differences from the desktop host:
//! - no filesystem: photos picked (File ▸ Add Photos…) or dropped onto the page are held in
//!   memory ([`store::MemStore`]) and decoded from bytes;
//! - export downloads the file through the browser;
//! - renders run inline on the main thread (one job per frame) — no worker threads yet;
//! - no persistence yet (UI prefs and the catalog reset on reload);
//! - the procedural demo library is loaded at start so the page is never empty.
//!
//! `?bench` in the URL runs a scripted first-paint / slider-latency measurement and logs it to the
//! console (see [`bench`]).
#![forbid(unsafe_code)]

pub mod bench;
pub mod store;

#[cfg(target_arch = "wasm32")]
mod web;
