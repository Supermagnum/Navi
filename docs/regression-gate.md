# Regression gate (local)

The host regression gate (`navi-ffi/tests/regression_gate.rs`) plans the
accepted long-trip and short-trip cases against real device packs. It is
**required before merging routing or map changes**. It is not a GitHub Actions
check: it needs the pack volume and reference lines on the machine that runs it.

Automatic CI does not compile or run this test. A plain `cargo test` on a
machine without packs neither fails nor spends time on it. The crate feature
`regression-gate` is off by default; without that feature Cargo does not build
the test. The test is also `#[ignore]`, and it returns immediately if
`NAVI_GATE_PACKS` or `NAVI_GATE_REFS` is unset or not a directory.

## Run

```bash
export NAVI_GATE_PACKS=/path/to/long-trip-packs
export NAVI_GATE_REFS=/path/to/reference-lines
# optional: NAVI_GATE_CASES=e,h,k   NAVI_GATE_WORK=...   NAVI_GATE_BASELINE=...
cargo test --release -p navi-ffi --features regression-gate \
  --test regression_gate -- --ignored --nocapture
```

`NAVI_GATE_PACKS` is the long-trip pack dir (manifests, graph tiles, corridor
skeletons, `elevation/`), same layout as the device `long-trip-packs` dir.
`NAVI_GATE_REFS` is the directory of external reference lines. References named
`gate:<file>` live with the gate in `navi-ffi/tests/regression_gate_refs/`.

Probe and skeleton CLIs (`navi-dest-hop-probe`, `navi-corridor-coarse`,
`navi-place-index`, and the other `navi-ffi` bins except `uniffi-bindgen`) are
behind the `diag-bins` feature. Build one by name:

```bash
cargo build -p navi-ffi --features diag-bins --bin navi-place-index
```

## What is not the gate

Fast library tests (`cargo test -p driver-break-core --lib` and
`cargo test -p navi-ffi --lib`) and the Android JVM unit tests run on every
push and pull request. Clippy, `cargo test --workspace`, deny/audit, detekt,
and the Linux workspace build are the manual workflow
(`.github/workflows/ci-manual.yml`). Instrumented tests stay on
`android-instrumented.yml`.
