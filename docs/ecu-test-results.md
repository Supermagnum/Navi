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
| Commit | `fe933c9d6d89b697320bb7abcc8020b10bb57c8c` (`fe933c9d`) |
| Host | Linux x86_64, rustc 1.98.0 (88d9e12ae 2026-08-18) |
| Scope | Pure decode / fuel-rate math; `DATA_SOURCE=none`; synthetic engine model in tests only |

## Host Rust (this run)

| Command | Result | Notes |
|---|---|---|
| `cargo test -p driver-break-core --lib ecu` | **PASS** — 24 passed, 0 failed, 0 ignored, 601 filtered | Name filter `ecu` also runs `weekly_rest_after_six_consecutive_working_days` (substring match). The other 23 are `ecu::*` unit tests. |
| `cargo test -p driver-break-core --test ecu_afr_fuel_rate` | **PASS** — 20 passed, 0 failed, 0 ignored | Golden suite plus scenario generator/assertions in `core/tests/ecu_afr_fuel_rate.rs`. |
| `cargo test -p driver-break-core ecu` (no `--lib`) | **Did not complete** | Cargo still compiles every `driver-break-core` integration binary. Several unrelated tests failed to link (`lld` signal 7 / bus error). Not a test assertion failure. Use `--lib` and `--test ecu_afr_fuel_rate` for a reliable ECU-only run. |
| `rustfmt --edition 2021 core/src/ecu/*.rs core/tests/ecu_afr_fuel_rate.rs core/tests/helpers/ecu_engine_model.rs` | **PASS** | ECU sources and generator only. Workspace `cargo fmt --all -- --check` was not run. |
| `cargo clippy -p driver-break-core --no-deps --lib -- -D warnings` | **PASS** (this crate check) | Touched `core/src/ecu` files included. |
| `cargo clippy -p driver-break-core --no-deps --test ecu_afr_fuel_rate -- -D warnings` | **PASS** | Generator and scenario assertions. |

### Lib tests matching `ecu` (24)

- `ecu::ambient::tests::{missing_baro_is_none, lower_pressure_is_higher, sea_level_near_zero, isa_round_trip_and_table}`
- `ecu::decode::tests::{j1939_example_five_l_h, no_data_is_none, elm_spaces_and_headers, spn96_na_is_none, pid_5e_zero_is_some_zero, elm_5e_example_five_l_h, pid_temps_throttle_pedal, flex_and_spn174_fuel_temp}`
- `ecu::fuel::tests::{diesel_maf_with_lambda_does_not_use_petrol_147, diesel_maf_without_lambda_is_none, l100km_requires_positive_speed, megasquirt_formula_and_range_skip, missing_level_or_tank_is_none_not_zero, petrol_maf_uses_formulas_density_not_ecu_md_074, blend_stoich_is_mass_fraction, fuel_temp_changes_maf_litres_not_volume_sources, petrol_cold_and_high_load_enrich_without_lambda, fuel_cut_needs_rate_or_lambda_evidence}`
- `ecu::tests::no_live_energy_is_none`
- `routing::rest::truck_multi_day::tests::weekly_rest_after_six_consecutive_working_days` (filter coincidence)

### `ecu_afr_fuel_rate` (20)

`self_test_report_passes`, `elm327_pid_table_ice_only`, `pid_0c_rpm_formula`,
`diesel_without_rate_or_lambda_stays_none`, `petrol_flex_changes_maf_rate`,
`j1939_lfe_illustrative_matches_spn183_scale`, `megasquirt_skips_out_of_range`,
`maf_density_disagrees_with_ecu_md_074`, `no_data_patterns`,
`refine_energy_cost_and_provider`, `write_scenario_results`,
`diesel_fuel_rises_with_load`, `maf_vs_altitude_and_iat_at_wot`,
`same_maf_lambda_same_litres`, `hotter_fuel_more_litres_on_maf_path`,
`e85_uses_more_litres_than_e0`, `colder_coolant_higher_petrol_idle`,
`overrun_fuel_cut_rules`, `diesel_never_uses_naive_as_result`,
`no_nan_or_inf_cells`.

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

Host-side ECU decode tests for this SHA: **43 dedicated tests passed** (23 lib
`ecu::*` + 20 integration), plus the coincidental truck-rest name match.

The APKs under `compiled/` predate the Step 1 decode/fuel changes on this
branch and were not rebuilt for this task.

Petrol without a measured lambda uses an **enrichment estimate** (quality
`Estimate`): cold idle AFR from the band midpoints below +40 C coolant, and
12.8 AFR at >=90 % load. Direct PID 5E / J1939 / MegaSquirt volumes stay
`Measured`. Fuel-cut overrun is `Some(0.0)` when PID 5E is zero or lambda is
lean; cold petrol (coolant below 50 C) keeps injecting unless that evidence
is present.

## How to regenerate

The numeric tables below are rewritten only when the env switch is set.
A normal test run regenerates the section in memory and fails if the
committed file differs.

```bash
NAVI_ECU_WRITE_RESULTS=1 cargo test -p driver-break-core \
  --test ecu_afr_fuel_rate write_scenario_results -- --nocapture
```

<!-- BEGIN GENERATED ECU SCENARIOS -->
The sensor inputs come from a simplified synthetic engine model built on estimated AFR bands; the decode and fuel-rate numbers are computed by Navi's real code; none of this is measured data from a vehicle.

Petrol/ethanol stoichiometric AFR is mixed by **mass fraction** (E10 ~14.10, E85 ~9.82). A volume-linear mix would give E85 ~9.86. Mode 01 PID 24/44 lambda saturates at 65535/32768 (~1.999), so diesel idle/cruise AFR that should be leaner than ~29 is encoded as 29.

### Summary (min / max across generated scenarios)

| Engine | Fuel | min L/h | max L/h | min L/100 km | max L/100 km | worst naive-14.7 error % |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| NA petrol 1.8 L | E0 | 0.000 | 18.670 | 0.00 | 23.34 | 32.0 |
| NA petrol 1.8 L | E10 | 5.660 | 5.660 | 6.29 | 6.29 | 3.6 |
| NA petrol 1.8 L | E70 | 7.245 | 7.245 | 8.05 | 8.05 | 24.7 |
| NA petrol 1.8 L | E85 | 4.311 | 7.787 | 8.65 | 8.65 | 47.2 |
| turbo petrol 1.8 L | E0 | 0.000 | 33.491 | 0.00 | 41.86 | 32.0 |
| turbo petrol 1.8 L | E10 | 7.760 | 7.760 | 8.62 | 8.62 | 3.6 |
| turbo petrol 1.8 L | E85 | 5.912 | 10.676 | 11.86 | 11.86 | 47.2 |
| NA diesel 1.9 L | diesel | 0.000 | 13.672 | 0.00 | 17.09 | 128.5 |
| turbo diesel 1.9 L | diesel | 0.000 | 24.056 | 0.00 | 30.07 | 203.3 |

