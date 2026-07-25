# Approach-instruction boxes

Temporary turn-by-turn guidance that appears as the vehicle **approaches an
upcoming maneuver**. Distinct from the persistent collapsed top/bottom drive
HUD bars ([`hud-layout.md`](hud-layout.md)).

**Implementation status: implemented** (Compose overlay + shared nav-guidance
state).

Reference pattern: Garmin-style large maneuver icon + distance + instruction
text, shown only when relevant. A measured Garmin top instruction strip occupied
roughly **~14% of screen height**; Navi treats that as a starting proportion for
this temporary overlay, not as a hard rule, and not as a substitute for the much
thinner collapsed persistent bars.

---

## Prior art: Navit

Navi’s `nav_*` icons come from the **Navit** project. Navit’s OSD (on-screen
display) is the direct prior art for this box:

| Navit piece | Behaviour |
|---|---|
| `navigation_next_turn` | Maneuver **image** only; nothing when not routing. Optional `level="1"` previews the turn *after* next. Day/night icon variants use `_bk` / `_wh` (already in Navi’s icon set) at several pixel sizes. |
| Current street | Separate **small** text, e.g. “Currently on ${navigation.item.street_name_systematic}” — continuous, low visual weight. |
| Next / upcoming street | Separate field near the turn arrow — the street you **turn onto**; larger emphasis. |
| Names | `street_name` (colloquial) vs `street_name_systematic` (route/ref, e.g. SS33). OSM equivalents: `name` vs `ref`. |
| Distance | Separate OSD **text** element — not baked into the turn icon. |
| Tap | User can **tap** the next-action arrow to dismiss/clear it. |
| Position | Live fix from **gpsd** or direct NMEA — relevant to Linux gpsd support ([`build-linux.md`](build-linux.md), sensors). |

**Navi v1 deviations (deliberate):**

1. **No tap-to-dismiss** on the approach box. Persistent HUD bars own tap-to-open
   settings; a dismiss target on the approach overlay would conflict with that
   model and with map panning. Auto-hide on maneuver complete / reroute / cancel
   only. Do not add tap-to-dismiss without revisiting this reasoning.
2. **Next-street only** on the approach box (the street after the maneuver).
   Navit’s continuous “currently on …” line lives on the **bottom drive HUD**
   instead ([`current-street.md`](current-street.md)) so the temporary overlay
   stays uncluttered. Prefer OSM **`name`** over **`ref`** when both exist
   (local name is more useful while driving); if only `ref` is known, show
   that; if neither, omit the name line on the approach box (do not invent).
   Current-street on the bottom bar falls back to a highway-class label when
   name/ref are missing.

