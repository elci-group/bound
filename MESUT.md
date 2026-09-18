# Mesut consumer integration

The existing `bound` CLI accepts `--mesut` to load real bundle files with Mesut.
The library exposes `bound_core::bundle_with_mesut(&options, &logger)` alongside
the unchanged synchronous `bundle` API. Both use the same discovery, filtering,
dependency resolution, redaction, limits, Furnace analysis, and snapshot rendering.

Each batch contains at most 32 files. Every file has a Blocking stage that reads
UTF-8 content and filesystem metadata, followed by a Compute stage that counts
lines and optionally computes SHA-256 from that content. Stage factories consume
their declared read output. Up to eight ready stages run concurrently. Results
are collected by their original sorted paths, regardless of completion order.
Unreadable/non-UTF-8 files remain skipped; dependency-discovery errors still
propagate. Hashes describe the original content, before redaction or truncation.

The adapter owns a Tokio runtime with timers and awaits Mesut shutdown on normal
and error returns. Async callers should use `spawn_blocking`. Runtime failures
surface as `BundleError::Io` with a Mesut prefix. The adapter serializes stage
payloads as JSON, so this opt-in integration makes no speedup claim. Batching
bounds intermediate file count, not bytes; Bound still reads whole files.
Concurrent changes to files are not an atomic snapshot with either backend.

Local dependency: `../mesut` beside this Bound checkout (in addition to Bound's
existing `../3form` dependency). Mesut source files are not modified by this
integration. The default CLI execution path remains synchronous.

```sh
cargo test --offline --workspace
cargo run --offline -p bound-cli -- '[rs]' crates --mesut --meta --meta-hash --tree --json --out /tmp/bound-mesut.json
cargo run --offline -p bound-cli -- '[rs]' crates --meta --meta-hash --tree --json --out /tmp/bound-sync.json
```

`crates/bound-core/tests/mesut_parity.rs` compares full snapshots (normalizing
only `generated_at`) and rendered text over real files, multiple batches,
metadata/hashes, tree/Furnace, filtering/dependencies, limits, redaction, ignored
files, empty files/selections, invalid UTF-8, and discovery failures.
