# Lightroom parity tracker

A living checklist of every Lightroom feature LightCraft aims to match, what we have, and what is missing. Agents and
humans pick work from **[Top gaps](#top-gaps)**; whoever lands a feature updates its row in the same commit.

- **Ids** come from the (local, gitignored) reference in `plan/lightroom/`: `LR-…` = feature catalog (03),
  `MENU-…` = menu items (04), `KEY-…` / `KEYC-…` = keyboard shortcuts (06, desktop / Classic), `LRC-…` = Classic
  extras (08). Ids are stable; never renumber.
- **Tier** as in the catalog: **P0** core, **P1** important parity, **P2** later / AI / niche, **OOS** out of scope.
- **Status**: ✅ done (usable end to end; polish may be noted) · 🟡 partial (note says what is missing, or the status
  is unverified) · ⬜ missing · 🚫 out of scope / not applicable.
- **Evidence**: `cmd:<id>` = command id (engine `command_specs()` or UI `UI_COMMANDS`), `ctl:<id>` = develop control
  id (`lightcraft-cli controls`), plus source files. A trailing `*` matches a prefix (`ctl:mixer.*`).
- Feature names and notes are our own words. No Adobe text, screenshots or assets belong here.

`cargo xtask parity` checks that every `cmd:`/`ctl:` id and file path below still exists (part of `cargo xtask ci`)
and prints the summary; `cargo xtask parity --write` refreshes the summary table below.

## Summary

<!-- parity:summary -->
| Section | ✅ | 🟡 | ⬜ | 🚫 | P0 done | P1 done |
|---|---:|---:|---:|---:|---:|---:|
| A. Import (IMP) | 2 | 4 | 7 | 1 | 1/4 (25%) | 1/4 (25%) |
| B. Library management (LIB) | 12 | 4 | 9 | 2 | 7/9 (78%) | 4/9 (44%) |
| C. Views & navigation (VIEW) | 9 | 2 | 6 | 0 | 7/9 (78%) | 1/4 (25%) |
| D. Search & filter (FILT) | 7 | 2 | 4 | 0 | 4/4 (100%) | 3/4 (75%) |
| E. Metadata (META) | 2 | 2 | 2 | 0 | 2/2 (100%) | 0/2 (0%) |
| F. Edit panel — global adjustments (EDIT) | 36 | 2 | 10 | 1 | 28/28 (100%) | 8/14 (57%) |
| G. Profiles (PROF) | 4 | 1 | 5 | 0 | 2/3 (67%) | 1/3 (33%) |
| H. Crop & rotate (CROP) | 7 | 2 | 0 | 1 | 6/6 (100%) | 1/3 (33%) |
| I. Remove / healing (REM) | 3 | 3 | 4 | 2 | 2/4 (50%) | 1/3 (33%) |
| J. Red eye (EYE) | 0 | 0 | 2 | 0 | — | 0/1 (0%) |
| K. Masking (MASK) | 6 | 10 | 7 | 0 | 4/8 (50%) | 2/5 (40%) |
| L. Presets (PRE) | 1 | 4 | 2 | 1 | 0/2 (0%) | 1/2 (50%) |
| M. Versions & history (VER) | 3 | 1 | 1 | 0 | 1/1 (100%) | 2/3 (67%) |
| N. Copy / paste / sync (SYNC) | 4 | 0 | 1 | 0 | 3/3 (100%) | 1/1 (100%) |
| O. Merge (MERGE) | 3 | 0 | 1 | 0 | — | — |
| P. Enhance (ENH) | 0 | 0 | 2 | 0 | — | — |
| Q. HDR (HDR) | 0 | 0 | 5 | 0 | — | — |
| R. Video (VID) | 0 | 0 | 5 | 1 | — | 0/2 (0%) |
| S. Export (EXP) | 4 | 6 | 8 | 0 | 2/7 (29%) | 2/7 (29%) |
| T. Share (SHARE) | 0 | 0 | 0 | 4 | — | — |
| U. Map & location (MAP) | 0 | 1 | 1 | 0 | — | 0/1 (0%) |
| V. Preferences (PREF) | 0 | 3 | 5 | 3 | 0/1 (0%) | 0/4 (0%) |
| W. Cloud & AI infrastructure (CLOUD / AI) | 0 | 1 | 1 | 2 | — | — |
| X. Cross-cutting behaviours (BEHAV) | 10 | 2 | 5 | 1 | 7/8 (88%) | 3/5 (60%) |
| Y. Menus | 46 | 18 | 21 | 8 | 37/47 (79%) | 7/23 (30%) |
| Z. Keyboard shortcuts (desktop) | 46 | 16 | 18 | 1 | 39/52 (75%) | 7/23 (30%) |
| Lightroom Classic extras | 4 | 19 | 56 | 9 | — | 3/21 (14%) |
| **Total** | 209 | 103 | 188 | 37 | 152/198 (77%) | 48/144 (33%) |
<!-- /parity:summary -->

## Top gaps

Ordered by tier, then user value, then (low) effort. Take the first one nobody is working on.

1. **LR-EXP-COLORSPACE** (P0) — export is sRGB only. Add Display P3 / Adobe RGB-compatible / ProPhoto-compatible /
   Rec.2020 output (our own primaries + ICC from `crates/codecs/src/icc.rs`). High value, low–medium effort.
2. **LR-MASK-OVERLAY + LR-MASK-PINS** (P0/P1) — show the evaluated mask alpha as a coloured overlay (all mask
   kinds, colour/opacity choice), not just brush dabs and outlines. High value, medium effort.
3. **LR-MASK-BRUSH + LR-MASK-SLIDERS** (P0) — apply the stored Auto Mask flag (edge-aware brush) and the local
   Noise / Moiré / Defringe sliders, which are stored but not rendered. High value, medium effort.
4. **LR-REM-SPOT-EDIT + LR-REM-BRUSH-PARAMS** (P0) — select a spot pin, move target/source, delete with ⌫,
   feather/opacity sliders, `[`/`]` size keys. Medium effort.
5. **LR-VIEW-PHOTOGRID** (P0) — group the justified grid by capture date with headers. Low–medium effort.
6. **LR-PRE-CREATE + LR-PRE-PANEL + LR-BEHAV-PREVIEW-HOVER** (P0/P1) — per-group checkboxes in Create Preset;
    live preview while hovering presets/profiles/versions. Medium effort.
7. **LR-PROF-DROPDOWN** (P0) — favourites/recent in the profile menu (and later a browser, LR-PROF-BROWSER).
8. **LR-LIB-KEYWORD** (P0) — rename/delete a keyword library-wide; keyword list in the left panel for browsing.
9. **LR-PREF-GENERAL + LR-IMP-RAWDEFAULT** (P0/P1) — a Settings dialog: raw defaults (preset or camera-specific),
    XMP prefs, cache, GPU. Medium effort.
10. **LR-IMP-ADD-DIALOG + LR-IMP-LOCAL** (P0) — an import review grid with per-photo checkboxes and destination
    album; browse folders before adding. Medium effort.
11. **LR-EXP-TYPE + LR-EXP-DIM** (P0) — Original (+XMP) and DNG export (writer exists in
    `crates/raw/src/dngwrite.rs`), short edge / width / height / megapixels, "don't enlarge", ppi.
12. **LR-IMP-FORMATS** (P0) — CR3, compressed NEF/RAF/ORF, RW2 v4 (preview only today); HEIC/AVIF decode (no
    permissive pure-Rust decoder yet). High value, high effort (clean sources needed).
13. **LR-VIEW-FULLSCREEN + LR-VIEW-NAVIGATOR + LR-VIEW-INFOOVERLAY** (P1) — view modes. Medium effort.
14. **LR-LIB-RENAME + LR-LIB-CAPTURETIME + LR-LIB-LABEL UI** (P1) — batch rename, capture-time edit, label menu/names.
15. **LR-EXP-BITDEPTH + LR-EXP-COMPRESSION** (P1) — 16-bit TIFF/PNG and TIFF compression choice.
16. **LR-EDIT-OPTICS-PROFILE** (P1) — a lens-profile database of our own (embedded DNG/maker corrections work today).

## Shortcuts: conflicts and missing bindings

Compared `plan/lightroom/06-shortcuts.md` (desktop part) with our bindings: command specs in `crates/engine/src/cmd/`,
`UI_COMMANDS` in `crates/ui-egui/src/menus.rs` and secondary bindings (`ALIASES`) in `crates/ui-egui/src/shortcuts.rs`.
`no_conflicting_bindings` (same file) fails when one key fires two actions.

**Fixed (M16.1):** added Lightroom-desktop keys as secondary bindings for existing commands — ⌘D Select None, ⇧E
Export dialog, Space Toggle zoom, ⇧M Create Version, ⇧X Reject + advance, ⇧U Unflag + advance. `⇧Y` fired both
Before/After Split and the History panel; History lost the binding. `W` and `⇧⌘I` also fired the engine command
under the UI command that wraps it (a no-op error); the UI command now wins.

**Deliberate differences (our key → Lightroom desktop key)** — each is a conflict with another binding we have:

| Action | Ours | Lightroom desktop | Why |
|---|---|---|---|
| Pick flag | P | Z | Z = toggle zoom (Classic convention); P is the Classic pick key |
| Photos panel (left) | ⌘⇧L | P | P = pick |
| Expand/collapse edit sections | ⌘⌥1–5 | ⌘1–6 | ⌘0 = Zoom to Fit, ⌘1 = Zoom 100 %; Geometry lives in the Crop panel |
| Histogram | ⌘⇧H | ⌘0 | ⌘0 = Zoom to Fit |
| Square Grid | ⇧G | (none; ⇧G = Guided Upright) | Guided Upright is a button in the Crop panel |
| Export dialog | ⌘⇧E (+ ⇧E) | ⇧E | ⌘⇧E is "Edit in Photoshop" there; no external-editor command yet |
| Crop overlay cycle | ⇧O | O | O = mask overlay; ⇧O (mask colour / overlay orientation) unused otherwise |
| Create Version | ⌘⇧S (+ ⇧M) | ⇧M (Windows: Ctrl+⇧S) | — |
| Select None | ⌘⇧A (+ ⌘D) | ⌘D | — |

**Still missing / broken:**
- No command yet: F full-screen preview, ⇧⌘F window full screen, ⇧⌘V paste selected,
  ⌘, settings, ⌘F focus search, ⌘G / ⇧⌘G stacks, A visualize spots, `[` `]` / ⇧`[` ⇧`]`
  brush size/feather, ⌃H / ⌃M merges, F1 help, ⇧6–9 label + advance.
- `⌫` in the Masking panel deletes the active mask; spots have no pin selection yet (⌫ does nothing in Remove).
- `H` opens Remove; Lightroom also uses it (Classic) to hide pins — no pin toggle yet.
- ⌘M / ⌘H / ⌘Q / ⌘W rely on the platform window defaults (unverified).

<!-- Sections below hold one row per id. Keep the column order: Id | Feature | Tier | Status | Evidence | Notes. -->

## A. Import (IMP)

| Id | Feature | Tier | Status | Evidence | Missing / notes |
|---|---|---|---|---|---|
| LR-IMP-ADD-DIALOG | Add photos/folders | P0 | 🟡 | `cmd:file.addPhotos`, `cmd:library.import` (`mode`, `album`) | file picker + import report; no review grid with per-photo checkboxes / destination album UI |
| LR-IMP-DRAGDROP | Drop files/folders to import | P0 | ✅ | `crates/ui-egui/src/lib.rs` (dropped files → `cmd:library.import`) | dropping onto a specific album not supported |
| LR-IMP-DUPES | Skip duplicates by content | P1 | ✅ | `crates/engine/src/import.rs`, `crates/engine/src/tests_import.rs` | |
| LR-IMP-DEVICE | Import from camera/card | P1 | ⬜ | | no device detection |
| LR-IMP-AUTO | Watched-folder auto import | P2 | ⬜ | | |
| LR-IMP-PRESET | Preset on import | P2 | ⬜ | | |
| LR-IMP-RAWDEFAULT | Raw defaults | P1 | ⬜ | `crates/engine/src/import.rs` | raws get embedded lens corrections on import; no user raw-default setting |
| LR-IMP-MIGRATE | Migrate other catalogs | OOS | 🚫 | | |
| LR-IMP-PROFILES | Import profiles & presets | P1 | 🟡 | `cmd:file.importPresets`, `cmd:preset.import` | presets (.lcpreset, XMP `crs:`) only; no profile import; Adobe profile formats are deliberately unsupported |
| LR-IMP-LOCAL | Work on files in place | P0 | 🟡 | `cmd:library.import` (mode add), `crates/engine/src/sidecar.rs`, `cmd:library.toggleAutoWriteXmp` | files referenced in place with XMP sidecars; no browse-a-folder-without-adding view |
| LR-IMP-SIDECAR-SPLIT | Separate XMP sidecar variants | P2 | ⬜ | `cmd:library.xmpPreferences` | sidecar naming option exists (stem/full), no split sidecars |
| LR-IMP-FORMATS | Supported formats | P0 | 🟡 | `crates/codecs/src/lib.rs`, `crates/raw/src/lib.rs` | JPEG, PNG, TIFF, WebP, JXL, PSD, GIF, BMP; DNG, CR2, ARW, NEF, RAF, RW2, PEF, ORF. Missing: CR3, compressed NEF/RAF/ORF, RW2 v4 (preview only), HEIC/AVIF decode |
| LR-IMP-CULL-AT-IMPORT | Culling analysis at import | P2 | ⬜ | | |
| LR-IMP-DNG-CONVERT | Convert to DNG on import [Classic] | P2 | ⬜ | `crates/raw/src/dngwrite.rs` | DNG writer exists, not wired to import |

## B. Library management (LIB)

| Id | Feature | Tier | Status | Evidence | Missing / notes |
|---|---|---|---|---|---|
| LR-LIB-ALLPHOTOS | All photos | P0 | ✅ | `cmd:library.source`, `crates/ui-egui/src/panels/left.rs` | |
| LR-LIB-RECENT-ADDED | Recently added | P1 | 🟡 | `cmd:library.source` (`recentlyAdded`) | not grouped by import session |
| LR-LIB-BYDATE | Browse by date | P1 | 🟡 | `cmd:library.filter` (`date`), `crates/ui-egui/src/panels/left.rs` | years only; no month/day tree |
| LR-LIB-ALBUM | Albums | P0 | ✅ | `cmd:album.create`, `cmd:album.rename`, `cmd:album.delete`, `cmd:album.addPhotos`, `cmd:album.removePhotos`, `cmd:dialog.newAlbum` | no drag photos onto album (LR-BEHAV-DRAGDROP), no album sort |
| LR-LIB-FOLDER | Folders of albums | P0 | ✅ | `cmd:album.create` (`folder`), `cmd:album.move` | moving is command-only (no drag, no "Move to" menu) |
| LR-LIB-SMARTALBUM | Smart albums | P1 | ✅ | `cmd:album.createSmart`, `cmd:album.setRules`, `crates/catalog/src/query.rs`, `crates/ui-egui/src/panels/filterbar.rs` | saved filters (rating/flag/label/kind/edited/keyword/camera/lens/date range/text/album), live; match-all only (no any/none rule groups, no rule editor dialog — rules come from the filter bar or `album.setRules`) |
| LR-LIB-SHARED-ALBUM | Shared albums | P2 | ⬜ | | needs a sharing service |
| LR-LIB-OFFLINE | Keep album offline | P2 | 🚫 | | not applicable: local-first library |
| LR-LIB-TARGET | Target album | P2 | ⬜ | | |
| LR-LIB-RATING | Star ratings | P0 | ✅ | `cmd:photo.rate` (`advance`), `crates/ui-egui/src/shortcuts.rs` | |
| LR-LIB-FLAG | Pick / reject flags | P0 | ✅ | `cmd:photo.pick`, `cmd:photo.reject`, `cmd:photo.unflag`, `cmd:photo.flag` | pick key is P (see Shortcuts); no flag cycle |
| LR-LIB-LABEL | Colour labels | P1 | 🟡 | `cmd:photo.label`, keys 6–9 in `crates/ui-egui/src/shortcuts.rs` | no label menu/buttons, no purple key, label names not editable |
| LR-LIB-KEYWORD | Keywords | P0 | 🟡 | `cmd:panel.keywords`, `cmd:photo.setMeta` (`addKeywords`/`removeKeywords`) | no library-wide rename/delete, no keyword browser in the left panel |
| LR-LIB-PEOPLE | People / faces | P2 | ⬜ | | |
| LR-LIB-STACK | Stacks | P1 | ✅ | `cmd:stack.group`, `cmd:stack.ungroup`, `cmd:stack.toggle`, `cmd:stack.setTop`, `cmd:stack.remove`, `cmd:stack.auto`, `crates/catalog/src/stacks.rs` | grid/filmstrip count badges, expand/collapse, auto-stack by capture time; no visual-similarity auto-stack |
| LR-LIB-VERSIONS | Versions | P1 | ✅ | `cmd:version.create` | see section M |
| LR-LIB-DELETE | Delete / Recently Deleted | P0 | ✅ | `cmd:photo.delete`, `cmd:photo.restore`, `cmd:photo.deletePermanently` | no confirmation dialog, no auto-purge after N days, no "Empty" |
| LR-LIB-REMOVE-ALBUM | Remove from album | P0 | ✅ | `cmd:album.removePhotos` | |
| LR-LIB-DUPLICATE | Duplicate a photo | P2 | ⬜ | | |
| LR-LIB-RENAME | Batch rename | P1 | ⬜ | | |
| LR-LIB-CAPTURETIME | Edit capture time | P1 | ⬜ | | `photo.setMeta` has no capture-time field |
| LR-LIB-SHOWFINDER | Reveal original in file manager | P0 | ⬜ | | |
| LR-LIB-COVER | Album cover | P2 | ✅ | `cmd:album.setCover` | |
| LR-LIB-CULL | Assisted culling | P2 | ⬜ | | |
| LR-LIB-ACTIVITY | Comments & likes | OOS | 🚫 | | |
| LR-LIB-QUICKCOLL | Quick collection [Classic] | P2 | ⬜ | | |
| LR-LIB-VIRTUALCOPY | Virtual copies [Classic] | P1 | ✅ | `cmd:photo.virtualCopy`, `crates/engine/src/cmd/organize.rs` | “Copy N” badge, stacked with the original, same albums, no XMP writes; no “Set Copy as Master” |

## C. Views & navigation (VIEW)

| Id | Feature | Tier | Status | Evidence | Missing / notes |
|---|---|---|---|---|---|
| LR-VIEW-PHOTOGRID | Justified photo grid | P0 | 🟡 | `cmd:view.photoGrid`, `crates/ui-egui/src/panels/grid.rs` | no grouping by date |
| LR-VIEW-SQUAREGRID | Square grid | P0 | ✅ | `cmd:view.squareGrid` | |
| LR-VIEW-DETAIL | Single-photo view | P0 | ✅ | `cmd:view.detail`, `crates/ui-egui/src/panels/detail.rs` | |
| LR-VIEW-EDIT | Edit view | P0 | ✅ | `cmd:panel.edit` | |
| LR-VIEW-FULLSCREEN | Full-screen preview | P1 | ⬜ | | |
| LR-VIEW-FILMSTRIP | Filmstrip | P0 | ✅ | `cmd:view.filmstrip` | |
| LR-VIEW-ZOOM | Zoom & pan | P0 | ✅ | `cmd:view.zoomFit`, `cmd:view.zoom100`, `cmd:view.zoomIn`, `cmd:view.zoomOut`, `cmd:view.zoomToggle` | steps 25–800 % (not 6–1600 %); Fill only in the bottom bar |
| LR-VIEW-NAVIGATOR | Navigator mini map | P1 | ⬜ | | |
| LR-VIEW-BEFOREAFTER | Before / after | P0 | 🟡 | `cmd:view.showOriginal`, `cmd:view.beforeAfter`, `cmd:view.beforeAfterSplit`, `cmd:view.beforeAfterTopBottom`, `cmd:view.beforeAfterSplitTopBottom` | all four layouts; "before" is always the original (no before-state from history) |
| LR-VIEW-COMPARE | Compare two photos | P1 | ✅ | `cmd:view.compare`, `cmd:compare.swap`, `cmd:compare.makeSelect`, `crates/ui-egui/src/panels/compare.rs` | select / candidate, synced zoom + pan, arrows move the candidate; no zoom-link toggle |
| LR-VIEW-SURVEY | Survey view [Classic] | P2 | ✅ | `cmd:view.survey`, `crates/ui-egui/src/panels/compare.rs` | selection tiled (≤ 48), keys act on the active photo, hover × removes |
| LR-VIEW-INFOOVERLAY | Info overlay on the photo | P1 | ⬜ | | |
| LR-VIEW-SLIDESHOW | Slideshow | P2 | ⬜ | | |
| LR-VIEW-SECONDWINDOW | Second display window [Classic] | P2 | ⬜ | | |
| LR-VIEW-CLIPPING | Clipping indicators | P0 | ✅ | `cmd:view.clipping` | |
| LR-VIEW-HISTOGRAM | Histogram | P0 | ✅ | `cmd:view.histogram`, `crates/ui-egui/src/panels/edit.rs` | no drag-to-adjust on the histogram |
| LR-VIEW-HDR-DISPLAY | HDR display output | P2 | ⬜ | | |

## D. Search & filter (FILT)

| Id | Feature | Tier | Status | Evidence | Missing / notes |
|---|---|---|---|---|---|
| LR-FILT-SEARCH-META | Text search | P0 | ✅ | `cmd:library.filter` (`text`), `crates/ui-egui/src/panels/topbar.rs`, `crates/catalog/src/query.rs` | fielded tokens (`rating:3`, `iso:>800`, `camera:…`); no suggestions dropdown |
| LR-FILT-SEARCH-AI | Natural-language search | P2 | ⬜ | | |
| LR-FILT-RATING | Rating filter | P0 | ✅ | `cmd:library.filter` (`rating`, `ratingOp`), `crates/ui-egui/src/panels/filterbar.rs` | ≥ / = / ≤ stars in the filter bar (`cmd:view.filterBar`) |
| LR-FILT-FLAG | Flag filter | P0 | ✅ | `cmd:library.filter` (`flag`), `crates/ui-egui/src/panels/filterbar.rs` | picked / rejected / unflagged (one at a time) |
| LR-FILT-LABEL | Colour-label filter | P1 | ✅ | `cmd:library.filter` (`label`), `crates/ui-egui/src/panels/filterbar.rs` | one label at a time; no “no label” choice |
| LR-FILT-TYPE | Type / edited filter | P1 | 🟡 | `cmd:library.filter` (`kind`, `edited`), `crates/ui-egui/src/panels/filterbar.rs` | photos / raw / videos, edited / unedited; no HDR/panorama/depth kinds |
| LR-FILT-KEYWORD | Keyword filter | P1 | ✅ | `cmd:library.filter` (`keyword`), `crates/ui-egui/src/panels/filterbar.rs` | keyword picker |
| LR-FILT-CAMERA | Camera / lens filter | P1 | ✅ | `cmd:library.filter` (`camera`, `lens`), `crates/ui-egui/src/panels/filterbar.rs` | camera and lens pickers |
| LR-FILT-LOCATION | Location filter | P2 | 🟡 | `cmd:library.filter` (`text`) | free-text match on the location field only |
| LR-FILT-PEOPLE | People filter | P2 | ⬜ | | |
| LR-FILT-CULL | Culling-score filters | P2 | ⬜ | | |
| LR-FILT-SORT | Sort | P0 | ✅ | `cmd:library.sort`, `crates/ui-egui/src/panels/bottombar.rs` | no colour-label or custom (manual) order |
| LR-FILT-SAVED | Filter presets [Classic] | P2 | ⬜ | | |

## E. Metadata (META)

| Id | Feature | Tier | Status | Evidence | Missing / notes |
|---|---|---|---|---|---|
| LR-META-INFO | Info panel | P0 | ✅ | `cmd:panel.info`, `cmd:photo.setMeta`, `crates/ui-egui/src/panels/right.rs` | no flash, creator field, map snippet or people |
| LR-META-COPYRIGHT-DEFAULT | Default copyright on import | P1 | ⬜ | | |
| LR-META-LOCATION | Location editing | P2 | 🟡 | `cmd:photo.setMeta` (`location`) | text field + GPS read; no map, no geocoding |
| LR-META-COPYPASTE | Copy / paste metadata | P2 | ⬜ | | |
| LR-META-XMP | XMP read/write | P0 | ✅ | `cmd:photo.saveMetadataToFile`, `cmd:photo.readMetadataFromFile`, `cmd:library.xmpPreferences`, `crates/engine/src/sidecar.rs`, `docs/xmp-interop.md` | |
| LR-META-EXIF-FULL | Full EXIF/IPTC [Classic] | P1 | 🟡 | `crates/meta/src/exif.rs`, `crates/meta/src/iptc.rs` | read and written on export; panel shows a subset; no metadata presets |

## F. Edit panel — global adjustments (EDIT)

| Id | Feature | Tier | Status | Evidence | Missing / notes |
|---|---|---|---|---|---|
| LR-EDIT-AUTO | Auto settings | P0 | ✅ | `cmd:develop.auto`, `crates/pipeline/src/auto.rs` | |
| LR-EDIT-BW | Black & white | P0 | ✅ | `cmd:develop.treatment` | |
| LR-EDIT-HDR-MODE | HDR editing | P2 | ⬜ | | |
| LR-EDIT-LIGHT-EXPOSURE | Exposure | P0 | ✅ | `ctl:light.exposure` | |
| LR-EDIT-LIGHT-CONTRAST | Contrast | P0 | ✅ | `ctl:light.contrast` | |
| LR-EDIT-LIGHT-HIGHLIGHTS | Highlights | P0 | ✅ | `ctl:light.highlights` | |
| LR-EDIT-LIGHT-SHADOWS | Shadows | P0 | ✅ | `ctl:light.shadows` | |
| LR-EDIT-LIGHT-WHITES | Whites | P0 | ✅ | `ctl:light.whites` | |
| LR-EDIT-LIGHT-BLACKS | Blacks | P0 | ✅ | `ctl:light.blacks` | |
| LR-EDIT-LIGHT-CURVE-PARAM | Parametric curve | P0 | ✅ | `ctl:curve.highlights`, `ctl:curve.lights`, `ctl:curve.darks`, `ctl:curve.shadows`, `ctl:curve.split*` | |
| LR-EDIT-LIGHT-CURVE-POINT | Point curve | P0 | ✅ | `cmd:develop.curve`, `crates/ui-egui/src/panels/edit.rs` | no curve presets (linear / medium / strong) |
| LR-EDIT-LIGHT-CURVE-RGB | Per-channel curves | P0 | ✅ | `cmd:develop.curve` (`channel`) | |
| LR-EDIT-LIGHT-CURVE-REFINESAT | Curve saturation compensation | P1 | ⬜ | | |
| LR-EDIT-LIGHT-CURVE-TAT | Drag-on-image curve adjust | P1 | ⬜ | | |
| LR-EDIT-COLOR-WB-PRESET | White-balance presets | P0 | ✅ | `cmd:develop.wb` | |
| LR-EDIT-COLOR-WB-PICKER | White-balance eyedropper | P0 | ✅ | `cmd:tool.wbPicker`, `cmd:develop.wbPick` | no magnified loupe while picking |
| LR-EDIT-COLOR-TEMP | Temperature | P0 | ✅ | `ctl:wb.temp` | relative scale for non-raw in the UI |
| LR-EDIT-COLOR-TINT | Tint | P0 | ✅ | `ctl:wb.tint` | |
| LR-EDIT-COLOR-VIBRANCE | Vibrance | P0 | ✅ | `ctl:color.vibrance` | |
| LR-EDIT-COLOR-SATURATION | Saturation | P0 | ✅ | `ctl:color.saturation` | |
| LR-EDIT-COLOR-MIXER-HSL | 8-band colour mixer | P0 | ✅ | `ctl:mixer.*` | no targeted (drag-on-image) mode |
| LR-EDIT-COLOR-MIXER-BW | B&W mix | P1 | 🟡 | `ctl:bw.*` | no auto mix |
| LR-EDIT-COLOR-POINTCOLOR | Point colour | P1 | ⬜ | | |
| LR-EDIT-COLOR-GRADING | Colour grading wheels | P0 | ✅ | `ctl:grading.*` | |
| LR-EDIT-EFFECTS-TEXTURE | Texture | P0 | ✅ | `ctl:effects.texture` | |
| LR-EDIT-EFFECTS-CLARITY | Clarity | P0 | ✅ | `ctl:effects.clarity` | |
| LR-EDIT-EFFECTS-DEHAZE | Dehaze | P0 | ✅ | `ctl:effects.dehaze` | |
| LR-EDIT-EFFECTS-VIGNETTE | Post-crop vignette | P0 | ✅ | `ctl:vignette.*`, `crates/pipeline/src/finish.rs`, `crates/ui-egui/src/panels/edit.rs` | style picker (Highlight / Color / Paint) in the Effects section |
| LR-EDIT-EFFECTS-GRAIN | Grain | P1 | ✅ | `ctl:grain.*` | |
| LR-EDIT-DETAIL-SHARPEN | Sharpening | P0 | ✅ | `ctl:detail.sharpenAmount`, `ctl:detail.sharpenRadius`, `ctl:detail.sharpenDetail`, `ctl:detail.sharpenMasking` | no Alt-drag mask preview |
| LR-EDIT-DETAIL-NR | Luminance noise reduction | P0 | ✅ | `ctl:detail.nrLuminance`, `ctl:detail.nrDetail`, `ctl:detail.nrContrast` | |
| LR-EDIT-DETAIL-CNR | Colour noise reduction | P0 | ✅ | `ctl:detail.nrColor`, `ctl:detail.nrColorDetail`, `ctl:detail.nrColorSmoothness` | |
| LR-EDIT-DETAIL-DENOISE | AI denoise | P2 | ⬜ | | settings field reserved, not rendered |
| LR-EDIT-DETAIL-RAWDETAILS | Improved demosaic toggle | P2 | ⬜ | | |
| LR-EDIT-DETAIL-SUPERRES | Super resolution | P2 | ⬜ | | |
| LR-EDIT-DETAIL-AISHARPEN | AI sharpen | OOS | 🚫 | | |
| LR-EDIT-OPTICS-CA | Remove chromatic aberration | P1 | ✅ | `crates/ui-egui/src/panels/edit.rs` (checkbox), `ctl:optics.caRed`, `ctl:optics.caBlue` | |
| LR-EDIT-OPTICS-PROFILE | Lens profile corrections | P1 | 🟡 | `ctl:optics.profileDistortion`, `ctl:optics.profileVignetting`, `crates/pipeline/src/optics.rs` | uses corrections embedded in DNG/raw files; no lens-profile database |
| LR-EDIT-OPTICS-DEFRINGE | Defringe | P1 | ✅ | `ctl:optics.defringe*` | no fringe eyedropper |
| LR-EDIT-OPTICS-MANUAL | Manual distortion / vignetting | P1 | ✅ | `ctl:optics.distortion`, `ctl:optics.vignetting`, `ctl:optics.vignettingMidpoint` | |
| LR-EDIT-GEOM-UPRIGHT | Upright | P1 | ✅ | `cmd:geometry.upright`, `cmd:geometry.guides` | |
| LR-EDIT-GEOM-MANUAL | Manual transform | P1 | ✅ | `ctl:geometry.*` | |
| LR-EDIT-GEOM-CONSTRAIN | Constrain crop | P1 | ✅ | `crates/ui-egui/src/panels/right.rs` (checkbox → `cmd:develop.merge`) | |
| LR-EDIT-GEOM-GRID | Grid while transforming | P2 | ⬜ | | |
| LR-EDIT-LENSBLUR | Lens blur | P2 | ⬜ | | settings field reserved, not rendered |
| LR-EDIT-CALIB | Calibration [Classic] | P1 | ⬜ | | needed for XMP interop |
| LR-EDIT-SECTION-TOGGLE | Section on/off | P1 | ✅ | `cmd:develop.sectionEnabled` | |
| LR-EDIT-RESET | Reset all / section / slider | P0 | ✅ | `cmd:develop.reset`, `cmd:develop.resetSection`, `cmd:develop.resetControl`, `crates/ui-egui/src/widgets.rs` (double-click) | no "reset to open" |
| LR-EDIT-SHOWORIG | Show original | P0 | ✅ | `cmd:view.showOriginal` | |

## G. Profiles (PROF)

| Id | Feature | Tier | Status | Evidence | Missing / notes |
|---|---|---|---|---|---|
| LR-PROF-DROPDOWN | Profile menu | P0 | 🟡 | `cmd:develop.profile`, `cmd:profiles.list`, `cmd:profiles.menu`, `cmd:profile.favorite`, `crates/ui-egui/src/panels/edit.rs` | Favorites, Recent (last 5), group submenus, favourite toggle; favourites/recent persist with the library; Amount slider under the menu for non-default profiles; no Browse… entry yet |
| LR-PROF-BROWSER | Profile browser | P1 | ⬜ | | |
| LR-PROF-ADOBE | Standard raw looks (own equivalents) | P0 | ✅ | `crates/engine/src/presets.rs` (`PROFILES`), `crates/pipeline/src/profiles.rs` | six own looks: Color, Neutral, Vivid, Landscape, Portrait, Monochrome |
| LR-PROF-ADAPTIVE | Adaptive profiles | P2 | ⬜ | | |
| LR-PROF-CAMERA | Camera-matching looks | P2 | ⬜ | | |
| LR-PROF-CREATIVE | Creative profiles (own) | P2 | ✅ | `cmd:develop.profile`, `crates/pipeline/src/profiles.rs` | 16 own looks in Film / Cinematic / Muted / B&W (tone + point-curve fades, colour grading, mixer / B&W mix); scale with `ctl:profile.amount`; sliders untouched |
| LR-PROF-LEGACY | Legacy profiles | P2 | ⬜ | | |
| LR-PROF-NONRAW | Profiles for non-raw files | P0 | ✅ | `cmd:develop.profile` | same looks apply to JPEG/TIFF |
| LR-PROF-AMOUNT | Profile amount | P1 | ✅ | `ctl:profile.amount` | |
| LR-PROF-IMPORT | Import profiles | P1 | ⬜ | | |

## H. Crop & rotate (CROP)

| Id | Feature | Tier | Status | Evidence | Missing / notes |
|---|---|---|---|---|---|
| LR-CROP-RECT | Crop rectangle | P0 | ✅ | `cmd:crop.set`, `crates/ui-egui/src/panels/detail.rs` | |
| LR-CROP-ASPECT | Aspect ratios | P0 | ✅ | `cmd:crop.aspect`, `cmd:crop.rotateAspect` | no "As Shot"; custom ratio via command params only |
| LR-CROP-STRAIGHTEN | Straighten tool | P0 | ✅ | `cmd:crop.straighten`, `cmd:crop.autoStraighten` | Straighten Tool button: drag along a horizon/vertical; double-click or Auto levels automatically |
| LR-CROP-AUTO | Auto straighten | P1 | ✅ | `cmd:crop.autoStraighten` | crop-angle leveling from detected horizon/plumb lines (consensus required) |
| LR-CROP-ANGLE | Angle slider | P0 | ✅ | `ctl:crop.angle` | |
| LR-CROP-ROTATE90 | Rotate 90° | P0 | ✅ | `cmd:photo.rotateLeft`, `cmd:photo.rotateRight` | |
| LR-CROP-FLIP | Flip | P0 | ✅ | `cmd:photo.flipHorizontal`, `cmd:photo.flipVertical` | |
| LR-CROP-OVERLAY | Crop overlays | P1 | 🟡 | `cmd:view.cropOverlay` | thirds, grid, golden ratio, diagonal; no triangle / spiral / aspect overlays or orientation cycle |
| LR-CROP-ZOOM | Zoom while cropping | P1 | 🟡 | | unverified |
| LR-CROP-GENEXPAND | Generative expand | OOS | 🚫 | | |

## I. Remove / healing (REM)

| Id | Feature | Tier | Status | Evidence | Missing / notes |
|---|---|---|---|---|---|
| LR-REM-CONTENTAWARE | Content-aware remove | P1 | 🟡 | `cmd:spot.add` (`mode: remove`), `crates/pipeline/src/spots.rs` | heal with automatic source; no patch synthesis (M8.4) |
| LR-REM-HEAL | Heal | P0 | ✅ | `cmd:spot.add` (`mode: heal`) | |
| LR-REM-CLONE | Clone | P0 | ✅ | `cmd:spot.add` (`mode: clone`) | |
| LR-REM-GEN | Generative remove | OOS | 🚫 | | |
| LR-REM-DETECT | Object detection for remove | P2 | ⬜ | | |
| LR-REM-BRUSH-PARAMS | Brush size / feather / opacity | P0 | 🟡 | `cmd:spot.add` (`size`, `feather`, `opacity`), `crates/ui-egui/src/panels/right.rs` | UI has size only; no `[` `]` keys |
| LR-REM-SPOT-EDIT | Edit existing spots | P0 | 🟡 | `cmd:spot.delete` | no pin selection, no moving target/source, ⌫ deletes the photo instead |
| LR-REM-VISUALIZE | Visualize spots | P1 | ⬜ | | |
| LR-REM-PEOPLE | Remove people (generative) | OOS | 🚫 | | |
| LR-REM-REFLECT | Remove reflections | P2 | ⬜ | | |
| LR-REM-DUST | Dust detection | P2 | ⬜ | | |
| LR-REM-SYNC | Sync spots | P1 | ✅ | `cmd:develop.copy` (`groups`), `crates/develop/src/presets.rs` | |

## J. Red eye (EYE)

| Id | Feature | Tier | Status | Evidence | Missing / notes |
|---|---|---|---|---|---|
| LR-EYE-RED | Red-eye correction | P1 | ⬜ | `cmd:panel.redeye` | placeholder panel; settings field not rendered |
| LR-EYE-PET | Pet eye | P2 | ⬜ | | |

## K. Masking (MASK)

| Id | Feature | Tier | Status | Evidence | Missing / notes |
|---|---|---|---|---|---|
| LR-MASK-PANEL | Masks panel | P0 | 🟡 | `cmd:panel.masking`, `cmd:mask.add`, `cmd:mask.select`, `cmd:mask.rename`, `cmd:mask.duplicate`, `cmd:mask.visible`, `cmd:mask.delete` | no reorder, no one-step duplicate-and-invert |
| LR-MASK-SUBJECT | Select subject | P2 | 🟡 | `cmd:mask.add` (`subject`), `crates/pipeline/src/masks.rs` | saliency heuristic, no segmentation model |
| LR-MASK-SKY | Select sky | P2 | 🟡 | `cmd:mask.add` (`sky`) | heuristic |
| LR-MASK-BACKGROUND | Select background | P2 | 🟡 | `cmd:mask.add` (`background`) | inverse of the subject heuristic |
| LR-MASK-OBJECTS | Object selection | P2 | ⬜ | | shape exists (falls back to the subject heuristic); no UI |
| LR-MASK-PEOPLE | People parts | P2 | ⬜ | | |
| LR-MASK-LANDSCAPE | Landscape classes | P2 | ⬜ | | shape exists, evaluates empty |
| LR-MASK-BRUSH | Brush mask | P0 | 🟡 | `cmd:tool.brush`, `cmd:mask.brushStroke` | size/feather/flow/density/erase; Auto Mask flag not applied; no A/B brushes, no pressure |
| LR-MASK-LINEAR | Linear gradient | P0 | ✅ | `cmd:tool.linear`, `cmd:mask.update` | |
| LR-MASK-RADIAL | Radial gradient | P0 | ✅ | `cmd:tool.radial`, `cmd:mask.update` | |
| LR-MASK-COLORRANGE | Colour range | P1 | 🟡 | `cmd:mask.add` (`colorRange`) | renders; sampling UX unverified |
| LR-MASK-LUMRANGE | Luminance range | P1 | 🟡 | `cmd:mask.add` (`luminanceRange`) | no luminance-map display |
| LR-MASK-DEPTHRANGE | Depth range | P2 | ⬜ | | shape exists, needs depth data |
| LR-MASK-COMBINE | Add / subtract / intersect | P0 | ✅ | `cmd:mask.addComponent` | |
| LR-MASK-INVERT | Invert | P0 | ✅ | `cmd:mask.invert` | |
| LR-MASK-AMOUNT | Mask amount | P1 | ✅ | `cmd:mask.adjust` (`amount`), `crates/ui-egui/src/panels/masking.rs` | |
| LR-MASK-FEATHER-EDGE | Refine mask edges | P2 | ⬜ | | |
| LR-MASK-SLIDERS | Local adjustment sliders | P0 | 🟡 | `cmd:mask.adjust`, `crates/pipeline/src/finish.rs` | noise / moiré / defringe stored but not rendered; no local curve or effect presets |
| LR-MASK-OVERLAY | Mask overlay | P0 | 🟡 | `cmd:view.maskOverlay`, `crates/ui-egui/src/panels/detail.rs` | brush dabs + shape outlines only; no rendered alpha, colour or mode options |
| LR-MASK-PINS | Pins | P1 | 🟡 | `crates/ui-egui/src/panels/detail.rs` | handles drawn; no show/hide-pins option |
| LR-MASK-UPDATE | Recompute AI masks | P2 | ⬜ | | |
| LR-MASK-SYNC | Copy masks to other photos | P1 | ✅ | `cmd:develop.copy` (`groups`) | |
| LR-MASK-ADAPTIVE | Adaptive presets | P2 | ⬜ | | |

## L. Presets (PRE)

| Id | Feature | Tier | Status | Evidence | Missing / notes |
|---|---|---|---|---|---|
| LR-PRE-PANEL | Presets panel | P0 | 🟡 | `cmd:panel.presets`, `cmd:preset.apply`, `crates/ui-egui/src/panels/presets.rs` | grouped list with amount; no hover preview |
| LR-PRE-CREATE | Create preset | P0 | 🟡 | `cmd:dialog.createPreset`, `cmd:preset.create` (`groups`) | dialog has no per-group checkboxes (fixed subset) |
| LR-PRE-MANAGE | Manage presets | P1 | 🟡 | `cmd:preset.delete`, `cmd:preset.favorite`, `cmd:preset.import`, `cmd:preset.export` | no rename, update-with-current, move group, hide groups |
| LR-PRE-AMOUNT | Preset amount | P1 | ✅ | `cmd:preset.apply` (`amount` 0–200) | |
| LR-PRE-ADAPTIVE | Adaptive presets | P2 | ⬜ | | |
| LR-PRE-PREMIUM | Built-in presets (own) | P2 | 🟡 | `crates/engine/src/presets.rs` | 18 own-authored presets |
| LR-PRE-RECOMMENDED | Community recommendations | OOS | 🚫 | | |
| LR-PRE-ONIMPORT | Apply during import | P2 | ⬜ | | |

## M. Versions & history (VER)

| Id | Feature | Tier | Status | Evidence | Missing / notes |
|---|---|---|---|---|---|
| LR-VER-CREATE | Create version | P1 | ✅ | `cmd:version.create` | |
| LR-VER-PANEL | Versions panel | P1 | 🟡 | `cmd:panel.versions`, `cmd:version.restore`, `cmd:version.delete` | no rename, update, hover preview, named/auto tabs |
| LR-VER-AUTO | Automatic versions | P2 | ⬜ | | `auto` flag reserved in the model |
| LR-VER-HISTORY | Edit history | P1 | ✅ | `cmd:panel.activity`, `cmd:history.list`, `cmd:history.restore` | |
| LR-VER-UNDO | Undo / redo | P0 | ✅ | `cmd:edit.undo`, `cmd:edit.redo` | |

## N. Copy / paste / sync (SYNC)

| Id | Feature | Tier | Status | Evidence | Missing / notes |
|---|---|---|---|---|---|
| LR-SYNC-COPY | Copy edit settings | P0 | ✅ | `cmd:develop.copy` | |
| LR-SYNC-CHOOSE | Choose settings to copy | P0 | ✅ | `cmd:dialog.copySettings` | groups are coarser than per-slider |
| LR-SYNC-PASTE | Paste to selection | P0 | ✅ | `cmd:develop.paste` | no separate "paste selected" (choose at copy time instead) |
| LR-SYNC-SYNCBTN | Sync active → selected | P1 | ✅ | `cmd:develop.sync` | |
| LR-SYNC-PREVIOUS | Paste from previous / auto sync [Classic] | P2 | ⬜ | | |

## O. Merge (MERGE)

| Id | Feature | Tier | Status | Evidence | Missing / notes |
|---|---|---|---|---|---|
| LR-MERGE-HDR | HDR merge | P2 | ✅ | `cmd:merge.hdr`, `cmd:dialog.mergeHdr` | auto align, deghost None–High + overlay, auto settings, float DNG, Create Stack; JPEG brackets treated as linear |
| LR-MERGE-PANO | Panorama | P2 | ✅ | `cmd:merge.panorama`, `cmd:dialog.mergePanorama` | spherical/cylindrical/perspective + auto, boundary warp, auto crop, fill edges (diffusion), DNG; no lens model / 360° wrap |
| LR-MERGE-HDRPANO | HDR panorama | P2 | ✅ | `cmd:merge.hdrPanorama`, `cmd:dialog.mergeHdrPanorama` | brackets grouped by EXIF |
| LR-MERGE-HEADLESS | Merge with last settings | P2 | ⬜ | | |

## P. Enhance (ENH)

| Id | Feature | Tier | Status | Evidence | Missing / notes |
|---|---|---|---|---|---|
| LR-ENH-DIALOG | Enhance dialog | P2 | ⬜ | | |
| LR-ENH-INPLACE | In-place enhance | P2 | ⬜ | | |

## Q. HDR (HDR)

| Id | Feature | Tier | Status | Evidence | Missing / notes |
|---|---|---|---|---|---|
| LR-HDR-EDIT | HDR editing | P2 | ⬜ | | |
| LR-HDR-SDRPREVIEW | SDR preview of HDR | P2 | ⬜ | | |
| LR-HDR-VISUALIZE | Visualize HDR range | P2 | ⬜ | | |
| LR-HDR-LIMIT | HDR headroom limit | P2 | ⬜ | | |
| LR-HDR-EXPORT | HDR export | P2 | ⬜ | | |

## R. Video (VID)

| Id | Feature | Tier | Status | Evidence | Missing / notes |
|---|---|---|---|---|---|
| LR-VID-PLAY | Video playback | P1 | ⬜ | `crates/catalog/src/model.rs` | videos can be catalogued (kind, duration); no player |
| LR-VID-TRIM | Trim | P1 | ⬜ | | |
| LR-VID-EDIT | Edits on video | P2 | ⬜ | | |
| LR-VID-COVER | Cover frame | P2 | ⬜ | | |
| LR-VID-EXPORT | Video export | P2 | ⬜ | | |
| LR-VID-PHOTO2VIDEO | Photo to video (generative) | OOS | 🚫 | | |

## S. Export (EXP)

| Id | Feature | Tier | Status | Evidence | Missing / notes |
|---|---|---|---|---|---|
| LR-EXP-DIALOG | Export dialog | P0 | 🟡 | `cmd:dialog.export`, `cmd:app.export`, `crates/ui-egui/src/panels/dialogs.rs`, `crates/engine/src/export.rs` | batch export works; no built-in or saved export presets |
| LR-EXP-TYPE | File types | P0 | 🟡 | `crates/engine/src/export.rs` (`ExportFormat`) | JPEG, PNG, TIFF, WebP, AVIF; no DNG, JXL or original (+XMP) |
| LR-EXP-DIM | Output size | P0 | 🟡 | `crates/ui-egui/src/panels/dialogs.rs` (long edge / full size) | no short edge / width / height / megapixels, ppi, don't-enlarge |
| LR-EXP-QUALITY | JPEG quality | P0 | ✅ | `cmd:app.export` (`quality`, `limitKb`) | |
| LR-EXP-BITDEPTH | Bit depth | P1 | ⬜ | | 8-bit only |
| LR-EXP-COMPRESSION | TIFF compression | P1 | ⬜ | | always Deflate |
| LR-EXP-COLORSPACE | Output colour space | P0 | ⬜ | `crates/engine/src/export.rs` | sRGB only |
| LR-EXP-HDR | HDR output | P2 | ⬜ | | |
| LR-EXP-SHARPEN | Output sharpening | P1 | ✅ | `cmd:app.export` (`sharpen`, `sharpenAmount`) | |
| LR-EXP-METADATA | Metadata policy | P1 | ✅ | `cmd:app.export` (`metadata`, `removeLocation`) | |
| LR-EXP-WATERMARK | Watermark | P1 | 🟡 | `crates/engine/src/export.rs` (`Watermark`) | text only; no graphic watermark |
| LR-EXP-NAMING | File naming | P1 | 🟡 | `cmd:app.export` (`naming`: `{name}`, `{seq}`) | no date tokens, custom start number |
| LR-EXP-LOCATION | Destination folder | P0 | 🟡 | `crates/ui-egui/src/panels/dialogs.rs` (folder field) | no folder picker, subfolder or name-conflict policy |
| LR-EXP-PREVIOUS | Export with previous settings | P0 | ✅ | `cmd:app.exportPrevious`, `cmd:dialog.export` | last options persist in prefs.json; dialog prefilled; no named export presets yet |
| LR-EXP-DNGOPT | DNG options | P2 | ⬜ | | |
| LR-EXP-ORIGINAL | Original + XMP | P1 | ⬜ | | |
| LR-EXP-PHOTOS | Export to the system photo library | P2 | ⬜ | | |
| LR-EXP-PSD | Round trip to an external editor | P2 | ⬜ | | |

## T. Share (SHARE)

| Id | Feature | Tier | Status | Evidence | Missing / notes |
|---|---|---|---|---|---|
| LR-SHARE-LINK | Web share link | OOS | 🚫 | | |
| LR-SHARE-INVITE | Invite collaborators | OOS | 🚫 | | |
| LR-SHARE-WEBGALLERY | Web galleries | OOS | 🚫 | | |
| LR-SHARE-COMMUNITY | Community edits | OOS | 🚫 | | |

## U. Map & location (MAP)

| Id | Feature | Tier | Status | Evidence | Missing / notes |
|---|---|---|---|---|---|
| LR-MAP-INFO | Location in the info panel | P1 | 🟡 | `cmd:photo.setMeta` (`location`), `crates/ui-egui/src/panels/right.rs` | shown as text; not editable in the panel; no map |
| LR-MAP-MODULE | Map module [Classic] | P2 | ⬜ | | |

## V. Preferences (PREF)

| Id | Feature | Tier | Status | Evidence | Missing / notes |
|---|---|---|---|---|---|
| LR-PREF-GENERAL | General settings | P0 | ⬜ | `cmd:library.xmpPreferences` | no settings dialog; only XMP preferences (by command) |
| LR-PREF-LOCALSTORAGE | Storage & cache | P1 | 🟡 | `cmd:library.clearPreviews`, `cmd:library.compact` | bounded thumbnail cache; no UI for size/location |
| LR-PREF-ACCOUNT | Account | OOS | 🚫 | | |
| LR-PREF-INTERFACE | Interface options | P1 | ⬜ | | |
| LR-PREF-PERFORMANCE | GPU / performance | P1 | 🟡 | `cmd:app.gpu` | by command only |
| LR-PREF-PEOPLE | Face recognition | P2 | ⬜ | | |
| LR-PREF-WATERMARK | Watermark settings | P1 | 🟡 | `crates/ui-egui/src/panels/dialogs.rs` | per export in the dialog; not saved as a preference |
| LR-PREF-SHORTCUTS | Shortcut customisation | — | 🚫 | | not customisable in the reference app either; a keymap editor would be an extra |
| LR-PREF-TECHPREVIEW | Early-access toggles | P2 | ⬜ | | |
| LR-PREF-NOTIFICATIONS | Notifications | OOS | 🚫 | | |
| LR-PREF-DEVICE | Device settings | P2 | ⬜ | | |

## W. Cloud & AI infrastructure (CLOUD / AI)

| Id | Feature | Tier | Status | Evidence | Missing / notes |
|---|---|---|---|---|---|
| LR-CLOUD-SYNC | Cloud sync | OOS | 🚫 | | |
| LR-CLOUD-SMARTPREVIEW | Editable proxies | P2 | 🟡 | `crates/engine/src/media.rs` | preview-size proxies drive the loupe; editing needs the original |
| LR-AI-UPDATE-INDICATOR | AI-settings update indicator | P2 | ⬜ | | |
| LR-AI-CREDITS | Generative credits | OOS | 🚫 | | |

## X. Cross-cutting behaviours (BEHAV)

| Id | Feature | Tier | Status | Evidence | Missing / notes |
|---|---|---|---|---|---|
| LR-BEHAV-AUTOSAVE | Instant autosave | P0 | ✅ | `crates/catalog/src/journal.rs`, `crates/engine/src/library.rs` | |
| LR-BEHAV-UNDO | Global undo | P0 | ✅ | `cmd:edit.undo`, `crates/engine/src/tests.rs` (`rating_flag_undo_redo`) | covers ratings, albums, deletes, edits |
| LR-BEHAV-MULTISELECT | Multi-selection | P0 | ✅ | `cmd:library.select` (`replace`/`add`/`toggle`/`range`), `cmd:library.selectAll` | |
| LR-BEHAV-BATCH | Batch apply to selection | P0 | ✅ | `cmd:photo.rate`, `cmd:develop.paste`, `cmd:preset.apply`, `cmd:app.export` | |
| LR-BEHAV-PREVIEW-HOVER | Hover previews | P1 | ⬜ | | presets, profiles, versions |
| LR-BEHAV-PROGRESSIVE | Progressive rendering | P0 | ✅ | `crates/engine/src/media.rs`, `crates/preview/src/lib.rs` | |
| LR-BEHAV-BG-TASKS | Background tasks | P0 | 🟡 | `crates/preview/src/lib.rs` (`JobPool`) | renders off the UI thread; no progress popover for import/export |
| LR-BEHAV-OFFLINE | Offline editing | P1 | ✅ | | local-first: everything works offline |
| LR-BEHAV-GPU | GPU acceleration | P0 | ✅ | `crates/gpu/src/render.rs`, `cmd:app.gpu`, `docs/gpu-pipeline.md` | CPU fallback |
| LR-BEHAV-DRAGDROP | Drag and drop | P1 | 🟡 | `crates/ui-egui/src/lib.rs` | files → app only; no photos → albums |
| LR-BEHAV-TOAST | Toast notifications | P1 | ✅ | `crates/ui-egui/src/panels/mod.rs` | |
| LR-BEHAV-EMPTY-STATES | Empty states | P1 | ✅ | `crates/ui-egui/src/panels/mod.rs` (`empty_message`) | |
| LR-BEHAV-TOOLTIPS | Tooltips with shortcuts | P0 | ✅ | `crates/ui-egui/src/panels/bottombar.rs` | |
| LR-BEHAV-ACCESS | Accessibility | P2 | ⬜ | | unverified (screen-reader labels not audited) |
| LR-BEHAV-LOCALIZE | Localisation | P2 | ⬜ | | |
| LR-BEHAV-LEARN | Tutorials | OOS | 🚫 | | |
| LR-BEHAV-WHATSNEW | What's new | P2 | ⬜ | | |
| LR-BEHAV-AI-EA | Early-access badges | P2 | ⬜ | | |

## Y. Menus

From `04-menu-tree.md`. ✅ = a command exists and is reachable from the UI (button, panel or shortcut). There is no
visible menu bar yet: the menu model is only exposed through the control channel (`ui.menu.list`).

| Id | Feature | Tier | Status | Evidence | Missing / notes |
|---|---|---|---|---|---|
| MENU-BAR | Menu bar rendering | P1 | ✅ | `crates/ui-egui/src/menubar.rs`, `apps/lightcraft/src/native_menu.rs` | native macOS menu bar (muda) with live labels/enabled/checked; in-window menus on web/Windows/Linux; ⌫ and X stay egui-handled (contextual), so they show no key in the native menu |
| MENU-APP-ABOUT | About | P2 | ✅ | `cmd:app.about` | |
| MENU-APP-SETTINGS | Settings… | P0 | ⬜ | | see LR-PREF-GENERAL |
| MENU-APP-UPDATES | Check for updates | P2 | ⬜ | | |
| MENU-APP-SYNC | Sync status / pause | OOS | 🚫 | | |
| MENU-APP-SIGNOUT | Sign out | OOS | 🚫 | | |
| MENU-APP-HIDE | Hide / hide others / show all | P1 | 🟡 | | platform window defaults (unverified) |
| MENU-APP-QUIT | Quit | P0 | 🟡 | | platform window defaults (unverified) |
| MENU-FILE-ADDPHOTOS | Add Photos… | P0 | ✅ | `cmd:file.addPhotos` | |
| MENU-FILE-ADDFOLDER | Add Folder… | P0 | 🟡 | `cmd:library.import` (folders, recursive) | no folder picker entry |
| MENU-FILE-MIGRATE | Migrate photos | OOS | 🚫 | | |
| MENU-FILE-NEWALBUM | New Album… | P0 | ✅ | `cmd:dialog.newAlbum` | |
| MENU-FILE-NEWFOLDER | New Folder… | P0 | ✅ | `cmd:dialog.newFolder` | ⇧⌘N |
| MENU-FILE-NEWSMART | New Smart Album… | P1 | ✅ | `cmd:dialog.newSmartAlbum` | saves the current view (source + filter) |
| MENU-FILE-IMPORTPROFILES | Import Profiles & Presets… | P1 | 🟡 | `cmd:file.importPresets` | presets only |
| MENU-FILE-EXPORT | Export… | P0 | ✅ | `cmd:dialog.export` | |
| MENU-FILE-EXPORTPREV | Export with Previous | P0 | ✅ | `cmd:app.exportPrevious` | ⌥⇧⌘E |
| MENU-FILE-EXPORTPRESETS | Export preset submenu | P0 | ⬜ | | |
| MENU-FILE-SHARE | Share / get link / invite | OOS | 🚫 | | |
| MENU-FILE-PHOTOSHOP | Edit in external editor | P2 | ⬜ | | |
| MENU-FILE-SHOWFINDER | Show in Finder | P0 | ✅ | `cmd:app.showInFinder` | ⌘R; Explorer on Windows, the folder on Linux; disabled for demo scenes and on the web |
| MENU-FILE-OFFLINE | Store album locally | P2 | 🚫 | | not applicable: local-first |
| MENU-FILE-CLOSE | Close Window | P1 | 🟡 | | platform window defaults (unverified) |
| MENU-EDIT-UNDO | Undo | P0 | ✅ | `cmd:edit.undo` | label does not name the step |
| MENU-EDIT-REDO | Redo | P0 | ✅ | `cmd:edit.redo` | |
| MENU-EDIT-COPYPASTE | Copy / paste (edit settings) | P0 | ✅ | `cmd:develop.copy`, `cmd:develop.paste` | |
| MENU-EDIT-CHOOSECOPY | Choose Edit Settings to Copy… | P0 | ✅ | `cmd:dialog.copySettings` | |
| MENU-EDIT-PASTESELECTED | Paste Selected Settings | P0 | ⬜ | | |
| MENU-EDIT-SELECTALL | Select All | P0 | ✅ | `cmd:library.selectAll` | |
| MENU-EDIT-SELECTNONE | Select None | P0 | ✅ | `cmd:library.selectNone` | |
| MENU-EDIT-SELECTBY | Select by flag / rating | P1 | ⬜ | | |
| MENU-EDIT-FIND | Find… | P0 | 🟡 | `crates/ui-egui/src/panels/topbar.rs` | search field; no command to focus it |
| MENU-VIEW-PHOTOGRID | Photo Grid | P0 | ✅ | `cmd:view.photoGrid` | |
| MENU-VIEW-SQUAREGRID | Square Grid | P0 | ✅ | `cmd:view.squareGrid` | |
| MENU-VIEW-DETAIL | Detail | P0 | ✅ | `cmd:view.detail` | |
| MENU-VIEW-EDIT | Edit | P0 | ✅ | `cmd:panel.edit` | |
| MENU-VIEW-FULLSCREENPREVIEW | Full Screen Preview | P1 | ⬜ | | |
| MENU-VIEW-ENTERFULLSCREEN | Enter Full Screen | P1 | ⬜ | | |
| MENU-VIEW-PHOTOSPANEL | Show/Hide photos panel | P0 | ✅ | `cmd:view.leftPanel` | |
| MENU-VIEW-FILMSTRIP | Show/Hide filmstrip | P0 | ✅ | `cmd:view.filmstrip` | |
| MENU-VIEW-INFO | Show/Hide info | P0 | ✅ | `cmd:panel.info` | |
| MENU-VIEW-KEYWORDS | Show/Hide keywords | P0 | ✅ | `cmd:panel.keywords` | |
| MENU-VIEW-ACTIVITY | Show/Hide activity (comments) | OOS | 🚫 | | our History panel is `panel.activity` |
| MENU-VIEW-VERSIONS | Show/Hide versions | P1 | ✅ | `cmd:panel.versions` | |
| MENU-VIEW-HISTOGRAM | Show/Hide histogram | P0 | ✅ | `cmd:view.histogram` | |
| MENU-VIEW-INFOOVERLAY | Show info overlay | P1 | ⬜ | | |
| MENU-VIEW-SHOWORIGINAL | Show Original | P0 | ✅ | `cmd:view.showOriginal` | |
| MENU-VIEW-BEFOREAFTER | Before/After submenu | P0 | ✅ | `cmd:view.beforeAfter`, `cmd:view.beforeAfterSplit`, `cmd:view.beforeAfterTopBottom`, `cmd:view.beforeAfterSplitTopBottom` | |
| MENU-VIEW-ZOOM | Zoom in / out / toggle / fit / 1:1 | P0 | ✅ | `cmd:view.zoomIn`, `cmd:view.zoomOut`, `cmd:view.zoomToggle`, `cmd:view.zoomFit`, `cmd:view.zoom100` | |
| MENU-VIEW-CLIPPING | Show Clipping | P0 | ✅ | `cmd:view.clipping` | |
| MENU-VIEW-MASKOVERLAY | Mask overlay / cycle colour | P0 | 🟡 | `cmd:view.maskOverlay` | no colour cycle |
| MENU-VIEW-INCLUDESUBFOLDERS | Include subfolders | P1 | 🟡 | `cmd:library.import` | folder import is always recursive |
| MENU-VIEW-SORT | Sort submenu | P0 | ✅ | `cmd:library.sort` | no colour-label key |
| MENU-VIEW-STACKS | Expand/collapse stacks | P1 | ✅ | `cmd:stack.expandAll`, `cmd:stack.collapseAll` | |
| MENU-VIEW-PHOTOCOUNT | Show photo counts | P2 | 🟡 | `crates/ui-egui/src/panels/left.rs` | counts always shown; no toggle |
| MENU-VIEW-HDR | HDR display options | P2 | ⬜ | | |
| MENU-PHOTO-ADDTOALBUM | Add to album | P0 | ✅ | `cmd:album.addPhotos` | |
| MENU-PHOTO-REMOVEFROMALBUM | Remove from album | P0 | ✅ | `cmd:album.removePhotos` | |
| MENU-PHOTO-RATE | Rate submenu | P0 | ✅ | `cmd:photo.rate` | |
| MENU-PHOTO-FLAG | Flag submenu | P0 | ✅ | `cmd:photo.flag` | |
| MENU-PHOTO-LABEL | Colour label submenu | P1 | 🟡 | `cmd:photo.label` | keys only; no menu, no label editing |
| MENU-PHOTO-ROTATE | Rotate left / right | P0 | ✅ | `cmd:photo.rotateLeft`, `cmd:photo.rotateRight` | |
| MENU-PHOTO-FLIP | Flip horizontal / vertical | P0 | ✅ | `cmd:photo.flipHorizontal`, `cmd:photo.flipVertical` | |
| MENU-PHOTO-CREATEVERSION | Create Version… | P1 | ✅ | `cmd:version.create` | no name prompt |
| MENU-PHOTO-STACK | Stack submenu | P1 | ✅ | `cmd:stack.group`, `cmd:stack.ungroup`, `cmd:dialog.autoStack` | |
| MENU-PHOTO-MERGE | Photo merge submenu | P2 | ⬜ | | |
| MENU-PHOTO-ENHANCE | Enhance… | P2 | ⬜ | | |
| MENU-PHOTO-AUTO | Auto settings | P0 | ✅ | `cmd:develop.auto` | |
| MENU-PHOTO-BW | Convert to B&W | P0 | ✅ | `cmd:develop.treatment` | |
| MENU-PHOTO-RESET | Reset edits / crop | P0 | ✅ | `cmd:develop.reset`, `cmd:crop.reset` | |
| MENU-PHOTO-UPDATEAI | Update AI settings | P2 | ⬜ | | |
| MENU-PHOTO-RENAME | Rename N photos… | P1 | ⬜ | | |
| MENU-PHOTO-CAPTURETIME | Edit capture time… | P1 | ⬜ | | |
| MENU-PHOTO-COVER | Set as album cover | P2 | ✅ | `cmd:album.setCover` | |
| MENU-PHOTO-DELETE | Delete N photos… | P0 | ✅ | `cmd:photo.delete` | no confirmation; static label |
| MENU-PHOTO-MOVETOCLOUD | Move/copy to cloud | OOS | 🚫 | | |
| MENU-WINDOW-MINIMIZE | Minimize / zoom | P1 | 🟡 | | platform window defaults (unverified) |
| MENU-WINDOW-PANELS | Panel switches | P0 | ✅ | `cmd:panel.edit`, `cmd:panel.crop`, `cmd:panel.remove`, `cmd:panel.masking`, `cmd:panel.presets`, `cmd:panel.versions` | |
| MENU-WINDOW-BRINGFRONT | Bring all to front | P2 | 🟡 | | platform window defaults (unverified) |
| MENU-HELP-HELP | Help | P2 | ⬜ | | |
| MENU-HELP-TUTORIALS | Tutorials | OOS | 🚫 | | |
| MENU-HELP-WHATSNEW | What's new | P2 | ⬜ | | |
| MENU-HELP-SHORTCUTS | Keyboard shortcuts | P1 | ✅ | `cmd:app.shortcuts` | |
| MENU-HELP-FEEDBACK | Send feedback | P2 | ⬜ | | |
| MENU-HELP-SYSINFO | System info | P2 | ⬜ | `cmd:library.info` | library info only |
| MENU-CTX-GRID | Photo context menu | P0 | 🟡 | `crates/ui-egui/src/panels/grid.rs` (`context_menu`) | rate, flag, add to album, copy/paste, reset, rotate, delete; no label, remove from album, export, reveal, version, cover |
| MENU-CTX-DETAIL | Loupe context menu | P1 | 🟡 | `crates/ui-egui/src/panels/detail.rs` | same as grid; no zoom submenu |
| MENU-CTX-ALBUM | Album / folder row menu | P0 | 🟡 | `crates/ui-egui/src/panels/left.rs` | add selected, rename, delete; no move-to, export album |
| MENU-CTX-MASK | Mask / component menu | P0 | 🟡 | `crates/ui-egui/src/panels/masking.rs` | add/subtract component; rename/duplicate by command only |
| MENU-CTX-PRESET | Preset menu | P1 | 🟡 | `crates/ui-egui/src/panels/presets.rs` | favourite, delete, export group; no rename/update/move |
| MENU-CTX-PROFILE | Profile favourites | P2 | ⬜ | | |
| MENU-CTX-VERSION | Version menu | P1 | ⬜ | | delete button only |
| MENU-CTX-KEYWORD | Keyword chip menu | P1 | 🟡 | `crates/ui-egui/src/panels/right.rs` | click removes; no rename/delete keyword |

## Z. Keyboard shortcuts (desktop)

From `06-shortcuts.md` part 1. Evidence is our binding; conflicts are explained in
[Shortcuts: conflicts and missing bindings](#shortcuts-conflicts-and-missing-bindings).

| Id | Feature | Tier | Status | Evidence | Missing / notes |
|---|---|---|---|---|---|
| KEY-CROP | Crop & rotate — C | P0 | ✅ | `cmd:panel.crop` | |
| KEY-DETAIL | Detail — D | P0 | ✅ | `cmd:view.detail` | |
| KEY-EDIT | Edit — E | P0 | ✅ | `cmd:panel.edit` | |
| KEY-FULLSCREEN | Full-screen preview — F | P1 | ⬜ | | no command |
| KEY-GRID | Grid — G | P0 | ✅ | `cmd:view.photoGrid` | |
| KEY-INFO | Info — I | P0 | ✅ | `cmd:panel.info` | |
| KEY-KEYWORDS | Keywords — K | P0 | ✅ | `cmd:panel.keywords` | |
| KEY-CLIPBOARD | Copy / paste edit settings — ⌘C / ⌘V | P0 | ✅ | `cmd:develop.copy`, `cmd:develop.paste` | ⌘X has nothing to cut outside text fields |
| KEY-UNDOREDO | Undo / redo — ⌘Z / ⇧⌘Z | P0 | ✅ | `cmd:edit.undo`, `cmd:edit.redo` | |
| KEY-MINIMIZE | Minimize — ⌘M | P1 | 🟡 | | platform default (unverified) |
| KEY-AUTO | Auto — ⇧A | P0 | ✅ | `cmd:develop.auto` | |
| KEY-PHOTOSHOP | External editor — ⇧⌘E | P2 | ⬜ | | key used by our export dialog |
| KEY-ROTATE | Rotate — ⌘[ / ⌘] | P0 | ✅ | `cmd:photo.rotateLeft`, `cmd:photo.rotateRight` | |
| KEY-ZOOM | Zoom in / out — ⌘= / ⌘− | P0 | ✅ | `cmd:view.zoomIn`, `cmd:view.zoomOut` | |
| KEY-SELECTALL | Select all — ⌘A | P0 | ✅ | `cmd:library.selectAll` | |
| KEY-SELECTNONE | Select none — ⌘D | P0 | ✅ | `cmd:library.selectNone` | secondary binding (primary ⌘⇧A) |
| KEY-PASTESELECTED | Paste selected — ⇧⌘V | P0 | ⬜ | | no command |
| KEY-PREFS | Settings — ⌘, | P0 | ⬜ | | no command |
| KEY-SEARCH | Search — ⌘F | P0 | ⬜ | | no focus-search command |
| KEY-VISUALIZESPOTS | Visualize spots — A | P1 | ⬜ | | |
| KEY-CYCLEOVERLAY | Cycle overlay — O | P0 | 🟡 | `cmd:view.maskOverlay`, `cmd:view.cropOverlay` | O toggles the mask overlay; crop overlays cycle on ⇧O |
| KEY-PHOTOSPANEL | Photos panel — P | P0 | 🟡 | `cmd:view.leftPanel` | bound to ⌘⇧L; P = pick |
| KEY-LINEAR | Linear gradient — L | P0 | ✅ | `cmd:tool.linear` | |
| KEY-RADIAL | Radial gradient — R | P0 | ✅ | `cmd:tool.radial` | |
| KEY-CLIPPING | Clipping — J | P0 | ✅ | `cmd:view.clipping` | |
| KEY-WB | White-balance selector — W | P0 | ✅ | `cmd:tool.wbPicker` | |
| KEY-FILMSTRIP | Filmstrip — / | P0 | ✅ | `cmd:view.filmstrip` | |
| KEY-SHOWORIGINAL | Show original — \ | P0 | ✅ | `cmd:view.showOriginal` | |
| KEY-TOGGLEZOOM | Toggle zoom — Space | P0 | ✅ | `cmd:view.zoomToggle` | Space is a secondary binding (primary Z) |
| KEY-MASKCOLOR | Cycle mask colour — ⇧O | P1 | ⬜ | | ⇧O cycles crop overlays |
| KEY-EXPORTPREV | Export with previous — ⌘E | P0 | 🟡 | `cmd:app.exportPrevious` | bound to ⌥⇧⌘E (Classic); ⌘E not bound |
| KEY-EXPORTDIALOG | Export dialog — ⇧E | P0 | ✅ | `cmd:dialog.export` | secondary binding (primary ⌘⇧E) |
| KEY-ENTERFULLSCREEN | Window full screen — ⇧⌘F | P1 | ⬜ | | |
| KEY-STACK | Group / ungroup stack — ⌘G / ⇧⌘G | P1 | ✅ | `cmd:stack.group`, `cmd:stack.ungroup` | also S expand/collapse, ⇧S top of stack |
| KEY-GUIDEDUPRIGHT | Guided Upright — ⇧G | P1 | 🟡 | `cmd:geometry.upright` | button in the crop panel; ⇧G = Square Grid |
| KEY-HIDE | Hide / hide others — ⌘H / ⌥⌘H | P1 | 🟡 | | platform default (unverified) |
| KEY-QUIT | Quit — ⌘Q | P0 | 🟡 | | platform default (unverified) |
| KEY-CREATEVERSION | Create version — ⇧M | P1 | ✅ | `cmd:version.create` | secondary binding (primary ⌘⇧S) |
| KEY-CLOSEWINDOW | Close window — ⌘W | P1 | 🟡 | | platform default (unverified) |
| KEY-DELETE | Delete photo — ⌫ | P0 | ✅ | `cmd:photo.delete` | |
| KEY-ADDPHOTOS | Add photos — ⇧⌘I | P0 | ✅ | `cmd:file.addPhotos` | |
| KEY-VERSIONS | Versions panel — ⇧V | P1 | ✅ | `cmd:panel.versions` | |
| KEY-SECTIONS | Expand/collapse edit sections — ⌘1…⌘6 | P1 | 🟡 | `cmd:section.light`, `cmd:section.color`, `cmd:section.effects`, `cmd:section.detail`, `cmd:section.optics` | bound to ⌘⌥1–5 |
| KEY-PRESETS | Presets panel — ⇧P | P0 | ✅ | `cmd:panel.presets` | |
| KEY-HISTOGRAM | Histogram — ⌘0 | P0 | 🟡 | `cmd:view.histogram` | bound to ⌘⇧H; ⌘0 = zoom to fit |
| KEY-BRUSHSIZE | Brush size — `[` / `]` | P0 | ⬜ | | brush size is UI state, no command |
| KEY-BRUSHFEATHER | Brush feather — ⇧`[` / ⇧`]` | P0 | ⬜ | | |
| KEY-BRUSH | Brush — B | P0 | ✅ | `cmd:tool.brush` | |
| KEY-HEAL | Remove / heal — H | P0 | ✅ | `cmd:panel.remove` | |
| KEY-MERGE | HDR / panorama merges — ⌃H ⇧⌃H ⌃M ⇧⌃M | P2 | ⬜ | | |
| KEY-PICK | Pick — Z | P0 | 🟡 | `cmd:photo.pick` | bound to P; Z toggles zoom |
| KEY-UNFLAG | Unflag — U | P0 | ✅ | `cmd:photo.unflag` | |
| KEY-REJECT | Reject — X | P0 | ✅ | `cmd:photo.reject` | swaps crop aspect while cropping |
| KEY-RATING | Ratings — 0…5 | P0 | ✅ | `crates/ui-egui/src/shortcuts.rs` → `cmd:photo.rate` | |
| KEY-LABELS | Labels — 6…9 | P1 | ✅ | `crates/ui-egui/src/shortcuts.rs` → `cmd:photo.label` | |
| KEY-MASKING | Masking — M | P0 | ✅ | `cmd:panel.masking` | |
| KEY-ERASE | Erase while held — ⌥ | P0 | 🟡 | `crates/ui-egui/src/panels/masking.rs` | Add/Erase toggle buttons; hold-to-erase unverified |
| KEY-RATEADVANCE | Rate and advance — ⇧0…5 | P1 | ✅ | `crates/ui-egui/src/shortcuts.rs` → `cmd:photo.rate` (`advance`) | |
| KEY-FLAGADVANCE | Flag and advance — ⇧Z / ⇧X / ⇧U | P1 | 🟡 | `cmd:photo.flag` (`advance`) | ⇧X and ⇧U bound; pick-and-advance has no key (⇧P = presets) |
| KEY-NEXTPREV | Next / previous — → / ← | P0 | ✅ | `cmd:library.next`, `cmd:library.previous` | |
| KEY-BA-CYCLE | Before/after — Y | P0 | ✅ | `cmd:view.beforeAfter` | toggles side by side (no cycling) |
| KEY-BA-TOPBOTTOM | Before/after top/bottom — ⌥Y | P1 | ✅ | `cmd:view.beforeAfterTopBottom`, `cmd:view.beforeAfterSplitTopBottom` | ⌥Y, ⇧⌥Y |
| KEY-BA-SPLIT | Split before/after — ⇧Y | P0 | ✅ | `cmd:view.beforeAfterSplit` | |
| KEY-CHOOSECOPY | Choose settings to copy — ⇧⌘C | P0 | ✅ | `cmd:dialog.copySettings` | |
| KEY-RESETALL | Reset all — ⇧⌘R | P0 | ✅ | `cmd:develop.reset` | |
| KEY-GENAI | Generative toggles / variations — ⌥⇧G, ⌥←/→ | OOS | 🚫 | | |
| KEY-DETECT | Detect objects — ⌥⇧O | P2 | ⬜ | | |
| KEY-CROP-CONSTRAIN | Lock crop aspect — A | P1 | ⬜ | | |
| KEY-CROP-SWAP | Swap crop orientation — X | P0 | ✅ | `cmd:crop.rotateAspect` | |
| KEY-CROP-OVERLAYORIENT | Crop overlay orientation — ⇧O | P1 | ⬜ | | |
| KEY-CROP-RESET | Reset crop — ⌥⌘R | P0 | ✅ | `cmd:crop.reset` | |
| KEY-STRAIGHTEN | Straighten while held — ⌘ drag | P1 | ⬜ | | |
| KEY-SLIDER-RESET | Reset slider — double-click | P0 | ✅ | `crates/ui-egui/src/widgets.rs` | |
| KEY-SLIDER-NUDGE | Nudge slider — ↑/↓ | P1 | 🟡 | `cmd:develop.adjust` | command exists; keyboard nudging unverified |
| KEY-SHORTCUTS | Shortcut list — ⌘/ | P1 | ✅ | `cmd:app.shortcuts` | |
| KEY-HELP | Help — F1 | P2 | ⬜ | | |
| KEY-VIDEO-PLAY | Play/pause video — Space | P1 | ⬜ | | |
| KEY-ESC | Leave tool / view — Esc | P0 | ✅ | `cmd:view.back` | |
| KEY-COMMIT | Commit tool — Return | P1 | 🟡 | | edits apply live; no explicit commit step |
| KEY-DELETE-PIN | Delete selected pin — ⌫ | P0 | 🟡 | `cmd:mask.delete` | ⌫ deletes the active mask in the Masking panel and never the photo while retouching; no spot pin selection yet |
| KEY-HIDEPINS | Hide pins — H | P2 | ⬜ | | H = Remove panel |

## Lightroom Classic extras

From `08-lightroom-classic-extras.md` (Classic-only features) and part 2 of `06-shortcuts.md` (Classic keys, grouped).

| Id | Feature | Tier | Status | Evidence | Missing / notes |
|---|---|---|---|---|---|
| LRC-LIB-IMPORT | Full import dialog | P1 | 🟡 | `cmd:library.import` (`mode: copy`) | copy/add + duplicate skip; no file renaming, apply-during-import, destination organising, import presets |
| LRC-LIB-AUTOIMPORT | Watched-folder import | P2 | ⬜ | | |
| LRC-LIB-TETHER | Tethered capture | P2 | ⬜ | | |
| LRC-LIB-VIEWS | Grid / loupe / compare / survey / people | P1 | 🟡 | `cmd:view.photoGrid`, `cmd:view.detail` | no compare, survey, people; no grid cell styles beyond filenames |
| LRC-LIB-COMPARE | Compare view | P1 | ✅ | `cmd:view.compare` | ⇧C (C is Crop here); swap, make select; zoom always linked |
| LRC-LIB-SURVEY | Survey view | P2 | ✅ | `cmd:view.survey` | N |
| LRC-LIB-REFVIEW | Reference view | P2 | ⬜ | | |
| LRC-LIB-CATALOG-PANEL | Catalog sets | P1 | 🟡 | `cmd:library.source` | all, recently added, picks, recently deleted; no missing/problem sets |
| LRC-LIB-FOLDERS | Disk folder tree | P1 | ⬜ | | |
| LRC-LIB-COLLECTIONS | Collections & sets | P1 | 🟡 | `cmd:album.create` | albums + folders; no smart / quick / target collections |
| LRC-LIB-SMARTCOLL | Smart-collection rules | P1 | 🟡 | `cmd:album.createSmart`, `crates/catalog/src/query.rs` | the filter fields + date range; no rule editor, any/none groups or operators beyond ≥/=/≤ |
| LRC-LIB-PUBLISH | Publish services | P2 | ⬜ | | |
| LRC-LIB-FILTERBAR | Library filter bar | P1 | 🟡 | `cmd:view.filterBar`, `crates/ui-egui/src/panels/filterbar.rs` | rating/flag/label/kind/edited/camera/lens/keyword, clear, save as smart album; no filter presets, lock, or multi-select columns |
| LRC-LIB-STACKS | Stacks (full) | P1 | 🟡 | `crates/catalog/src/stacks.rs` | group/ungroup/toggle/top/remove/auto by time; no split stack or move up/down |
| LRC-LIB-VC | Virtual copies | P1 | ✅ | `cmd:photo.virtualCopy` | ⌘' |
| LRC-LIB-LABELS | Colour-label sets | P1 | ⬜ | | |
| LRC-LIB-KEYWORDS | Hierarchical keywords, sets, painter | P1 | ⬜ | | flat keywords only (LR-LIB-KEYWORD) |
| LRC-LIB-METADATA | Metadata panel & presets | P1 | 🟡 | `cmd:photo.setMeta`, `cmd:photo.saveMetadataToFile`, `cmd:photo.readMetadataFromFile` | no metadata presets, capture-time edit, copyright status |
| LRC-LIB-QUICKDEV | Quick develop | P2 | ⬜ | | |
| LRC-LIB-PEOPLE | People view | P2 | ⬜ | | |
| LRC-LIB-COMMENTS | Comments panel | P2 | ⬜ | | |
| LRC-LIB-VISUALSEARCH | Find similar photos | P2 | ⬜ | | |
| LRC-LIB-MISSING | Missing files & relink | P1 | ⬜ | | |
| LRC-LIB-CONVERT | Convert to DNG | P2 | ⬜ | `crates/raw/src/dngwrite.rs` | writer exists, not exposed |
| LRC-LIB-PREVIEWS | Build / discard previews | P1 | 🟡 | `cmd:library.clearPreviews`, `crates/preview/src/lib.rs` | disk thumbnail cache; no build-1:1 / smart previews |
| LRC-LIB-SLIDESHOW-IMPROMPTU | Impromptu slideshow | P2 | ⬜ | | |
| LRC-DEV-SNAPSHOTS | Named snapshots | P1 | ✅ | `cmd:version.create`, `cmd:version.restore` | = versions |
| LRC-DEV-HISTORY | Full history panel | P1 | 🟡 | `cmd:history.list`, `cmd:history.restore` | no clear, no snapshot-from-step |
| LRC-DEV-SOFTPROOF | Soft proofing | P2 | ⬜ | | |
| LRC-DEV-AUTOSYNC | Sync / auto sync / paste previous | P1 | 🟡 | `cmd:develop.sync` | no auto sync or paste-from-previous |
| LRC-DEV-MATCHEXP | Match total exposures | P2 | ⬜ | | |
| LRC-DEV-CALIB | Calibration panel | P1 | ⬜ | | |
| LRC-DEV-TAT | Targeted adjustment tools | P1 | ⬜ | | |
| LRC-DEV-DEFAULTS | Per-camera raw defaults | P1 | ⬜ | | |
| LRC-DEV-VIEWOPTIONS | Develop view options | P2 | ⬜ | | |
| LRC-DEV-VIDEO | Video frame capture | P2 | ⬜ | | |
| LRC-MAP-VIEW | Map view | P2 | ⬜ | | |
| LRC-MAP-GEOTAG | Drag photos onto the map | P2 | ⬜ | | |
| LRC-MAP-LOCATIONS | Saved locations | P2 | ⬜ | | |
| LRC-MAP-TRACKLOG | GPS track logs | P2 | ⬜ | | |
| LRC-MAP-FILTER | Location filter bar | P2 | ⬜ | | |
| LRC-MAP-REVGEO | Reverse geocoding | OOS | 🚫 | | |
| LRC-BOOK-SETTINGS | Book settings | P2 | ⬜ | | |
| LRC-BOOK-AUTOLAYOUT | Book auto layout | P2 | ⬜ | | |
| LRC-BOOK-PAGE | Book pages & templates | P2 | ⬜ | | |
| LRC-BOOK-GUIDES | Book guides | P2 | ⬜ | | |
| LRC-BOOK-CELL | Book cell padding | P2 | ⬜ | | |
| LRC-BOOK-TEXT | Book photo/page text | P2 | ⬜ | | |
| LRC-BOOK-TYPE | Book typography | P2 | ⬜ | | |
| LRC-BOOK-BG | Book backgrounds | P2 | ⬜ | | |
| LRC-BOOK-VIEWS | Book views | P2 | ⬜ | | |
| LRC-BOOK-EXPORT | Book export (PDF/JPEG) | P2 | ⬜ | | |
| LRC-SS-TEMPLATES | Slideshow templates | P2 | ⬜ | | |
| LRC-SS-OPTIONS | Slideshow options | P2 | ⬜ | | |
| LRC-SS-LAYOUT | Slideshow layout | P2 | ⬜ | | |
| LRC-SS-OVERLAYS | Slideshow overlays | P2 | ⬜ | | |
| LRC-SS-BACKDROP | Slideshow backdrop | P2 | ⬜ | | |
| LRC-SS-TITLES | Slideshow titles | P2 | ⬜ | | |
| LRC-SS-MUSIC | Slideshow music | P2 | ⬜ | | |
| LRC-SS-PLAYBACK | Slideshow playback | P2 | ⬜ | | |
| LRC-SS-EXPORT | Slideshow export | P2 | ⬜ | | |
| LRC-PRINT-LAYOUTSTYLE | Print layout styles | P2 | ⬜ | | |
| LRC-PRINT-IMAGESETTINGS | Print image settings | P2 | ⬜ | | |
| LRC-PRINT-LAYOUT | Print layout | P2 | ⬜ | | |
| LRC-PRINT-GUIDES | Print guides | P2 | ⬜ | | |
| LRC-PRINT-CELLS | Picture-package cells | P2 | ⬜ | | |
| LRC-PRINT-PAGE | Print page options | P2 | ⬜ | | |
| LRC-PRINT-JOB | Print job & colour management | P2 | ⬜ | | |
| LRC-PRINT-TEMPLATES | Print templates | P2 | ⬜ | | |
| LRC-WEB-LAYOUT | Web gallery layouts | OOS | 🚫 | | |
| LRC-WEB-SITEINFO | Web gallery site info | OOS | 🚫 | | |
| LRC-WEB-COLOR | Web gallery colours | OOS | 🚫 | | |
| LRC-WEB-APPEARANCE | Web gallery appearance | OOS | 🚫 | | |
| LRC-WEB-IMAGEINFO | Web gallery image info | OOS | 🚫 | | |
| LRC-WEB-OUTPUT | Web gallery output | OOS | 🚫 | | |
| LRC-WEB-UPLOAD | Web gallery upload | OOS | 🚫 | | |
| KEYC-PANELS | Classic panel keys (Tab, ⇧Tab, T, F5–F8, solo) | P2 | ⬜ | | |
| KEYC-MODULES | Classic module switching (⌘⌥1–7) | P2 | 🚫 | | no modules in LightCraft |
| KEYC-VIEWS | Classic view keys (E, G, C, N, L, F, I, ⇧R, ⌘⌥0) | P2 | 🟡 | `cmd:view.photoGrid`, `cmd:view.zoom100` | G works; E opens Edit (not loupe); no compare/survey/lights-out/screen modes |
| KEYC-SECONDWINDOW | Classic secondary-window keys | P2 | ⬜ | | |
| KEYC-CATALOG | Classic photo/catalog keys (⇧⌘I, ⌘', ⌘R, F2, ⌫, ⇧⌘E…) | P2 | 🟡 | `cmd:library.import`, `cmd:photo.delete`, `cmd:dialog.export` | import, delete, export work; no virtual copy, reveal, rename |
| KEYC-COMPARE | Classic grid/compare keys (Z, Home/End, =/−, ⌘⇧D, S…) | P2 | 🟡 | `cmd:view.zoomToggle` | Z toggles zoom; no compare, stacks, thumbnail-size keys |
| KEYC-RATING | Classic rating/flag keys (1–5, ⇧1–5, 6–9, P, X, U, ⇧X, ⇧U, `[` `]`, \`) | P2 | 🟡 | `cmd:photo.rate`, `cmd:photo.pick`, `cmd:photo.flag` | most work; no ⇧P / ⇧6–9 advance, rating `[` `]`, flag cycle, filter-bar keys |
| KEYC-COLLECTIONS | Classic collection keys (⌘N, B…) | P2 | 🟡 | `cmd:dialog.newAlbum` | ⌘N new album; no quick collection |
| KEYC-METADATA | Classic keyword/metadata keys (⌘K, ⌘S, ⌘⌥⇧C/V…) | P2 | 🟡 | `cmd:photo.saveMetadataToFile` | ⌘S saves metadata; no keyword sets, metadata copy/paste |
| KEYC-DEVELOP | Classic develop keys (V, ⌘U, ⇧⌘U, R, Q, K, M, ⇧M, ⇧W, ⇧J, ⇧Q…) | P2 | 🟡 | `cmd:develop.treatment`, `cmd:develop.reset`, `cmd:crop.reset` | V, ⇧⌘R, ⌥⌘R, W, J, Y, ⇧Y, \ match; R/K/M/⇧M differ; no Classic keymap layer |
| KEYC-MODULE-OUTPUT | Book / slideshow / print / map / web keys | P2 | ⬜ | | modules not implemented |
| KEYC-HELP | Classic help keys (⌘/, F1) | P2 | 🟡 | `cmd:app.shortcuts` | ⌘/ only |
