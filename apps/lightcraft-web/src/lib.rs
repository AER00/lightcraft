//! LightCraft in the browser: the same egui UI as the desktop app, compiled to WASM.
//!
//! Build with `cargo xtask web` (→ `target/web/`), then serve that folder over HTTP (see
//! `docs/web.md`). Differences from the desktop host:
//! - the library lives in browser storage (OPFS, or IndexedDB where OPFS can't be written from the
//!   main thread): catalog journal, presets, view state and prefs are mirrored in memory and
//!   flushed in the background ([`files`]); imported originals are stored by content hash
//!   ([`store`]); a new library starts with the procedural demo photos;
//! - renders run inline on the main thread (one job per frame) — no worker threads yet;
//! - export downloads the file through the browser.
//!
//! `?bench` in the URL runs a scripted first-paint / slider-latency measurement and logs it to the
//! console (see [`bench`]); `?store=idb` forces the IndexedDB backend, `?store=memory` disables
//! persistence and `?reset` clears the stored library.
#![forbid(unsafe_code)]

pub mod bench;
pub mod files;
pub mod store;

#[cfg(target_arch = "wasm32")]
mod backend;
#[cfg(target_arch = "wasm32")]
mod web;
