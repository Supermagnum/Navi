# Approach-instruction boxes

Temporary turn-by-turn guidance that appears as the vehicle **approaches an
upcoming maneuver**. Distinct from the persistent collapsed top/bottom drive
HUD bars ([`hud-layout.md`](hud-layout.md)).

**Implementation status: deferred.** This document is a design brief only.
No Compose UI, maneuver distance triggers, or host wiring for this box ship in
the current pass. Implementation should follow once live maneuver / distance
state exists and can be shared with voice guidance
([`voice-guidance.md`](voice-guidance.md)).

Reference pattern: Garmin-style large maneuver icon + distance + instruction
text, shown only when relevant. A measured Garmin top instruction strip occupied
roughly **~14% of screen height**; Navi should treat that as a starting
proportion for this temporary overlay, not as a hard rule, and not confuse it
with the much thinner collapsed persistent bars.

---

## 1. Relationship to persistent HUD bars

| Element | Role | Persistence |
|---|---|---|
| Top drive HUD (collapsed) | Altitude; tap → map/display settings | Always (chrome on) |
| Bottom drive HUD (collapsed) | Zoom −/+, break time, trip ETA, eco leaf; tap → drive settings | Always (chrome on) |
| **Approach-instruction box** | Next maneuver icon + distance + road name | **Only near a maneuver** |

The approach box is an **additional** layer. It must not replace either bar, and
must not cover the collapsed top/bottom bars (place it in the map band between
them, or immediately under the top bar with clear margins).

**Tap behavior:** informational only — **no tap action**. Persistent bars already
own tap-to-open settings; giving the approach box a tap target would conflict
with that model and with map panning.

**Eco icon:** stays **exclusively on the bottom persistent bar**. Do not mirror
eco onto the approach box (avoids the leaf flickering in/out as the box
appears and disappears).

---

## 2. Trigger conditions

Exact distances are **TBD** at implementation time; document the chosen values
in this file when locked. Intended pattern:

### Appear (first threshold)

| Item | Guidance |
|---|---|
| Distance to next maneuver | First show in the **~500 m – 1 km** band (common turn-by-turn UX) |
| Required state | Active navigation route with a known next maneuver |
| Units | Compare in meters internally; display per user km/miles preference |

### Closer / urgency (second threshold)

| Item | Guidance |
|---|---|
| Distance | **~100 – 200 m** (or junction complexity–based) |
| UI change | Enlarge box, strengthen typography, and/or show a second panel (lane / exit detail) similar to Garmin’s dual-panel pattern near complex junctions |
| Optional | Lane guidance / exit board only when route data provides it |

### Disappear

| Cause | Behaviour |
|---|---|
| Maneuver completed | Hide after the vehicle passes the maneuver geometry (or after a short settle delay) |
| Reroute / maneuver changed | Hide immediately; re-evaluate thresholds for the new next maneuver |
| Navigation ended / cancelled | Hide |
| GPS loss | Keep last instruction briefly or hide — decide at implementation; prefer not to show stale distance |

Voice prompts ([`voice-guidance.md`](voice-guidance.md)) and this box **must share
the same maneuver + distance state**. Do not maintain a second independent
distance clock that can drift out of sync with spoken “in 500 meters / left”.

---

## 3. Box content

| Field | Source | Notes |
|---|---|---|
| Maneuver icon | Existing `nav_*` icon set ([`icons.md`](icons.md)) | left / right / straight / merge / keep-left / keep-right / U-turn / roundabout-exit-N / etc. |
| Distance | Live distance to maneuver point | User unit preference (km / miles); update continuously while visible |
| Street / road name | Route / OSM name for the maneuver | Omit line if unknown; do not invent |
| Roundabout exit | Exit index / count | Align wording with voice fragments (first / second / third exit) so visual and spoken guidance match |

Do **not** put trip ETA, break countdown, altitude, or eco on this box — those
belong on the persistent bars.

---

## 4. Layout and sizing

```
+------------------------------------------+
|  [collapsed top HUD]                     |
|  [approach box — only when near turn]    |  ~14% height starting point while active
|                                          |
|              map                         |
|                                          |
|  [collapsed bottom HUD]                  |  ~6.4% reference for persistent bottom strip
+------------------------------------------+
```

| Topic | Recommendation |
|---|---|
| Horizontal | Full width with same edge inset as HUD bars (~10.dp), or slightly inset card |
| Vertical | Between top bar and map center; never over bottom zoom −/+ |
| Active height | Start near **~14% of screen height** (Garmin instruction-bar reference); may be slightly smaller because this is temporary |
| Closer state | May grow or spawn a second lane/exit panel without covering persistent bars |
| Dismiss | Auto only (see triggers); no user “X” required for v1 |

---

## 5. Voice guidance alignment

| Concern | Rule |
|---|---|
| Shared state | One maneuver cursor + distance-to-maneuver publisher feeds **both** UI box and voice |
| Phrase keys | Roundabout exit numbers and left/right keys match `nav_*` icons and `/sounds` clip keys |
| Timing | First appearance threshold may precede the first spoken prompt; closer threshold may align with “now” / final call |
| Offline | Box works offline from local route geometry; no network |

See [`voice-guidance.md`](voice-guidance.md) for clip packs and planned playback.

---

## 6. Implementation checklist (when picked up)

1. Define and document final appear / closer / hide distances in this file.
2. Expose shared maneuver distance state from the routing / navigation host (UniFFI or Android-side).
3. Compose overlay (e.g. `ApproachInstructionBox`) in `MainActivity`’s map `Box`, above the map and clear of bars.
4. Wire `nav_*` rasterization via existing icon pipeline.
5. Instrument screenshots under `docs/images/hud/` (or `docs/images/approach/`) and extend HUD verification only after bars remain stable.
6. Keep eco on the bottom bar only; keep settings taps on the persistent bars only.

---

## 7. Out of scope for this doc pass

- Building the Compose UI
- Choosing final meter thresholds
- Lane-level OSM parsing
- Changing voice playback implementation
