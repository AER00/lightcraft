# LightCraft — instructions for agents

LightCraft is a clean-room, open-source, pure-Rust photo library + non-destructive raw developer targeting Adobe Lightroom parity (and beyond). Native on macOS, Windows, Linux; web via WASM. Sibling of `../printcraft` (Acrobat), `../photocraft` (Photoshop), `../drawcraft` (Illustrator) and `../filmcraft` (Premiere), with the same conventions.

## Start every session here
1. Read `plan/STATUS.md` (current milestone, next unchecked task, blockers).
2. Read the task in `plan/execution-plan.md` §3, the relevant section of `plan/architecture.md`, and the README/docs of the crate you touch. Behaviour/visual reference: `plan/lightroom/` (incl. `10-observed-ui.md` + screenshots).
3. Follow the autonomous operation protocol (`plan/execution-plan.md` §7). Don't stop to ask unless §7 lists the decision as the user's.

`plan/` is gitignored (local-only).

## Non-negotiables
- **Clean-room.** Never read/disassemble anything inside Adobe app bundles (names/listings only). Never copy Adobe icons, presets, profiles (DCP), lens profiles (LCP), camera matrices, fonts. Observation of the installed Lightroom is read-only (it syncs the user's personal library: never import/edit/rate/delete there). Never copy GPL/LGPL/AGPL code (darktable, RawTherapee, ART, LibRaw, rawspeed, rawloader, rawler, lensfun, dcraw-derived GPL code…).
- **Pure Rust** in the product. No C/C++ dependencies.
- **Layering** (`plan/architecture.md` §3, enforced by `cargo xtask layers`): nothing below L5 depends on egui/eframe/winit/rfd.
- **Everything is a command** (`crates/engine`): id, label, menu path, shortcut, params, enabled(), run(). UI, CLI, control channel and MCP all dispatch by id. Every slider is a `develop` control spec.
- **Resolution independence:** settings use normalized image coordinates and relative radii; previews and exports must match.
- **Quality gates** before every commit: `cargo xtask ci` (fmt, clippy -D warnings, tests, layers, wasm).
- **Commits:** one task id per commit (`M2.3: local Laplacian highlights/shadows`). Only green states. End messages with the attribution line required by the environment.

## Assets: icons, images, fonts (ABSOLUTE RULE — never violate)
- **Never use any iconography, image, artwork, font, sound or other asset from Adobe products** (no Lightroom/Creative Cloud icons, no screenshots, no presets/profiles/LUTs, no UI bitmaps — not even as a temporary placeholder or "reference copy"). Observing Adobe's UI to imitate *layout and behaviour* is allowed; copying or tracing its assets is not.
- Every asset in the repository must be one of: **our own original work** (e.g. icons drawn in code as vectors, procedurally generated demo photos), **public domain / CC0**, **Creative Commons** (CC-BY / CC-BY-SA with attribution honoured), **OFL** (fonts), or **permissive open-source** (MIT/Apache-2.0/BSD/ISC) — or contributed by a person who created the asset and licenses it openly.
- **Every asset must have an entry in `assets/ATTRIBUTION.md`** (path, title, author/creator, source URL or "original work", licence, date added, modifications) and its licence text when required (e.g. `assets/fonts/OFL-*.txt`). Add the entry in the same commit as the asset. Assets without an attribution entry must not be committed.
- Icons drawn in code (e.g. `crates/ui-egui/src/icons.rs`) are original work and are recorded in `assets/ATTRIBUTION.md` as such; do not trace them from Adobe icons.
- Demo/test images: generated procedurally by `lightcraft-scenes`, or CC0 downloads kept in the gitignored `corpus/` with their source recorded. Screenshots of Adobe apps live only in the gitignored `plan/` and are never committed or published.
- When in doubt about an asset's licence: don't use it.

## Running and looking at the app
- `cargo run --release -p lightcraft -- --control 7980` opens the desktop app with the JSON-lines control server (see `docs/control-protocol.md`).
- For UI work, **look at the result**: drive via the control channel and take `ui.screenshot`, compare with `plan/lightroom/screenshots/`.
- MCP: `lightcraft-cli mcp` (see `docs/mcp.md`).
- Shell gotcha: `mv`/`cp` are aliased interactive here — use `/bin/mv -f` / `/bin/cp -f`.
- Parallel agents: separate git worktrees and `CARGO_TARGET_DIR=target/agent-<name>`; keep every `Cargo.toml` valid at all times (the `crates/*` glob means one broken manifest breaks everyone).
- Test corpora: `cargo xtask corpus --download` into `corpus/` (gitignored, CC0 only). Never commit media.