### NA diesel 1.9 L / combined

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | AFR used | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | naive fixed-14.7 L/h | naive error % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| cold start idle -30 C | 900 | 0 | 17.38 | 2.000 | 29.00 | 0.863 | 2.500 | None (standstill) | MafDerived / LambdaMaf | 5.713 | 128.5 |
| motorway 110 km/h +40 C day | 2800 | 110 | 43.23 | 2.000 | 29.00 | 0.832 | 6.450 | 5.86 | MafDerived / LambdaMaf | 14.211 | 120.3 |
| full load 1500 m at 0 C IAT | 3500 | 80 | 54.98 | 1.200 | 17.40 | 0.832 | 13.672 | 17.09 | MafDerived / LambdaMaf | 18.073 | 32.2 |
| downhill fuel cut, warm | 2200 | 80 | 28.98 | 2.000 | n/a | n/a | 0.0 (fuel cut) | 0.0 (fuel cut) | Pid5E / Measured | 9.526 | n/a |
| downhill in gear, cold engine | 2200 | 80 | 28.98 | 2.000 | n/a | n/a | 0.0 (fuel cut) | 0.0 (fuel cut) | Pid5E / Measured | 9.526 | n/a |

Example chain (this row goes through `decode.rs` then `fuel.rs`):

```
410C0E10
410D00
411006CA
41050A
410F0A
415A14
410433
41430033
413365
4124FFFF
J1939 SPN174 raw=10
decoded MAF="17.38" lambda="2.000" coolant/IAT not printed
fuel_l_h = 17.38 * 3600 / (29.00 * 0.863 * 1000) = 2.500 L/h
```

Combined points reuse the same decode path: idle L/100 km is None (standstill); downhill cut is 0.0 L/h when evidence is present.

### NA diesel 1.9 L / diesel / altitude

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | AFR used | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | naive fixed-14.7 L/h | naive error % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| 0 m | 2500 | 90 | 43.85 | 1.200 | 17.40 | 0.832 | 10.904 | 12.12 | MafDerived / LambdaMaf | 14.414 | 32.2 |
| 500 m | 2500 | 90 | 41.31 | 1.200 | 17.40 | 0.832 | 10.273 | 11.41 | MafDerived / LambdaMaf | 13.580 | 32.2 |
| 1000 m | 2500 | 90 | 38.89 | 1.200 | 17.40 | 0.832 | 9.671 | 10.75 | MafDerived / LambdaMaf | 12.784 | 32.2 |
| 1500 m | 2500 | 90 | 36.59 | 1.200 | 17.40 | 0.832 | 9.099 | 10.11 | MafDerived / LambdaMaf | 12.028 | 32.2 |
| 2000 m | 2500 | 90 | 34.40 | 1.200 | 17.40 | 0.832 | 8.554 | 9.50 | MafDerived / LambdaMaf | 11.308 | 32.2 |
| 2500 m | 2500 | 90 | 32.32 | 1.200 | 17.40 | 0.832 | 8.037 | 8.93 | MafDerived / LambdaMaf | 10.624 | 32.2 |

Example chain (this row goes through `decode.rs` then `fuel.rs`):

```
410C2710
410D5A
41101121
410582
410F3C
415AFF
4104FF
414300FF
413365
4124999A
J1939 SPN174 raw=55
decoded MAF="43.85" lambda="1.200" coolant/IAT not printed
fuel_l_h = 43.85 * 3600 / (17.40 * 0.832 * 1000) = 10.904 L/h
```

From 0 m to 1000 m, decoded fuel rate changes by **11.3%** (model air mass and, for NA diesel, smoke-limit AFR).

### NA diesel 1.9 L / diesel / coolant

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | AFR used | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | naive fixed-14.7 L/h | naive error % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| coolant -30 C | 900 | 0 | 14.41 | 2.000 | 29.00 | 0.832 | 2.150 | None (standstill) | MafDerived / LambdaMaf | 4.737 | 120.3 |
| coolant 0 C | 900 | 0 | 14.41 | 2.000 | 29.00 | 0.832 | 2.150 | None (standstill) | MafDerived / LambdaMaf | 4.737 | 120.3 |
| coolant 20 C | 900 | 0 | 14.41 | 2.000 | 29.00 | 0.832 | 2.150 | None (standstill) | MafDerived / LambdaMaf | 4.737 | 120.3 |
| coolant 40 C | 900 | 0 | 14.41 | 2.000 | 29.00 | 0.832 | 2.150 | None (standstill) | MafDerived / LambdaMaf | 4.737 | 120.3 |
| coolant 90 C | 900 | 0 | 14.41 | 2.000 | 29.00 | 0.832 | 2.150 | None (standstill) | MafDerived / LambdaMaf | 4.737 | 120.3 |

Example chain (this row goes through `decode.rs` then `fuel.rs`):

```
410C0E10
410D00
411005A1
410582
410F3C
415A14
410433
41430033
413365
4124FFFF
J1939 SPN174 raw=55
decoded MAF="14.41" lambda="2.000" coolant/IAT not printed
fuel_l_h = 14.41 * 3600 / (29.00 * 0.832 * 1000) = 2.150 L/h
```

Petrol without a richer lambda still gets a cold-engine estimate AFR below +40 C coolant; diesel L/h follows the modelled (encoded) lambda.

### NA diesel 1.9 L / diesel / fuel temp

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | AFR used | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | naive fixed-14.7 L/h | naive error % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| fuel -30 C | 2500 | 90 | 41.47 | 2.000 | 29.00 | 0.863 | 5.965 | 6.63 | MafDerived / LambdaMaf | 13.632 | 128.5 |
| fuel 0 C | 2500 | 90 | 41.47 | 2.000 | 29.00 | 0.842 | 6.112 | 6.79 | MafDerived / LambdaMaf | 13.632 | 123.1 |
| fuel 15 C | 2500 | 90 | 41.47 | 2.000 | 29.00 | 0.832 | 6.188 | 6.88 | MafDerived / LambdaMaf | 13.632 | 120.3 |
| fuel 40 C | 2500 | 90 | 41.47 | 2.000 | 29.00 | 0.815 | 6.319 | 7.02 | MafDerived / LambdaMaf | 13.632 | 115.7 |
| fuel 60 C | 2500 | 90 | 41.47 | 2.000 | 29.00 | 0.801 | 6.428 | 7.14 | MafDerived / LambdaMaf | 13.632 | 112.1 |

Example chain (this row goes through `decode.rs` then `fuel.rs`):

```
410C2710
410D5A
41101033
410582
410F3C
415A80
410480
41430080
413365
4124FFFF
J1939 SPN174 raw=55
decoded MAF="41.47" lambda="2.000" coolant/IAT not printed
fuel_l_h = 41.47 * 3600 / (29.00 * 0.832 * 1000) = 6.188 L/h
```

MAF path: hotter fuel is less dense, so the same fuel mass is more litres. Volume PIDs would not show this.

### NA diesel 1.9 L / diesel / intake air

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | AFR used | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | naive fixed-14.7 L/h | naive error % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| IAT -30 C | 2500 | 90 | 52.87 | 1.200 | 17.40 | 0.832 | 13.147 | 14.61 | MafDerived / LambdaMaf | 17.380 | 32.2 |
| IAT 0 C | 2500 | 90 | 47.06 | 1.200 | 17.40 | 0.832 | 11.702 | 13.00 | MafDerived / LambdaMaf | 15.470 | 32.2 |
| IAT 20 C | 2500 | 90 | 43.85 | 1.200 | 17.40 | 0.832 | 10.904 | 12.12 | MafDerived / LambdaMaf | 14.414 | 32.2 |
| IAT 40 C | 2500 | 90 | 41.05 | 1.200 | 17.40 | 0.832 | 10.208 | 11.34 | MafDerived / LambdaMaf | 13.494 | 32.2 |

Example chain (this row goes through `decode.rs` then `fuel.rs`):

