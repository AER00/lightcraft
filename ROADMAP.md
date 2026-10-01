# LightCraft roadmap

Milestones toward full Adobe Lightroom parity (cloud Lightroom first, then every Lightroom Classic module), with wall-clock
estimates for continuous (24/7) agent-driven development with 4–6 parallel agents. Estimates are calibrated on the sibling
projects (DrawCraft reached its first four milestones in ≈ 4½ h) and are revised as milestones land.

**Status legend:** ✅ done · 🚧 in progress · ⬜ not started

| # | Milestone | Scope (summary) | Estimate (h) | Status |
|---|---|---|---|---|
| M0 | Skeleton + visual shell | workspace, xtask CI + layering, geom/color/raster, develop model, pipeline v0, catalog v0, engine commands, Lightroom-look UI (grid, loupe, filmstrip, Edit panel), control channel, MCP, web build | 3–5 | 🚧 (UI, pipeline, engine, control channel ✅; MCP/CLI, web 🚧) |
| M1 | Library core | import (JPEG/PNG/TIFF/WebP), EXIF/XMP, persistent catalog (op log + snapshots), albums, ratings/flags/labels, filter/search/sort, thumbnail cache, 100k-photo grid | 6–10 | 🚧 (import, codecs, EXIF/XMP, albums, search ✅; persistence 🚧) |
| M2 | Pipeline v1 (quality) | WB temp/tint, profiles, local tone mapping (highlights/shadows), curves, HSL, point colour, colour grading, texture/clarity/dehaze, vignette, grain, B&W, auto tone/WB, histogram, before/after | 10–15 | ⬜ |
| M3 | RAW I | TIFF/DNG (LJ92, deflate, tiles, opcodes), demosaic (AHD/PPG/bilinear), highlight recovery, DNG colour model, CR2, NEF, ARW, embedded previews | 10–15 | 🚧 (DNG, CR2, ARW, NEF uncompressed, embedded previews ✅; NEF Huffman ⬜) |
| M4 | Crop, geometry, optics | crop tool + overlays, straighten, Upright (auto/level/vertical/full/guided), manual transforms, CA, defringe, manual lens corrections | 6–10 | ⬜ |
| M5 | Performance | source pyramids, wgpu compute pipeline (CPU oracle), draft/full renders, prefetch, budgets (16 ms slider updates on 24 MP) | 10–15 | ⬜ |
| M6 | Masking | brush, linear/radial gradients, colour/luminance/depth range, add/subtract/intersect/invert, all local adjustments, masks panel | 8–12 | ⬜ |
| M7 | Detail | sharpening + masking preview, luminance/colour NR, Denoise, Raw Details, Super Resolution | 6–10 | ⬜ |
| M8 | Heal / Remove | content-aware remove (PatchMatch), heal, clone, brush spots, visualize spots, red/pet eye | 6–10 | ⬜ |
| M9 | Presets, profiles, versions, sync | preset browser + amount, create/import presets, profile browser, versions, history, copy/paste/sync settings | 5–8 | ⬜ |
| M10 | Export & share | export dialog (JPEG/PNG/TIFF/DNG/AVIF/JXL/original), sizing, sharpening, metadata, watermark, naming, batch jobs, XMP sidecars, HDR export | 6–10 | ⬜ |
| M11 | RAW II | CR3, RAF (X-Trans), ORF, RW2, PEF, SRW, 3FR, IIQ + long tail; camera calibration DB; HEIC/AVIF/JXL import | 20–35 | 🚧 (RAF uncompressed, RW2 packed, PEF, ORF uncompressed ✅) |
| M12 | AI & smart features | subject/sky/background/people/object masks, semantic search, faces/People (permissively licensed models, pure-Rust inference) | 20–40 | ⬜ |
| M13 | Merge | HDR merge (deghost), panorama (projections, boundary warp, fill edges), HDR panorama | 10–15 | ⬜ |
| M14 | Video | import/playback/trim via FilmCraft crates, global edits + presets on video, video export | 6–10 | ⬜ |
| M15 | Classic modules | Map, Book, Slideshow, Print, Web; smart collections, stacks, virtual copies, publish services, tethering | 25–40 | ⬜ |
| M16 | 1.0 polish | preferences, shortcut editor, accessibility, localization, packaging (dmg/msi/AppImage/web), hardening | 10–20 | ⬜ |

