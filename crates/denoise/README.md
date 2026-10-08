# lightcraft-denoise

L1 pure-Rust AI RAW denoise primitives and CPU inference. No weights are bundled.
See [user workflow and limitations](../../docs/denoise.md).

`manifest` and `known` validate opt-in models; `onnx` reads a bounded operator subset
into `net`; `cpu` executes it and `reference` supplies scalar checks. `bayer`, `tiles`
and `run` pack sensor data and blend overlapping tiles while preserving clipped
highlights. `product` reads/writes the disposable `.lcdn` cache. `runtime` supplies
the installation self-test. Synthetic networks are generated in Rust for tests.

Run `cargo test -p lightcraft-denoise`. Real-model tests are ignored by default and
require an explicitly supplied local model. The GPU twin is `lightcraft-gpu::nn`;
model management, background jobs and export integration live in `lightcraft-engine`.
