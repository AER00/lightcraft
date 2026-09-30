# LightCraft

A fast, beautiful, open-source photo library and non-destructive raw developer — a clean-room, pure-Rust
alternative to Adobe Lightroom. Runs on macOS, Windows, Linux and the web (WASM).

- Engine-first Cargo workspace; the egui UI is one swappable crate.
- Everything is a command: every menu item, slider, brush stroke and crop drag is drivable over a JSON control
  channel and an MCP server, so agents can cull, develop and export.
- Scene-referred, wide-gamut float pipeline; own RAW decoders; local-first catalog.

```sh
cargo run --release -p lightcraft            # desktop app
cargo run --release -p lightcraft -- --control 7980   # with the automation channel
cargo run -p lightcraft-cli -- mcp           # MCP server (stdio)
cargo xtask ci                               # fmt, clippy, tests, layering, wasm
```

Licence: MIT OR Apache-2.0. By the artcraft team.
