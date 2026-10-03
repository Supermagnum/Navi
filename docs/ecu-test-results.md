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
| Commit | `329aef46441f0611f92b4ba56cf60d2da33a4a1f` (`329aef46`) |
| Host | Linux x86_64, rustc 1.98.0 (88d9e12ae 2026-08-18) |
| Scope | Pure decode / fuel-rate math; `DATA_SOURCE=none`; synthetic engine model in tests only |

## Host Rust (this run)

| Command | Result | Notes |
|---|---|---|
| `cargo test -p driver-break-core --lib ecu` | **PASS** — 27 passed, 0 failed, 0 ignored, 601 filtered | Name filter `ecu` also runs `weekly_rest_after_six_consecutive_working_days`. The other 26 are `ecu::*` unit tests. |
| `cargo test -p driver-break-core --test ecu_afr_fuel_rate` | **PASS** — 23 passed, 0 failed, 0 ignored | Golden suite plus scenario generator/assertions. |
| `cargo test -p driver-break-core ecu` (no `--lib`) | **Did not complete** | Unrelated `lld` bus error on other integration binaries. Use `--lib` and `--test ecu_afr_fuel_rate`. |
| `rustfmt --edition 2021 core/src/ecu/*.rs core/tests/ecu_afr_fuel_rate.rs core/tests/helpers/ecu_engine_model.rs` | **PASS** | ECU sources and generator only. |
| `cargo clippy -p driver-break-core --no-deps --lib -- -D warnings` | **PASS** | Touched `core/src/ecu` files included. |
| `cargo clippy -p driver-break-core --no-deps --test ecu_afr_fuel_rate -- -D warnings` | **PASS** | Generator and scenario assertions. |

### Lib tests matching `ecu` (27)

- `ecu::ambient::tests::{missing_baro_is_none, lower_pressure_is_higher, sea_level_near_zero, isa_round_trip_and_table}`
- `ecu::decode::tests::{j1939_example_five_l_h, no_data_is_none, elm_spaces_and_headers, spn96_na_is_none, pid_5e_zero_is_some_zero, elm_5e_example_five_l_h, pid_temps_throttle_pedal, flex_and_spn174_fuel_temp, pid4f_scales_lambda_and_no_data, pid_torque_and_diesel_rejects_saturated_maf}`
- `ecu::fuel::tests::{diesel_maf_with_lambda_does_not_use_petrol_147, diesel_maf_without_lambda_is_none, l100km_requires_positive_speed, megasquirt_formula_and_range_skip, missing_level_or_tank_is_none_not_zero, petrol_maf_uses_formulas_density_not_ecu_md_074, blend_stoich_is_mass_fraction, fuel_temp_changes_maf_litres_not_volume_sources, petrol_cold_and_high_load_enrich_without_lambda, fuel_cut_needs_rate_or_lambda_evidence, torque_check_vector_30pct_210nm_2000rpm}`
- `ecu::tests::no_live_energy_is_none`
- `routing::rest::truck_multi_day::tests::weekly_rest_after_six_consecutive_working_days` (filter coincidence)

### `ecu_afr_fuel_rate` (23)

Previous suite plus `diesel_never_uses_saturated_lambda_as_afr`,
`diesel_idle_and_cruise_torque_error_bound` (Navi vs true within 20 % on the
torque path), `diesel_coolant_sweep_not_flat`.

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
| `adb logcat -s NaviEcu` on a device or emulator | None captured (`adb devices` empty). Commands below. |
| Android instrumented `EcuAfrSelfTestInstrumentedTest` | Not re-run |
| `./gradlew :app:ktlintCheck` | Not re-run |
| UniFFI `ecu_afr_self_test` / `ecuAfrSelfTest()` on device | Not re-run (Rust `self_test_report_passes` covers the same golden report) |
| Workspace `cargo fmt --all -- --check` | Not run |
| Workspace Clippy | Not run |

Host-side ECU decode tests for this SHA: **49 dedicated tests passed** (26 lib
`ecu::*` + 23 integration), plus the coincidental truck-rest name match.

`compiled/` APKs on this branch were rebuilt for `arm64-v8a` and `x86_64`
(`./scripts/build-android-native.sh all release`, then
`:app:assembleRelease` / `:app:assembleDebug`). The release APK is unsigned
(no upload keystore on this host). No device or emulator was attached.

On a phone or emulator:

```bash
adb install -r compiled/navi-debug.apk
adb logcat -c
adb shell am start -n no.navi.app/.MainActivity
# After opening Tools / running ecuAfrSelfTest():
adb logcat -s NaviEcu
```

**Lambda / fuel-cut:** PID 24/34/44 are decoded as SAE J1979 lambda (lean > 1),
not phi. A value within 1 % of the PID 4F maximum (default 2) is saturated and
is never used as AFR. Diesel then uses torque x BSFC. Fuel cut is PID 5E = 0
or actual torque <= 0 while moving; saturated lambda alone is not cut. Petrol
overrun may corroborate with saturated lambda, closed throttle, and coolant
>= 50 C.

## Known limits

- PID 24/34/44 still cannot report diesel idle AFR of 35-100; the cap is a
  flag, not a measurement. Vehicles without PID 62/63 (and without 5E) get
  `None` on diesel idle/cruise.
- Torque x BSFC uses 230 g/kWh diesel / 280 g/kWh petrol, widened below 25 %
  torque, plus a small idle intercept. It is an estimate. Scenario tables
  stay within 20 % of the model's true MAF/AFR rate on that path because the
  encoder inverts the same function (leftover is PID 62 1 % quantization).
- Nothing in these tables is measured on a vehicle.

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

Petrol/ethanol stoichiometric AFR is mixed by **mass fraction** (E10 ~14.10, E85 ~9.82). PID 24/34/44 report SAE J1979 **lambda** (AFR/AFRstoich, lean greater than 1), despite the standard's 'equivalence ratio' name. Default maximum is 2 (PID 4F byte A = 0). A reading within 1 % of that cap is saturated and is not used as AFR. Diesel idle/cruise then use torque x BSFC instead of the cap AFR.

### Summary (min / max across generated scenarios)

| Engine | Fuel | min L/h | max L/h | min L/100 km | max L/100 km | worst naive-14.7 error % |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| NA petrol 1.8 L | E0 | 0.000 | 18.670 | 0.00 | 23.34 | 31.9 |
| NA petrol 1.8 L | E10 | 5.660 | 5.660 | 6.29 | 6.29 | 0.6 |
| NA petrol 1.8 L | E70 | 7.245 | 7.245 | 8.05 | 8.05 | 4.1 |
| NA petrol 1.8 L | E85 | 4.311 | 7.787 | 8.65 | 8.65 | 20.9 |
| turbo petrol 1.8 L | E0 | 0.000 | 33.491 | 0.00 | 41.86 | 31.9 |
| turbo petrol 1.8 L | E10 | 7.760 | 7.760 | 8.62 | 8.62 | 0.6 |
| turbo petrol 1.8 L | E85 | 5.912 | 10.676 | 11.86 | 11.86 | 20.9 |
| NA diesel 1.9 L | diesel | 0.000 | 13.672 | 0.00 | 17.09 | 507.6 |
| turbo diesel 1.9 L | diesel | 0.000 | 24.056 | 0.00 | 30.07 | 507.9 |

