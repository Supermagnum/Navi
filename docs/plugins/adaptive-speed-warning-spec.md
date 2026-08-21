# Adaptive speed warning plugin (specification)

**Status:** specification only — not implemented.  
**Path:** `docs/plugins/adaptive-speed-warning-spec.md`  
**Architecture:** planned WASM guest via `plugin-host` / `plugin-sdk` and
capability-gated `HostApi` ([`plugins.md`](../plugins.md)).
**System requirements** (all plugins): user **enable/disable** toggle; any
device link uses host-mediated **USB** / **Bluetooth**
([`plugins.md` — enable/disable](../plugins.md#enable--disable-required),
[USB/Bluetooth](../plugins.md#external-device-io--usb-and-bluetooth-required)).

Working title / id suggestion: `adaptive_speed_warning` / `speed_warning`.

Spoken, escalating overspeed alerts when GPS speed is above the **applicable**
posted / conditional / highway-fallback limit already resolved for the bottom
HUD ([`current-street.md`](../current-street.md)). The guest owns the tier
state machine; the host owns speed/limit truth and audio I/O.

This is **not** a ticket predictor and **not** a replacement for HUD overspeed
chrome. Design goal: **be effective, not annoying.** A system that is ignored
is worse than no system.

---

## Disclaimer (must appear in the plugin UI)

Tiers are a **safety / UX heuristic**. They do **not** match any jurisdiction’s
fine bands, penalty points, or criminal thresholds. Do not present boundaries
to the user as “this is when you would get a ticket.” The driver remains
responsible for obeying posted limits. Map `maxspeed` and GNSS speed can be
wrong or stale.

---

## Goals

1. Warn with **escalating spoken urgency** as the driver remains further over
   the applicable limit, using **percentage over limit** as the primary metric.
2. Tolerate brief, legitimate overspeed (for example overtaking on a rural
   single carriageway) without nagging, while still catching sustained speeding.
3. Reuse host-resolved speed and limit — never invent a limit in WASM.
4. Stay silent unless the HUD would already treat the fix as overspeed (GNSS
   floor), so GPS jitter does not create a second, tighter audio path.
5. Play through the **same host audio stack** planned for voice guidance
   ([`voice-guidance.md`](../voice-guidance.md)); the guest only chooses phrase
   keys and optional earcons.

## Non-goals

- Implementing the plugin in this documentation pass.
- Matching legal enforcement tables (fines, points, licence suspension).
- Changing [`OverspeedHud`](../../app/src/main/java/no/navi/app/OverspeedHud.kt)
  from display-only chrome into an alert engine.
- Replacing turn-by-turn voice guidance or the one-shot road-sign / camera
  tones in [`custom-alert-sounds-spec.md`](custom-alert-sounds-spec.md).
- Opening the audio device, GNSS, or map files from WASM.
- Speaking when the applicable limit is unknown.

---

## Relationship to existing Navi surfaces

| Surface | Role today | This plugin |
|---|---|---|
| Bottom HUD `{speed} / {limit} km/h` | Visual overspeed colour via `OverspeedHud` (`MARGIN_KMH = 3.0`, widened by `speedAccuracyKmh`) | **Unchanged.** Audio must not fire unless `OverspeedHud.isOverspeed` would be true for the same fix. |
| `current_speed_limit_kmh` / `road_near_info` / `resolve_speed_limit_kmh` | Sticky nearest-edge limit: conditional → posted → highway-class fallback | Host snapshot fields; guest does not re-resolve OSM tags. |
| `overspeed_delta_kmh` | `speed − limit` convenience for HUD | Useful debug; **not** the escalation metric (see [§ Core trigger](#core-trigger-metric)). |
| Custom alert sounds `overspeed` clip | Planned short beep, ~60 s repeat throttle | **Earcon at tier transitions** when both plugins are enabled — not a second repeating nag. See [§ Alert sounds](#relationship-to-custom-alert-sounds). |
| Children-zone proximity (`source=children_proximity`, code `142`) | Visual approach box; not a speed-limit source | Optional **arm-delay shortening** while the merged warning is active ([`road-signs.md`](../road-signs.md)). |
| Voice guidance | Turn-by-turn phrase keys / Piper | Shared playback; this plugin uses a **separate phrase-key family** (`speed_warn_*`). Maneuver prompts outrank mid-tier nags; critical overspeed may outrank non-safety chatter (host mix policy). |
| Travel profiles | Car, EV, truck, motorcycle, bicycle, hiking, … | **Motor profiles only** (Car / CarElectric, Truck / TruckElectric, MobileHome, Motorcycle / MotorcycleElectric). Off for Bicycle, Hiking, and a future Horse profile. |

Internal units stay **km/h** (same as HUD and UniFFI). `overPct` is
dimensionless, so a future mph display does not change the math.

---

## Core trigger metric

Primary trigger is **percentage over the applicable limit**, not a fixed km/h
excess:

```
overPct = (currentSpeedKmh - postedLimitKmh) / postedLimitKmh
```

Use the same `postedLimitKmh` the HUD shows (conditional / posted / class
fallback). If the limit is missing, non-finite, or `<= 0`, the plugin is
**silent** (same “never invent” rule as
[`instrument-cluster-agl-spec.md`](instrument-cluster-agl-spec.md)).

### Rationale

A fixed km/h band is proportionally more severe on a 30 km/h residential or
children-zone road than on a 100 km/h motorway. Percentage tiers keep felt
urgency more consistent across OSM highway classes. This is a UX heuristic,
not a claim about any country’s penalty schedule.

### GNSS floor (Navi-specific)

HUD chrome already rejects sub-noise overspeed (`3.0 km/h` or the fix’s speed
accuracy, whichever is larger). Audio **additionally** requires that floor:

1. `OverspeedHud.isOverspeed(speed, limit, speedAccuracy)` is true, **and**
2. `overPct` is in tier 1 or higher.

On a 30 km/h road, 5% is only 1.5 km/h — tighter than the HUD margin — so
tier 1 must not speak from percentage alone. On an 80 km/h road, 5% is 4 km/h
and sits just above the default HUD floor.

---

## Escalation tiers

Boundaries are **configurable constants** in `plugin_kv` (expect field tuning
and optional per-market packs). The table is a starting point.

| Tier | `overPct` range | Voice behaviour | Notes |
|---|---|---|---|
| 0 — Silent | below 5% (and always when HUD is not overspeed) | none | GPS / speedometer noise and common slack |
| 1 — Gentle nudge | 5–15% | Calm, single mention, no repeat | Phrase-key example: `speed_warn_gentle` |
| 2 — Notice | 15–25% | Firmer tone; one repeat if not corrected | `speed_warn_notice` |
| 3 — Insistent | 25–40% | Frequent, shorter sentences | Irritation, not alarm |
| 4 — Alarm | 40–70% | Can’t-ignore cadence; clipped commands | `speed_warn_alarm` |
| 5 — Critical | 70%+ | Maximum urgency | Host may duck non-safety UI/audio |

English examples for persona only (“You’re a bit over the limit.” / “Slow
down.”). Shipped copy lives in voice packs / i18n catalogs
([`i18n-translation-spec.md`](i18n-translation-spec.md)), not hardcoded in the
guest.

### Voice persona

- Mid tiers: **naggy / irritating rather than fear-inducing.** Irritation tends
  to produce a lift off the accelerator; alarm tones raise stress without
  reliably improving compliance (aviation aural-alert practice: calm-but-firm
  even at high urgency).
- Message length shrinks as urgency rises: full sentences at low tiers, clipped
  commands at high tiers.
- A short, distinct **earcon** before the voice line at **tier transitions**
  so urgency is recognized before the phrase is parsed (reuse
  `alert_sound_play` / category `overspeed` — see below).

---

## Timing model

Two independent timers. Do not conflate them. Guest state; host supplies
monotonic time on each snapshot (or the guest uses fuel-cheap tick deltas
from `clock_read` / a dedicated `monotonic_ms` field).

### Arm delay

Time the driver must remain **at or above that tier’s threshold** before that
tier’s audio begins.

- **Default arm delay: 60 seconds** at low/mid tiers — long enough for a
  routine overtake on a rural single carriageway; short enough to catch
  sustained speeding. Aligns with the ~60 s overspeed repeat throttle already
  sketched in [`custom-alert-sounds-spec.md`](custom-alert-sounds-spec.md).
- **Arm delay shrinks at higher tiers.** 60 s is plausible for “10% over while
  passing,” not for “50%+ over.”

v1 **stepped** taper (configurable; not a claim that this curve is final):

| Tier | Default arm delay |
|---|---|
| 1 | 60 s |
| 2 | 45 s |
| 3 | 20 s |
| 4 | 8 s |
| 5 | 0–3 s |

### Disarm / reset

Time to silence and reset once speed is back under the **tier-0 / HUD**
condition.

- **Default disarm: a few seconds** (much shorter than arm delay) so compliance
  is rewarded immediately.
- Disarm **resets tier state to 0**, not merely mute. A later overspeed event
  re-enters at tier 1 and restarts the arm timer; it must not resume
  mid-escalation.

### Road-type modulation (recommended, not required for v1)

Apply the full 60 s arm delay primarily on **rural / single-carriageway** OSM
classes, where overtaking a slow vehicle is normal.

Shorter arm delay on:

- `highway=motorway` / `motorway_link` (and typically `trunk` when treated as
  dual carriageway in the snapshot),
- urban-ish classes (`residential`, `living_street`),
- while a **children-zone proximity** warning is the merged approach chrome.

Use `highway_class_base` keys already aligned with
[`current-street.md`](../current-street.md) / `eta::highway_class_display_label`.
The guest must not parse the PBF.

---

## Alert fatigue mitigation

- **Hysteresis:** require the new tier condition to hold for a short smoothing
  window (2–3 s, same order as HUD road-label stickiness) before escalating or
  de-escalating, so GPS jitter near a boundary does not flap audio.
- **Cooldown after acknowledgment (optional v1):** if Tools later exposes
  dismiss / mute-this-instance, inhibit the same tier for a cooldown window.
  Without an ack control, v1 is **enable/disable + master mute** only.
- **One voice line in flight:** do not queue overlapping `speed_warn_*` clips.
  Host audio focus / Android DND apply (host duty).

---

## Relationship to custom alert sounds

[`custom-alert-sounds-spec.md`](custom-alert-sounds-spec.md) maps categories to
**short tones**. This plugin maps **overspeed magnitude + time** to **spoken
tiers**.

When both are enabled:

1. This plugin owns overspeed **voice** and the arm/disarm state machine.
2. The `overspeed` alert-sound clip fires only as an **earcon on tier
   transition** (and not on every GPS poll, and not on a parallel 60 s beep
   loop).
3. Road-sign, children-proximity, and camera one-shots stay with custom alert
   sounds (distance-phase model, not `overPct`).

If this plugin is disabled, custom alert sounds may keep the simple throttled
overspeed beep.

---

## Host snapshot (authoritative)

Host assembles one JSON (or packed struct) per GPS / sim fix from existing
FFI — guest is read-only:

```json
{
  "speed_kmh": 94.2,
  "limit_kmh": 80.0,
  "limit_known": true,
  "speed_accuracy_kmh": 2.1,
  "hud_overspeed": true,
  "highway": "primary",
  "profile": "Car",
  "children_zone_active": false,
  "monotonic_ms": 184320
}
```

| Field | Source |
|---|---|
| `speed_kmh` | Same fix as HUD (`Location.speed` or simulator) |
| `limit_kmh` / `limit_known` | `road_near_info` / `current_speed_limit_kmh` |
| `speed_accuracy_kmh` | Android speed accuracy when present |
| `hud_overspeed` | `OverspeedHud.isOverspeed(...)` |
| `highway` | Sticky nearest-edge class (`highway_class_base`) |
| `profile` | Active travel profile |
| `children_zone_active` | Merged warning is children-zone proximity or tagged `142` |
| `monotonic_ms` | Host monotonic clock for arm/disarm |

Never speak when `profile` is a non-motor mode, navigation is inactive, or
simulation is paused (unless a debug hook).

---

## Host capabilities (proposed)

Implemented today (reuse): `log`, `position_read`.

Proposed additions (declare in the manifest only after HostApi exists):

| Capability | Purpose |
|---|---|
| `road_speed_state_read` (new) | Snapshot above (speed, limit, HUD flag, highway, profile, children-zone) |
| `voice_speak` / `voice_pack_query` | Queue `speed_warn_*` phrase keys; host owns rodio / Android output |
| `alert_sound_play` | Optional earcon at tier transition (`overspeed` category) |
| `plugin_kv` / `storage` | Tier boundaries, arm/disarm constants, enable flag, last-tier timestamps |
| `admin_region_read` | Optional per-market packs (open question) |
| `log` | Diagnostics under `Documents/debug/adaptive-speed-warning/` |

The guest must **not** open `/dev/snd`, GNSS, or the network.

Fuel/timeout: O(1) arithmetic per fix; no PBF or graph work in WASM. Same
T0/T1 budget rules as other plugins ([`plugins.md`](../plugins.md)).

---

## Settings

- Master **Adaptive speed warning** toggle (default **on** for motor profiles,
  **off** otherwise — product decision at implementation time).
- Optional: expose arm delay and whether road-type modulation is enabled.
- Do **not** expose raw “ticket threshold” copy next to the sliders.

---

## Open questions

1. **Exact tier boundaries** — the table is a starting point; validate against
   driving traces and annoyance vs usefulness.
2. **Overtake heuristic (v2).** Shape of the speed curve (ramp up → brief hold
   → ramp down) vs flat sustained overspeed. Flat arm timers are enough for v1.
3. **Arm-delay taper** — stepped table above vs a linear map from `overPct`.
4. **Per-market packs** — should boundaries or voice tone follow
   [`jurisdiction-rules.md`](../jurisdiction-rules.md) (decline rather than
   guess), given differing enforcement norms?
5. **Ack / mute-this-instance UX** — if added, how it interacts with cooldown.
6. **Low-limit multiplier** — posted limits below e.g. 50 km/h, or active
   children-zone chrome, might use a steeper curve than the default table.
   Children-zone **arm shortening** is the v1 hook; a separate multiplier is
   optional.

---

## Explicitly out of scope (v1)

- Jurisdiction-accurate legal threshold matching.
- Guest-side OSM / PBF parsing.
- Final voice-line copywriting per language (voice-pack / i18n docs).
- Changing HUD `MARGIN_KMH` or limit-resolution policy in core.

---

## Testing (when implemented)

- Unit: `overPct` → tier from sample snapshots; silent when `limit_known` is
  false or `hud_overspeed` is false.
- Unit: arm delay not elapsed → no `voice_speak`; disarm resets to tier 0.
- Device / sim: reuse the overspeed simulator path
  (`SimOverspeedInstrumentedTest`) — a brief injected spike must not speak;
  a sustained hold at a given `overPct` must enter the matching tier after
  the arm delay.
- Interaction: with custom alert sounds enabled, overspeed beep is an earcon
  at transition only, not a parallel 60 s loop.
- Missing voice clip → log line, no crash; same decode rules as voice
  guidance.

---

## References

- [`current-street.md`](../current-street.md) — HUD speed/limit and overspeed chrome
- [`API.md`](../API.md) — `road_near_info`, speed-limit helpers; warning helpers for related chrome
- [`voice-guidance.md`](../voice-guidance.md) — rodio / Symphonia playback
- [`plugins/custom-alert-sounds-spec.md`](custom-alert-sounds-spec.md) — short tones vs spoken tiers
- [`plugins/instrument-cluster-agl-spec.md`](instrument-cluster-agl-spec.md) — cluster export of HUD overspeed + approach warnings
- [`road-signs.md`](../road-signs.md) — children-zone proximity
- [`jurisdiction-rules.md`](../jurisdiction-rules.md) — optional per-market packs
- [`plugins.md`](../plugins.md) — capability sketch and design rules