```
410C2710
410D5A
41101121
410582
410F3C
415AFF
4104FF
414300FF
413365
4124999A
J1939 SPN174 raw=55
decoded MAF="43.85" lambda="1.200" coolant/IAT not printed
fuel_l_h = 43.85 * 3600 / (17.40 * 0.832 * 1000) = 10.904 L/h
```

Decoded MAF falls as intake air warms (ideal gas in the model). With MAF+lambda, L/h tracks that MAF; `fuel.rs` does not apply an extra IAT correction.

### NA diesel 1.9 L / diesel / load

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | AFR used | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | naive fixed-14.7 L/h | naive error % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| load 10% | 2500 | 90 | 39.56 | 2.000 | 29.00 | 0.832 | 5.903 | 6.56 | MafDerived / LambdaMaf | 13.004 | 120.3 |
| load 25% | 2500 | 90 | 40.27 | 2.000 | 29.00 | 0.832 | 6.009 | 6.68 | MafDerived / LambdaMaf | 13.238 | 120.3 |
| load 50% | 2500 | 90 | 41.47 | 2.000 | 29.00 | 0.832 | 6.188 | 6.88 | MafDerived / LambdaMaf | 13.632 | 120.3 |
| load 75% | 2500 | 90 | 42.66 | 2.000 | 29.00 | 0.832 | 6.365 | 7.07 | MafDerived / LambdaMaf | 14.023 | 120.3 |
| load 100% | 2500 | 90 | 43.85 | 1.200 | 17.40 | 0.832 | 10.904 | 12.12 | MafDerived / LambdaMaf | 14.414 | 32.2 |

Example chain (this row goes through `decode.rs` then `fuel.rs`):

```
410C2710
410D5A
41101033
410582
410F3C
415A80
410480
41430080
413365
4124FFFF
J1939 SPN174 raw=55
decoded MAF="41.47" lambda="2.000" coolant/IAT not printed
fuel_l_h = 41.47 * 3600 / (29.00 * 0.832 * 1000) = 6.188 L/h
```

Fuel rate from 10% to 100% load goes **5.903** to **10.904 L/h** at this rpm.

### NA diesel 1.9 L / diesel / throttle

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | AFR used | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | naive fixed-14.7 L/h | naive error % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| throttle 0% (overrun) | 2500 | 90 | 39.46 | 2.000 | n/a | n/a | 0.0 (fuel cut) | 0.0 (fuel cut) | Pid5E / Measured | 12.971 | n/a |
| throttle 10% | 2500 | 90 | 39.56 | 2.000 | 29.00 | 0.832 | 5.903 | 6.56 | MafDerived / LambdaMaf | 13.004 | 120.3 |
| throttle 25% | 2500 | 90 | 40.27 | 2.000 | 29.00 | 0.832 | 6.009 | 6.68 | MafDerived / LambdaMaf | 13.238 | 120.3 |
| throttle 50% | 2500 | 90 | 41.47 | 2.000 | 29.00 | 0.832 | 6.188 | 6.88 | MafDerived / LambdaMaf | 13.632 | 120.3 |
| throttle 75% | 2500 | 90 | 42.66 | 2.000 | 29.00 | 0.832 | 6.365 | 7.07 | MafDerived / LambdaMaf | 14.023 | 120.3 |
| throttle 100% | 2500 | 90 | 43.85 | 1.200 | 17.40 | 0.832 | 10.904 | 12.12 | MafDerived / LambdaMaf | 14.414 | 32.2 |

Example chain (this row goes through `decode.rs` then `fuel.rs`):

```
410C2710
410D5A
41101033
410582
410F3C
415A80
410480
41430080
413365
4124FFFF
J1939 SPN174 raw=55
decoded MAF="41.47" lambda="2.000" coolant/IAT not printed
fuel_l_h = 41.47 * 3600 / (29.00 * 0.832 * 1000) = 6.188 L/h
```

Throttle 0% is overrun: diesels and warm petrol encode PID 5E = 0 (Some(0.0)); cold petrol keeps injecting.

### NA petrol 1.8 L / E0 / altitude

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | AFR used | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | naive fixed-14.7 L/h | naive error % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| 0 m | 2500 | 90 | 39.28 | 0.867 | 12.75 | 0.745 | 14.887 | 16.54 | MafDerived / LambdaMaf | 12.912 | -13.3 |
| 500 m | 2500 | 90 | 37.01 | 0.867 | 12.75 | 0.745 | 14.027 | 15.59 | MafDerived / LambdaMaf | 12.166 | -13.3 |
| 1000 m | 2500 | 90 | 34.84 | 0.867 | 12.75 | 0.745 | 13.204 | 14.67 | MafDerived / LambdaMaf | 11.453 | -13.3 |
| 1500 m | 2500 | 90 | 32.78 | 0.867 | 12.75 | 0.745 | 12.424 | 13.80 | MafDerived / LambdaMaf | 10.776 | -13.3 |
| 2000 m | 2500 | 90 | 30.82 | 0.867 | 12.75 | 0.745 | 11.681 | 12.98 | MafDerived / LambdaMaf | 10.131 | -13.3 |
| 2500 m | 2500 | 90 | 28.95 | 0.867 | 12.75 | 0.745 | 10.972 | 12.19 | MafDerived / LambdaMaf | 9.517 | -13.3 |

Example chain (this row goes through `decode.rs` then `fuel.rs`):

```
410C2710
410D5A
41100F58
410582
410F3C
4111FF
4104FF
414300FF
413365
415200
41246F05
J1939 SPN174 raw=55
decoded MAF="39.28" lambda="0.867" coolant/IAT not printed
fuel_l_h = 39.28 * 3600 / (12.75 * 0.745 * 1000) = 14.887 L/h
```

From 0 m to 1000 m, decoded fuel rate changes by **11.3%** (model air mass and, for NA diesel, smoke-limit AFR).

### NA petrol 1.8 L / E0 / coolant

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | AFR used | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | naive fixed-14.7 L/h | naive error % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| coolant -30 C | 900 | 0 | 3.47 | 0.680 | 10.00 | 0.745 | 1.677 | None (standstill) | MafDerived / LambdaMaf | 1.141 | -32.0 |
| coolant 0 C | 900 | 0 | 3.47 | 0.816 | 12.00 | 0.745 | 1.397 | None (standstill) | MafDerived / LambdaMaf | 1.141 | -18.4 |
| coolant 20 C | 900 | 0 | 3.47 | 0.884 | 13.00 | 0.745 | 1.290 | None (standstill) | MafDerived / LambdaMaf | 1.141 | -11.6 |
| coolant 40 C | 900 | 0 | 3.47 | 0.959 | 14.10 | 0.745 | 1.189 | None (standstill) | MafDerived / LambdaMaf | 1.141 | -4.1 |
| coolant 90 C | 900 | 0 | 3.47 | 1.000 | 14.70 | 0.745 | 1.141 | None (standstill) | MafDerived / LambdaMaf | 1.141 | 0.0 |

Example chain (this row goes through `decode.rs` then `fuel.rs`):

```
410C0E10
410D00
4110015B
410582
410F3C
411114
410433
41430033
413365
415200
41248000
J1939 SPN174 raw=55
decoded MAF="3.47" lambda="1.000" coolant/IAT not printed
fuel_l_h = 3.47 * 3600 / (14.70 * 0.745 * 1000) = 1.141 L/h
```

Petrol without a richer lambda still gets a cold-engine estimate AFR below +40 C coolant; diesel L/h follows the modelled (encoded) lambda.

