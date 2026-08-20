# Custom alert sounds plugin (specification)

**Status:** specification only — not implemented.  
**Path:** `docs/plugins/custom-alert-sounds-spec.md`  
**Architecture:** planned WASM guest via `plugin-host` / `plugin-sdk` and
capability-gated `HostApi` ([`plugins.md`](../plugins.md)).

Working title / id suggestion: `custom_alert_sounds` / `alert_sounds`.

This plugin maps **warning categories** already implemented (or planned) in Navi
to **user-customizable audio clips**. It does **not** own playback machinery —
that reuses the host audio stack planned for voice guidance
([`voice-guidance.md`](../voice-guidance.md): `rodio` + Symphonia decode,
`cpal`/Android-native output fallback).

---

## Goals

1. Play short, distinct alert tones when specific safety warnings enter the
   **urgency** phase of the shared approach-distance model (see
   [§ Alert timing](#alert-timing)).
2. Let users drop in their own sound files per category via a documented folder
   layout under app storage (with a sensible bundled default set).
3. Subscribe to the same warning-event stream that drives visual approach chrome
   (`RoadSignWarningBox`, speed-camera box, overspeed HUD, seasonal-closure
   notices) — no second independent distance clock.
4. Keep real audio device access in the **trusted native host**; the WASM guest
   decides *which* clip to request *when*, consistent with the instrument-cluster
   plugin’s sandbox reasoning ([`instrument-cluster-agl-spec.md`](instrument-cluster-agl-spec.md)).

## Non-goals

- Building a new audio stack (no second decoder/output path in the guest).
- Continuous or looping alerts through the whole 750 m approach window.
- Replacing turn-by-turn voice guidance ([`voice-guidance.md`](../voice-guidance.md)).
- Fetching sound packs from the network inside WASM.
- Legal advice on horn use or local noise regulations — user responsibility.

---

## Relationship to existing UX

| Visual surface | Shared state | Alert category (this plugin) |
|---|---|---|
| Bottom HUD overspeed styling | Posted limit vs GPS speed | `overspeed` |
| `RoadSignWarningBox` (tagged signs) | `nearest_road_sign_warning_json` | `road_sign_*` (by sign class — see below) |
| `RoadSignWarningBox` (children-zone fallback) | `nearest_school_proximity_warning_json` | `children_proximity` |
| Speed-camera approach box | `nearest_speed_camera_warning_json` | `speed_camera_point` / `speed_camera_section` |
| Seasonal closure / conditional access banner | Route-plan / graph conditional eval | `seasonal_closure` |
| (Future) road-info / incident overlay | Planned `road_info` plugin | `incident_ahead` (placeholder) |
| (Future) weather-plugin wind/hazard chips | Planned `weather` plugin | `weather_hazard` (placeholder) |

Explicit tagged road signs **outrank** proximity fallback in the host merge
order today; the plugin must respect the same merged warning the UI shows — not
fire for a suppressed fallback.

---

## Alert timing

Reuse the approach distance phases documented in
[`approach-instructions.md`](../approach-instructions.md) and
[`road-signs.md`](../road-signs.md):

| Phase | Distance | Visual | Audio (this plugin) |
|---|---|---|---|
| Hidden | &gt; 750 m or ≤ 25 m passed | Hidden | **Silent** |
| Appear | ≤ 750 m and &gt; 150 m | Standard box | **Silent** (optional subtle chime is a product decision — default **off**) |
| Urgency | ≤ 150 m | Stronger emphasis | **Single short alert** on **first entry** into urgency for this warning instance |
| Passed / reroute / no route | — | Hide | Cancel pending alert; do not replay until a new warning instance |

Rules:

1. **One shot per warning instance** at urgency entry — not on every GPS poll.
2. **No continuous audio** while the driver remains in the 150–25 m band.
3. **Debounce** when multiple categories would fire for the same fix: play at
   most one clip per urgency transition; use host merge priority (same order as
   visual chrome: explicit sign &gt; children proximity &gt; camera, etc.).
4. **Overspeed:** fire when crossing into sustained overspeed (host-defined
   threshold aligned with HUD red styling), with a minimum repeat interval
   (e.g. 60 s) so a long overspeed stretch does not spam — distinct from
   one-shot road-sign urgency. **Spoken escalating nags** are a different
   plugin ([`adaptive-speed-warning-spec.md`](adaptive-speed-warning-spec.md));
   when that guest is enabled, this category is an **earcon at tier
   transition** only, not a parallel beep loop.

---

## Sound pack structure

Stable layout so a category pack is a **folder drop-in** (bundled defaults under
assets; user overrides under writable storage):

```text
{dataDir}/sounds/alerts/
  defaults/                          # read-only bundled mirror (optional seed)
    overspeed.ogg
    road_sign_regulatory.ogg
    road_sign_warning.ogg
    road_sign_informational.ogg
    children_proximity.ogg
    speed_camera_point.ogg
    speed_camera_section.ogg
    seasonal_closure.ogg
  user/                              # user overrides (same basenames)
    overspeed.mp3
    children_proximity.wav
  manifest.json                      # optional: per-file gain, format hint
```

Resolution order for category `children_proximity`:

1. `{dataDir}/sounds/alerts/user/children_proximity.{ogg,mp3,wav,flac}`
2. `{dataDir}/sounds/alerts/defaults/children_proximity.*`
3. Host built-in asset fallback (if any)

**Formats:** OGG and MP3 preferred (Symphonia decode path shared with voice
guidance). WAV/FLAC optional. Keep clips **short** (≤ 2 s recommended).

Example `manifest.json` (optional, not required for v1):

```json
{
  "version": 1,
  "categories": {
    "overspeed": { "file": "user/overspeed.ogg", "gain_db": -3.0 },
    "children_proximity": { "file": "defaults/children_proximity.ogg" }
  }
}
```

---

## Category-to-sound mapping

### Implemented categories (v1 target)

| Category id | Trigger | Default tone character | Notes |
|---|---|---|---|
| `overspeed` | GPS speed &gt; posted limit (+ host hysteresis) | Short beep / double beep | Tied to bottom HUD limit display; repeats throttled unless [`adaptive-speed-warning-spec.md`](adaptive-speed-warning-spec.md) owns the voice path (then: earcon on tier change only) |
| `road_sign_regulatory` | Approach urgency for stop/yield/prohibition-class signs (`1xx` fareskilt where catalogue marks regulatory intent, e.g. stop, no entry) | Firm, attention-grabbing | Sub-type of tagged `road_sign` warnings |
| `road_sign_warning` | Approach urgency for hazard triangles (`1xx` general warnings: animals, ice, curves, **142 Children**) | Standard warning chime | Includes tagged `NO:142` **and** `children_proximity` fallback (same clip — driver message is identical) |
| `road_sign_informational` | Services / direction plates that still surface in warning box (if any) | Soft ping | Lower priority; may share clip with `road_sign_warning` in bundled defaults |
| `children_proximity` | Route-corridor fallback for school / kindergarten / playground POIs (`source=children_proximity`, code `142`) | Same as `road_sign_warning` by default | User may override separately; host may alias to `road_sign_warning` clip |
| `speed_camera_point` | Point camera enters urgency phase | Distinct camera tone | Jurisdiction opt-in respected |
| `speed_camera_section` | Section-control enter event | Different tone from point camera | Uses section enter/exit UX, not only distance phases |
| `seasonal_closure` | Hard-filtered conditional way on active route (departure-time eval) | Informational alert | Fires once when closure affects planned corridor (plan-time or reroute), not every 150 m |

Road-sign sub-types: the guest reads merged warning JSON (`code`, `icon_key`,
`source`) and maps through a small host-provided **sign-class table** derived
from catalogue metadata ([`road-signs.md`](../road-signs.md)) — the guest does
not parse SVG or OSM tags directly.

### Future / placeholder categories (documented, not wired)

| Category id | Planned source | Status |
|---|---|---|
| `incident_ahead` | `road_info` plugin — accidents, convoys, temporary hazards | **Not implemented** — reserve clip slot + settings row |
| `weather_hazard` | `weather` plugin — high wind, lightning, flood along route | **Not implemented** |
| `hazard_report` | User/community hazard reports (if added) | **Not implemented** |

Settings UI (future): toggles per category + master mute; respect Android
“Do not disturb” / car audio focus where applicable (host duty).

---

## Host capabilities (proposed)

Implemented today (reuse): `log`, `position_read`.

Proposed additions (declare in manifest only after HostApi exists):

| Capability | Purpose |
|---|---|
| `warning_event_subscribe` (new) | Guest registers interest; host pushes JSON events on phase transitions |
| `alert_sound_play` (new) | Host queues clip by category id + resolved path; owns AudioTrack/rodio |
| `alert_sound_catalog` (new) | List installed overrides + bundled defaults |
| `plugin_kv` / `storage` (new) | Per-category enable flags, gain, last-play timestamps (debounce) |
| `log` | Diagnostics under `Documents/debug/custom-alert-sounds/` |

The guest must **not** open `/dev/snd`, files outside `{dataDir}/sounds/alerts/`,
or the network.

### Example warning event (host → guest, sketch)

```json
{
  "event": "phase_enter",
  "category": "road_sign_warning",
  "subcategory": "142",
  "source": "children_proximity",
  "phase": "urgency",
  "distance_m": 67.2,
  "warning_id": "w-20260820-142-vallset"
}
```

Guest returns `{ "play": "children_proximity" }` or `{ "play": null }` if
muted/disabled.

---

## Plugin architecture fit

Same pattern as [`instrument-cluster-agl-spec.md`](instrument-cluster-agl-spec.md):

1. **Host** assembles authoritative warning state on each GPS/sim fix (already
   done for visual chrome).
2. **Host** detects phase **transitions** (appear→urgency, not every tick).
3. **Guest** (optional) maps event → category → file; applies user prefs.
4. **Host** calls shared `play_alert_clip(path)` (rodio/Symphonia stack).

If the guest is disabled, the host may still play bundled defaults — product
decision at implementation time.

Fuel/timeout: event handling is O(1) per fix; no PBF or graph work in WASM.

---

## Settings and safety

- Master **Alert sounds** toggle (default on for motor profiles, off for hiking
  — product decision).
- Per-category toggles in Tools or Drive settings sheet.
- **No alert** when navigation inactive or simulation paused (unless debug hook).
- Clips must be **non-blocking** on T2 UI thread — decode/play on audio thread
  only ([`plugins.md`](../plugins.md) tier rules).
- User-provided files: validate size (&lt; 500 KB per clip recommended) and
  duration in host before decode.

---

## Testing (when implemented)

- Unit: category map from sample warning JSON (tagged 109 vs 142 vs
  `children_proximity`).
- Device: urgency transition fires exactly once; cluster of school+kindergarten
  POIs → one clip; explicit `NO:142` suppresses duplicate proximity clip.
- Overspeed: throttle repeat while continuously over limit.
- Missing user file → bundled default → silent fail with log line (no crash).

---

## References

- [`road-signs.md`](../road-signs.md) — catalogue, approach phases, children-zone fallback
- [`voice-guidance.md`](../voice-guidance.md) — rodio / Symphonia playback stack
- [`approach-instructions.md`](../approach-instructions.md) — 750 / 150 / 25 m phases
- [`plugins/instrument-cluster-agl-spec.md`](instrument-cluster-agl-spec.md) — host-mediated I/O; exports the same warning categories to clusters
- [`plugins/adaptive-speed-warning-spec.md`](adaptive-speed-warning-spec.md) — spoken escalating overspeed (percentage tiers)
- [`plugins.md`](../plugins.md) — capability sketch and design rules
