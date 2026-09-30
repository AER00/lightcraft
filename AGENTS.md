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

## Running and looking at the app
- `cargo run --release -p lightcraft -- --control 7980` opens the desktop app with the JSON-lines control server (see `docs/control-protocol.md`).
- For UI work, **look at the result**: drive via the control channel and take `ui.screenshot`, compare with `plan/lightroom/screenshots/`.
- MCP: `lightcraft-cli mcp` (see `docs/mcp.md`).
- Shell gotcha: `mv`/`cp` are aliased interactive here — use `/bin/mv -f` / `/bin/cp -f`.
- Parallel agents: separate git worktrees and `CARGO_TARGET_DIR=target/agent-<name>`; keep every `Cargo.toml` valid at all times (the `crates/*` glob means one broken manifest breaks everyone).
- Test corpora: `cargo xtask corpus --download` into `corpus/` (gitignored, CC0 only). Never commit media.