### NA diesel 1.9 L / combined

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | true AFR (model) | AFR used | lambda sat | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | Navi vs true % | naive fixed-14.7 L/h | naive vs true % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: |
| cold start idle -30 C | 900 | 0 | 17.38 | 2.000 | 42.50 | n/a | yes | 0.863 | 1.711 | None (standstill) | TorqueBsfc / TorqueEstimate | 0.3 | 5.713 | 235.0 |
| motorway 110 km/h +40 C day | 2800 | 110 | 43.23 | 2.000 | 36.50 | n/a | yes | 0.832 | 5.087 | 4.62 | TorqueBsfc / TorqueEstimate | -0.7 | 14.211 | 177.3 |
| full load 1500 m at 0 C IAT | 3500 | 80 | 54.98 | 1.200 | 17.40 | 17.40 | no | 0.832 | 13.672 | 17.09 | MafDerived / LambdaMaf | -0.0 | 18.073 | 32.2 |
| downhill fuel cut, warm | 2200 | 80 | 28.98 | 2.000 | n/a | n/a | yes | n/a | 0.0 (fuel cut) | 0.0 (fuel cut) | Pid5E / Measured | 0.0 | 9.526 | n/a |
| downhill in gear, cold engine | 2200 | 80 | 28.98 | 2.000 | n/a | n/a | yes | n/a | 0.0 (fuel cut) | 0.0 (fuel cut) | Pid5E / Measured | 0.0 | 9.526 | n/a |

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
414F00000000
416198
416298
416300D2
J1939 SPN174 raw=10
decoded MAF="17.38" lambda="2.000" coolant/IAT not printed
torque path: power_kW = torque_pct/100 * Tref * rpm * 2*pi/60 / 1000; fuel_g_h = power * BSFC + idle; L/h = fuel_g_h / (rho*1000)
```

Combined points reuse the same decode path: idle L/100 km is None (standstill); downhill cut is 0.0 L/h when evidence is present.

### NA diesel 1.9 L / diesel / altitude

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | true AFR (model) | AFR used | lambda sat | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | Navi vs true % | naive fixed-14.7 L/h | naive vs true % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: |
| 0 m | 2500 | 90 | 43.85 | 1.200 | 17.40 | 17.40 | no | 0.832 | 10.904 | 12.12 | MafDerived / LambdaMaf | 0.0 | 14.414 | 32.2 |
| 500 m | 2500 | 90 | 41.31 | 1.200 | 17.40 | 17.40 | no | 0.832 | 10.273 | 11.41 | MafDerived / LambdaMaf | -0.0 | 13.580 | 32.2 |
| 1000 m | 2500 | 90 | 38.89 | 1.200 | 17.40 | 17.40 | no | 0.832 | 9.671 | 10.75 | MafDerived / LambdaMaf | -0.0 | 12.784 | 32.2 |
| 1500 m | 2500 | 90 | 36.59 | 1.200 | 17.40 | 17.40 | no | 0.832 | 9.099 | 10.11 | MafDerived / LambdaMaf | -0.0 | 12.028 | 32.2 |
| 2000 m | 2500 | 90 | 34.40 | 1.200 | 17.40 | 17.40 | no | 0.832 | 8.554 | 9.50 | MafDerived / LambdaMaf | -0.0 | 11.308 | 32.2 |
| 2500 m | 2500 | 90 | 32.32 | 1.200 | 17.40 | 17.40 | no | 0.832 | 8.037 | 8.93 | MafDerived / LambdaMaf | 0.0 | 10.624 | 32.2 |

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
414F00000000
4161C0
4162C0
416300D2
J1939 SPN174 raw=55
decoded MAF="43.85" lambda="1.200" coolant/IAT not printed
fuel_l_h = 43.85 * 3600 / (17.40 * 0.832 * 1000) = 10.904 L/h
```

From 0 m to 1000 m, decoded fuel rate changes by **11.3%** (model air mass and, for NA diesel, smoke-limit AFR).

### NA diesel 1.9 L / diesel / coolant

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | true AFR (model) | AFR used | lambda sat | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | Navi vs true % | naive fixed-14.7 L/h | naive vs true % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: |
| coolant -30 C | 900 | 0 | 14.41 | 2.000 | 42.50 | n/a | yes | 0.832 | 1.462 | None (standstill) | TorqueBsfc / TorqueEstimate | -0.4 | 4.737 | 222.8 |
| coolant 0 C | 900 | 0 | 14.41 | 2.000 | 60.00 | n/a | yes | 0.832 | 1.068 | None (standstill) | TorqueBsfc / TorqueEstimate | 2.7 | 4.737 | 355.7 |
| coolant 20 C | 900 | 0 | 14.41 | 2.000 | 77.50 | n/a | yes | 0.832 | 0.818 | None (standstill) | TorqueBsfc / TorqueEstimate | 1.7 | 4.737 | 488.7 |
| coolant 40 C | 900 | 0 | 14.41 | 2.000 | 80.00 | n/a | yes | 0.832 | 0.751 | None (standstill) | TorqueBsfc / TorqueEstimate | -3.7 | 4.737 | 507.6 |
| coolant 90 C | 900 | 0 | 14.41 | 2.000 | 72.50 | n/a | yes | 0.832 | 0.884 | None (standstill) | TorqueBsfc / TorqueEstimate | 2.8 | 4.737 | 450.7 |

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
414F00000000
416185
416285
416300D2
J1939 SPN174 raw=55
decoded MAF="14.41" lambda="2.000" coolant/IAT not printed
torque path: power_kW = torque_pct/100 * Tref * rpm * 2*pi/60 / 1000; fuel_g_h = power * BSFC + idle; L/h = fuel_g_h / (rho*1000)
```

Petrol cold idle encodes a richer lambda (MAF path). Diesel idle lambda is saturated; L/h follows encoded torque from the model AFR, so coolant is no longer a flat column.

### NA diesel 1.9 L / diesel / fuel temp

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | true AFR (model) | AFR used | lambda sat | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | Navi vs true % | naive fixed-14.7 L/h | naive vs true % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: |
| fuel -30 C | 2500 | 90 | 41.47 | 2.000 | 36.50 | n/a | yes | 0.863 | 4.740 | 5.27 | TorqueBsfc / TorqueEstimate | 0.0 | 13.632 | 187.7 |
| fuel 0 C | 2500 | 90 | 41.47 | 2.000 | 36.50 | n/a | yes | 0.842 | 4.856 | 5.40 | TorqueBsfc / TorqueEstimate | 0.0 | 13.632 | 180.8 |
| fuel 15 C | 2500 | 90 | 41.47 | 2.000 | 36.50 | n/a | yes | 0.832 | 4.917 | 5.46 | TorqueBsfc / TorqueEstimate | 0.0 | 13.632 | 177.3 |
| fuel 40 C | 2500 | 90 | 41.47 | 2.000 | 36.50 | n/a | yes | 0.815 | 5.021 | 5.58 | TorqueBsfc / TorqueEstimate | 0.0 | 13.632 | 171.6 |
| fuel 60 C | 2500 | 90 | 41.47 | 2.000 | 36.50 | n/a | yes | 0.801 | 5.107 | 5.67 | TorqueBsfc / TorqueEstimate | 0.0 | 13.632 | 167.0 |

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
414F00000000
416199
416299
416300D2
J1939 SPN174 raw=55
decoded MAF="41.47" lambda="2.000" coolant/IAT not printed
torque path: power_kW = torque_pct/100 * Tref * rpm * 2*pi/60 / 1000; fuel_g_h = power * BSFC + idle; L/h = fuel_g_h / (rho*1000)
```

MAF path: hotter fuel is less dense, so the same fuel mass is more litres. Volume PIDs would not show this.