Sources: [Navit OSD](https://navit.readthedocs.io/en/latest/user/configuration/OSD.html),
[OSD layouts](https://navit.readthedocs.io/en/latest/user/configuration/OSD_Layouts.html).

---

## 1. Relationship to persistent HUD bars

| Element | Role | Persistence |
|---|---|---|
| Top drive HUD (collapsed) | Altitude; tap → map/display settings | Always (chrome on) |
| Bottom drive HUD (collapsed) | Zoom −/+, **current street**, break time, trip ETA, eco leaf; tap → drive settings | Always (chrome on) |
| **Approach-instruction box** | Next maneuver icon + distance + road name | **Only near a maneuver** |

The approach box is an **additional** layer. It must not replace either bar, and
must not cover the collapsed top/bottom bars (place it in the map band between
them, or immediately under the top bar with clear margins).

**Tap behavior:** informational only — **no tap action** (see Prior art).

**Eco icon:** stays **exclusively on the bottom persistent bar**. Do not mirror
eco onto the approach box.

---

## 2. Trigger conditions (locked)

Distances are meters internally; display follows user km/miles preference.

| Phase | Distance to next maneuver | UI |
|---|---|---|
| **Hidden** | &gt; **750 m** (or no active maneuver) | Box not shown |
| **Appear** | ≤ **750 m** and &gt; **150 m** | Standard approach box (~14% height band) |
| **Closer / urgency** | ≤ **150 m** | Larger typography / stronger emphasis (`urgency` style) |
| **Hide (passed)** | ≤ **25 m** or maneuver cursor advances | Hide; re-evaluate next maneuver |
| **Hide (nav)** | Reroute, cancel, or navigation ended | Hide immediately |
| **Hide (no route)** | No planned corridor (`polyline` blank) | Hide; do not show approach copy without a route |
| **GPS stale** | No fix update for guidance | Prefer hide rather than show stale distance |

Voice prompts ([`voice-guidance.md`](voice-guidance.md)) and this box **must share
the same** `NavGuidance` publisher (maneuver kind, distance_m, street name,
roundabout exit). Do not maintain a second independent distance clock.

---

## 3. Box content

| Field | Source | Notes |
|---|---|---|
| Maneuver icon | `nav_*` via usvg/resvg ([`icons.md`](icons.md)) | Day theme `_bk` by default |
| Distance | Shared `distance_m` | e.g. `450 m` / `0.3 mi` |
| Next street / road | OSM `name`, else `ref` | **One line** (`maxLines=1`, no soft wrap). Omit if unknown |
| House number | From place hit / address parse when known | Own line under street; omit if unknown |
| Postcode | From place hit / address parse when known | Own line under house number; omit if unknown |
| Roundabout exit | Exit index | Align with voice: first / second / third |
| Current street | Bottom HUD only | See [`current-street.md`](current-street.md) |

Do **not** put trip ETA, break countdown, altitude, or eco on this box.

---

## 4. Layout and sizing

```
+------------------------------------------+
|  [collapsed top HUD — full width]        |
|  [approach card — left, hug content]     |  only with planned route + near turn
|                                          |
|              map                         |
|                                          |
|  [collapsed bottom HUD — full width]     |
+------------------------------------------+
```

| Topic | Recommendation |
|---|---|
| Horizontal | **Compact, left-aligned, hug content** via `IntrinsicSize.Max` (not full width). Cap ~420.dp so long street names stay on **one line** (no mid-word wrap). House number and postcode each get their own line under the street. Persistent HUD bars remain full width. |
| Vertical | Under top bar; never over bottom zoom −/+ |
| Appear | ~14% screen height starting point for the card |
| Urgency | Slightly taller / stronger type; still clear of bottom bar |
| Dismiss | Auto only; no “X” / no tap |
| Shape | Plain **rectangle** (`RectangleShape` / square corners; implemented as a `Box` with background + border — not a Material `Surface`, so HUD bars’ `RoundedCornerShape` is untouched). |
| No route | Box **must stay hidden** when `MapRouteState.polyline` is empty — even if approach guidance state is accidentally active. |

Composable: `ApproachInstructionBox` in `MainActivity`’s map `Box`.

### Route line while the box is shown

Active navigation draws the corridor via MapLibre `GeoJsonSource` + `LineLayer`
(`route-src` / `route-line`, colour `#C62828`). On the Automotive emulator with
the **Vulkan** MapLibre SDK, that `LineLayer` paints correctly (see corridor
[`route_map.png`](images/route_map.png)).

Earlier approach verification shots showed countdown UI **without** a route line
because the instrumented test injected only `ApproachGuidanceState` and left
`MapRouteState.polyline` empty — not because of the moving-icons GLES
Circle/Symbol silent-paint failure. Those markers still need the Compose
screen-space overlay; the route polyline does not share that root cause under
Vulkan.

Approach verification uses a **host-planned** car corridor
**Grimåsfeltet (Raufoss) → Nysethvegen / Tollerud** (~2 km on ostlandet) — not a
synthetic stub polyline. Approach copy for **Nysethvegen**. Host fixture:
`cargo test -p driver-break-core --test raufoss_approach_route -- --ignored`.
Device shots from `ApproachInstructionInstrumentedTest` are local only; they are
**not** part of the GitHub `docs/images/` allowlist ([`pictures.md`](pictures.md)).

---

## 5. Voice guidance alignment

| Concern | Rule |
|---|---|
| Shared state | `NavGuidance` / host hooks feed **both** UI and future voice |
| Phrase keys | Roundabout exit + left/right match `nav_*` and `/sounds` keys |
| Timing | Appear (750 m) may precede first spoken prompt; urgency (150 m) may align with final call |
| Offline | Box works offline from local route / injected guidance |

---

## 6. Implementation checklist

1. Locked appear **750 m** / urgency **150 m** / hide **25 m** — done.
   Source of truth: Rust `APPROACH_*_M` in `core/src/nav/mod.rs`, UniFFI
   `approachAppearM` / `approachUrgencyM` / `approachHideM`. Phase styling uses
   `approachPhaseForDistance`. Maneuver-cursor advance in `RouteProgressTracker`
   uses `hideDistanceM` defaulting to `approachHideM()` (metres) — no local magic
   number.
2. Shared guidance state (`core` nav module + Android hooks) — done.
3. `ApproachInstructionBox` Compose overlay — done.
4. `nav_*` rasterization — done.
5. Verification via `ApproachInstructionInstrumentedTest` — required.
6. Eco on bottom bar only; no tap on approach box — done.

---

## 7. Out of scope (still)

- Lane-level OSM parsing / second panel
- Navit-style tap-to-clear
- Full Ferrostar SDK (optional later publisher into the same `NavGuidance` bus)

Current-street bottom HUD line is implemented separately — see
[`current-street.md`](current-street.md).
