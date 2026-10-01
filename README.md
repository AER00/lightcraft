# LightCraft

A fast, beautiful, open-source photo library and non-destructive raw developer — a clean-room, pure-Rust
alternative to Adobe Lightroom. Runs on macOS, Windows, Linux and the web (WASM).

- Engine-first Cargo workspace; the egui UI is one swappable crate.
- Everything is a command: every menu item, slider, brush stroke and crop drag is drivable over a JSON control
  channel and an MCP server, so agents can cull, develop and export.
- Scene-referred, wide-gamut float pipeline; own RAW decoders; local-first catalog.

```sh
cargo run --release -p lightcraft                      # desktop app
cargo run --release -p lightcraft -- --control 7980    # with the automation channel
cargo run -p lightcraft-cli -- mcp                     # MCP server (stdio)
cargo xtask ci                                         # fmt, clippy, tests, layering, wasm
```

See [ROADMAP.md](ROADMAP.md) for milestones and estimates.

## Crafting Apps

Clean-room, pure-Rust creative tools from the same workshop — native on macOS, Windows and Linux, and in the browser.

<table>
  <tr>
    <td width="20%" valign="top"><a href="https://github.com/storytold/photocraft"><b>PhotoCraft</b></a><br><sub>Layered raster image editor — a Photoshop-class app with layers, masks, adjustments, brushes and PSD support.</sub></td>
    <td width="20%" valign="top"><a href="https://github.com/storytold/drawcraft"><b>DrawCraft</b></a><br><sub>Vector illustration — an Illustrator-class app with pen tools, live shapes, Pathfinder, type and SVG/PDF.</sub></td>
    <td width="20%" valign="top"><a href="https://github.com/storytold/filmcraft"><b>FilmCraft</b></a><br><sub>Non-linear video editor — a Premiere-class app with its own H.264/ProRes/AAC codecs, timeline and colour.</sub></td>
    <td width="20%" valign="top"><a href="https://github.com/storytold/lightcraft"><b>LightCraft</b></a><br><sub>Photo library and raw developer — a Lightroom-class app with a scene-referred pipeline, masking and presets.</sub></td>
    <td width="20%" valign="top"><a href="https://github.com/storytold/printcraft"><b>PrintCraft</b></a><br><sub>PDF viewer and editor — an Acrobat-class app for reading, organizing, annotating and editing PDFs.</sub></td>
  </tr>
</table>

Licence: MIT OR Apache-2.0. By the artcraft team.
