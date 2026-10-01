# LightCraft in the browser

`apps/lightcraft-web` runs the same egui UI as the desktop app (`crates/ui-egui`) in the browser,
compiled to WebAssembly and drawn with WebGL2 (eframe's `glow` backend).

## Build and run locally

One-time setup:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version <the wasm-bindgen version in Cargo.lock> --locked
```

`cargo xtask web` checks the CLI against `Cargo.lock` and prints the exact install command if
it's missing or a different version.

Build, then serve:

```sh
cargo xtask web            # → <target>/web/{index.html, lightcraft_web.js, lightcraft_web_bg.wasm}
cargo xtask web --serve    # build, then serve on http://127.0.0.1:8080/ (or `--serve 9000`)
cargo xtask web --dev      # unoptimized build with debug info (faster to compile, slow to run)
```

`<target>` is `target/` unless `CARGO_TARGET_DIR` is set. Any static HTTP server works, as long as
it serves `.wasm` as `application/wasm`. The bundle can't be opened from `file://`. If `wasm-opt`
(binaryen) is on `PATH`, the release build also runs it over the module.

The release build uses the `web` Cargo profile (release + thin LTO, no debug info).

## What works

- **Demo library on load.** The procedural scenes from `lightcraft-scenes` are loaded at
  start, so the page always has something in it.
- **Importing photos with no filesystem.** *File ▸ Add Photos…* (<kbd>⌘⇧I</kbd>) opens the
  browser's file picker. You can also drop files anywhere on the page. The bytes are kept in memory
  (`store::MemStore`) under synthetic paths (`mem/<n>/<name>`) and decoded by the same engine code
  as the desktop app (`lightcraft_engine::files::{probe_bytes, load_bytes}`). That covers
  JPEG/PNG/TIFF/WebP and the supported raw formats.
- **Every develop control** (sliders, curves, mixer, grading, masking, crop…) works as it does
  on the desktop, since it's the same crate.
- **Export downloads the file.** *Export…* (<kbd>⌘⇧E</kbd>) runs the same `app.export` path as
  the desktop app (`lightcraft_engine::export`: JPEG/PNG/TIFF/WebP, sizing, naming). The host's
  `write` service hands each file to the browser as a download instead of writing it to disk.

## Not yet

- **Persistence.** Nothing is saved across reloads: the catalog, edits, imported files and UI
  prefs all start fresh. The planned route is IndexedDB/OPFS for the op log, and file handles
  where the browser supports them.
- **Worker threads.** Renders run on the main thread, one job per frame. On a cold load the grid
  fills in over a few seconds, and the UI can stall briefly while a thumbnail is generated. The fix
  is Web Workers, which need wasm threads (`+atomics`, SharedArrayBuffer, COOP/COEP headers).
- **Bundle size.** The module is about 12 MB uncompressed, because it includes every codec and
  raw decoder. Serving it gzip/brotli-compressed or running `wasm-opt` shrinks it a lot.
- **Control channel / MCP.** These are desktop-only, because they need a TCP socket.
- **AVIF export** is untested in the browser; it's the one encoder that may not be wasm-safe.

## Measuring

Open `http://127.0.0.1:8080/?bench` to run a scripted measurement:

1. Wait for the grid thumbnails.
2. Open the first photo in Detail.
3. Drag Exposure through 8 steps, exactly as a slider drag does (begin interaction → `develop.set`
   ×8 → end).

The page then logs one console line:

```
lightcraft-bench {"first_frame_ms":…,"thumbs_done_ms":…,"slider_draft_ms":[…],"slider_draft_median_ms":…,"slider_job_ms":[…],"release_full_ms":…}
```

- `slider_draft_ms`: time from the `develop.set` command until the new loupe texture is ready.
- `slider_job_ms`: the pipeline's share of that time.
- `release_full_ms`: the full-quality render after the drag ends.
