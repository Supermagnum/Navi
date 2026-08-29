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

### Current state (2026-08 triage)

- `plugin-host` pins **wasmtime** major **`29`** (lockfile **`29.0.1`**), with
  features `cranelift` + `runtime` + `gc-drc` only (not Winch).
- **No shipped artifact links `plugin-host` today.** `navi-ffi` (Android
  `libnavi.so`), `navi-desktop`, `navi-linux`, and `driver-break-core` do not
  depend on `navi-plugin-host` / wasmtime. Example guests (`log-hello`,
  `busy-loop`) run only under CI / local isolation tests.
- Known wasmtime advisories for that pin are **suppressed in**
  [`deny.toml`](../deny.toml) on that basis (temporary until an upgrade, not a
  permanent “N/A forever” waiver). Suppressed RustSec IDs and GHSA aliases:

| RustSec | GHSA |
|---|---|
| `RUSTSEC-2025-0046` | [GHSA-fm79-3f68-h2fc](https://github.com/bytecodealliance/wasmtime/security/advisories/GHSA-fm79-3f68-h2fc) |
| `RUSTSEC-2025-0118` | [GHSA-hc7m-r6v8-hg9q](https://github.com/advisories/GHSA-hc7m-r6v8-hg9q) |
| `RUSTSEC-2026-0006` | [GHSA-vc8c-j3xm-xj73](https://github.com/advisories/GHSA-vc8c-j3xm-xj73) |
| `RUSTSEC-2026-0020` | [GHSA-852m-cvvp-9p4w](https://github.com/advisories/GHSA-852m-cvvp-9p4w) |
| `RUSTSEC-2026-0021` | [GHSA-243v-98vx-264h](https://github.com/advisories/GHSA-243v-98vx-264h) |
| `RUSTSEC-2026-0085` | [GHSA-m758-wjhj-p3jq](https://github.com/advisories/GHSA-m758-wjhj-p3jq) |
| `RUSTSEC-2026-0086` | [GHSA-m9w2-8782-2946](https://github.com/advisories/GHSA-m9w2-8782-2946) |
| `RUSTSEC-2026-0087` | [GHSA-qqfj-4vcm-26hv](https://github.com/advisories/GHSA-qqfj-4vcm-26hv) |
| `RUSTSEC-2026-0088` | [GHSA-6wgr-89rj-399p](https://github.com/advisories/GHSA-6wgr-89rj-399p) |
| `RUSTSEC-2026-0089` | [GHSA-q49f-xg75-m9xw](https://github.com/advisories/GHSA-q49f-xg75-m9xw) |
| `RUSTSEC-2026-0091` | [GHSA-394w-hwhg-8vgm](https://github.com/advisories/GHSA-394w-hwhg-8vgm) |
| `RUSTSEC-2026-0092` | [GHSA-jxhv-7h78-9775](https://github.com/advisories/GHSA-jxhv-7h78-9775) |
| `RUSTSEC-2026-0093` | [GHSA-hx6p-xpx3-jvvv](https://github.com/advisories/GHSA-hx6p-xpx3-jvvv) |
| `RUSTSEC-2026-0094` | [GHSA-f984-pcp8-v2p7](https://github.com/advisories/GHSA-f984-pcp8-v2p7) |
| `RUSTSEC-2026-0095` | [GHSA-xx5w-cvp6-jv83](https://github.com/advisories/GHSA-xx5w-cvp6-jv83) |
| `RUSTSEC-2026-0096` | [GHSA-jhxm-h53p-jm7w](https://github.com/bytecodealliance/wasmtime/security/advisories/GHSA-jhxm-h53p-jm7w) |
| `RUSTSEC-2026-0222` | [GHSA-hgjw-h833-99q9](https://github.com/bytecodealliance/wasmtime/security/advisories/GHSA-hgjw-h833-99q9) |

(Example-guest `wee_alloc` unmaintained advisory `RUSTSEC-2022-0054` /
[GHSA-rc23-xxgq-x27g](https://github.com/advisories/GHSA-rc23-xxgq-x27g) is
allowlisted separately; replace or drop that allocator before shipping
production guests that need a custom alloc.)

### Required before linking into a shipped binary

**Before** depending on `navi-plugin-host` from `navi-ffi`, the Android native
build / APK packaging path, or `navi-desktop` (or any other user-facing
artifact) for a real product plugin (APRS, Wikipedia, camping aids, …):

1. **Migrate wasmtime from major 29 to at least `36.0.7`** (or a newer
   maintained line that includes those fixes). This is a **breaking embedder
   API migration**, not a lockfile-only patch bump within 29.x. Prefer
   clearing the matching `deny.toml` ignores in the same change once CI
   `cargo deny` / `cargo audit` are clean on the new pin.
2. **Re-verify feature / backend assumptions** at migration time — do not
   assume the 2026-08 triage still holds. Today’s host uses Cranelift only,
   classic `Module`/`Linker`, no WASI, no Component Model, no pooling
   allocator, no Winch. Under that config, several Dependabot “critical”
   items (e.g. Winch sandbox escape) and many WASI/component advisories are
   not exercised; Cranelift x86-64 codegen issues are mainly relevant to
   CI/dev host-triple isolation, not the current APK/desktop link. Those
   conclusions must be re-checked against the new wasmtime version, enabled
   features, and the architectures the linked host will run on (including
   Android **aarch64**, where Cranelift guest-heap advisories such as
   `RUSTSEC-2026-0096` would matter once the host is in `libnavi.so`).

Until that migration lands, treat “product plugins not shipped” as a
**temporary** mitigation that **expires** the moment `plugin-host` is linked
into a shipped binary.

## Crates

| Crate | Role |
|---|---|
| `plugin-host` | Load manifest + `.wasm`, capability gate, fuel + epoch timeout, HostApi |
| `plugin-sdk` | `no_std` guest helpers (`host_log`, `host_position`, …) |
| `plugins/log-hello` | Reference plugin: one log line |
| `plugins/busy-loop` | Reference plugin: infinite loop (isolation tests) |
| `plugins/weather/` | Weather plugin **assets only** — Meteocons SVG trees ([`plugins/weather-plugin.md`](plugins/weather-plugin.md)); no guest `.wasm` yet |

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
| **Assets (vendored)** | `plugins/weather/icons/` (static, 2,076 SVGs) and `plugins/weather/animated-icons/` (SMIL, 1,900 SVGs); refresh via `scripts/vendor-meteocons.sh` |
| **Providers** | Free/open APIs preferred (e.g. Open-Meteo, national met services). Weather Underground–class feeds only where Terms of Use and API keys allow; keys stay in host secrets, never in WASM. |
| **Host duties** | HTTPS fetch (opt-in network), cache JSON in SQLite, rate-limit; resolve `{style}/{slug}.svg` from vendored trees |
| **Guest duties** | Select stations near route / position; map provider codes to Meteocons slugs; format HUD chips |
| **Proposed caps** | `position_read`, `weather_read` (new), `log` |
| **Offline** | Last-known cache only; no silent background refresh without user opt-in |
| **Notes** | **Assets only** — no `plugin.json` / guest WASM / `weather_read` yet. Lottie pack not vendored. |

APRS WX beacons (`b`/`t`/`h` keys) remain a radio-side path; this plugin is the
internet weather overlay.

### 3. Road info (`road_info`)

| | |
|---|---|
| **Benefit** | Closed roads, mountain convoy schedules, accidents / temporary hazards |
| **Sources** | National road authorities, DATEX-II style feeds, OSM notes/`highway=*` diffs, user reports — always opt-in network |
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
| **Notes** | Spec only — not implemented. Navi never talks to raw LoRa modules. Recommended radio: **Meshstick** USB SX1262 TCXO stick (USB head units; unique identity, traceability, flat mount face) or BLE Meshtastic boards (tablets). Text messaging is a later arm on the same PortNum dispatch. |

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
   Android APK, or `navi-desktop` until the
   [wasmtime upgrade gate](#gate-upgrade-wasmtime-before-shipping-any-product-plugin)
   is done (29 → ≥36.0.7+; breaking embedder migration).
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
