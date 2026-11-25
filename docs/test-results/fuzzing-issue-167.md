# Local fuzz campaign (issue #167)

Date: **2026-11-25** (Europe/Oslo). Host: self-hosted Linux, **24** cores,
**31 GiB** RAM. Branch: **`cursor/fuzzing-input-hardening-de43`**. Toolchain:
**`nightly-2026-06-15`** (`rustc 1.98.0-nightly`). Issue:
[Supermagnum/Navi#167](https://github.com/Supermagnum/Navi/issues/167).

Campaign: all seven current `cargo-fuzz` targets in parallel, **3600 s** each
(`--jobs 2`, `-timeout=10`, `-rss_limit_mb=2048`). Started
**12:23:21** Europe/Oslo, finished **13:24:16** Europe/Oslo. Logs:
`/tmp/fuzz-167/logs/`.

Grown corpora from this run were **not** committed (reseeded to the small
seed set under `fuzz/corpus/`). No crash artifacts under `fuzz/artifacts/`.

---

## Verdict

| Target | Wall time | Approx. runs | Final cov / ft | Corp | Exec/s (late) | oom/timeout/crash | Exit |
|---|---|---|---|---|---|---|---|
| `gpx_import` | 3651 s | 116.5 M | 2921 / 13028 | 3692 | 14884 | 0/0/0 | 0 |
| `datex_situation` | 3645 s | 26.1 M | 2775 / 12360 | 2746 | 3029 | 0/0/0 | 0 |
| `gaia_decode` | 3652 s | 256.2 M | 80 / 287 | 53 | 35029 | 0/0/0 | 0 |
| `dump_caps_gate` | 3634 s | 12.3 M | 598 / 2962 | 1073 | 1779 | 0/0/0 | 0 |
| `anytone_channel_csv` | 3652 s | 3.7 M | 916 / 4337 | 904 | 524 | 0/0/0 | 0 |
| `repeater_current_json` | 3653 s | 331.5 M | 2261 / 7646 | 2871 | 42895 | 0/0/0 | 0 |
| `zst_decompress` | 3613 s | 24.5 M | 166 / 426 | 34 | 3766 | 0/0/0 | 0 |

**Overall: PASS** for the current target set — each ran ≥ 1 hour with no
unresolved crashes, timeouts, or OOMs. Batch `fail=0`.

`anytone_channel_csv` is the slowest (in-memory SQLite per input) but stayed
well above 100 exec/s. `gaia_decode` coverage plateaus early (small decoder
surface); high exec/s, no crashes.

---

## Setup

| Item | Value |
|---|---|
| Command shape | `cargo +nightly-2026-06-15 fuzz run <target> --jobs 2 -- -max_total_time=3600 -timeout=10 -rss_limit_mb=2048` |
| Seeds | committed `fuzz/corpus/<target>/` (fixtures + hand-made GAIA/zst) |
| Product hardening in tree | `decompress_zst_capped` / 32 MiB `ZST_DECOMPRESSED_MAX` |
| CI | `.github/workflows/fuzz.yml` is **workflow_dispatch only** (not mandatory) |

---

## Still open on #167

MWM container / feature / routing / fallback targets (4 h each when added),
PBF/pack readers, plugin-host HostApi, Kotlin/Jazzer, full hardening
checklist, and promoting any future crashes to regression tests.