### NA diesel 1.9 L / diesel / intake air

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | true AFR (model) | AFR used | lambda sat | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | Navi vs true % | naive fixed-14.7 L/h | naive vs true % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: |
| IAT -30 C | 2500 | 90 | 52.87 | 1.200 | 17.40 | 17.40 | no | 0.832 | 13.147 | 14.61 | MafDerived / LambdaMaf | 0.0 | 17.380 | 32.2 |
| IAT 0 C | 2500 | 90 | 47.06 | 1.200 | 17.40 | 17.40 | no | 0.832 | 11.702 | 13.00 | MafDerived / LambdaMaf | 0.0 | 15.470 | 32.2 |
| IAT 20 C | 2500 | 90 | 43.85 | 1.200 | 17.40 | 17.40 | no | 0.832 | 10.904 | 12.12 | MafDerived / LambdaMaf | 0.0 | 14.414 | 32.2 |
| IAT 40 C | 2500 | 90 | 41.05 | 1.200 | 17.40 | 17.40 | no | 0.832 | 10.208 | 11.34 | MafDerived / LambdaMaf | 0.0 | 13.494 | 32.2 |

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
414F00000000
4161C0
4162C0
416300D2
J1939 SPN174 raw=55
decoded MAF="43.85" lambda="1.200" coolant/IAT not printed
fuel_l_h = 43.85 * 3600 / (17.40 * 0.832 * 1000) = 10.904 L/h
```

Decoded MAF falls as intake air warms (ideal gas in the model). With MAF+lambda, L/h tracks that MAF; `fuel.rs` does not apply an extra IAT correction.

### NA diesel 1.9 L / diesel / load

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | true AFR (model) | AFR used | lambda sat | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | Navi vs true % | naive fixed-14.7 L/h | naive vs true % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: |
| load 10% | 2500 | 90 | 39.56 | 2.000 | 36.50 | n/a | yes | 0.832 | 4.765 | 5.29 | TorqueBsfc / TorqueEstimate | 1.6 | 13.004 | 177.3 |
| load 25% | 2500 | 90 | 40.27 | 2.000 | 36.50 | n/a | yes | 0.832 | 4.765 | 5.29 | TorqueBsfc / TorqueEstimate | -0.2 | 13.238 | 177.3 |
| load 50% | 2500 | 90 | 41.47 | 2.000 | 36.50 | n/a | yes | 0.832 | 4.917 | 5.46 | TorqueBsfc / TorqueEstimate | 0.0 | 13.632 | 177.3 |
| load 75% | 2500 | 90 | 42.66 | 2.000 | 36.50 | n/a | yes | 0.832 | 5.069 | 5.63 | TorqueBsfc / TorqueEstimate | 0.2 | 14.023 | 177.3 |
| load 100% | 2500 | 90 | 43.85 | 1.200 | 17.40 | 17.40 | no | 0.832 | 10.904 | 12.12 | MafDerived / LambdaMaf | 0.0 | 14.414 | 32.2 |

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
414F00000000
416199
416299
416300D2
J1939 SPN174 raw=55
decoded MAF="41.47" lambda="2.000" coolant/IAT not printed
torque path: power_kW = torque_pct/100 * Tref * rpm * 2*pi/60 / 1000; fuel_g_h = power * BSFC + idle; L/h = fuel_g_h / (rho*1000)
```

Fuel rate from 10% to 100% load goes **4.765** to **10.904 L/h** at this rpm.

### NA diesel 1.9 L / diesel / throttle

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | true AFR (model) | AFR used | lambda sat | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | Navi vs true % | naive fixed-14.7 L/h | naive vs true % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: |
| throttle 0% (overrun) | 2500 | 90 | 39.46 | 2.000 | n/a | n/a | yes | n/a | 0.0 (fuel cut) | 0.0 (fuel cut) | Pid5E / Measured | 0.0 | 12.971 | n/a |
| throttle 10% | 2500 | 90 | 39.56 | 2.000 | 36.50 | n/a | yes | 0.832 | 4.765 | 5.29 | TorqueBsfc / TorqueEstimate | 1.6 | 13.004 | 177.3 |
| throttle 25% | 2500 | 90 | 40.27 | 2.000 | 36.50 | n/a | yes | 0.832 | 4.765 | 5.29 | TorqueBsfc / TorqueEstimate | -0.2 | 13.238 | 177.3 |
| throttle 50% | 2500 | 90 | 41.47 | 2.000 | 36.50 | n/a | yes | 0.832 | 4.917 | 5.46 | TorqueBsfc / TorqueEstimate | 0.0 | 13.632 | 177.3 |
| throttle 75% | 2500 | 90 | 42.66 | 2.000 | 36.50 | n/a | yes | 0.832 | 5.069 | 5.63 | TorqueBsfc / TorqueEstimate | 0.2 | 14.023 | 177.3 |
| throttle 100% | 2500 | 90 | 43.85 | 1.200 | 17.40 | 17.40 | no | 0.832 | 10.904 | 12.12 | MafDerived / LambdaMaf | 0.0 | 14.414 | 32.2 |

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
414F00000000
416199
416299
416300D2
J1939 SPN174 raw=55
decoded MAF="41.47" lambda="2.000" coolant/IAT not printed
torque path: power_kW = torque_pct/100 * Tref * rpm * 2*pi/60 / 1000; fuel_g_h = power * BSFC + idle; L/h = fuel_g_h / (rho*1000)
```

Throttle 0% is overrun: diesels and warm petrol encode PID 5E = 0 (Some(0.0)); cold petrol keeps injecting.

### NA petrol 1.8 L / E0 / altitude

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | true AFR (model) | AFR used | lambda sat | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | Navi vs true % | naive fixed-14.7 L/h | naive vs true % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: |
| 0 m | 2500 | 90 | 39.28 | 0.867 | 12.75 | 12.75 | no | 0.745 | 14.887 | 16.54 | MafDerived / LambdaMaf | -0.0 | 12.912 | -13.3 |
| 500 m | 2500 | 90 | 37.01 | 0.867 | 12.75 | 12.75 | no | 0.745 | 14.027 | 15.59 | MafDerived / LambdaMaf | 0.0 | 12.166 | -13.3 |
| 1000 m | 2500 | 90 | 34.84 | 0.867 | 12.75 | 12.75 | no | 0.745 | 13.204 | 14.67 | MafDerived / LambdaMaf | -0.0 | 11.453 | -13.3 |
| 1500 m | 2500 | 90 | 32.78 | 0.867 | 12.75 | 12.75 | no | 0.745 | 12.424 | 13.80 | MafDerived / LambdaMaf | -0.0 | 10.776 | -13.3 |
| 2000 m | 2500 | 90 | 30.82 | 0.867 | 12.75 | 12.75 | no | 0.745 | 11.681 | 12.98 | MafDerived / LambdaMaf | 0.0 | 10.131 | -13.3 |
| 2500 m | 2500 | 90 | 28.95 | 0.867 | 12.75 | 12.75 | no | 0.745 | 10.972 | 12.19 | MafDerived / LambdaMaf | -0.0 | 9.517 | -13.3 |

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
414F00000000
4161DE
4162DE
41630096
J1939 SPN174 raw=55
decoded MAF="39.28" lambda="0.867" coolant/IAT not printed
fuel_l_h = 39.28 * 3600 / (12.75 * 0.745 * 1000) = 14.887 L/h
```

From 0 m to 1000 m, decoded fuel rate changes by **11.3%** (model air mass and, for NA diesel, smoke-limit AFR).