## Totals

| Target | Milestones | Wall-clock estimate |
|---|---|---|
| Cloud-Lightroom feature complete | M0–M11, M13–M14 | **≈ 130–180 h** (6–8 days, 24/7) |
| Full parity incl. Classic modules and AI | M0–M16 | **≈ 200–300 h** (9–13 days, 24/7) |

## Risks that coding hours alone don't retire

- **AI features** (subject/sky/people masks, generative remove) need model weights with licences we can ship; classical
  fallbacks first. No permissively licensed sky-segmentation or raw-denoise model was found — we may need to train our own.
- **Camera colour and lens data** is a data problem: we never use Adobe's matrices, DCPs or LCPs. DNG-embedded data first,
  then our own calibration; long-tail camera/lens coverage grows over time.
- **Legal decisions pending:** whether GPL-licensed *prose* format descriptions (e.g. the public CR3 write-up) may be read
  by a designated engineer to produce an internal spec; freedom-to-operate review for local Laplacian filters, PatchMatch and
  HEVC (HEIC).
- **Look parity** with Adobe's default rendering is subjective tuning against our own reference targets.

## Raw format coverage and known gaps

Decoded (CC0 corpus from raw.pixls.us, `cargo xtask corpus --download`, `crates/raw/tests/corpus.rs`): DNG (uncompressed,
LJ92, Deflate, float, linear), CR2, ARW (uncompressed, ARW2, LJ92), NEF/NRW uncompressed, RAF uncompressed (Bayer and
X-Trans), RW2 packed 12/14-bit, PEF (uncompressed and Huffman), ORF uncompressed (16-bit and 12-bit packed). Every
supported container also yields its embedded JPEG preview (CR3 too), and the engine shows that preview for raw variants
it can't decode yet.

Not decoded yet — preview only (no permissively licensed description; black-box analysis incomplete):
- **Nikon Huffman NEF** (lossless / lossy). Black-box findings so far: maker note `0x0096` holds per-parity predictor
  seeds (2048 for 14-bit lossless); ≈ 9 bits per pixel; the category-0 code word is a 6-bit rotation of `011111`. The
  Huffman tables are not stored in the files, and an automatic table search (beam search over prefix codes scored by
  smoothness and bit rate, validated on Pentax files whose tables *are* stored) did not converge.
- **Panasonic RW2 raw format 4** (quantised): the block layout is known (0x4000-byte chunks rotated by 0x1ff8; 128-bit
  blocks of two 12-bit seeds plus four groups of a 2-bit scale and three 8-bit codes; scales 0/1 are ×1/×2 differences
  from the same-colour pixel two to the left), the reconstruction rule for scales 2/3 is not established.
- **Olympus compressed ORF**, **Fujifilm compressed RAF**, **Canon CR3/CRX** (M11.1), **Canon sRAW/mRAW**, lossy DNG.

**Camera colour matrices:** non-DNG raws use the documented neutral fallback (camera RGB ≈ linear sRGB, flagged
`matrix_is_fallback`) with the file's as-shot white-balance multipliers. Clean sources to evaluate next: manufacturer
matrices stored in the files themselves (Olympus ImageProcessing `ColorMatrix`, Pentax/Panasonic equivalents) and our
own chart-based calibration (M11.4). Adobe matrices are never used.

## Log
- 2026-09-30: roadmap created; M0 in progress; research docs (Lightroom reference, Rust imaging ecosystem) complete.
- 2026-09-30 (later): app running with the full Lightroom-style UI; pipeline v0; DNG/CR2/ARW; README showcase. ≈ 8 h elapsed.
- 2026-10-01: RAW II formats: RAF (uncompressed Bayer + X-Trans), RW2 (packed), PEF (incl. Huffman), ORF (uncompressed); embedded previews for every container incl. CR3; raw corpus test with 37 CC0 samples.