### NA petrol 1.8 L / E0 / fuel temp

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | AFR used | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | naive fixed-14.7 L/h | naive error % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| fuel -30 C | 2500 | 90 | 16.60 | 1.000 | 14.70 | 0.777 | 5.233 | 5.81 | MafDerived / LambdaMaf | 5.457 | 4.3 |
| fuel 0 C | 2500 | 90 | 16.60 | 1.000 | 14.70 | 0.756 | 5.380 | 5.98 | MafDerived / LambdaMaf | 5.457 | 1.4 |
| fuel 15 C | 2500 | 90 | 16.60 | 1.000 | 14.70 | 0.745 | 5.457 | 6.06 | MafDerived / LambdaMaf | 5.457 | 0.0 |
| fuel 40 C | 2500 | 90 | 16.60 | 1.000 | 14.70 | 0.727 | 5.590 | 6.21 | MafDerived / LambdaMaf | 5.457 | -2.4 |
| fuel 60 C | 2500 | 90 | 16.60 | 1.000 | 14.70 | 0.713 | 5.700 | 6.33 | MafDerived / LambdaMaf | 5.457 | -4.3 |

Example chain (this row goes through `decode.rs` then `fuel.rs`):

```
410C2710
410D5A
4110067C
410582
410F3C
411159
410480
41430080
413365
415200
41248000
J1939 SPN174 raw=55
decoded MAF="16.60" lambda="1.000" coolant/IAT not printed
fuel_l_h = 16.60 * 3600 / (14.70 * 0.745 * 1000) = 5.457 L/h
```

MAF path: hotter fuel is less dense, so the same fuel mass is more litres. Volume PIDs would not show this.

### NA petrol 1.8 L / E0 / intake air

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | AFR used | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | naive fixed-14.7 L/h | naive error % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| IAT -30 C | 2500 | 90 | 47.36 | 0.867 | 12.75 | 0.745 | 17.949 | 19.94 | MafDerived / LambdaMaf | 15.568 | -13.3 |
| IAT 0 C | 2500 | 90 | 42.16 | 0.867 | 12.75 | 0.745 | 15.979 | 17.75 | MafDerived / LambdaMaf | 13.859 | -13.3 |
| IAT 20 C | 2500 | 90 | 39.28 | 0.867 | 12.75 | 0.745 | 14.887 | 16.54 | MafDerived / LambdaMaf | 12.912 | -13.3 |
| IAT 40 C | 2500 | 90 | 36.77 | 0.867 | 12.75 | 0.745 | 13.936 | 15.48 | MafDerived / LambdaMaf | 12.087 | -13.3 |

Example chain (this row goes through `decode.rs` then `fuel.rs`):

```
410C2710
410D5A
41100F58
410582
410F3C
4111FF
4104FF
414300FF
413365
415200
41246F05
J1939 SPN174 raw=55
decoded MAF="39.28" lambda="0.867" coolant/IAT not printed
fuel_l_h = 39.28 * 3600 / (12.75 * 0.745 * 1000) = 14.887 L/h
```

Decoded MAF falls as intake air warms (ideal gas in the model). With MAF+lambda, L/h tracks that MAF; `fuel.rs` does not apply an extra IAT correction.

### NA petrol 1.8 L / E0 / load

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | AFR used | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | naive fixed-14.7 L/h | naive error % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| load 10% | 2500 | 90 | 10.57 | 1.000 | 14.70 | 0.745 | 3.475 | 3.86 | MafDerived / LambdaMaf | 3.475 | 0.0 |
| load 25% | 2500 | 90 | 12.55 | 1.000 | 14.70 | 0.745 | 4.125 | 4.58 | MafDerived / LambdaMaf | 4.125 | 0.0 |
| load 50% | 2500 | 90 | 16.60 | 1.000 | 14.70 | 0.745 | 5.457 | 6.06 | MafDerived / LambdaMaf | 5.457 | 0.0 |
| load 75% | 2500 | 90 | 27.78 | 1.000 | 14.70 | 0.745 | 9.132 | 10.15 | MafDerived / LambdaMaf | 9.132 | 0.0 |
| load 100% | 2500 | 90 | 39.28 | 0.867 | 12.75 | 0.745 | 14.887 | 16.54 | MafDerived / LambdaMaf | 12.912 | -13.3 |

Example chain (this row goes through `decode.rs` then `fuel.rs`):

```
410C2710
410D5A
4110067C
410582
410F3C
411159
410480
41430080
413365
415200
41248000
J1939 SPN174 raw=55
decoded MAF="16.60" lambda="1.000" coolant/IAT not printed
fuel_l_h = 16.60 * 3600 / (14.70 * 0.745 * 1000) = 5.457 L/h
```

Fuel rate from 10% to 100% load goes **3.475** to **14.887 L/h** at this rpm.

### NA petrol 1.8 L / E0 / throttle

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | AFR used | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | naive fixed-14.7 L/h | naive error % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| throttle 0% (overrun) | 2500 | 90 | 7.84 | 1.500 | n/a | n/a | 0.0 (fuel cut) | 0.0 (fuel cut) | Pid5E / Measured | 2.577 | n/a |
| throttle 10% | 2500 | 90 | 10.10 | 1.000 | 14.70 | 0.745 | 3.320 | 3.69 | MafDerived / LambdaMaf | 3.320 | 0.0 |
| throttle 25% | 2500 | 90 | 13.85 | 1.000 | 14.70 | 0.745 | 4.553 | 5.06 | MafDerived / LambdaMaf | 4.553 | 0.0 |
| throttle 50% | 2500 | 90 | 21.09 | 1.000 | 14.70 | 0.745 | 6.933 | 7.70 | MafDerived / LambdaMaf | 6.933 | 0.0 |
| throttle 75% | 2500 | 90 | 29.57 | 1.000 | 14.70 | 0.745 | 9.720 | 10.80 | MafDerived / LambdaMaf | 9.720 | 0.0 |
| throttle 100% | 2500 | 90 | 39.28 | 0.867 | 12.75 | 0.745 | 14.887 | 16.54 | MafDerived / LambdaMaf | 12.912 | -13.3 |

Example chain (this row goes through `decode.rs` then `fuel.rs`):

```
410C2710
410D5A
4110083D
410582
410F3C
411180
410480
41430080
413365
415200
41248000
J1939 SPN174 raw=55
decoded MAF="21.09" lambda="1.000" coolant/IAT not printed
fuel_l_h = 21.09 * 3600 / (14.70 * 0.745 * 1000) = 6.933 L/h
```

Throttle 0% is overrun: diesels and warm petrol encode PID 5E = 0 (Some(0.0)); cold petrol keeps injecting.

### NA petrol 1.8 L / combined

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | AFR used | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | naive fixed-14.7 L/h | naive error % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| cold start idle -30 C | 900 | 0 | 4.18 | 0.680 | 10.00 | 0.777 | 1.937 | None (standstill) | MafDerived / LambdaMaf | 1.374 | -29.1 |
| motorway 110 km/h +40 C day | 2800 | 110 | 17.40 | 1.000 | 14.70 | 0.745 | 5.720 | 5.20 | MafDerived / LambdaMaf | 5.720 | 0.0 |
| full load 1500 m at 0 C IAT | 3500 | 80 | 49.26 | 0.867 | 12.75 | 0.745 | 18.670 | 23.34 | MafDerived / LambdaMaf | 16.193 | -13.3 |
| downhill fuel cut, warm | 2200 | 80 | 6.90 | 1.500 | n/a | n/a | 0.0 (fuel cut) | 0.0 (fuel cut) | Pid5E / Measured | 2.268 | n/a |
| downhill in gear, cold engine | 2200 | 80 | 6.90 | 0.884 | 13.00 | 0.745 | 2.565 | 3.21 | MafDerived / LambdaMaf | 2.268 | -11.6 |
| E85 at -20 C coolant and fuel | 900 | 0 | 6.92 | 0.726 | 7.12 | 0.812 | 4.311 | None (standstill) | MafDerived / LambdaMaf | 2.275 | -47.2 |

