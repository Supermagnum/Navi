# ECU AFR / fuel-rate test results

Canonical evidence log for ICE decode tests on `feat/ecu-afr-tests`. Protocol
notes remain in [`ECU.md`](ECU.md). Android campaign chronology stays in
[`android-test-results.md`](android-test-results.md) (Item 18). Route
integration results stay in [`test-results.md`](test-results.md).

This file did not exist on the branch until this record. Nearby docs mention
the suite but do not record pass counts, SHA, or commands for this run.

## Environment

| Item | Value |
|---|---|
| Date | 2026-10-03 (UTC) |
| Branch | `feat/ecu-afr-tests` |
| Commit | `9069c0a42665566e16ccefbe496c43bea1541eb2` (`9069c0a4`) |
| Host | Linux x86_64, rustc 1.98.0 |
| Scope | Pure decode / fuel-rate math; `DATA_SOURCE=none` |

## Host Rust (this run)

| Command | Result | Notes |
|---|---|---|
| `cargo test -p driver-break-core --lib ecu` | **PASS** — 17 passed, 0 failed, 0 ignored, 601 filtered | Name filter `ecu` also runs `weekly_rest_after_six_consecutive_working_days` (substring match). The other 16 are `ecu::*` unit tests. |
| `cargo test -p driver-break-core --test ecu_afr_fuel_rate` | **PASS** — 10 passed, 0 failed, 0 ignored | Golden suite in `core/tests/ecu_afr_fuel_rate.rs`. |
| `cargo test -p driver-break-core ecu` (no `--lib`) | **Did not complete** | Cargo still compiles every `driver-break-core` integration binary. Several unrelated tests failed to link (`lld` signal 7 / bus error). Not a test assertion failure. Use `--lib` and `--test ecu_afr_fuel_rate` for a reliable ECU-only run. |
| `rustfmt --check --edition 2021 core/src/ecu/*.rs core/tests/ecu_afr_fuel_rate.rs` | **PASS** | ECU sources only. Workspace `cargo fmt --all -- --check` was not run. |
| Workspace Clippy (`-D warnings`) | **Not run** | No clippy transcript recorded for this SHA in this session. |

### Lib tests matching `ecu` (17)

- `ecu::ambient::tests::{missing_baro_is_none, lower_pressure_is_higher, sea_level_near_zero}`
- `ecu::decode::tests::{j1939_example_five_l_h, no_data_is_none, elm_spaces_and_headers, spn96_na_is_none, pid_5e_zero_is_some_zero, elm_5e_example_five_l_h}`
- `ecu::fuel::tests::{diesel_maf_with_lambda_does_not_use_petrol_147, diesel_maf_without_lambda_is_none, l100km_requires_positive_speed, megasquirt_formula_and_range_skip, missing_level_or_tank_is_none_not_zero, petrol_maf_uses_formulas_density_not_ecu_md_074}`
- `ecu::tests::no_live_energy_is_none`
- `routing::rest::truck_multi_day::tests::weekly_rest_after_six_consecutive_working_days` (filter coincidence)

### `ecu_afr_fuel_rate` (10)

`self_test_report_passes`, `elm327_pid_table_ice_only`, `pid_0c_rpm_formula`,
`diesel_without_rate_or_lambda_stays_none`, `petrol_flex_changes_maf_rate`,
`j1939_lfe_illustrative_matches_spn183_scale`, `megasquirt_skips_out_of_range`,
`maf_density_disagrees_with_ecu_md_074`, `no_data_patterns`,
`refine_energy_cost_and_provider`.

Commands used:

```bash
cargo test -p driver-break-core --lib ecu
cargo test -p driver-break-core --test ecu_afr_fuel_rate
```

## Not run (this session)

| Check | Status |
|---|---|
| Live ELM327 / Bluetooth / serial / SocketCAN / MegaSquirt adapter | Not run (no hardware; decode remains adapter-free) |
| Signed APK install and launch | Not run |
| `adb logcat -s NaviEcu` on a device or emulator | None captured |
| Android instrumented `EcuAfrSelfTestInstrumentedTest` | Not re-run |
| `./gradlew :app:ktlintCheck` | Not re-run |
| UniFFI `ecu_afr_self_test` / `ecuAfrSelfTest()` on device | Not re-run (Rust `self_test_report_passes` covers the same golden report) |
| Workspace `cargo fmt --all -- --check` | Not run |
| Workspace Clippy | Not run |

Host-side ECU decode tests for this SHA: **26 dedicated tests passed** (16 lib
`ecu::*` + 10 integration), plus the coincidental truck-rest name match.