### NA petrol 1.8 L / E0 / coolant

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | true AFR (model) | AFR used | lambda sat | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | Navi vs true % | naive fixed-14.7 L/h | naive vs true % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: |
| coolant -30 C | 900 | 0 | 3.47 | 0.680 | 10.00 | 10.00 | no | 0.745 | 1.677 | None (standstill) | MafDerived / LambdaMaf | 0.1 | 1.141 | -31.9 |
| coolant 0 C | 900 | 0 | 3.47 | 0.816 | 12.00 | 12.00 | no | 0.745 | 1.397 | None (standstill) | MafDerived / LambdaMaf | 0.1 | 1.141 | -18.3 |
| coolant 20 C | 900 | 0 | 3.47 | 0.884 | 13.00 | 13.00 | no | 0.745 | 1.290 | None (standstill) | MafDerived / LambdaMaf | 0.1 | 1.141 | -11.5 |
| coolant 40 C | 900 | 0 | 3.47 | 0.959 | 14.10 | 14.10 | no | 0.745 | 1.189 | None (standstill) | MafDerived / LambdaMaf | 0.1 | 1.141 | -4.0 |
| coolant 90 C | 900 | 0 | 3.47 | 1.000 | 14.70 | 14.70 | no | 0.745 | 1.141 | None (standstill) | MafDerived / LambdaMaf | 0.1 | 1.141 | 0.1 |

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
414F00000000
41618A
41628A
41630096
J1939 SPN174 raw=55
decoded MAF="3.47" lambda="1.000" coolant/IAT not printed
fuel_l_h = 3.47 * 3600 / (14.70 * 0.745 * 1000) = 1.141 L/h
```

Petrol cold idle encodes a richer lambda (MAF path). Diesel idle lambda is saturated; L/h follows encoded torque from the model AFR, so coolant is no longer a flat column.

### NA petrol 1.8 L / E0 / fuel temp

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | true AFR (model) | AFR used | lambda sat | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | Navi vs true % | naive fixed-14.7 L/h | naive vs true % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: |
| fuel -30 C | 2500 | 90 | 16.60 | 1.000 | 14.70 | 14.70 | no | 0.777 | 5.233 | 5.81 | MafDerived / LambdaMaf | 0.0 | 5.457 | 4.3 |
| fuel 0 C | 2500 | 90 | 16.60 | 1.000 | 14.70 | 14.70 | no | 0.756 | 5.380 | 5.98 | MafDerived / LambdaMaf | 0.0 | 5.457 | 1.4 |
| fuel 15 C | 2500 | 90 | 16.60 | 1.000 | 14.70 | 14.70 | no | 0.745 | 5.457 | 6.06 | MafDerived / LambdaMaf | 0.0 | 5.457 | 0.0 |
| fuel 40 C | 2500 | 90 | 16.60 | 1.000 | 14.70 | 14.70 | no | 0.727 | 5.590 | 6.21 | MafDerived / LambdaMaf | 0.0 | 5.457 | -2.4 |
| fuel 60 C | 2500 | 90 | 16.60 | 1.000 | 14.70 | 14.70 | no | 0.713 | 5.700 | 6.33 | MafDerived / LambdaMaf | 0.0 | 5.457 | -4.3 |

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
414F00000000
41619E
41629E
41630096
J1939 SPN174 raw=55
decoded MAF="16.60" lambda="1.000" coolant/IAT not printed
fuel_l_h = 16.60 * 3600 / (14.70 * 0.745 * 1000) = 5.457 L/h
```

MAF path: hotter fuel is less dense, so the same fuel mass is more litres. Volume PIDs would not show this.

### NA petrol 1.8 L / E0 / intake air

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | true AFR (model) | AFR used | lambda sat | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | Navi vs true % | naive fixed-14.7 L/h | naive vs true % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: |
| IAT -30 C | 2500 | 90 | 47.36 | 0.867 | 12.75 | 12.75 | no | 0.745 | 17.949 | 19.94 | MafDerived / LambdaMaf | -0.0 | 15.568 | -13.3 |
| IAT 0 C | 2500 | 90 | 42.16 | 0.867 | 12.75 | 12.75 | no | 0.745 | 15.979 | 17.75 | MafDerived / LambdaMaf | 0.0 | 13.859 | -13.3 |
| IAT 20 C | 2500 | 90 | 39.28 | 0.867 | 12.75 | 12.75 | no | 0.745 | 14.887 | 16.54 | MafDerived / LambdaMaf | -0.0 | 12.912 | -13.3 |
| IAT 40 C | 2500 | 90 | 36.77 | 0.867 | 12.75 | 12.75 | no | 0.745 | 13.936 | 15.48 | MafDerived / LambdaMaf | -0.0 | 12.087 | -13.3 |

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
414F00000000
4161DE
4162DE
41630096
J1939 SPN174 raw=55
decoded MAF="39.28" lambda="0.867" coolant/IAT not printed
fuel_l_h = 39.28 * 3600 / (12.75 * 0.745 * 1000) = 14.887 L/h
```

Decoded MAF falls as intake air warms (ideal gas in the model). With MAF+lambda, L/h tracks that MAF; `fuel.rs` does not apply an extra IAT correction.

### NA petrol 1.8 L / E0 / load

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | true AFR (model) | AFR used | lambda sat | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | Navi vs true % | naive fixed-14.7 L/h | naive vs true % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: |
| load 10% | 2500 | 90 | 10.57 | 1.000 | 14.70 | 14.70 | no | 0.745 | 3.475 | 3.86 | MafDerived / LambdaMaf | -0.0 | 3.475 | -0.0 |
| load 25% | 2500 | 90 | 12.55 | 1.000 | 14.70 | 14.70 | no | 0.745 | 4.125 | 4.58 | MafDerived / LambdaMaf | 0.0 | 4.125 | 0.0 |
| load 50% | 2500 | 90 | 16.60 | 1.000 | 14.70 | 14.70 | no | 0.745 | 5.457 | 6.06 | MafDerived / LambdaMaf | 0.0 | 5.457 | 0.0 |
| load 75% | 2500 | 90 | 27.78 | 1.000 | 14.70 | 14.70 | no | 0.745 | 9.132 | 10.15 | MafDerived / LambdaMaf | 0.0 | 9.132 | 0.0 |
| load 100% | 2500 | 90 | 39.28 | 0.867 | 12.75 | 12.75 | no | 0.745 | 14.887 | 16.54 | MafDerived / LambdaMaf | -0.0 | 12.912 | -13.3 |

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
414F00000000
41619E
41629E
41630096
J1939 SPN174 raw=55
decoded MAF="16.60" lambda="1.000" coolant/IAT not printed
fuel_l_h = 16.60 * 3600 / (14.70 * 0.745 * 1000) = 5.457 L/h
```

Fuel rate from 10% to 100% load goes **3.475** to **14.887 L/h** at this rpm.

### NA petrol 1.8 L / E0 / throttle

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | true AFR (model) | AFR used | lambda sat | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | Navi vs true % | naive fixed-14.7 L/h | naive vs true % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: |
| throttle 0% (overrun) | 2500 | 90 | 7.84 | 2.000 | n/a | n/a | yes | n/a | 0.0 (fuel cut) | 0.0 (fuel cut) | Pid5E / Measured | 0.0 | 2.577 | n/a |
| throttle 10% | 2500 | 90 | 10.10 | 1.000 | 14.70 | 14.70 | no | 0.745 | 3.320 | 3.69 | MafDerived / LambdaMaf | 0.0 | 3.320 | 0.0 |
| throttle 25% | 2500 | 90 | 13.85 | 1.000 | 14.70 | 14.70 | no | 0.745 | 4.553 | 5.06 | MafDerived / LambdaMaf | 0.0 | 4.553 | 0.0 |
| throttle 50% | 2500 | 90 | 21.09 | 1.000 | 14.70 | 14.70 | no | 0.745 | 6.933 | 7.70 | MafDerived / LambdaMaf | -0.0 | 6.933 | -0.0 |
| throttle 75% | 2500 | 90 | 29.57 | 1.000 | 14.70 | 14.70 | no | 0.745 | 9.720 | 10.80 | MafDerived / LambdaMaf | 0.0 | 9.720 | 0.0 |
| throttle 100% | 2500 | 90 | 39.28 | 0.867 | 12.75 | 12.75 | no | 0.745 | 14.887 | 16.54 | MafDerived / LambdaMaf | -0.0 | 12.912 | -13.3 |

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
414F00000000
4161A8
4162A8
41630096
J1939 SPN174 raw=55
decoded MAF="21.09" lambda="1.000" coolant/IAT not printed
fuel_l_h = 21.09 * 3600 / (14.70 * 0.745 * 1000) = 6.933 L/h
```

Throttle 0% is overrun: diesels and warm petrol encode PID 5E = 0 (Some(0.0)); cold petrol keeps injecting.

### NA petrol 1.8 L / combined

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | true AFR (model) | AFR used | lambda sat | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | Navi vs true % | naive fixed-14.7 L/h | naive vs true % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: |
| cold start idle -30 C | 900 | 0 | 4.18 | 0.680 | 10.00 | 10.00 | no | 0.777 | 1.937 | None (standstill) | MafDerived / LambdaMaf | 0.0 | 1.374 | -29.1 |
| motorway 110 km/h +40 C day | 2800 | 110 | 17.40 | 1.000 | 14.70 | 14.70 | no | 0.745 | 5.720 | 5.20 | MafDerived / LambdaMaf | -0.0 | 5.720 | -0.0 |
| full load 1500 m at 0 C IAT | 3500 | 80 | 49.26 | 0.867 | 12.75 | 12.75 | no | 0.745 | 18.670 | 23.34 | MafDerived / LambdaMaf | 0.0 | 16.193 | -13.3 |
| downhill fuel cut, warm | 2200 | 80 | 6.90 | 2.000 | n/a | n/a | yes | n/a | 0.0 (fuel cut) | 0.0 (fuel cut) | Pid5E / Measured | 0.0 | 2.268 | n/a |
| downhill in gear, cold engine | 2200 | 80 | 6.90 | 0.884 | 13.00 | 13.00 | no | 0.745 | 2.565 | 3.21 | MafDerived / LambdaMaf | -0.0 | 2.268 | -11.6 |
| E85 at -20 C coolant and fuel | 900 | 0 | 6.92 | 0.726 | 10.67 | 7.12 | no | 0.812 | 4.311 | None (standstill) | MafDerived / LambdaMaf | 49.9 | 2.275 | -20.9 |

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
414F00000000
41619E
41629E
41630096
J1939 SPN174 raw=10
decoded MAF="4.18" lambda="0.680" coolant/IAT not printed
fuel_l_h = 4.18 * 3600 / (10.00 * 0.777 * 1000) = 1.937 L/h
```

