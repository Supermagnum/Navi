# Fuzzing and input hardening (issue #167)

Coverage-guided fuzzing for parsers that read external data. Safe Rust/Kotlin
rule out memory corruption, but malformed input can still panic, OOM, hang, or
mis-parse. This document is the living map of targets, caps, fallback rules,
and known limits.

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
cargo +nightly-2026-06-15 fuzz list
cargo +nightly-2026-06-15 fuzz run gpx_import -- -max_total_time=60
```

Minimize a crash:

```bash
cargo +nightly-2026-06-15 fuzz cmin <target>
cargo +nightly-2026-06-15 fuzz tmin <target> artifacts/<target>/crash-<hash>
```

Commit a minimized reproducer under the **normal** test suite (not only under
`fuzz/artifacts/`).

## CI

Workflow: `.github/workflows/fuzz.yml` (**Fuzzing**).

| Trigger | Duration | Notes |
|---|---|---|
| `workflow_dispatch` only | Configurable (`duration_seconds`, `targets`) | Manual — no push / PR / schedule triggers |

Normal merge CI (`.github/workflows/ci.yml`) still uses the pinned stable
toolchain only. Fuzz failures on PR do not block merge by themselves; treat
artifacts as must-fix.

## Target inventory

### CAT / radio (`navi-cat`)

| Target | API | Notes |
|---|---|---|
| `gaia_decode` | `gaia_decode` + `parse_body` | Split/concat frames |
| `benshi_rf_ch` | `ChannelRecord::from_reply` + encode round-trip | 25-byte RF channel |
| `benshi_settings_patch` | `SettingsBlock` + `write_payload_channel_a_only` | Asserts VFO-B fingerprint preserved when re-parsed |
| `benshi_dev_info` | `DevInfo::from_reply` | |
| `benshi_ht_status` | `HtStatus::from_reply` | |
| `benshi_freq_mode_notif` | `FreqModeStatusNotif::from_event_body` | Notification type 14 |
| `dump_caps_gate` | `gate_from_dump_caps` | Fail-closed gating |
| `rigctld_reply` | `parse_rigctld_reply` / `parse_rigctld_simple_value` | Extended `RPRT n` |

### Repeater data

| Target | API | Notes |
|---|---|---|
| `anytone_channel_csv` | `decode_cps_bytes` + `import_anytone_channel_csv` | UTF-8 / Windows-1252 path |
| `repeater_csv_bytes` | `repeaterbook::parse_csv_bytes` | Ragged rows / huge fields |
| `repeater_current_json` | `mirror::parse_current` | |
| `repeater_checksums` | `checksum_for_file` | `checksums.sha256` lines |
| `zst_decompress` | `decompress_zst_capped` | Decompression-bomb guard |
| `parse_mhz` | `normalize_mhz_str` / `parse_mhz` | `-0,6 Mhz`, `88.5hz` |
| `maidenhead` | `maidenhead_to_latlon` | 2–8 char locators |
| `osm_repeater_relation_xml` | `import_osm_relation_xml` | Tag soup |
| `repeater_kml_point` | `parse_kml_point` | Rejects `0,0` / NaN |

### Maps — in-tree (Navi sidecar / catalog / packs)

**Important:** CoMaps `.mwm` container / feature / routing / search binary
readers live in **CoMaps C++** (`liborganicmaps`), not in Rust. In-tree
`core/src/mwm/` covers catalog, download, fingerprint, and the **NVSC sidecar**.
Corrupt `.mwm` files fail inside CoMaps; Navi does **not** implement a
per-corrupt-MWM “fall back to PBF for the same region” enum (classic `.rkyv` /
PBF is a separate kill-switch / pack path).

| Target | API | Notes |
|---|---|---|
| `mwm_sidecar` | `SidecarReader::open_bytes` | Caps: 64 MiB decompress, 256 sections |
| `mwm_catalog_json` | `parse_catalog_json` | `navi-mwm-catalog/v2` |
| `mwm_countries_txt` | `parse_countries_txt` | Signature fail-closed separately |
| `navi_manifest_json` | `NaviManifest` serde | Pack manifest |
| `flat_graph_pack` | `load_graph_pack` | Tempfile + mmap; wrong magic / truncated |
| `pbf_extract_gate` | `pbf_is_real_extract` | Stub / BlobHeader gating |

### Shared OSM tag parsers

| Target | API |
|---|---|
| `osm_maxspeed` | `parse_maxspeed_kmh`, `parse_metric`, `is_tunnel_tag`, `classify_surface_tags` |
| `osm_conditional_maxspeed` | `conditional_maxspeed_kmh_at` |

### Other parsers

| Target | API |
|---|---|
| `gpx_import` | `parse_gpx_bytes` |
| `datex_situation` | `parse_situation_publication` |
| `plugin_manifest` | `PluginManifest::from_json_str` |

### Plugin host (misbehaving guest)

Not a libFuzzer binary: `plugin-host/tests/isolation.rs` stages
`busy-loop`, `trap-guest`, and `memory-bomb` WASM fixtures. Host must stay up
(`FuelExhausted` / `Timeout` / `MemoryExceeded` / classified trap). Defaults:
5e6 fuel, 250 ms epoch, 16 MiB memory.

### Kotlin

| Check | Where |
|---|---|
| Malformed JSON / random blobs | `ExternalJsonHardeningTest` (JVM unit test) |
| CSV parse | Covered in Rust (`anytone_channel_csv` / `repeater_csv_bytes`); SAF drop path is filesystem |
| UniFFI panic surface | Generated `InternalException("Rust panic")`; app uses `runCatching` widely — instrumented panic→exception check is build-config dependent |

## Size caps

| Input | Cap | Where |
|---|---|---|
| GPX import (app) | 40 MiB | Kotlin `GpxImport.MAX_BYTES` |
| GPX fuzz harness | 4 MiB | `fuzz_targets/gpx_import.rs` |
| DATEX XML harness | 2 MiB | `fuzz_targets/datex_situation.rs` |
| GAIA / Benshi harness | 64 KiB | harnesses |
| Repeater zst (product) | **32 MiB** | `ZST_DECOMPRESSED_MAX` |
| ZST fuzz harness out | 1 MiB | `zst_decompress` |
| Sidecar decompress | **64 MiB** | `SIDECAR_DECOMPRESSED_MAX` |
| Sidecar section_count | **256** | `SIDECAR_MAX_SECTIONS` |
| rigctld reply | **1 MiB** | `RIGCTLD_REPLY_MAX_BYTES` |
| Flat graph pack fuzz file | 4 MiB | harness |

## Hardening checklist

| Item | Status |
|---|---|
| Typed errors on parser paths (no silent `unwrap` in new APIs) | Done for extracted APIs; ongoing for legacy paths |
| Checked length/offset arithmetic on sidecar directory | Done (`Truncated` / `TooManySections`) |
| Decompress caps (repeater zst + sidecar zstd) | Done |
| Max sizes for frames / CSV / JSON / packs (harness + product where noted) | Partial — see table |
| Recursion/depth limits for OSM XML / JSON | Open for custom XML lite |
| MWM format version up front | Sidecar `FORMAT_VERSION`; CoMaps `.mwm` version inside native |
| mmap bounds | Pack load validates preamble / lengths; CoMaps mmap is native |
| Fallback decision once per region | Classic pack vs MWM is flag/catalog based — documented; no half-MWM/half-PBF merge in Rust |
| FFI panic → Kotlin exception | UniFFI CALL_PANIC path; release-android may abort — known limit |
| wasmtime fuel / epoch / memory | Enforced; isolation tests green |
| Kotlin catch around Rust / JSON | Pattern + `ExternalJsonHardeningTest` |
| Review unsafe / C deps (hamlib, sqlite, zstd) | Ongoing; zstd paths capped |

## Local campaigns

Prior 1-hour campaign (seven original targets, 2026-11-25):
[`docs/test-results/fuzzing-issue-167.md`](test-results/fuzzing-issue-167.md) —
**PASS**, 0 crash/oom/timeout.

Expanded campaign (2026-11-25 → 2026-11-27, Europe/Oslo):
[`docs/test-results/fuzzing-issue-167-expanded.md`](test-results/fuzzing-issue-167-expanded.md) —
1 h on all remaining targets + 4 h on `mwm_sidecar` / `flat_graph_pack`.
Almost all **PASS**; one real finding in `osm_conditional_maxspeed`
(opening-hours-syntax panic on `/` interval syntax) — fixed via
safe-character pre-filter + `catch_unwind`, plus regression test.

## Known limits

- Fuzz crate is a **separate** Cargo workspace under `fuzz/` so nightly
  libFuzzer builds do not fight the pinned 1.98 workspace.
  `fuzz/rust-toolchain.toml` pins `nightly-2026-06-15`.
- After a local run, libFuzzer grows `fuzz/corpus/<target>/`. Do **not**
  commit the whole grown corpus without `cargo fuzz cmin`; keep seeds small.
- `anytone_channel_csv` / OSM XML importers open SQLite; slower than pure
  parsers.
- CoMaps `.mwm` binary format is **out of tree** for cargo-fuzz; structure-aware
  MWM and true MWM→PBF fallback targets require CoMaps harnesses or a Rust port.
- No full Jazzer Gradle job yet (Kotlin uses property/unit hardening tests).
