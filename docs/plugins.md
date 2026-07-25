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
[`jurisdiction-rules.md`](jurisdiction-rules.md). See [Ideas for beneficial plugins](#ideas-for-beneficial-plugins)
below as an open invitation / roadmap, not as incomplete core features.

Navi runs untrusted extension code in a **wasmtime** sandbox (`plugin-host`), not
in the routing / sensor / UI process address space as native code.

Guest plugins compile to `wasm32-unknown-unknown` and call a narrow HostApi via
WASM imports. **WASI filesystem and network are not linked** — only the
capabilities declared in the plugin manifest are wired. Host-owned code (Android
service, native accessory) may open USB/serial/network and feed sanitized
snapshots into the core; WASM guests must not get raw sockets.

## Crates

| Crate | Role |
|---|---|
| `plugin-host` | Load manifest + `.wasm`, capability gate, fuel + epoch timeout, HostApi |
| `plugin-sdk` | `no_std` guest helpers (`host_log`, `host_position`, …) |
| `plugins/log-hello` | Reference plugin: one log line |
| `plugins/busy-loop` | Reference plugin: infinite loop (isolation tests) |

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

Range display already clamps to **50–150 km** in core — plugins must respect
that (no global dump onto the map).

### 2. Weather (`weather`)

| | |
|---|---|
| **Benefit** | Current conditions / alerts along route (wind, precip, temp, pressure) |
| **Providers** | Free/open APIs preferred (e.g. Open-Meteo, national met services). Weather Underground–class feeds only where Terms of Use and API keys allow; keys stay in host secrets, never in WASM. |
| **Host duties** | HTTPS fetch (opt-in network), cache JSON in SQLite, rate-limit |
| **Guest duties** | Select stations near route / position; format HUD chips |
| **Proposed caps** | `position_read`, `weather_read` (new), `log` |
| **Offline** | Last-known cache only; no silent background refresh without user opt-in |

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
| **Host duties** | Serial/USB CAT dialect (Kenwood/Yaesu/Icom/…); never auto-TX |
| **Proposed caps** | `position_read`, `repeater_query` (new), `cat_vfo_set` (new, host-gated), `log` |
| **Safety** | Read/query free; **TX inhibited** unless user explicitly arms PTT path |

Auto-tune summary (full detail in CAT.md): if a NFM amateur repeater is within
**150 km**, resolve output frequency, shift/offset, and CTCSS/DCS, then program
**VFO 1** only.

### 5. ECU / EV telemetry (`ecu`)

| | |
|---|---|
| **Benefit** | Live fuel rate / SoC / power for eco reweight and range UI |
| **Docs** | [`ECU.md`](ECU.md) — OBD-II, J1939, MegaSquirt examples → `LiveEnergySnapshot` |
| **ICE** | `fuel_rate_l_h` from PID `5E` / J1939 LFE / MS pulse-width |
| **EV / hybrid** | `state_of_charge_pct`, `power_kw` (traction / HV), optional remaining range |
| **Host duties** | Bluetooth SPP / USB / SocketCAN; read-only diagnostics |
| **Core effect** | `refine_energy_cost` / `LiveEnergyProvider` on T1 |
| **Proposed caps** | `ecu_read` (new), `log` — no DTC clear / programming |

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
| **Benefit** | Export Navi nav state (speed, limit, next maneuver, ETA, break timing, eco, polyline) to open-source clusters and AGL via VSS/Kuksa.val, with a simple JSON fallback |
| **Docs** | [`plugins/instrument-cluster-agl-spec.md`](plugins/instrument-cluster-agl-spec.md) — VSS mapping + `Vehicle.Private.Navi.*`, `navi.cluster.v1` JSON, host-mediated publish, AGL scope boundary |
| **Host duties** | Assemble guidance snapshot; implement Kuksa.val / loopback JSON / log sinks; user opt-in; never grant raw sockets to WASM |
| **Guest duties** | Decide what/when to publish; call `vehicle_signal_publish`; enforce no-route clear for rest fields |
| **Proposed caps** | `nav_guidance_read` (new), `vehicle_signal_publish` (new), `position_read`, `log` |
| **Notes** | Spec only — not implemented. Export only (not ECU in). Not an AGL `afm` / Wayland packaging effort. |

### 10. UI language / translation (`i18n` / `ui_translation`)

| | |
|---|---|
| **Benefit** | Offline UI string packs (BCP 47 locales) so the Compose host is not English-only forever |
| **Docs** | [`plugins/i18n-translation-spec.md`](plugins/i18n-translation-spec.md) — catalog layout, fallback to English, host-owned lookup, when to show a language control |
| **Host duties** | Load packs from `{dataDir}/i18n/`; resolve message ids in Compose; persist `ui_locale`; install packs via Tools (no WASM sockets) |
| **Guest duties** | Optional: validate packs, suggest locale; must not fetch translations itself |
| **Proposed caps** | `i18n_catalog_query`, `i18n_string_resolve`, `i18n_locale_get` / `i18n_locale_set`, `plugin_kv` / `storage`, `log` |
| **Notes** | Spec only — not implemented. **Today the app UI is English only and has no language toggle.** Parallel markdown (`Norwegian.md`, etc.) is documentation, not in-app i18n. |

### 11. Animated icons (`animated_icons` / `icon_anim`)

| | |
|---|---|
| **Benefit** | Synfig-authored motion for HUD / selected markers without putting a Synfig runtime in the routing core |
| **Docs** | [`plugins/animated-icons-spec.md`](plugins/animated-icons-spec.md) — Synfig → SVG frames / packs, host player, reduce motion; static Inkscape flow stays in [`icons.md`](icons.md) |
| **Host duties** | Load `{dataDir}/icon_anim/{key}/` packs; advance frames; call existing SVG rasterize; respect reduce motion |
| **Guest duties** | Optional: validate packs / choose keys; must not download packs or play `.sif` |
| **Proposed caps** | `icon_anim_query`, `icon_anim_frame`, `plugin_kv` / `storage`, `log` |
| **Notes** | Spec only — not implemented. Core still renders one SVG per `rasterize_key` call. |

### Capability sketch (not in ABI yet)

| Proposed | Purpose |
|---|---|
| `track_upsert` | Push APRS / track stations into host `TrackStore` |
| `weather_read` | Read cached weather samples near lat/lon |
| `incident_query` / `incident_write` | Road closures / convoy / accident overlays |
| `repeater_query` | Nearest NFM repeaters from onboard DB (+ optional RepeaterBook sync) |
| `cat_vfo_set` | Ask host to program VFO 1 (frequency, offset, tone) |
| `ecu_read` | Latest `LiveEnergySnapshot` |
| `voice_speak` / `voice_pack_query` | Queue guidance utterance or list installed voice packs |
| `route_read` | Active corridor samples / junction hints for camping or resupply plugins |
| `safety_config_read` | `SafetyConfig` (e.g. `min_building_distance_m`) for shared overnight distance |
| `fuel_config_read` | Tank / energy capacity for resupply usable-range math |
| `admin_region_read` | Country / county for lat/lon (right-to-roam rule pack) |
| `clock_read` | Current date for seasonal fire-ban guidance |
| `plugin_kv` / `storage` | Small per-plugin persist (e.g. two-night camping memory, POI confirmations) |
| `nav_guidance_read` | Active route / maneuver / ETA / break / eco / polyline snapshot for cluster export |
| `vehicle_signal_publish` | Ask host to publish VSS path/values or `navi.cluster.v1` JSON (host owns Kuksa/UDP/WS) |
| `i18n_catalog_query` | List installed UI translation packs |
| `i18n_string_resolve` | Resolve message id (+ args) for the active locale |
| `i18n_locale_get` / `i18n_locale_set` | Read/write persisted UI locale preference |
| `icon_anim_query` | List installed animated-icon packs (key, fps, frames) |
| `icon_anim_frame` | Resolve SVG bytes/path for `key` + frame index |

Add a capability to `plugin-host` `Capability` enum + HostApi **before** shipping
any guest that needs it. Until then, host-native services may write into core
via UniFFI without WASM.

## Design rules for all plugins

1. **Offline-first:** network is opt-in; core routing must work with plugins
   disabled.
2. **No silent map-data mutation:** OSM extracts and graph caches change only via
   explicit user actions ([`osm-updates.md`](osm-updates.md)).
3. **Tier priorities:** plugins run at T0/T1 priority budgets — never block UI.
4. **Privacy:** no VIN / callsign / location upload unless the user enables it.
5. **Range discipline:** map overlays respect the same ~150 km class of filters
   used by tracks and CAT repeater search unless the user raises a documented
   clamp.