Combined points reuse the same decode path: idle L/100 km is None (standstill); downhill cut is 0.0 L/h when evidence is present.

### NA petrol 1.8 L / flex refuel

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | true AFR (model) | AFR used | lambda sat | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | Navi vs true % | naive fixed-14.7 L/h | naive vs true % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: |
| before refuel E10 | 2500 | 90 | 16.60 | 1.000 | 14.70 | 14.10 | no | 0.749 | 5.660 | 6.29 | MafDerived / LambdaMaf | 4.3 | 5.457 | 0.6 |
| after refuel E70 | 2500 | 90 | 16.60 | 1.000 | 14.70 | 10.64 | no | 0.776 | 7.245 | 8.05 | MafDerived / LambdaMaf | 38.3 | 5.457 | 4.1 |

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
414F00000000
41619E
41629E
41630096
J1939 SPN174 raw=55
decoded MAF="16.60" lambda="1.000" coolant/IAT not printed
fuel_l_h = 16.60 * 3600 / (14.10 * 0.749 * 1000) = 5.660 L/h
```

Rows share the baseline rpm/speed unless the varied input is that axis.

### NA petrol 1.8 L / fuel type

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | true AFR (model) | AFR used | lambda sat | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | Navi vs true % | naive fixed-14.7 L/h | naive vs true % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: |
| E0 | 2500 | 90 | 16.60 | 1.000 | 14.70 | 14.70 | no | 0.745 | 5.457 | 6.06 | MafDerived / LambdaMaf | 0.0 | 5.457 | 0.0 |
| E10 | 2500 | 90 | 16.60 | 1.000 | 14.70 | 14.10 | no | 0.749 | 5.660 | 6.29 | MafDerived / LambdaMaf | 4.3 | 5.457 | 0.6 |
| E85 | 2500 | 90 | 16.60 | 1.000 | 14.70 | 9.81 | no | 0.782 | 7.787 | 8.65 | MafDerived / LambdaMaf | 49.9 | 5.457 | 5.0 |

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
414F00000000
41619E
41629E
41630096
J1939 SPN174 raw=55
decoded MAF="16.60" lambda="1.000" coolant/IAT not printed
fuel_l_h = 16.60 * 3600 / (14.70 * 0.745 * 1000) = 5.457 L/h
```

Same air mass and lambda: E85 uses more litres than E0 because stoich AFR and density both move toward ethanol.

### turbo diesel 1.9 L / combined

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | true AFR (model) | AFR used | lambda sat | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | Navi vs true % | naive fixed-14.7 L/h | naive vs true % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: |
| cold start idle -30 C | 900 | 0 | 20.64 | 2.000 | 42.50 | n/a | yes | 0.863 | 2.027 | None (standstill) | TorqueBsfc / TorqueEstimate | 0.1 | 6.785 | 235.0 |
| motorway 110 km/h +40 C day | 2800 | 110 | 61.46 | 2.000 | 40.00 | n/a | yes | 0.832 | 6.619 | 6.02 | TorqueBsfc / TorqueEstimate | -0.4 | 20.203 | 203.9 |
| full load 1500 m at 0 C IAT | 3500 | 80 | 116.75 | 1.448 | 21.00 | 21.00 | no | 0.832 | 24.056 | 30.07 | MafDerived / LambdaMaf | 0.0 | 38.378 | 59.5 |
| downhill fuel cut, warm | 2200 | 80 | 31.59 | 2.000 | n/a | n/a | yes | n/a | 0.0 (fuel cut) | 0.0 (fuel cut) | Pid5E / Measured | 0.0 | 10.384 | n/a |
| downhill in gear, cold engine | 2200 | 80 | 31.59 | 2.000 | n/a | n/a | yes | n/a | 0.0 (fuel cut) | 0.0 (fuel cut) | Pid5E / Measured | 0.0 | 10.384 | n/a |
| J1939 LFE truck-style part load | 2500 | 90 | 60.90 | 2.000 | 40.00 | n/a | yes | n/a | 6.600 | 7.33 | J1939LfeIllustrative / Measured | 0.2 | 20.019 | 203.9 |

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
414F00000000
41619E
41629E
416300D2
J1939 SPN174 raw=10
decoded MAF="20.64" lambda="2.000" coolant/IAT not printed
torque path: power_kW = torque_pct/100 * Tref * rpm * 2*pi/60 / 1000; fuel_g_h = power * BSFC + idle; L/h = fuel_g_h / (rho*1000)
```

Combined points reuse the same decode path: idle L/100 km is None (standstill); downhill cut is 0.0 L/h when evidence is present.

### turbo diesel 1.9 L / diesel / altitude

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | true AFR (model) | AFR used | lambda sat | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | Navi vs true % | naive fixed-14.7 L/h | naive vs true % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: |
| 0 m | 2500 | 90 | 84.82 | 1.448 | 21.00 | 21.00 | no | 0.832 | 17.477 | 19.42 | MafDerived / LambdaMaf | 0.0 | 27.882 | 59.5 |
| 500 m | 2500 | 90 | 82.42 | 1.448 | 21.00 | 21.00 | no | 0.832 | 16.982 | 18.87 | MafDerived / LambdaMaf | -0.0 | 27.093 | 59.5 |
| 1000 m | 2500 | 90 | 80.01 | 1.448 | 21.00 | 21.00 | no | 0.832 | 16.486 | 18.32 | MafDerived / LambdaMaf | 0.0 | 26.301 | 59.5 |
| 1500 m | 2500 | 90 | 77.70 | 1.448 | 21.00 | 21.00 | no | 0.832 | 16.010 | 17.79 | MafDerived / LambdaMaf | -0.0 | 25.542 | 59.5 |
| 2000 m | 2500 | 90 | 75.51 | 1.448 | 21.00 | 21.00 | no | 0.832 | 15.558 | 17.29 | MafDerived / LambdaMaf | -0.0 | 24.822 | 59.5 |
| 2500 m | 2500 | 90 | 73.43 | 1.448 | 21.00 | 21.00 | no | 0.832 | 15.130 | 16.81 | MafDerived / LambdaMaf | -0.0 | 24.138 | 59.5 |

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
414F00000000
4161EC
4162EC
416300D2
J1939 SPN174 raw=55
decoded MAF="84.82" lambda="1.448" coolant/IAT not printed
fuel_l_h = 84.82 * 3600 / (21.00 * 0.832 * 1000) = 17.477 L/h
```

From 0 m to 1000 m, decoded fuel rate changes by **5.7%** (model air mass and, for NA diesel, smoke-limit AFR).