Example chain (this row goes through `decode.rs` then `fuel.rs`):

```
410C0E10
410D00
411001A2
41050A
410F0A
411114
410433
41430033
413365
415200
41245713
J1939 SPN174 raw=10
decoded MAF="4.18" lambda="0.680" coolant/IAT not printed
fuel_l_h = 4.18 * 3600 / (10.00 * 0.777 * 1000) = 1.937 L/h
```

Combined points reuse the same decode path: idle L/100 km is None (standstill); downhill cut is 0.0 L/h when evidence is present.

### NA petrol 1.8 L / flex refuel

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | AFR used | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | naive fixed-14.7 L/h | naive error % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| before refuel E10 | 2500 | 90 | 16.60 | 1.000 | 14.10 | 0.749 | 5.660 | 6.29 | MafDerived / LambdaMaf | 5.457 | -3.6 |
| after refuel E70 | 2500 | 90 | 16.60 | 1.000 | 10.64 | 0.776 | 7.245 | 8.05 | MafDerived / LambdaMaf | 5.457 | -24.7 |

Example chain (this row goes through `decode.rs` then `fuel.rs`):

```
410C2710
410D5A
4110067C
410582
410F3C
411159
410480
41430080
413365
41521A
41248000
J1939 SPN174 raw=55
decoded MAF="16.60" lambda="1.000" coolant/IAT not printed
fuel_l_h = 16.60 * 3600 / (14.10 * 0.749 * 1000) = 5.660 L/h
```

Rows share the baseline rpm/speed unless the varied input is that axis.

### NA petrol 1.8 L / fuel type

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | AFR used | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | naive fixed-14.7 L/h | naive error % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| E0 | 2500 | 90 | 16.60 | 1.000 | 14.70 | 0.745 | 5.457 | 6.06 | MafDerived / LambdaMaf | 5.457 | 0.0 |
| E10 | 2500 | 90 | 16.60 | 1.000 | 14.10 | 0.749 | 5.660 | 6.29 | MafDerived / LambdaMaf | 5.457 | -3.6 |
| E85 | 2500 | 90 | 16.60 | 1.000 | 9.81 | 0.782 | 7.787 | 8.65 | MafDerived / LambdaMaf | 5.457 | -29.9 |

Example chain (this row goes through `decode.rs` then `fuel.rs`):

```
410C2710
410D5A
4110067C
410582
410F3C
411159
410480
41430080
413365
415200
41248000
J1939 SPN174 raw=55
decoded MAF="16.60" lambda="1.000" coolant/IAT not printed
fuel_l_h = 16.60 * 3600 / (14.70 * 0.745 * 1000) = 5.457 L/h
```

Same air mass and lambda: E85 uses more litres than E0 because stoich AFR and density both move toward ethanol.

### turbo diesel 1.9 L / combined

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | AFR used | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | naive fixed-14.7 L/h | naive error % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| cold start idle -30 C | 900 | 0 | 20.64 | 2.000 | 29.00 | 0.863 | 2.969 | None (standstill) | MafDerived / LambdaMaf | 6.785 | 128.5 |
| motorway 110 km/h +40 C day | 2800 | 110 | 61.46 | 2.000 | 29.00 | 0.832 | 9.170 | 8.34 | MafDerived / LambdaMaf | 20.203 | 120.3 |
| full load 1500 m at 0 C IAT | 3500 | 80 | 116.75 | 1.448 | 21.00 | 0.832 | 24.056 | 30.07 | MafDerived / LambdaMaf | 38.378 | 59.5 |
| downhill fuel cut, warm | 2200 | 80 | 31.59 | 2.000 | n/a | n/a | 0.0 (fuel cut) | 0.0 (fuel cut) | Pid5E / Measured | 10.384 | n/a |
| downhill in gear, cold engine | 2200 | 80 | 31.59 | 2.000 | n/a | n/a | 0.0 (fuel cut) | 0.0 (fuel cut) | Pid5E / Measured | 10.384 | n/a |
| J1939 LFE truck-style part load | 2500 | 90 | 60.90 | 2.000 | 29.00 | n/a | 6.600 | 7.33 | J1939LfeIllustrative / Measured | 20.019 | 203.3 |

Example chain (this row goes through `decode.rs` then `fuel.rs`):

```
410C0E10
410D00
41100810
41050A
410F0A
415A14
410433
41430033
413365
4124FFFF
J1939 SPN174 raw=10
decoded MAF="20.64" lambda="2.000" coolant/IAT not printed
fuel_l_h = 20.64 * 3600 / (29.00 * 0.863 * 1000) = 2.969 L/h
```

Combined points reuse the same decode path: idle L/100 km is None (standstill); downhill cut is 0.0 L/h when evidence is present.

### turbo diesel 1.9 L / diesel / altitude

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | AFR used | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | naive fixed-14.7 L/h | naive error % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| 0 m | 2500 | 90 | 84.82 | 1.448 | 21.00 | 0.832 | 17.477 | 19.42 | MafDerived / LambdaMaf | 27.882 | 59.5 |
| 500 m | 2500 | 90 | 82.42 | 1.448 | 21.00 | 0.832 | 16.982 | 18.87 | MafDerived / LambdaMaf | 27.093 | 59.5 |
| 1000 m | 2500 | 90 | 80.01 | 1.448 | 21.00 | 0.832 | 16.486 | 18.32 | MafDerived / LambdaMaf | 26.301 | 59.5 |
| 1500 m | 2500 | 90 | 77.70 | 1.448 | 21.00 | 0.832 | 16.010 | 17.79 | MafDerived / LambdaMaf | 25.542 | 59.5 |
| 2000 m | 2500 | 90 | 75.51 | 1.448 | 21.00 | 0.832 | 15.558 | 17.29 | MafDerived / LambdaMaf | 24.822 | 59.5 |
| 2500 m | 2500 | 90 | 73.43 | 1.448 | 21.00 | 0.832 | 15.130 | 16.81 | MafDerived / LambdaMaf | 24.138 | 59.5 |

Example chain (this row goes through `decode.rs` then `fuel.rs`):

```
410C2710
410D5A
41102122
410582
410F3C
415AFF
4104FF
414300FF
413365
4124B961
J1939 SPN174 raw=55
decoded MAF="84.82" lambda="1.448" coolant/IAT not printed
fuel_l_h = 84.82 * 3600 / (21.00 * 0.832 * 1000) = 17.477 L/h
```

From 0 m to 1000 m, decoded fuel rate changes by **5.7%** (model air mass and, for NA diesel, smoke-limit AFR).

