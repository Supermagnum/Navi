# Fuzzing and input hardening (issue #167)

Coverage-guided fuzzing for parsers that read external data. This is **not**
part of the automatic PR / merge CI (`.github/workflows/ci.yml`). Run it by
hand when you want a longer look at crash surface.

Issue: [Supermagnum/Navi#167](https://github.com/Supermagnum/Navi/issues/167).

## How to run (local)

Needs a **pinned** nightly for `cargo-fuzz` / libFuzzer. The pinned `1.98.0`
toolchain in `rust-toolchain.toml` stays for normal builds.

Use `nightly-2026-06-15` (same pin as `.github/workflows/fuzz.yml`). Floating
tip nightly has broken `filetime` / `fs_set_times` against our dependency set.

```bash
rustup install nightly-2026-06-15
cargo +nightly-2026-06-15 install cargo-fuzz
cd fuzz
cargo +nightly-2026-06-15 fuzz run gpx_import -- -max_total_time=60
```

List targets:

```bash
cargo +nightly-2026-06-15 fuzz list
```

Minimize a crash:

```bash
cargo +nightly-2026-06-15 fuzz cmin <target>
cargo +nightly-2026-06-15 fuzz tmin <target> artifacts/<target>/crash-<hash>
```

Commit a minimized reproducer under the normal test suite (not only under
`fuzz/artifacts/`).

## CI (user-triggered only)

Workflow: `.github/workflows/fuzz.yml` (**Fuzzing**).

- Trigger: Actions → Fuzzing → Run workflow (`workflow_dispatch` only).
- Inputs: `duration_seconds` (default `60`), `targets` (`all` or a
  comma-separated list).
- Not a required status check. Failure does not fail automatic CI.
- No `push` / `pull_request` / `schedule` triggers.

## Current targets

| Target | Crate / API | Seeds |
|---|---|---|
| `gpx_import` | `driver_break_core::import::gpx_import::parse_gpx_bytes` | `core/tests/fixtures/gpx/` |
| `datex_situation` | `driver_break_core::datex::parse_situation_publication` | `core/tests/fixtures/datex/` |
| `gaia_decode` | `navi_cat::benshi::codec::{gaia_decode,parse_body}` | hand-made GAIA frames |
| `dump_caps_gate` | `navi_cat::gate_from_dump_caps` | `navi-cat/tests/fixtures/dump_caps/` |
| `anytone_channel_csv` | `decode_cps_bytes` + `import_anytone_channel_csv` | `testdata/cat/anytone/` |
| `repeater_current_json` | `navi_cat::mirror::parse_current` | minimal JSON |
| `zst_decompress` | `navi_cat::mirror::decompress_zst_capped` (1 MiB harness cap) | tiny zst + garbage |

Seed corpora live under `fuzz/corpus/<target>/` and are committed (small).

## Size caps (chosen so far)

| Input | Cap | Where |
|---|---|---|
| GPX import (app) | 40 MiB | Kotlin `GpxImport.MAX_BYTES` |
| GPX fuzz harness | 4 MiB | `fuzz_targets/gpx_import.rs` |
| DATEX XML harness | 2 MiB | `fuzz_targets/datex_situation.rs` |
| GAIA buffer harness | 64 KiB | `fuzz_targets/gaia_decode.rs` |
| Repeater zst decompress | **32 MiB** | `navi_cat::mirror::ZST_DECOMPRESSED_MAX` |
| ZST fuzz harness | 1 MiB out | `fuzz_targets/zst_decompress.rs` |

## Hardening done in this slice

- `decompress_zst` / `decompress_zst_capped`: refuses to expand past
  `ZST_DECOMPRESSED_MAX` (decompression-bomb guard for navi-server repeater
  payloads).
- Fuzz harnesses bail early on oversized inputs so libFuzzer cannot OOM the
  runner before the parser runs.

## Local 1-hour campaign (2026-11-25)

All seven targets ran in parallel for ~3600 s on a 24-core / 31 GiB host
(`nightly-2026-06-15`, `--jobs 2`). Result: **0** oom / timeout / crash on
every target; batch exit 0. Full table:
[`docs/test-results/fuzzing-issue-167.md`](test-results/fuzzing-issue-167.md).

## Still open (from #167)

Not yet covered here; track on the issue:

- MWM container / feature / routing / search-section readers and MWM→PBF
  fallback target
- Structure-aware MWM fuzzer
- PBF blob / string-table / dense-node paths
- Pack/manifest readers, OSM tag parsers shared by both paths
- Plugin-host HostApi decoding + misbehaving WASM guest
- Kotlin / Jazzer (CSV SAF import, settings JSON, UniFFI exception surface)
- Full hardening checklist (varint caps, trie depth limits, mmap bounds, FFI
  panic strategy tests)
- Multi-hour local campaigns and crash→regression promotion workflow

## Known limits

- Fuzz crate is a **separate** Cargo workspace under `fuzz/` so nightly
  libFuzzer builds do not fight the pinned 1.98 workspace.
  `fuzz/rust-toolchain.toml` pins `nightly-2026-06-15`.
- After a local run, libFuzzer grows `fuzz/corpus/<target>/`. Do **not**
  commit the whole grown corpus without `cargo fuzz cmin`; keep seeds small.
- `anytone_channel_csv` opens an in-memory SQLite DB per input; slower than
  pure parsers.
- No Kotlin / Jazzer job yet.