### turbo diesel 1.9 L / diesel / coolant

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | true AFR (model) | AFR used | lambda sat | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | Navi vs true % | naive fixed-14.7 L/h | naive vs true % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: |
| coolant -30 C | 900 | 0 | 17.12 | 2.000 | 42.50 | n/a | yes | 0.832 | 1.720 | None (standstill) | TorqueBsfc / TorqueEstimate | -1.3 | 5.628 | 223.0 |
| coolant 0 C | 900 | 0 | 17.12 | 2.000 | 60.00 | n/a | yes | 0.832 | 1.232 | None (standstill) | TorqueBsfc / TorqueEstimate | -0.2 | 5.628 | 355.9 |
| coolant 20 C | 900 | 0 | 17.12 | 2.000 | 77.50 | n/a | yes | 0.832 | 0.947 | None (standstill) | TorqueBsfc / TorqueEstimate | -0.8 | 5.628 | 488.9 |
| coolant 40 C | 900 | 0 | 17.12 | 2.000 | 80.00 | n/a | yes | 0.832 | 0.947 | None (standstill) | TorqueBsfc / TorqueEstimate | 2.3 | 5.628 | 507.9 |
| coolant 90 C | 900 | 0 | 17.12 | 2.000 | 80.00 | n/a | yes | 0.832 | 0.947 | None (standstill) | TorqueBsfc / TorqueEstimate | 2.3 | 5.628 | 507.9 |

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
414F00000000
416186
416286
416300D2
J1939 SPN174 raw=55
decoded MAF="17.12" lambda="2.000" coolant/IAT not printed
torque path: power_kW = torque_pct/100 * Tref * rpm * 2*pi/60 / 1000; fuel_g_h = power * BSFC + idle; L/h = fuel_g_h / (rho*1000)
```

Petrol cold idle encodes a richer lambda (MAF path). Diesel idle lambda is saturated; L/h follows encoded torque from the model AFR, so coolant is no longer a flat column.

### turbo diesel 1.9 L / diesel / fuel temp

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | true AFR (model) | AFR used | lambda sat | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | Navi vs true % | naive fixed-14.7 L/h | naive vs true % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: |
| fuel -30 C | 2500 | 90 | 60.90 | 2.000 | 40.00 | n/a | yes | 0.863 | 6.351 | 7.06 | TorqueBsfc / TorqueEstimate | 0.0 | 20.019 | 215.2 |
| fuel 0 C | 2500 | 90 | 60.90 | 2.000 | 40.00 | n/a | yes | 0.842 | 6.507 | 7.23 | TorqueBsfc / TorqueEstimate | 0.0 | 20.019 | 207.6 |
| fuel 15 C | 2500 | 90 | 60.90 | 2.000 | 40.00 | n/a | yes | 0.832 | 6.588 | 7.32 | TorqueBsfc / TorqueEstimate | 0.0 | 20.019 | 203.9 |
| fuel 40 C | 2500 | 90 | 60.90 | 2.000 | 40.00 | n/a | yes | 0.815 | 6.728 | 7.48 | TorqueBsfc / TorqueEstimate | 0.0 | 20.019 | 197.6 |
| fuel 60 C | 2500 | 90 | 60.90 | 2.000 | 40.00 | n/a | yes | 0.801 | 6.844 | 7.60 | TorqueBsfc / TorqueEstimate | 0.0 | 20.019 | 192.5 |

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
414F00000000
4161A4
4162A4
416300D2
J1939 SPN174 raw=55
decoded MAF="60.90" lambda="2.000" coolant/IAT not printed
torque path: power_kW = torque_pct/100 * Tref * rpm * 2*pi/60 / 1000; fuel_g_h = power * BSFC + idle; L/h = fuel_g_h / (rho*1000)
```

MAF path: hotter fuel is less dense, so the same fuel mass is more litres. Volume PIDs would not show this.

### turbo diesel 1.9 L / diesel / intake air

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | true AFR (model) | AFR used | lambda sat | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | Navi vs true % | naive fixed-14.7 L/h | naive vs true % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: |
| IAT -30 C | 2500 | 90 | 102.26 | 1.448 | 21.00 | 21.00 | no | 0.832 | 21.070 | 23.41 | MafDerived / LambdaMaf | -0.0 | 33.615 | 59.5 |
| IAT 0 C | 2500 | 90 | 91.03 | 1.448 | 21.00 | 21.00 | no | 0.832 | 18.756 | 20.84 | MafDerived / LambdaMaf | 0.0 | 29.924 | 59.5 |
| IAT 20 C | 2500 | 90 | 84.82 | 1.448 | 21.00 | 21.00 | no | 0.832 | 17.477 | 19.42 | MafDerived / LambdaMaf | 0.0 | 27.882 | 59.5 |
| IAT 40 C | 2500 | 90 | 79.40 | 1.448 | 21.00 | 21.00 | no | 0.832 | 16.360 | 18.18 | MafDerived / LambdaMaf | -0.0 | 26.101 | 59.5 |

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
414F00000000
4161EC
4162EC
416300D2
J1939 SPN174 raw=55
decoded MAF="84.82" lambda="1.448" coolant/IAT not printed
fuel_l_h = 84.82 * 3600 / (21.00 * 0.832 * 1000) = 17.477 L/h
```

Decoded MAF falls as intake air warms (ideal gas in the model). With MAF+lambda, L/h tracks that MAF; `fuel.rs` does not apply an extra IAT correction.

### turbo diesel 1.9 L / diesel / load

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | true AFR (model) | AFR used | lambda sat | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | Navi vs true % | naive fixed-14.7 L/h | naive vs true % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: |
| load 10% | 2500 | 90 | 43.27 | 2.000 | 40.00 | n/a | yes | 0.832 | 4.613 | 5.13 | TorqueBsfc / TorqueEstimate | -1.4 | 14.224 | 203.9 |
| load 25% | 2500 | 90 | 49.71 | 2.000 | 40.00 | n/a | yes | 0.832 | 5.373 | 5.97 | TorqueBsfc / TorqueEstimate | -0.1 | 16.341 | 203.9 |
| load 50% | 2500 | 90 | 60.90 | 2.000 | 40.00 | n/a | yes | 0.832 | 6.588 | 7.32 | TorqueBsfc / TorqueEstimate | 0.0 | 20.019 | 203.9 |
| load 75% | 2500 | 90 | 72.65 | 2.000 | 40.00 | n/a | yes | 0.832 | 7.804 | 8.67 | TorqueBsfc / TorqueEstimate | -0.7 | 23.882 | 203.9 |
| load 100% | 2500 | 90 | 84.82 | 1.448 | 21.00 | 21.00 | no | 0.832 | 17.477 | 19.42 | MafDerived / LambdaMaf | 0.0 | 27.882 | 59.5 |

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
414F00000000
4161A4
4162A4
416300D2
J1939 SPN174 raw=55
decoded MAF="60.90" lambda="2.000" coolant/IAT not printed
torque path: power_kW = torque_pct/100 * Tref * rpm * 2*pi/60 / 1000; fuel_g_h = power * BSFC + idle; L/h = fuel_g_h / (rho*1000)
```

Fuel rate from 10% to 100% load goes **4.613** to **17.477 L/h** at this rpm.