### turbo diesel 1.9 L / diesel / coolant

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | AFR used | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | naive fixed-14.7 L/h | naive error % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| coolant -30 C | 900 | 0 | 17.12 | 2.000 | 29.00 | 0.832 | 2.554 | None (standstill) | MafDerived / LambdaMaf | 5.628 | 120.3 |
| coolant 0 C | 900 | 0 | 17.12 | 2.000 | 29.00 | 0.832 | 2.554 | None (standstill) | MafDerived / LambdaMaf | 5.628 | 120.3 |
| coolant 20 C | 900 | 0 | 17.12 | 2.000 | 29.00 | 0.832 | 2.554 | None (standstill) | MafDerived / LambdaMaf | 5.628 | 120.3 |
| coolant 40 C | 900 | 0 | 17.12 | 2.000 | 29.00 | 0.832 | 2.554 | None (standstill) | MafDerived / LambdaMaf | 5.628 | 120.3 |
| coolant 90 C | 900 | 0 | 17.12 | 2.000 | 29.00 | 0.832 | 2.554 | None (standstill) | MafDerived / LambdaMaf | 5.628 | 120.3 |

Example chain (this row goes through `decode.rs` then `fuel.rs`):

```
410C0E10
410D00
411006B0
410582
410F3C
415A14
410433
41430033
413365
4124FFFF
J1939 SPN174 raw=55
decoded MAF="17.12" lambda="2.000" coolant/IAT not printed
fuel_l_h = 17.12 * 3600 / (29.00 * 0.832 * 1000) = 2.554 L/h
```

Petrol without a richer lambda still gets a cold-engine estimate AFR below +40 C coolant; diesel L/h follows the modelled (encoded) lambda.

### turbo diesel 1.9 L / diesel / fuel temp

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | AFR used | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | naive fixed-14.7 L/h | naive error % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| fuel -30 C | 2500 | 90 | 60.90 | 2.000 | 29.00 | 0.863 | 8.760 | 9.73 | MafDerived / LambdaMaf | 20.019 | 128.5 |
| fuel 0 C | 2500 | 90 | 60.90 | 2.000 | 29.00 | 0.842 | 8.975 | 9.97 | MafDerived / LambdaMaf | 20.019 | 123.1 |
| fuel 15 C | 2500 | 90 | 60.90 | 2.000 | 29.00 | 0.832 | 9.087 | 10.10 | MafDerived / LambdaMaf | 20.019 | 120.3 |
| fuel 40 C | 2500 | 90 | 60.90 | 2.000 | 29.00 | 0.815 | 9.279 | 10.31 | MafDerived / LambdaMaf | 20.019 | 115.7 |
| fuel 60 C | 2500 | 90 | 60.90 | 2.000 | 29.00 | 0.801 | 9.439 | 10.49 | MafDerived / LambdaMaf | 20.019 | 112.1 |

Example chain (this row goes through `decode.rs` then `fuel.rs`):

```
410C2710
410D5A
411017CA
410582
410F3C
415A80
410480
41430080
413365
4124FFFF
J1939 SPN174 raw=55
decoded MAF="60.90" lambda="2.000" coolant/IAT not printed
fuel_l_h = 60.90 * 3600 / (29.00 * 0.832 * 1000) = 9.087 L/h
```

MAF path: hotter fuel is less dense, so the same fuel mass is more litres. Volume PIDs would not show this.

### turbo diesel 1.9 L / diesel / intake air

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | AFR used | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | naive fixed-14.7 L/h | naive error % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| IAT -30 C | 2500 | 90 | 102.26 | 1.448 | 21.00 | 0.832 | 21.070 | 23.41 | MafDerived / LambdaMaf | 33.615 | 59.5 |
| IAT 0 C | 2500 | 90 | 91.03 | 1.448 | 21.00 | 0.832 | 18.756 | 20.84 | MafDerived / LambdaMaf | 29.924 | 59.5 |
| IAT 20 C | 2500 | 90 | 84.82 | 1.448 | 21.00 | 0.832 | 17.477 | 19.42 | MafDerived / LambdaMaf | 27.882 | 59.5 |
| IAT 40 C | 2500 | 90 | 79.40 | 1.448 | 21.00 | 0.832 | 16.360 | 18.18 | MafDerived / LambdaMaf | 26.101 | 59.5 |

Example chain (this row goes through `decode.rs` then `fuel.rs`):

```
410C2710
410D5A
41102122
410582
410F3C
415AFF
4104FF
414300FF
413365
4124B961
J1939 SPN174 raw=55
decoded MAF="84.82" lambda="1.448" coolant/IAT not printed
fuel_l_h = 84.82 * 3600 / (21.00 * 0.832 * 1000) = 17.477 L/h
```

Decoded MAF falls as intake air warms (ideal gas in the model). With MAF+lambda, L/h tracks that MAF; `fuel.rs` does not apply an extra IAT correction.

### turbo diesel 1.9 L / diesel / load

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | AFR used | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | naive fixed-14.7 L/h | naive error % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| load 10% | 2500 | 90 | 43.27 | 2.000 | 29.00 | 0.832 | 6.456 | 7.17 | MafDerived / LambdaMaf | 14.224 | 120.3 |
| load 25% | 2500 | 90 | 49.71 | 2.000 | 29.00 | 0.832 | 7.417 | 8.24 | MafDerived / LambdaMaf | 16.341 | 120.3 |
| load 50% | 2500 | 90 | 60.90 | 2.000 | 29.00 | 0.832 | 9.087 | 10.10 | MafDerived / LambdaMaf | 20.019 | 120.3 |
| load 75% | 2500 | 90 | 72.65 | 2.000 | 29.00 | 0.832 | 10.840 | 12.04 | MafDerived / LambdaMaf | 23.882 | 120.3 |
| load 100% | 2500 | 90 | 84.82 | 1.448 | 21.00 | 0.832 | 17.477 | 19.42 | MafDerived / LambdaMaf | 27.882 | 59.5 |

Example chain (this row goes through `decode.rs` then `fuel.rs`):

```
410C2710
410D5A
411017CA
410582
410F3C
415A80
410480
41430080
413365
4124FFFF
J1939 SPN174 raw=55
decoded MAF="60.90" lambda="2.000" coolant/IAT not printed
fuel_l_h = 60.90 * 3600 / (29.00 * 0.832 * 1000) = 9.087 L/h
```

Fuel rate from 10% to 100% load goes **6.456** to **17.477 L/h** at this rpm.

### turbo diesel 1.9 L / diesel / throttle

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | AFR used | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | naive fixed-14.7 L/h | naive error % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| throttle 0% (overrun) | 2500 | 90 | 42.42 | 2.000 | n/a | n/a | 0.0 (fuel cut) | 0.0 (fuel cut) | Pid5E / Measured | 13.944 | n/a |
| throttle 10% | 2500 | 90 | 43.27 | 2.000 | 29.00 | 0.832 | 6.456 | 7.17 | MafDerived / LambdaMaf | 14.224 | 120.3 |
| throttle 25% | 2500 | 90 | 49.71 | 2.000 | 29.00 | 0.832 | 7.417 | 8.24 | MafDerived / LambdaMaf | 16.341 | 120.3 |
| throttle 50% | 2500 | 90 | 60.90 | 2.000 | 29.00 | 0.832 | 9.087 | 10.10 | MafDerived / LambdaMaf | 20.019 | 120.3 |
| throttle 75% | 2500 | 90 | 72.65 | 2.000 | 29.00 | 0.832 | 10.840 | 12.04 | MafDerived / LambdaMaf | 23.882 | 120.3 |
| throttle 100% | 2500 | 90 | 84.82 | 1.448 | 21.00 | 0.832 | 17.477 | 19.42 | MafDerived / LambdaMaf | 27.882 | 59.5 |

