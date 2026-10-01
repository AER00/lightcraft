# GPU develop pipeline — design note (M5.2, not implemented yet)

The CPU pipeline (`crates/pipeline`) is the reference ("oracle"). A `lightcraft-gpu` crate (L3) will
evaluate the same stages with wgpu compute shaders (WGSL), on native (Metal/Vulkan/DX12) and on the
web (WebGPU). This note records the plan so the CPU work in M5 stays compatible with it.

## Why, and what the CPU path already does
- CPU numbers (24 MP source, 2560 px preview, ~2.5 MP loupe; see `crates/engine/examples/render_bench.rs`):
  the per-pixel stage (`finish`) costs ~50 ns/px of CPU time with typical edits (+~65 ns/px with
  vibrance/saturation: the OkLCh round trip), noise reduction + spatial planes another ~80 ns/px
  when they have to be recomputed. With all cores and the stage cache this meets the CPU budgets
  for tone/colour drags (~7 ms draft, ~18 ms full at 2.5 MP); a GPU makes cold full-quality
  renders, NR drags, 4–8K displays and 1:1 zoom of 60 MP files interactive, and frees the CPU for
  previews/import.
- The CPU path is already structured the way the GPU path wants it:
  - **Stage cache** (`StageCache`): resampled source → white-balanced/denoised image → spatial
    planes (log luminance, guided base, clarity/texture bands, dark channel) → per-pixel stage. Each
    stage is keyed by the settings that feed it. On the GPU the same keys decide which textures are
    still valid; a tone/colour/exposure drag re-runs one full-screen compute pass.
  - **Exposure after the spatial filters** (shift-equivariant on log luminance), so exposure is a
    per-pixel uniform, not a cache invalidation.
  - **Draft quality** during drags (smaller output, capped base radius), full on release.

## Architecture
- `lightcraft-gpu` owns a `wgpu::Device` (shared with eframe's when running in the desktop app, so
  the loupe texture never leaves the GPU; headless device for CLI/MCP/export).
- Resident per-photo state: the source pyramid as `rgba16float` textures (levels /2, /4, … down to
  the thumbnail), uploaded once per photo; the stage textures mirror `StageCache` entries
  (`r32float` planes, `rgba16float` images).
- Kernels (one WGSL module each, workgroups of 16×16, storage textures):
  1. `sample`: orientation + crop/rotate + flips from the nearest pyramid level (bilinear on the
     level ≥ output scale — matches the CPU's area-prefiltered resample within tolerance).
  2. `wb`: 3×3 matrix (+ spots as instanced quads: copy/blend from the source offset).
  3. `nr`: guided filter on log luminance (box sums via two-pass separable running sums in shared
     memory, or a summed-area table for large radii) + chroma Gaussian.
  4. `planes`: log luminance; guided base / clarity (fast guided filter: coefficients on a /s
     subsampled grid, bilinear upsample — exactly the CPU `guided_fast` structure); texture
     Gaussian (3 box passes); dark channel Gaussian + airlight (parallel reduction: histogram of the
     dark channel → 99.5th percentile).
  5. `masks`: one pass per mask shape (analytic shapes evaluate per pixel; brushes rasterized as
     instanced soft discs; range masks read the image/planes).
  6. `finish`: the per-pixel stage, a straight port of `finish.rs` (tone LUT and curve LUTs as 1D
     textures, colour ops in OkLCh, gamut map, sRGB encode, grain hash). Output `rgba8unorm` for
     display, `rgba16float`/`rgba32float` for export.
  7. `histogram`: atomics into a storage buffer (256 bins × 4).
- Readback only for export and for `ui.screenshot`/MCP renders.

## Correctness: CPU oracle and equivalence tests
- Every kernel has a CPU twin; tests render the synthetic scenes (`lightcraft-scenes`) on both and
  require max |Δ| ≤ 1/1023 in display-encoded values (architecture §1.6), plus the existing pipeline
  tests run against the GPU renderer behind a feature flag. Tests skip (not fail) when no adapter is
  available (CI machines), with a software adapter (wgpu's GL/lavapipe) where possible.
- Float determinism: avoid fast-math-sensitive constructs (fused multiply-add differences are within
  tolerance); reductions (airlight, histogram) use integer atomics.

## Scheduling
- One render queue per view; a new revision cancels queued work for the old one (CPU already drops
  superseded jobs per slot). Drafts during drags at display size (GPU can afford full size), full
  quality (larger NR/guided radii at source resolution for 1:1) on release.
- Fallback order: GPU → CPU (`render_cached`) when no adapter, on device loss, or for wasm builds
  without WebGPU.

## Open questions
- f16 vs f32 storage for planes (bandwidth vs. guided-filter precision: start with f32).
- Sharing the device with eframe on the web (single WebGPU device per canvas).
- Large exports (60 MP+) exceed texture limits on some adapters (8192/16384): tile with overlap
  equal to the largest filter radius (the planes' radii are known per settings).