### turbo diesel 1.9 L / diesel / throttle

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | true AFR (model) | AFR used | lambda sat | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | Navi vs true % | naive fixed-14.7 L/h | naive vs true % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: |
| throttle 0% (overrun) | 2500 | 90 | 42.42 | 2.000 | n/a | n/a | yes | n/a | 0.0 (fuel cut) | 0.0 (fuel cut) | Pid5E / Measured | 0.0 | 13.944 | n/a |
| throttle 10% | 2500 | 90 | 43.27 | 2.000 | 40.00 | n/a | yes | 0.832 | 4.613 | 5.13 | TorqueBsfc / TorqueEstimate | -1.4 | 14.224 | 203.9 |
| throttle 25% | 2500 | 90 | 49.71 | 2.000 | 40.00 | n/a | yes | 0.832 | 5.373 | 5.97 | TorqueBsfc / TorqueEstimate | -0.1 | 16.341 | 203.9 |
| throttle 50% | 2500 | 90 | 60.90 | 2.000 | 40.00 | n/a | yes | 0.832 | 6.588 | 7.32 | TorqueBsfc / TorqueEstimate | 0.0 | 20.019 | 203.9 |
| throttle 75% | 2500 | 90 | 72.65 | 2.000 | 40.00 | n/a | yes | 0.832 | 7.804 | 8.67 | TorqueBsfc / TorqueEstimate | -0.7 | 23.882 | 203.9 |
| throttle 100% | 2500 | 90 | 84.82 | 1.448 | 21.00 | 21.00 | no | 0.832 | 17.477 | 19.42 | MafDerived / LambdaMaf | 0.0 | 27.882 | 59.5 |

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
414F00000000
4161A4
4162A4
416300D2
J1939 SPN174 raw=55
decoded MAF="60.90" lambda="2.000" coolant/IAT not printed
torque path: power_kW = torque_pct/100 * Tref * rpm * 2*pi/60 / 1000; fuel_g_h = power * BSFC + idle; L/h = fuel_g_h / (rho*1000)
```

Throttle 0% is overrun: diesels and warm petrol encode PID 5E = 0 (Some(0.0)); cold petrol keeps injecting.

### turbo petrol 1.8 L / E0 / altitude

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | true AFR (model) | AFR used | lambda sat | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | Navi vs true % | naive fixed-14.7 L/h | naive vs true % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: |
| 0 m | 2500 | 90 | 62.42 | 0.827 | 12.15 | 12.15 | no | 0.745 | 24.825 | 27.58 | MafDerived / LambdaMaf | 0.0 | 20.519 | -17.3 |
| 500 m | 2500 | 90 | 60.27 | 0.827 | 12.15 | 12.15 | no | 0.745 | 23.970 | 26.63 | MafDerived / LambdaMaf | -0.0 | 19.812 | -17.3 |
| 1000 m | 2500 | 90 | 58.11 | 0.827 | 12.15 | 12.15 | no | 0.745 | 23.111 | 25.68 | MafDerived / LambdaMaf | 0.0 | 19.102 | -17.3 |
| 1500 m | 2500 | 90 | 56.04 | 0.827 | 12.15 | 12.15 | no | 0.745 | 22.288 | 24.76 | MafDerived / LambdaMaf | -0.0 | 18.422 | -17.4 |
| 2000 m | 2500 | 90 | 54.08 | 0.827 | 12.15 | 12.15 | no | 0.745 | 21.508 | 23.90 | MafDerived / LambdaMaf | -0.0 | 17.777 | -17.3 |
| 2500 m | 2500 | 90 | 52.22 | 0.827 | 12.15 | 12.15 | no | 0.745 | 20.768 | 23.08 | MafDerived / LambdaMaf | 0.0 | 17.166 | -17.3 |

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
414F00000000
4161F5
4162F5
41630096
J1939 SPN174 raw=55
decoded MAF="62.42" lambda="0.827" coolant/IAT not printed
fuel_l_h = 62.42 * 3600 / (12.15 * 0.745 * 1000) = 24.825 L/h
```

From 0 m to 1000 m, decoded fuel rate changes by **6.9%** (model air mass and, for NA diesel, smoke-limit AFR).

### turbo petrol 1.8 L / E0 / coolant

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | true AFR (model) | AFR used | lambda sat | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | Navi vs true % | naive fixed-14.7 L/h | naive vs true % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: |
| coolant -30 C | 900 | 0 | 3.91 | 0.680 | 10.00 | 10.00 | no | 0.745 | 1.889 | None (standstill) | MafDerived / LambdaMaf | 0.1 | 1.285 | -31.9 |
| coolant 0 C | 900 | 0 | 3.91 | 0.816 | 12.00 | 12.00 | no | 0.745 | 1.575 | None (standstill) | MafDerived / LambdaMaf | 0.1 | 1.285 | -18.3 |
| coolant 20 C | 900 | 0 | 3.91 | 0.884 | 13.00 | 13.00 | no | 0.745 | 1.453 | None (standstill) | MafDerived / LambdaMaf | 0.1 | 1.285 | -11.5 |
| coolant 40 C | 900 | 0 | 3.91 | 0.959 | 14.10 | 14.10 | no | 0.745 | 1.340 | None (standstill) | MafDerived / LambdaMaf | 0.1 | 1.285 | -4.0 |
| coolant 90 C | 900 | 0 | 3.91 | 1.000 | 14.70 | 14.70 | no | 0.745 | 1.285 | None (standstill) | MafDerived / LambdaMaf | 0.1 | 1.285 | 0.1 |

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
414F00000000
41618D
41628D
41630096
J1939 SPN174 raw=55
decoded MAF="3.91" lambda="1.000" coolant/IAT not printed
fuel_l_h = 3.91 * 3600 / (14.70 * 0.745 * 1000) = 1.285 L/h
```

Petrol cold idle encodes a richer lambda (MAF path). Diesel idle lambda is saturated; L/h follows encoded torque from the model AFR, so coolant is no longer a flat column.

### turbo petrol 1.8 L / E0 / fuel temp

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | true AFR (model) | AFR used | lambda sat | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | Navi vs true % | naive fixed-14.7 L/h | naive vs true % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: |
| fuel -30 C | 2500 | 90 | 22.76 | 1.000 | 14.70 | 14.70 | no | 0.777 | 7.175 | 7.97 | MafDerived / LambdaMaf | 0.0 | 7.482 | 4.3 |
| fuel 0 C | 2500 | 90 | 22.76 | 1.000 | 14.70 | 14.70 | no | 0.756 | 7.377 | 8.20 | MafDerived / LambdaMaf | 0.0 | 7.482 | 1.4 |
| fuel 15 C | 2500 | 90 | 22.76 | 1.000 | 14.70 | 14.70 | no | 0.745 | 7.482 | 8.31 | MafDerived / LambdaMaf | 0.0 | 7.482 | 0.0 |
| fuel 40 C | 2500 | 90 | 22.76 | 1.000 | 14.70 | 14.70 | no | 0.727 | 7.664 | 8.52 | MafDerived / LambdaMaf | 0.0 | 7.482 | -2.4 |
| fuel 60 C | 2500 | 90 | 22.76 | 1.000 | 14.70 | 14.70 | no | 0.713 | 7.816 | 8.68 | MafDerived / LambdaMaf | 0.0 | 7.482 | -4.3 |

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
414F00000000
4161AC
4162AC
41630096
J1939 SPN174 raw=55
decoded MAF="22.76" lambda="1.000" coolant/IAT not printed
fuel_l_h = 22.76 * 3600 / (14.70 * 0.745 * 1000) = 7.482 L/h
```

MAF path: hotter fuel is less dense, so the same fuel mass is more litres. Volume PIDs would not show this.