Example chain (this row goes through `decode.rs` then `fuel.rs`):

```
410C2710
410D5A
411017CA
410582
410F3C
415A80
410480
41430080
413365
4124FFFF
J1939 SPN174 raw=55
decoded MAF="60.90" lambda="2.000" coolant/IAT not printed
fuel_l_h = 60.90 * 3600 / (29.00 * 0.832 * 1000) = 9.087 L/h
```

Throttle 0% is overrun: diesels and warm petrol encode PID 5E = 0 (Some(0.0)); cold petrol keeps injecting.

### turbo petrol 1.8 L / E0 / altitude

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | AFR used | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | naive fixed-14.7 L/h | naive error % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| 0 m | 2500 | 90 | 62.42 | 0.827 | 12.15 | 0.745 | 24.825 | 27.58 | MafDerived / LambdaMaf | 20.519 | -17.3 |
| 500 m | 2500 | 90 | 60.27 | 0.827 | 12.15 | 0.745 | 23.970 | 26.63 | MafDerived / LambdaMaf | 19.812 | -17.3 |
| 1000 m | 2500 | 90 | 58.11 | 0.827 | 12.15 | 0.745 | 23.111 | 25.68 | MafDerived / LambdaMaf | 19.102 | -17.3 |
| 1500 m | 2500 | 90 | 56.04 | 0.827 | 12.15 | 0.745 | 22.288 | 24.76 | MafDerived / LambdaMaf | 18.422 | -17.3 |
| 2000 m | 2500 | 90 | 54.08 | 0.827 | 12.15 | 0.745 | 21.508 | 23.90 | MafDerived / LambdaMaf | 17.777 | -17.3 |
| 2500 m | 2500 | 90 | 52.22 | 0.827 | 12.15 | 0.745 | 20.768 | 23.08 | MafDerived / LambdaMaf | 17.166 | -17.3 |

Example chain (this row goes through `decode.rs` then `fuel.rs`):

```
410C2710
410D5A
41101862
410582
410F3C
4111FF
4104FF
414300FF
413365
415200
412469CC
J1939 SPN174 raw=55
decoded MAF="62.42" lambda="0.827" coolant/IAT not printed
fuel_l_h = 62.42 * 3600 / (12.15 * 0.745 * 1000) = 24.825 L/h
```

From 0 m to 1000 m, decoded fuel rate changes by **6.9%** (model air mass and, for NA diesel, smoke-limit AFR).

### turbo petrol 1.8 L / E0 / coolant

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | AFR used | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | naive fixed-14.7 L/h | naive error % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| coolant -30 C | 900 | 0 | 3.91 | 0.680 | 10.00 | 0.745 | 1.889 | None (standstill) | MafDerived / LambdaMaf | 1.285 | -32.0 |
| coolant 0 C | 900 | 0 | 3.91 | 0.816 | 12.00 | 0.745 | 1.575 | None (standstill) | MafDerived / LambdaMaf | 1.285 | -18.4 |
| coolant 20 C | 900 | 0 | 3.91 | 0.884 | 13.00 | 0.745 | 1.453 | None (standstill) | MafDerived / LambdaMaf | 1.285 | -11.6 |
| coolant 40 C | 900 | 0 | 3.91 | 0.959 | 14.10 | 0.745 | 1.340 | None (standstill) | MafDerived / LambdaMaf | 1.285 | -4.1 |
| coolant 90 C | 900 | 0 | 3.91 | 1.000 | 14.70 | 0.745 | 1.285 | None (standstill) | MafDerived / LambdaMaf | 1.285 | 0.0 |

Example chain (this row goes through `decode.rs` then `fuel.rs`):

```
410C0E10
410D00
41100187
410582
410F3C
411114
410433
41430033
413365
415200
41248000
J1939 SPN174 raw=55
decoded MAF="3.91" lambda="1.000" coolant/IAT not printed
fuel_l_h = 3.91 * 3600 / (14.70 * 0.745 * 1000) = 1.285 L/h
```

Petrol without a richer lambda still gets a cold-engine estimate AFR below +40 C coolant; diesel L/h follows the modelled (encoded) lambda.

### turbo petrol 1.8 L / E0 / fuel temp

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | AFR used | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | naive fixed-14.7 L/h | naive error % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| fuel -30 C | 2500 | 90 | 22.76 | 1.000 | 14.70 | 0.777 | 7.175 | 7.97 | MafDerived / LambdaMaf | 7.482 | 4.3 |
| fuel 0 C | 2500 | 90 | 22.76 | 1.000 | 14.70 | 0.756 | 7.377 | 8.20 | MafDerived / LambdaMaf | 7.482 | 1.4 |
| fuel 15 C | 2500 | 90 | 22.76 | 1.000 | 14.70 | 0.745 | 7.482 | 8.31 | MafDerived / LambdaMaf | 7.482 | 0.0 |
| fuel 40 C | 2500 | 90 | 22.76 | 1.000 | 14.70 | 0.727 | 7.664 | 8.52 | MafDerived / LambdaMaf | 7.482 | -2.4 |
| fuel 60 C | 2500 | 90 | 22.76 | 1.000 | 14.70 | 0.713 | 7.816 | 8.68 | MafDerived / LambdaMaf | 7.482 | -4.3 |

Example chain (this row goes through `decode.rs` then `fuel.rs`):

```
410C2710
410D5A
411008E4
410582
410F3C
411159
410480
41430080
413365
415200
41248000
J1939 SPN174 raw=55
decoded MAF="22.76" lambda="1.000" coolant/IAT not printed
fuel_l_h = 22.76 * 3600 / (14.70 * 0.745 * 1000) = 7.482 L/h
```

MAF path: hotter fuel is less dense, so the same fuel mass is more litres. Volume PIDs would not show this.

### turbo petrol 1.8 L / E0 / intake air

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | AFR used | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | naive fixed-14.7 L/h | naive error % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| IAT -30 C | 2500 | 90 | 75.25 | 0.827 | 12.15 | 0.745 | 29.928 | 33.25 | MafDerived / LambdaMaf | 24.736 | -17.3 |
| IAT 0 C | 2500 | 90 | 66.99 | 0.827 | 12.15 | 0.745 | 26.643 | 29.60 | MafDerived / LambdaMaf | 22.021 | -17.3 |
| IAT 20 C | 2500 | 90 | 62.42 | 0.827 | 12.15 | 0.745 | 24.825 | 27.58 | MafDerived / LambdaMaf | 20.519 | -17.3 |
| IAT 40 C | 2500 | 90 | 58.43 | 0.827 | 12.15 | 0.745 | 23.238 | 25.82 | MafDerived / LambdaMaf | 19.207 | -17.3 |

Example chain (this row goes through `decode.rs` then `fuel.rs`):

```
410C2710
410D5A
41101862
410582
410F3C
4111FF
4104FF
414300FF
413365
415200
412469CC
J1939 SPN174 raw=55
decoded MAF="62.42" lambda="0.827" coolant/IAT not printed
fuel_l_h = 62.42 * 3600 / (12.15 * 0.745 * 1000) = 24.825 L/h
```

Decoded MAF falls as intake air warms (ideal gas in the model). With MAF+lambda, L/h tracks that MAF; `fuel.rs` does not apply an extra IAT correction.

