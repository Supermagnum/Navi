# Plugin system

## Status (intentional at this stage)

The **plugin host infrastructure** is implemented and tested: wasmtime sandbox,
manifest parsing, capability gating, HostApi imports, fuel budget, and epoch
timeout isolation (`plugin-host` + `log-hello` / `busy-loop` examples).

**No product content plugins** have been built yet (allemannsretten camping-spot
spec, marine traffic, weather overlays, bathymetry, sonar, RTL-SDR/APRS-adjacent
guests, UI translation packs, animated icon packs, etc.). That is **expected and intentional**, not a bug or an oversight —
the host exists so future contributors can ship those plugins independently
without changing the navigation core. Country/region-dependent packs (camping,
future driving-hour families, horse access, …) follow the reusable pattern in
[`jurisdiction-rules.md`](jurisdiction-rules.md). **Product requirements** for
the system (not optional): users can **enable/disable** each plugin, and
hardware-facing plugins can talk over **USB** and **Bluetooth** via the host —
see [Enable / disable](#enable--disable-required) and
[External device I/O](#external-device-io--usb-and-bluetooth-required). See
[Ideas for beneficial plugins](#ideas-for-beneficial-plugins)
below as an open invitation / roadmap, not as incomplete core features.

Navi runs untrusted extension code in a **wasmtime** sandbox (`plugin-host`), not
in the routing / sensor / UI process address space as native code.

Guest plugins compile to `wasm32-unknown-unknown` and call a narrow HostApi via
WASM imports. **WASI filesystem and network are not linked** — only the
capabilities declared in the plugin manifest are wired. Host-owned code (Android
service, native accessory) may open USB/serial/network and feed sanitized
snapshots into the core; WASM guests must not get raw sockets.

## Gate: upgrade wasmtime before shipping any product plugin

**Source of truth for this gate.** `docs/status.md` only points here.

### Current state (2026-08)

- `plugin-host` pins **wasmtime** major **`48`** (lockfile **`48.0.1`**), with
  features `cranelift` + `runtime` + `gc-drc` only (not Winch).
- **No shipped artifact links `plugin-host` today.** `navi-ffi` (Android
  `libnavi.so`), `navi-desktop`, `navi-linux`, and `driver-break-core` do not
  depend on `navi-plugin-host` / wasmtime. Example guests (`log-hello`,
  `busy-loop`) run under host-triple CI isolation tests and the Android
  **aarch64** smoke (`scripts/plugin-host-android-aarch64-smoke.sh`).
- The wasmtime 29-era RustSec ignores have been **cleared from**
  [`deny.toml`](../deny.toml) (and the matching `.cargo/audit.toml` list).
  Remaining ignores are unrelated: `bincode` unmaintained (`RUSTSEC-2025-0141`)
  and example-guest `wee_alloc` (`RUSTSEC-2022-0054` /
  [GHSA-rc23-xxgq-x27g](https://github.com/advisories/GHSA-rc23-xxgq-x27g)).
  Replace or drop that allocator before shipping production guests that need a
  custom alloc.
- **Feature / backend confirmation (2026-08-29):** `cargo tree -p
  navi-plugin-host -e features -i wasmtime` shows only `cranelift`, `runtime`,
  and `gc-drc` (plus transitive internals those enable). No `wasi`,
  `component-model`, `winch`, or `pooling-allocator`. Only `plugin-host`
  declares a `wasmtime` dependency in the workspace, so a future
  `navi-ffi` → `navi-plugin-host` link will not pull alternate wasmtime
  features via another crate (guarded by `scripts/check-plugin-host-gate.sh`).
- **Android aarch64 verification (2026-08-29):**
  - **Link:** `android_isolation_smoke` cross-compiles for
    `aarch64-linux-android` (NDK) in CI job `plugin-host-android-aarch64`
    and via `scripts/plugin-host-android-aarch64-smoke.sh`.
  - **Execute (Cranelift aarch64 ISA):** the same isolation checks run as an
    `aarch64-unknown-linux-gnu` binary under **QEMU user-mode** in CI
    (`--qemu`) so Cranelift’s **aarch64** backend is exercised (the
    `RUSTSEC-2026-0096` class). Host-triple `cargo test` alone is not this check.
  - **Execute (Bionic / on-device):** verified **2026-08-29** on Samsung
    **SM-P613** (Galaxy Tab A7 Lite Wi-Fi), `ro.product.cpu.abi=arm64-v8a`,
    Android **14** (API **34**), serial `R52TB0JQEDE`, via
    `scripts/plugin-host-android-aarch64-smoke.sh --adb`. Pushed the
    NDK-linked `aarch64-linux-android` binary to `/data/local/tmp` and ran it
    under real Bionic; output `android_isolation_smoke: all checks passed`
    (capability deny, `log-hello` load/call, busy-loop fuel/timeout kill —
    matched the GNU+QEMU result; no crash, hang, or trap misclassification).
    Note: modern x86_64 Android emulators refuse arm64 system images, and the
    NDK does not ship `/system/bin/linker64` for Bionic user-mode QEMU — hence
    CI keeps `--qemu` (GNU aarch64 via apt `gcc-aarch64-linux-gnu`) while
    on-device `--adb` covers Bionic.
- **Advisory coverage:** `deny.toml` `[graph].targets` includes
  `aarch64-linux-android` / `x86_64-linux-android`; CI also runs
  `cargo deny check advisories --target aarch64-linux-android` and
  `cargo audit --target-arch aarch64 --target-os android`.

### Required before linking into a shipped binary

The version bump (wasmtime 29 → 48.0.1), deny/audit ignore cleanup, feature
graph confirmation, and Android **aarch64** isolation smoke are **done**.
**Before** depending on `navi-plugin-host` from `navi-ffi`, the Android native
build / APK packaging path, or `navi-desktop` (or any other user-facing
artifact) for a real product plugin (APRS, Wikipedia, camping aids, …):

1. **Keep the aarch64 smoke green** on the wasmtime pin you intend to ship —
   CI job `plugin-host-android-aarch64` /
   `scripts/plugin-host-android-aarch64-smoke.sh` (Android NDK link + QEMU
   aarch64 Cranelift exec). Bionic on-device execution was already verified
   once (SM-P613 / Android 14 / arm64-v8a, 2026-08-29); re-run `--adb` after
   wasmtime or host embedder changes that touch fuel/epoch/trap paths.
2. **Re-run** `scripts/check-plugin-host-gate.sh` after any wasmtime or
   workspace dependency change so WASI / Component Model / Winch features
   cannot appear via feature unification.
3. Then remove the premature-link guard in that script (and this gate section)
   in the same change that adds the `navi-ffi` / desktop dependency.

Until a product link lands, treat “product plugins not shipped” as intentional;
the premature-link CI guard fails the build if `navi-ffi`, `navi-desktop`, or
`navi-linux` grows a `navi-plugin-host` dependency early.

## Crates

| Crate | Role |
|---|---|
| `plugin-host` | Load manifest + `.wasm`, capability gate, fuel + epoch timeout, HostApi |
| `plugin-sdk` | `no_std` guest helpers (`host_log`, `host_position`, …) |
| `plugins/log-hello` | Reference plugin: one log line |
| `plugins/busy-loop` | Reference plugin: infinite loop (isolation tests) |
| `plugins/weather/` | Weather plugin — Meteocons assets + guest scaffold; product HUD/map use host UniFFI ([`plugins/weather-plugin.md`](plugins/weather-plugin.md)) |

## Manifest (`plugin.json`)

```json
{
  "name": "log_hello",
  "version": "0.1.0",
  "entry": "plugin_main",
  "capabilities": ["log"],
  "fuel_limit": 5000000,
  "timeout_ms": 500,
  "wasm": "plugin.wasm"
}
```

Capabilities are validated **before** the module is instantiated. Unknown
capability names reject the load. Requested capabilities must be a subset of the
host policy set passed to `PluginHost::load_dir`.

## Enable / disable (required)

Every installed plugin **must** be individually **enableable and disableable** by
the user. This is a product requirement for the plugin system, not an optional
per-plugin nicety.

| Rule | Detail |
|---|---|
| **Per-plugin toggle** | Host UI (Tools / Settings) shows one on/off control per installed plugin (manifest `name`). |
| **Persisted** | Enabled/disabled state is stored on device (e.g. `plugin_kv` / host preferences). Survives app restart. |
| **Default** | Newly installed plugins start **disabled** until the user turns them on (opt-in), unless a ship checklist documents an exception. |
| **Disabled = not called** | A disabled plugin is not invoked on route plan, tick, or event hooks. Its HostApi imports must not run. Capability grants alone do not keep it live. |
| **Core without plugins** | Routing, map, GPS, and HUD must keep working with **all** plugins disabled ([Design rules](#design-rules-for-all-plugins)). |
| **Load vs enable** | The host may still **discover** / validate a `.wasm` + `plugin.json` while disabled; enable only gates **execution** and side effects (audio, overlays, accessory I/O). |

Future host API sketch (not in ABI yet): `plugin_list` / `plugin_set_enabled(name, bool)`
owned by the Android/desktop host, not by guest WASM.

## External device I/O — USB and Bluetooth (required)

Plugins that talk to hardware **must** be able to use **USB** and/or **Bluetooth**
(including BLE where the device requires it). Guests never open raw sockets or
raw HID/serial themselves — the **host** owns the transport and feeds sanitized
data (or accepts sanitized commands) through capability-gated HostApi imports.

| Rule | Detail |
|---|---|
| **Transports** | **USB** (USB-serial, USB accessory / AOA, wired UART over USB) and **Bluetooth** (Classic SPP and/or BLE GATT) are first-class plugin I/O paths. |
| **Host-mediated** | WASM guests request open/read/write/close via HostApi. Host opens the OS device, enforces permissions, rate-limits, and may disconnect on disable / app background policy. |
| **User consent** | Pairing, USB permission dialogs, and “allow this plugin to use accessory X” stay in the host UI. Disabling a plugin closes its sessions. |
| **No silent TX** | Radio CAT, OBD write, or any transmit path stays gated (read/query free where safe; TX only with explicit arm — see CAT / ECU ideas below). |
| **Offline routing** | Accessory I/O must not block T2 UI or A*. Dropped links degrade the plugin feature, not core nav. |

### Proposed capabilities (not in ABI yet)

| Proposed | Purpose |
|---|---|
| `accessory_list` | List paired / attached USB and Bluetooth devices the host is willing to expose |
| `accessory_open` | Open a session (plugin id + device id + mode: usb-serial / bt-spp / ble) |
| `accessory_read` / `accessory_write` | Exchange bytes or framed messages; host may impose max length / rate |
| `accessory_close` | Release the session (also implied when the plugin is disabled) |
| `accessory_events` | Optional push of connect/disconnect / permission-denied |

Until those land, **host-native** services (as already sketched for ECU Bluetooth /
USB, CAT serial/USB, DIY e-bike USB-serial) may open the link and push snapshots
into core; a future WASM guest then only calls `ecu_read` / `cat_vfo_set` /
`ebike_telemetry_read`-class imports.

Specs that need a cable or radio (**ECU**, **CAT**, **ebike_telemetry**,
**lora_convoy**, APRS TNC, future instruments) must document which
transport(s) they use and that they honour enable/disable (sessions closed
when off).

## Debug files (USB/MTP)

On-device diagnostic and debug artifacts must be **USB/MTP-retrievable** without
adb, under the same shared tree as the core diagnostic session log
([`debugging.md`](debugging.md#3b-diagnostic-session-log-on-device-file)):

| Writer | Path |
|---|---|
| **Core** (Tools → Diagnostic logging) | `Documents/debug/navi_session_YYYY-MM-DD_HH-mm-ss.log` |
| **Plugin** | `Documents/debug/<plugin-name>/…` |

Examples:

```text
Documents/debug/navi_session_2026-07-31_12-52-13.log
Documents/debug/right-to-roam-camping/session_….log
Documents/debug/safety-resupply/last_run.json
```

In a file browser / MTP mount:

```text
Internal storage → Documents → debug → <plugin-name> → …
```

Rules for plugin authors:

1. Use a stable folder name matching the plugin manifest `name` (lowercase,
   hyphen or underscore — same string users will see on USB).
2. Prefer `Documents/debug/<plugin-name>/`. If the host falls back to
   `Download/debug` for the core log, plugins must use that same root’s
   `debug/<plugin-name>/` tree.
3. Do **not** write plugin debug files into app-private storage only, into the
   core `navi_session_*.log` file, or into a top-level `/debug` folder (not
   creatable without `MANAGE_EXTERNAL_STORAGE`).
4. The current `log` HostApi import goes to the host logger (logcat / host
   process); durable USB-visible files need a future host capability (or
   host-owned code writing into `Documents/debug/<plugin-name>/` on the
   guest’s behalf). Specs that mention “diagnostics” should target this layout.

## Capabilities (HostApi) — implemented today

| Capability | Import | Purpose |
|---|---|---|
| `log` | `navi.log(ptr, len)` | UTF-8 log line to the host |
| `position_read` | `navi.get_position(out_ptr) -> i32` | Write `lat,lon` as two little-endian `f64` |
| `poi_query` | `navi.poi_query(...)` | JSON POI list into guest buffer |
| `poi_write` | `navi.poi_write(ptr, len)` | Upsert POI from guest JSON |

## Isolation limits

- **Fuel**: instruction budget (`DEFAULT_FUEL` = 5_000_000 unless overridden).
- **Wall-clock**: epoch interruption (`DEFAULT_TIMEOUT_MS` = 250 unless overridden).

A plugin that busy-loops is terminated with `CallOutcome::FuelExhausted` or
`CallOutcome::Timeout`. Isolation is covered by
`plugin-host/tests/isolation.rs` (load→call→return for `log-hello`; kill +
host-thread heartbeat for `busy-loop`).

## Building a reference plugin

```bash
cargo build --release --target wasm32-unknown-unknown \
  --manifest-path plugins/log-hello/Cargo.toml
```

Copy the produced `.wasm` next to `plugin.json` as `plugin.wasm`, then:

```bash
cargo test -p navi-plugin-host --test isolation -- --nocapture
```

---

## Ideas for beneficial plugins

None of the plugins below are shipped yet. They are the intended extension
surface: each should stay sandboxed (or host-side with a thin capability), push
normalized data into core stores, and never starve T2 UI/audio. Protocol and
DSP docs already exist where noted.

### 1. APRS (`aprs` / `aprs_sdr`)

| | |
|---|---|
| **Benefit** | Live tactical positions, weather beacons, short messages on map |
| **Docs to implement** | [`APRS.md`](APRS.md) (fields, `TrackStore` range), [`APRS-SDR.md`](APRS-SDR.md) (AFSK/AX.25 stages, IF offset), crate [`rtl-sdr-rs`](https://crates.io/crates/rtl-sdr-rs) for IQ |
| **Host duties** | Own RTL-SDR / TNC; decode frames; upsert `TrackStation` |
| **Guest duties** | Optional: filter/annotate beacons, map symbol keys, messaging UI helpers |
| **Proposed caps** | `position_read`, `track_upsert` (new), `log`; network/USB stay host-side |
| **Related work** | [SDRoxide issue #150](https://github.com/dividebysandwich/sdroxide/issues/150#event-29920362864) — Navi asked about an APRS plugin; upstream is considering APRS as built-in with the **decoder in its own crate**, which this plugin could reuse rather than duplicating AFSK/AX.25 |

Range display already clamps to **50–150 km** in core — plugins must respect
that (no global dump onto the map).

### 2. Weather (`weather`)

| | |
|---|---|
| **Benefit** | Current conditions / alerts along route (wind, precip, temp, pressure) |
| **Docs** | [`plugins/weather-plugin.md`](plugins/weather-plugin.md) — asset layout, provider mapping. Icon purposes: [`plugins/weather-icons-reference.md`](plugins/weather-icons-reference.md) |
| **Assets (vendored)** | `plugins/weather/icons/` (static) and `plugins/weather/animated-icons/` (SMIL); refresh via `scripts/vendor-meteocons.sh` |
| **Providers** | MET Norway Locationforecast (primary) → Open-Meteo failover. Attribution without the “Yr” brand. |
| **Host duties** | HTTPS fetch (opt-in), SQLite cache, rate-limit / Expires+IMS; resolve `fill/{slug}.svg`; HUD chip + optional city map symbols |
| **Guest duties** | Scaffolded WASM guest (`plugin.json`); product APK does not load plugin-host yet |
| **Caps** | `position_read`, `weather_read`, `log` (in ABI; unused by product until wasmtime gate lifts) |
| **Offline** | Last-known cache only; no silent background refresh without user opt-in |
| **Shipped UI** | Tools → **Weather overlay** (default OFF); nested **Show weather symbols on map** (default OFF; `place:city`, zoom ≤ 8, cap 10, 56 px, nearest-to-center) |
| **Not shipped** | SMIL / other icon styles; town/village tiers; viewport-batching; corridor overlay; safety-resupply WBGT hookup; product plugin-host link |

APRS WX beacons (`b`/`t`/`h` keys) remain a radio-side path; this plugin is the
internet weather overlay.

### 3. Road info (`road_info`)

| | |
|---|---|
| **Benefit** | Closed roads, mountain convoy schedules, accidents / temporary hazards |
| **Sources** | National road authorities, DATEX-II style feeds, OSM notes/`highway=*` diffs, user reports — always opt-in network |
| **Research** | [`plugins/traffic-information.md`](plugins/traffic-information.md) — why a free / global / ~1-minute source does not exist today; RTL-SDR RDS-TMC / DAB-TPEG alternative under consideration |
| **NPRA DATEX client** | [`plugins/datex-npra-client.md`](plugins/datex-npra-client.md) — Norway DATEX II v3.1 pull (access form, Basic Auth, snapshot endpoints; not shipped) |
| **Host duties** | Fetch + validate; store incidents with bbox + expiry |
| **Core effect** | Soft or hard edge penalties / avoid flags during A* (future graph hook) |
| **Proposed caps** | `position_read`, `incident_query` / `incident_write` (new), `log` |
| **UI** | Map banners + route recalc prompt; never rewrite the `.pbf` silently |

### 4. CAT radio control (`cat`)

| | |
|---|---|
| **Benefit** | Set VFO frequency / offset / CTCSS from nearby NFM repeaters while driving |
| **Docs** | [`CAT.md`](CAT.md) — auto-tune algorithm, RepeaterBook + OSM onboard DB, Innlandsnettet example |
| **Host duties** | Serial/USB CAT dialect (Kenwood/Yaesu/Icom/…); never auto-TX; honour plugin enable/disable (close sessions when off) |
| **Proposed caps** | `position_read`, `repeater_query` (new), `cat_vfo_set` (new, host-gated), `accessory_*` (USB), `log` |
| **Safety** | Read/query free; **TX inhibited** unless user explicitly arms PTT path |

Auto-tune summary (full detail in CAT.md): if a NFM amateur repeater is within
**150 km**, resolve output frequency, shift/offset, and CTCSS/DCS, then program
**VFO 1** only.

Same client/display split as the planned LoRa convoy plugin (Navi does not
implement the RF/mesh layer): [`plugins/lora-convoy-spec.md`](plugins/lora-convoy-spec.md).

### 5. ECU / EV telemetry (`ecu`)

| | |
|---|---|
| **Benefit** | Live fuel rate / SoC / power for eco reweight and range UI |
| **Docs** | [`ECU.md`](ECU.md) — OBD-II, J1939, MegaSquirt examples → `LiveEnergySnapshot` |
| **ICE** | `fuel_rate_l_h` from PID `5E` / J1939 LFE / MS pulse-width |
| **EV / hybrid** | `state_of_charge_pct`, `power_kw` (traction / HV), optional remaining range |
| **Host duties** | Bluetooth SPP / USB / SocketCAN; read-only diagnostics; honour plugin enable/disable |
| **Core effect** | `refine_energy_cost` / `LiveEnergyProvider` on T1 |
| **Proposed caps** | `ecu_read` (new), `accessory_*` (USB / Bluetooth), `log` — no DTC clear / programming |

### 5b. DIY wired e-bike telemetry (`ebike_telemetry`)

| | |
|---|---|
| **Benefit** | Live assist power, pack SoC, regen, and DIY-computed remaining time/distance for Electric Cycle — supplements physics-based `EbikeConfig` estimates when a custom display/BMS is cabled in |
| **Docs** | [`ebike-telemetry-diy.md`](ebike-telemetry-diy.md) — open `$NAVIPWR` ASCII over USB-serial (CAN optional); no open cross-vendor standard exists |
| **Transport** | **Wired USB-serial** primary; CAN secondary; **Bluetooth/BLE** allowed when the DIY display/BMS exposes it — same host-mediated `accessory_*` path as ECU |
| **Host duties** | Open UART/CAN/BT; parse checksummed `$NAVIPWR`; expose latest snapshot (WASM cannot open serial); close on plugin disable |
| **Core effect** | Feed `LiveEnergySnapshot`-class SoC/power (+ optional remaining); physics climb/range stays default offline |
| **Proposed caps** | `ebike_telemetry_read` (new) / host `read_ebike_serial_telemetry()`, `log` |
| **Notes** | Spec only — not implemented. Distinct from reverse-engineering commercial Bosch/Bafang/STEPS buses (deferred, high maintenance). |

### 6. Voice guidance (`voice` / `voice_guidance`)

| | |
|---|---|
| **Benefit** | Turn-by-turn spoken directions (recorded packs; optional Piper TTS) |
| **Docs** | [`voice-guidance.md`](voice-guidance.md) — clip layout, fragment keys, localization open questions, rodio/cpal and Piper/ONNX spikes |
| **Host duties** | Audio output (rodio/cpal or Android-native fallback); load `/sounds/<lang>/<gender>/`; mute/volume + language/gender settings |
| **Guest duties** | Optional: phrase assembly / pack selection; triggers from nav maneuver state |
| **Proposed caps** | `position_read`, `voice_speak` / `voice_pack_query` (new), `log` |
| **Notes** | Recorded voice is the default path; Piper is additive and gated on Android ONNX. Spoken guidance is a legitimate foreground audio interruption (unlike silent background routing). Per-language concat vs whole-phrase clips is an open design question. |

### 7. Right-to-roam overnight camping (`right_to_roam_camping`)

| | |
|---|---|
| **Benefit** | Suggest legal wild-camping positions along a route where a broad right-to-roam exists (Norway *allemannsretten* and country-aware packs) |
| **Docs** | [`plugins/right-to-roam-camping-spec.md`](plugins/right-to-roam-camping-spec.md) — intersection/track candidates, shared 150 m `SafetyConfig`, two-night plugin state, fire/foraging/leave-no-trace guidance, multi-country rules |
| **Host duties** | Expose route/junction hints, POI/area queries, safety config, admin region, clock; render suggestion cards + disclaimer |
| **Guest duties** | Rank road∩track seeds, walk short distance along track, filter by country rules, persist two-night keys, assemble guidance text |
| **Proposed caps** | `position_read`, `poi_query`, `route_read`, `safety_config_read`, `admin_region_read`, `clock_read`, `plugin_kv` / `storage`, `log` |
| **Notes** | Spec only — not implemented. Informational guidance, not legal advice. No Nordic wild-camp logic for England/Wales or Denmark. Country packs and “decline rather than guess” are the reference example in [`jurisdiction-rules.md`](jurisdiction-rules.md). |

### 8. Safety / resupply lookahead (`safety` / `resupply_safety`)

| | |
|---|---|
| **Benefit** | Before departure, warn when the largest gap between *reliable* fuel or water stops exceeds usable range (tank/load minus conservative buffer), with stricter buffers in remote/arid terrain |
| **Docs** | [`plugins/safety-resupply.md`](plugins/safety-resupply.md) — confidence scoring, Köppen/remote mode, buffer selection, lookahead, heat/water for foot travel, confirmation cache |
| **Host duties** | Corridor POI scan, tank/water capacity, HUD chips / spoken summary, optional weather WBGT; persist confirmations |
| **Guest duties** | Score POIs, decide remote mode, pick buffer, run lookahead gap detection, optional heat water estimate |
| **Proposed caps** | `position_read`, `poi_query`, `route_read`, `fuel_config_read`, `plugin_kv` / `storage`, `weather_read` (optional), `log` |
| **Notes** | Spec only — not implemented. Distinct from core `SafetyConfig` (POI radii / overnight distances). Conservative guidance, not a supply guarantee. |

### 9. Instrument cluster / AGL signal export (`instrument_cluster`)

| | |
|---|---|
| **Benefit** | Export Navi nav state (speed, limit, overspeed chrome, next maneuver, ETA, break timing, eco, polyline, **and merged approach warnings** — road signs, children-zone proximity, speed cameras, seasonal closures) to open-source clusters and AGL via VSS/Kuksa.val, with a simple JSON fallback |
| **Docs** | [`plugins/instrument-cluster-agl-spec.md`](plugins/instrument-cluster-agl-spec.md) — VSS mapping + `Vehicle.Private.Navi.*` (including `Warning.*`), `navi.cluster.v1` JSON, host-mediated publish, AGL scope boundary |
| **Host duties** | Assemble guidance + **merged warning** snapshot (same UniFFI / merge order as map chrome); implement Kuksa.val / loopback JSON / log sinks; user opt-in; never grant raw sockets to WASM |
| **Guest duties** | Decide what/when to publish; call `vehicle_signal_publish`; enforce no-route clear for rest fields and clear warning leaves when chrome is hidden |
| **Proposed caps** | `nav_guidance_read` (new), `vehicle_signal_publish` (new), `position_read`, `log` |
| **Notes** | Spec only — not implemented. Export only (not ECU in; not alert audio). Not an AGL `afm` / Wayland packaging effort. |

### 10. UI language / translation (`i18n` / `ui_translation`)

| | |
|---|---|
| **Benefit** | Offline UI string packs (BCP 47 locales) so the Compose host is not English-only forever |
| **Docs** | [`plugins/i18n-translation-spec.md`](plugins/i18n-translation-spec.md) — catalog layout, fallback to English, host-owned lookup, when to show a language control |
| **Host duties** | Load packs from `{dataDir}/i18n/`; resolve message ids in Compose; persist `ui_locale`; install packs via Tools (no WASM sockets) |
| **Guest duties** | Optional: validate packs, suggest locale; must not fetch translations itself |
| **Proposed caps** | `i18n_catalog_query`, `i18n_string_resolve`, `i18n_locale_get` / `i18n_locale_set`, `plugin_kv` / `storage`, `log` |
| **Notes** | Spec only — not implemented. **Today the app UI is English only and has no language toggle.** Do not infer UI language from GPS/SIM. Parallel markdown (`docs/Norwegian.md`, etc.) is documentation, not in-app i18n. |

### 11. Animated icons (`animated_icons` / `icon_anim`)

| | |
|---|---|
| **Benefit** | Synfig-authored motion for HUD / selected markers without putting a Synfig runtime in the routing core |
| **Docs** | [`plugins/animated-icons-spec.md`](plugins/animated-icons-spec.md) — Synfig → SVG frames / packs, host player, reduce motion; static Inkscape flow stays in [`icons.md`](icons.md) |
| **Host duties** | Load `{dataDir}/icon_anim/{key}/` packs; advance frames; call existing SVG rasterize; respect reduce motion |
| **Guest duties** | Optional: validate packs / choose keys; must not download packs or play `.sif` |
| **Proposed caps** | `icon_anim_query`, `icon_anim_frame`, `plugin_kv` / `storage`, `log` |
| **Notes** | Spec only — not implemented. Core still renders one SVG per `rasterize_key` call. Road-sign catalogue (`no_sign_*`) remains static NLOD art unless an optional anim overlay is added later. |

### 12. Custom alert sounds (`custom_alert_sounds` / `alert_sounds`)

| | |
|---|---|
| **Benefit** | User-customizable short audio alerts for overspeed, road-sign approach warnings (including children-zone proximity fallback), speed cameras, seasonal closures, and future hazard categories |
| **Docs** | [`plugins/custom-alert-sounds-spec.md`](plugins/custom-alert-sounds-spec.md) — category mapping, sound pack layout, urgency-phase timing, host `alert_sound_play` capability |
| **Host duties** | Shared rodio/Symphonia playback (same stack as voice guidance); phase-transition events; resolve `{dataDir}/sounds/alerts/` files; audio focus / mute |
| **Guest duties** | Map warning events → category → clip; per-category enable flags; debounce overspeed repeats |
| **Proposed caps** | `warning_event_subscribe` (new), `alert_sound_play` (new), `alert_sound_catalog` (new), `plugin_kv` / `storage`, `log` |
| **Notes** | Spec only — not implemented. One short tone at **urgency** phase entry (750/150/25 m model), not continuous audio through the approach window. |

### 13. Horse trekking (`horse_trekking` / `horse_trek`)

| | |
|---|---|
| **Benefit** | Horse-scale water and support-service lookahead, per-protected-area access guidance, and field notes for equestrian trekking — while core Horse routing preference stays a soft cost model (prefer unpaved / bridleway / `route=horse`, town-bypass penalty) |
| **Docs** | [`plugins/horse-trekking-spec.md`](plugins/horse-trekking-spec.md) — BRouter-informed cost preference (reuse network-pref shape), horse daily water volumes, Norwegian *verneforskrift* per-area rules, vet/farrier/stable POIs, Tysbast informational note; core profile walkthrough [`horse-profile.md`](horse-profile.md) |
| **Host duties** | Corridor POI scan, protected-area queries, jurisdiction packs; render advisory cards + disclaimer; optional soft-pref weights when Horse profile exists |
| **Guest duties** | Water-gap / support-service lookahead, protected-area pack lookup (decline if unknown), static toxic-plant content |
| **Proposed caps** | `position_read`, `poi_query`, `route_read`, `admin_region_read`, `protected_area_query` (new), `plugin_kv` / `storage`, `log` |
| **Notes** | Spec only — not implemented. **Hiking remains the accepted interim stopgap** and does not apply horse-specific access, pace, water, or park rules. |

### 14. Adaptive speed warning (`adaptive_speed_warning` / `speed_warning`)

| | |
|---|---|
| **Benefit** | Escalating spoken overspeed alerts by **percentage over** the HUD’s applicable limit, with a long arm delay so routine overtakes do not nag, and faster disarm when the driver corrects |
| **Docs** | [`plugins/adaptive-speed-warning-spec.md`](plugins/adaptive-speed-warning-spec.md) — `overPct` tiers, arm/disarm timers, GNSS/HUD floor, road-class modulation, split from custom-alert-sounds tones |
| **Host duties** | Authoritative speed/limit/highway snapshot (`road_near_info` / `OverspeedHud`); `voice_speak` + optional `overspeed` earcon; audio focus / mute; USB debug under `Documents/debug/adaptive-speed-warning/` |
| **Guest duties** | Tier state machine, arm/disarm, phrase-key selection; configurable constants in `plugin_kv` |
| **Proposed caps** | `road_speed_state_read` (new), `voice_speak` / `voice_pack_query`, `alert_sound_play`, `plugin_kv` / `storage`, `admin_region_read` (optional), `log` |
| **Notes** | Spec only — not implemented. Motor profiles only. Not a ticket predictor. HUD chrome stays display-only. |

### 15. LoRa convoy status (`lora_convoy` / `convoy`)

| | |
|---|---|
| **Benefit** | Share location, speed, fuel, and vehicle battery charge across a convoy over a LoRa mesh so low fuel is visible to the rest of the group without requiring direct radio range |
| **Docs** | [`plugins/lora-convoy-spec.md`](plugins/lora-convoy-spec.md) — Meshtastic client/display split (same shape as CAT), private-portnum `VehicleStatus`, two independent BLE sessions (radio vs Android companion), seq/stale table |
| **Host duties** | BLE central to a Meshtastic-flashed node (`meshtastic` crate, `bluetooth-le` + `tokio`); separate BLE session for passenger Android GATT writes; encode/decode; `convoy_store`; overlay + warn thresholds; close both sessions on disable |
| **Guest duties** | Optional: chip formatting / list sort after wasmtime gate; must not open BLE |
| **Proposed caps** | `accessory_*` (two BLE sessions), `convoy_status_read` (new), `position_read`, `ecu_read` / `ebike_telemetry_read` (optional onboard), `plugin_kv` / `storage`, `log` |
| **Safety** | Driver does not enter fuel/battery on the Navi device while moving — companion is passenger-operable. Mesh TX only while the plugin is enabled. Informational overlay, not collision avoidance. |
| **Notes** | Spec only — not implemented. Navi never talks to raw LoRa modules. Recommended radio: [**Meshstick**](https://www.elecrow.com/meshstick-usb-to-spi-sx1262-tcxo-lora-usb-stick-usb-plug-and-play-meshtastic-lora-mesh-node.html) USB SX1262 TCXO stick (USB head units; unique identity, traceability, flat mount face) or BLE Meshtastic boards (tablets). Text messaging is a later arm on the same PortNum dispatch. |

### Capability sketch (not in ABI yet)

| Proposed | Purpose |
|---|---|
| `track_upsert` | Push APRS / track stations into host `TrackStore` |
| `weather_read` | Read cached weather samples near lat/lon |
| `incident_query` / `incident_write` | Road closures / convoy / accident overlays |
| `repeater_query` | Nearest NFM repeaters from onboard DB (+ optional RepeaterBook sync) |
| `cat_vfo_set` | Ask host to program VFO 1 (frequency, offset, tone) |
| `ecu_read` | Latest `LiveEnergySnapshot` |
| `ebike_telemetry_read` | Latest DIY `$NAVIPWR` / wired e-bike snapshot (host owns serial) |
| `voice_speak` / `voice_pack_query` | Queue guidance utterance or list installed voice packs |
| `route_read` | Active corridor samples / junction hints for camping or resupply plugins |
| `safety_config_read` | `SafetyConfig` (e.g. `min_building_distance_m`) for shared overnight distance |
| `fuel_config_read` | Tank / energy capacity for resupply usable-range math |
| `admin_region_read` | Country / county for lat/lon (right-to-roam rule pack) |
| `clock_read` | Current date for seasonal fire-ban guidance |
| `plugin_kv` / `storage` | Small per-plugin persist (e.g. two-night camping memory, POI confirmations) |
| `nav_guidance_read` | Active route / maneuver / ETA / break / eco / polyline / **merged approach warning** / overspeed snapshot for cluster export |
| `vehicle_signal_publish` | Ask host to publish VSS path/values or `navi.cluster.v1` JSON (host owns Kuksa/UDP/WS) |
| `i18n_catalog_query` | List installed UI translation packs |
| `i18n_string_resolve` | Resolve message id (+ args) for the active locale |
| `i18n_locale_get` / `i18n_locale_set` | Read/write persisted UI locale preference |
| `icon_anim_query` | List installed animated-icon packs (key, fps, frames) |
| `icon_anim_frame` | Resolve SVG bytes/path for `key` + frame index |
| `protected_area_query` | Protected-area polygons / ids crossed by corridor (horse *verneforskrift* packs) |
| `warning_event_subscribe` | Push approach-phase transitions for road signs, cameras, overspeed, closures |
| `alert_sound_play` | Queue short alert clip by category (host owns audio device) |
| `alert_sound_catalog` | List bundled + user override alert sound files |
| `road_speed_state_read` | Speed, applicable limit, HUD overspeed flag, highway class, profile (adaptive speed warning) |
| `accessory_list` / `accessory_open` / `accessory_read` / `accessory_write` / `accessory_close` | Host-mediated **USB** and **Bluetooth** (SPP/BLE) I/O for hardware plugins |
| `convoy_status_read` | Last-known convoy table (position, speed, fuel/battery, seq, stale) for LoRa convoy UI |
| `plugin_list` / `plugin_set_enabled` | Host-owned inventory and per-plugin enable/disable (UI + persistence) |

Add a capability to `plugin-host` `Capability` enum + HostApi **before** shipping
any guest that needs it. Until then, host-native services may write into core
via UniFFI without WASM.

## Design rules for all plugins

1. **Wasmtime ship gate:** do not link `plugin-host` into `navi-ffi`, the
   Android APK, or `navi-desktop` until the remaining steps in the
   [wasmtime upgrade gate](#gate-upgrade-wasmtime-before-shipping-any-product-plugin)
   are done (aarch64 smoke kept green; gate script updated when linking).
   The crate pin is 48.0.1 with Cranelift-only features confirmed.
2. **Offline-first:** network is opt-in; core routing must work with plugins
   disabled.
3. **Enable / disable:** every plugin has a user-facing on/off control; disabled
   plugins are not executed and must release USB/Bluetooth sessions
   ([Enable / disable](#enable--disable-required)).
4. **USB / Bluetooth:** hardware-facing plugins use host-mediated USB and/or
   Bluetooth — never raw guest sockets
   ([External device I/O](#external-device-io--usb-and-bluetooth-required)).
5. **No silent map-data mutation:** OSM extracts and graph caches change only via
   explicit user actions ([`osm-updates.md`](osm-updates.md)).
6. **Tier priorities:** plugins run at T0/T1 priority budgets — never block UI.
7. **Privacy:** no VIN / callsign / location upload unless the user enables it.
8. **Range discipline:** map overlays respect the same ~150 km class of filters
   used by tracks and CAT repeater search unless the user raises a documented
   clamp.