### turbo petrol 1.8 L / E0 / intake air

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | true AFR (model) | AFR used | lambda sat | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | Navi vs true % | naive fixed-14.7 L/h | naive vs true % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: |
| IAT -30 C | 2500 | 90 | 75.25 | 0.827 | 12.15 | 12.15 | no | 0.745 | 29.928 | 33.25 | MafDerived / LambdaMaf | -0.0 | 24.736 | -17.4 |
| IAT 0 C | 2500 | 90 | 66.99 | 0.827 | 12.15 | 12.15 | no | 0.745 | 26.643 | 29.60 | MafDerived / LambdaMaf | 0.0 | 22.021 | -17.3 |
| IAT 20 C | 2500 | 90 | 62.42 | 0.827 | 12.15 | 12.15 | no | 0.745 | 24.825 | 27.58 | MafDerived / LambdaMaf | 0.0 | 20.519 | -17.3 |
| IAT 40 C | 2500 | 90 | 58.43 | 0.827 | 12.15 | 12.15 | no | 0.745 | 23.238 | 25.82 | MafDerived / LambdaMaf | -0.0 | 19.207 | -17.4 |

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
414F00000000
4161F5
4162F5
41630096
J1939 SPN174 raw=55
decoded MAF="62.42" lambda="0.827" coolant/IAT not printed
fuel_l_h = 62.42 * 3600 / (12.15 * 0.745 * 1000) = 24.825 L/h
```

Decoded MAF falls as intake air warms (ideal gas in the model). With MAF+lambda, L/h tracks that MAF; `fuel.rs` does not apply an extra IAT correction.

### turbo petrol 1.8 L / E0 / load

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | true AFR (model) | AFR used | lambda sat | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | Navi vs true % | naive fixed-14.7 L/h | naive vs true % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: |
| load 10% | 2500 | 90 | 12.45 | 1.000 | 14.70 | 14.70 | no | 0.745 | 4.093 | 4.55 | MafDerived / LambdaMaf | 0.0 | 4.093 | 0.0 |
| load 25% | 2500 | 90 | 15.81 | 1.000 | 14.70 | 14.70 | no | 0.745 | 5.197 | 5.77 | MafDerived / LambdaMaf | -0.0 | 5.197 | -0.0 |
| load 50% | 2500 | 90 | 22.76 | 1.000 | 14.70 | 14.70 | no | 0.745 | 7.482 | 8.31 | MafDerived / LambdaMaf | 0.0 | 7.482 | 0.0 |
| load 75% | 2500 | 90 | 42.18 | 1.000 | 14.70 | 14.70 | no | 0.745 | 13.865 | 15.41 | MafDerived / LambdaMaf | -0.0 | 13.865 | -0.0 |
| load 100% | 2500 | 90 | 62.42 | 0.827 | 12.15 | 12.15 | no | 0.745 | 24.825 | 27.58 | MafDerived / LambdaMaf | 0.0 | 20.519 | -17.3 |

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
414F00000000
4161AC
4162AC
41630096
J1939 SPN174 raw=55
decoded MAF="22.76" lambda="1.000" coolant/IAT not printed
fuel_l_h = 22.76 * 3600 / (14.70 * 0.745 * 1000) = 7.482 L/h
```

Fuel rate from 10% to 100% load goes **4.093** to **24.825 L/h** at this rpm.

### turbo petrol 1.8 L / E0 / throttle

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | true AFR (model) | AFR used | lambda sat | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | Navi vs true % | naive fixed-14.7 L/h | naive vs true % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: |
| throttle 0% (overrun) | 2500 | 90 | 7.84 | 2.000 | n/a | n/a | yes | n/a | 0.0 (fuel cut) | 0.0 (fuel cut) | Pid5E / Measured | 0.0 | 2.577 | n/a |
| throttle 10% | 2500 | 90 | 11.65 | 1.000 | 14.70 | 14.70 | no | 0.745 | 3.830 | 4.26 | MafDerived / LambdaMaf | 0.0 | 3.830 | 0.0 |
| throttle 25% | 2500 | 90 | 18.04 | 1.000 | 14.70 | 14.70 | no | 0.745 | 5.930 | 6.59 | MafDerived / LambdaMaf | 0.0 | 5.930 | 0.0 |
| throttle 50% | 2500 | 90 | 30.53 | 1.000 | 14.70 | 14.70 | no | 0.745 | 10.036 | 11.15 | MafDerived / LambdaMaf | -0.0 | 10.036 | -0.0 |
| throttle 75% | 2500 | 90 | 45.33 | 1.000 | 14.70 | 14.70 | no | 0.745 | 14.901 | 16.56 | MafDerived / LambdaMaf | 0.0 | 14.901 | 0.0 |
| throttle 100% | 2500 | 90 | 62.42 | 0.827 | 12.15 | 12.15 | no | 0.745 | 24.825 | 27.58 | MafDerived / LambdaMaf | 0.0 | 20.519 | -17.3 |

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
414F00000000
4161BD
4162BD
41630096
J1939 SPN174 raw=55
decoded MAF="30.53" lambda="1.000" coolant/IAT not printed
fuel_l_h = 30.53 * 3600 / (14.70 * 0.745 * 1000) = 10.036 L/h
```

Throttle 0% is overrun: diesels and warm petrol encode PID 5E = 0 (Some(0.0)); cold petrol keeps injecting.

### turbo petrol 1.8 L / combined

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | true AFR (model) | AFR used | lambda sat | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | Navi vs true % | naive fixed-14.7 L/h | naive vs true % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: |
| cold start idle -30 C | 900 | 0 | 4.71 | 0.680 | 10.00 | 10.00 | no | 0.777 | 2.183 | None (standstill) | MafDerived / LambdaMaf | -0.0 | 1.548 | -29.1 |
| motorway 110 km/h +40 C day | 2800 | 110 | 23.86 | 1.000 | 14.70 | 14.70 | no | 0.745 | 7.843 | 7.13 | MafDerived / LambdaMaf | -0.0 | 7.843 | -0.0 |
| full load 1500 m at 0 C IAT | 3500 | 80 | 84.21 | 0.827 | 12.15 | 12.15 | no | 0.745 | 33.491 | 41.86 | MafDerived / LambdaMaf | 0.0 | 27.682 | -17.3 |
| downhill fuel cut, warm | 2200 | 80 | 6.90 | 2.000 | n/a | n/a | yes | n/a | 0.0 (fuel cut) | 0.0 (fuel cut) | Pid5E / Measured | 0.0 | 2.268 | n/a |
| downhill in gear, cold engine | 2200 | 80 | 6.90 | 0.884 | 13.00 | 13.00 | no | 0.745 | 2.565 | 3.21 | MafDerived / LambdaMaf | -0.0 | 2.268 | -11.6 |
| E85 at -20 C coolant and fuel | 900 | 0 | 9.49 | 0.726 | 10.67 | 7.12 | no | 0.812 | 5.912 | None (standstill) | MafDerived / LambdaMaf | 49.9 | 3.120 | -20.9 |

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
414F00000000
4161A3
4162A3
41630096
J1939 SPN174 raw=10
decoded MAF="4.71" lambda="0.680" coolant/IAT not printed
fuel_l_h = 4.71 * 3600 / (10.00 * 0.777 * 1000) = 2.183 L/h
```

Combined points reuse the same decode path: idle L/100 km is None (standstill); downhill cut is 0.0 L/h when evidence is present.

### turbo petrol 1.8 L / fuel type

| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | true AFR (model) | AFR used | lambda sat | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | Navi vs true % | naive fixed-14.7 L/h | naive vs true % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: |
| E0 | 2500 | 90 | 22.76 | 1.000 | 14.70 | 14.70 | no | 0.745 | 7.482 | 8.31 | MafDerived / LambdaMaf | 0.0 | 7.482 | 0.0 |
| E10 | 2500 | 90 | 22.76 | 1.000 | 14.70 | 14.10 | no | 0.749 | 7.760 | 8.62 | MafDerived / LambdaMaf | 4.3 | 7.482 | 0.6 |
| E85 | 2500 | 90 | 22.76 | 1.000 | 14.70 | 9.81 | no | 0.782 | 10.676 | 11.86 | MafDerived / LambdaMaf | 49.9 | 7.482 | 5.0 |

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
414F00000000
4161AC
4162AC
41630096
J1939 SPN174 raw=55
decoded MAF="22.76" lambda="1.000" coolant/IAT not printed
fuel_l_h = 22.76 * 3600 / (14.70 * 0.745 * 1000) = 7.482 L/h
```

Same air mass and lambda: E85 uses more litres than E0 because stoich AFR and density both move toward ethanol.

### Inputs with no effect on L/h (when MAF + lambda are both present)

- **Intake air temperature:** the MAF reading is already a mass flow. The same decoded MAF and lambda give the same L/h at -30 C and +40 C IAT; IAT only changes L/h because it changes the modelled (then encoded) MAF.
- **Altitude / barometric pressure:** same rule. MAF is not pressure-corrected in `fuel.rs`. Altitude changes L/h only by changing the encoded MAF (ideal-gas air mass) and, for NA diesel, the smoke-limited AFR.
- **Throttle vs pedal:** diesels have no throttle plate; PID 5A/49 is recorded and does not enter the MAF formula.
- **Saturated lambda:** not used as AFR. Diesel idle/cruise go through torque x BSFC so coolant can still move the rate via the encoded torque.
<!-- END GENERATED ECU SCENARIOS -->