### turbo petrol 1.8 L / E0 / load

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | AFR used | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | naive fixed-14.7 L/h | naive error % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| load 10% | 2500 | 90 | 12.45 | 1.000 | 14.70 | 0.745 | 4.093 | 4.55 | MafDerived / LambdaMaf | 4.093 | 0.0 |
| load 25% | 2500 | 90 | 15.81 | 1.000 | 14.70 | 0.745 | 5.197 | 5.77 | MafDerived / LambdaMaf | 5.197 | 0.0 |
| load 50% | 2500 | 90 | 22.76 | 1.000 | 14.70 | 0.745 | 7.482 | 8.31 | MafDerived / LambdaMaf | 7.482 | 0.0 |
| load 75% | 2500 | 90 | 42.18 | 1.000 | 14.70 | 0.745 | 13.865 | 15.41 | MafDerived / LambdaMaf | 13.865 | 0.0 |
| load 100% | 2500 | 90 | 62.42 | 0.827 | 12.15 | 0.745 | 24.825 | 27.58 | MafDerived / LambdaMaf | 20.519 | -17.3 |

Example chain (this row goes through `decode.rs` then `fuel.rs`):

```
410C2710
410D5A
411008E4
410582
410F3C
411159
410480
41430080
413365
415200
41248000
J1939 SPN174 raw=55
decoded MAF="22.76" lambda="1.000" coolant/IAT not printed
fuel_l_h = 22.76 * 3600 / (14.70 * 0.745 * 1000) = 7.482 L/h
```

Fuel rate from 10% to 100% load goes **4.093** to **24.825 L/h** at this rpm.

### turbo petrol 1.8 L / E0 / throttle

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | AFR used | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | naive fixed-14.7 L/h | naive error % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| throttle 0% (overrun) | 2500 | 90 | 7.84 | 1.500 | n/a | n/a | 0.0 (fuel cut) | 0.0 (fuel cut) | Pid5E / Measured | 2.577 | n/a |
| throttle 10% | 2500 | 90 | 11.65 | 1.000 | 14.70 | 0.745 | 3.830 | 4.26 | MafDerived / LambdaMaf | 3.830 | 0.0 |
| throttle 25% | 2500 | 90 | 18.04 | 1.000 | 14.70 | 0.745 | 5.930 | 6.59 | MafDerived / LambdaMaf | 5.930 | 0.0 |
| throttle 50% | 2500 | 90 | 30.53 | 1.000 | 14.70 | 0.745 | 10.036 | 11.15 | MafDerived / LambdaMaf | 10.036 | 0.0 |
| throttle 75% | 2500 | 90 | 45.33 | 1.000 | 14.70 | 0.745 | 14.901 | 16.56 | MafDerived / LambdaMaf | 14.901 | 0.0 |
| throttle 100% | 2500 | 90 | 62.42 | 0.827 | 12.15 | 0.745 | 24.825 | 27.58 | MafDerived / LambdaMaf | 20.519 | -17.3 |

Example chain (this row goes through `decode.rs` then `fuel.rs`):

```
410C2710
410D5A
41100BED
410582
410F3C
411180
410480
41430080
413365
415200
41248000
J1939 SPN174 raw=55
decoded MAF="30.53" lambda="1.000" coolant/IAT not printed
fuel_l_h = 30.53 * 3600 / (14.70 * 0.745 * 1000) = 10.036 L/h
```

Throttle 0% is overrun: diesels and warm petrol encode PID 5E = 0 (Some(0.0)); cold petrol keeps injecting.

### turbo petrol 1.8 L / combined

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | AFR used | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | naive fixed-14.7 L/h | naive error % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| cold start idle -30 C | 900 | 0 | 4.71 | 0.680 | 10.00 | 0.777 | 2.183 | None (standstill) | MafDerived / LambdaMaf | 1.548 | -29.1 |
| motorway 110 km/h +40 C day | 2800 | 110 | 23.86 | 1.000 | 14.70 | 0.745 | 7.843 | 7.13 | MafDerived / LambdaMaf | 7.843 | 0.0 |
| full load 1500 m at 0 C IAT | 3500 | 80 | 84.21 | 0.827 | 12.15 | 0.745 | 33.491 | 41.86 | MafDerived / LambdaMaf | 27.682 | -17.3 |
| downhill fuel cut, warm | 2200 | 80 | 6.90 | 1.500 | n/a | n/a | 0.0 (fuel cut) | 0.0 (fuel cut) | Pid5E / Measured | 2.268 | n/a |
| downhill in gear, cold engine | 2200 | 80 | 6.90 | 0.884 | 13.00 | 0.745 | 2.565 | 3.21 | MafDerived / LambdaMaf | 2.268 | -11.6 |
| E85 at -20 C coolant and fuel | 900 | 0 | 9.49 | 0.726 | 7.12 | 0.812 | 5.912 | None (standstill) | MafDerived / LambdaMaf | 3.120 | -47.2 |

Example chain (this row goes through `decode.rs` then `fuel.rs`):

```
410C0E10
410D00
411001D7
41050A
410F0A
411114
410433
41430033
413365
415200
41245713
J1939 SPN174 raw=10
decoded MAF="4.71" lambda="0.680" coolant/IAT not printed
fuel_l_h = 4.71 * 3600 / (10.00 * 0.777 * 1000) = 2.183 L/h
```

Combined points reuse the same decode path: idle L/100 km is None (standstill); downhill cut is 0.0 L/h when evidence is present.

### turbo petrol 1.8 L / fuel type

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | AFR used | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | naive fixed-14.7 L/h | naive error % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| E0 | 2500 | 90 | 22.76 | 1.000 | 14.70 | 0.745 | 7.482 | 8.31 | MafDerived / LambdaMaf | 7.482 | 0.0 |
| E10 | 2500 | 90 | 22.76 | 1.000 | 14.10 | 0.749 | 7.760 | 8.62 | MafDerived / LambdaMaf | 7.482 | -3.6 |
| E85 | 2500 | 90 | 22.76 | 1.000 | 9.81 | 0.782 | 10.676 | 11.86 | MafDerived / LambdaMaf | 7.482 | -29.9 |

Example chain (this row goes through `decode.rs` then `fuel.rs`):

```
410C2710
410D5A
411008E4
410582
410F3C
411159
410480
41430080
413365
415200
41248000
J1939 SPN174 raw=55
decoded MAF="22.76" lambda="1.000" coolant/IAT not printed
fuel_l_h = 22.76 * 3600 / (14.70 * 0.745 * 1000) = 7.482 L/h
```

Same air mass and lambda: E85 uses more litres than E0 because stoich AFR and density both move toward ethanol.

### Inputs with no effect on L/h (when MAF + lambda are both present)

- **Intake air temperature:** the MAF reading is already a mass flow. The same decoded MAF and lambda give the same L/h at -30 C and +40 C IAT; IAT only changes L/h because it changes the modelled (then encoded) MAF.
- **Altitude / barometric pressure:** same rule. MAF is not pressure-corrected in `fuel.rs`. Altitude changes L/h only by changing the encoded MAF (ideal-gas air mass) and, for NA diesel, the smoke-limited AFR.
- **Throttle vs pedal:** diesels have no throttle plate; PID 5A/49 is recorded and does not enter the MAF formula.
- **Diesel coolant at idle:** the model AFR is 35-100, but PID 24 cannot encode lambda above ~2.0, so decoded diesel idle fuel rate does not move with coolant.
<!-- END GENERATED ECU SCENARIOS -->
