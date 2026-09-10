# POI look-ahead cone plugin (specification)

**Status:** implemented (host-native UniFFI + MapHudPrefs; WASM guest scaffold).  
**Path:** `docs/plugins/poi-lookahead-cone-spec.md`  
**Architecture:** product path is host-native (`navi-ffi` + Android prefs/HUD), with an
optional WASM guest scaffold under `plugins/poi-lookahead/` via `plugin-host` /
`plugin-sdk` (capability-gated per [`plugins.md`](../plugins.md)). Product APK
does not load wasmtime guests yet.  
**System requirements** (all plugins): user **enable/disable** toggle
([`plugins.md` — enable/disable](../plugins.md#enable--disable-required)).

Working title / id: `poi_lookahead` / `poi_cone`.

**Resolved implementation assumptions** (also listed in the PR description):

1. Full existing `General` category (not a narrower attraction-only subset).
2. Cone half-angle ±30° (60° total), not the hazard cone's ±60°.
3. Single `CraftBrewery` category / `shop-alcohol` icon (no per-drink icons).
4. Unnamed attractions use generic category labels (not filtered out).
5. Ride on existing `PoiIndex` / pack load — no dedicated on-disk look-ahead cache.
6. Hours-unknown shown with "(hours unknown)"; Settings toggle "Hide when hours
   unknown" defaults off.

**`poi_query` decision:** keep HostApi `poi_query` radius-only; enrich JSON with
`open_now` and drop `open_now=false` before the guest buffer. Cone membership
uses host UniFFI `poi_lookahead_query_json` (heading + 850 m / ±30°). Guest-side
bearing filter would need heading on `Position`, which is absent today — less
churn than extending the ABI for a product path that is host-native anyway.

**`live_hazard.rs`:** untouched. Geometry is duplicated in `core/src/poi/lookahead.rs`
with separate constants.

This is **not** a safety/hazard warning. It is a "worth a look" discovery
surface: attractions, fishing spots, breweries, and cider makers on or near
the road ahead that the driver might want to stop and check out — a viewpoint,
a photo opportunity, a place to cast a line, a place to taste something local.
It reuses the **geometry** of the existing route-independent look-ahead cone
(`live_hazard.rs`,
[`road-signs.md` — Live hazard cone](../road-signs.md#live-hazard-cone-without-a-route-300-m--product-name-look-forward))
and the **POI classification** already used by rest/overnight planning
(`driver-break-core`, [`poi.md`](../poi.md)), but is a distinct cone with its
own radius, angle, and category filter — it does not touch, widen, or
repurpose the existing 300 m / ±60° hazard cone or its FFI surface.

---

## Goals

1. Surface nearby-ahead **attractions, fishing spots, craft breweries, and cider
   makers** — things a driver might choose to detour a few minutes for — using
   the same `PoiCategory` classification `driver-break-core` already applies for
   rest/overnight POI search ([`poi.md`](../poi.md)), not a new tag-matching
   scheme.
2. Use a forward-looking **cone** anchored on GPS position + heading, same
   shape of mechanism as speed bumps / road signs / children zones on the
   existing route-independent look-ahead cone, but with **its own radius and
   angle** (below) — not the hazard cone's 300 m / ±60°.
3. Work with or without an active route (route-independent, mirroring the
   existing "Look forward" cone's no-route behaviour).
4. Stay a **discovery / informational** surface: no urgency phases, no audio
   alert, no "brake now" chrome. A quiet marker/HUD chip the driver can glance
   at and ignore.
5. Explicitly leave out overnight-style POIs (cabins, huts, campsites) and
   plain water sources — this cone is about things worth *stopping to look
   at*, not shelter or logistics, which the existing Cabin/OvernightFacility/
   RestArea/Lodging categories and radii already cover.
6. **Respect opening hours.** Do not surface a POI that is known to be
   **closed right now**. Discovering something neat that turns out to be shut
   is a failed discovery; open-now filtering is required for v1, not optional
   polish.

## Non-goals

- Implementing the plugin in this documentation pass.
- Any hazard, safety, or legal-warning framing — this must not reuse the
  "approach urgency" phase language (`APPROACH_URGENCY_M`) or audio earcons
  from `road-signs.md` / `custom-alert-sounds-spec.md`.
- Changing `core/src/routing/live_hazard.rs`, its 300 m radius, or its ±60°
  half-angle. This plugin's cone is a **separate** constant set (see below),
  not a widened hazard cone.
- Re-deciding the `CraftBrewery` OR-set (already includes `brewery=cider` in
  core). Plugin implementation still must not rewrite classifier rules ad hoc.
- Full-route corridor scanning (that is the existing 200 m corridor / auto-via
  behaviour in `poi.md` and `road-signs.md`). This is the no-route-required,
  heading-driven cone specifically.
- Filtering by ratings, reviews, or other data OSM does not reliably carry
  (opening hours **are** in scope; see [Opening hours](#opening-hours)).

---

## Relationship to existing Navi surfaces

| Existing surface | What this plugin reuses | What differs |
|---|---|---|
| Live hazard cone, no route (`road-signs.md`, `live_hazard.rs`) | Cone-from-heading geometry: `bearing_deg`, `angle_diff_deg`, haversine distance, cell-windowed loading, "works without an active route" | Radius (850 m vs 300 m), half-angle (see below vs ±60°), category set (POI discovery vs road signs/children/cameras/bumps), no approach-urgency audio phase |
| `PoiCategory` / `PoiIndex::nearest` (`poi.md`, `driver-break-core`) | Category definitions and OSM tag rules for **General** (attraction, museum, viewpoint, artwork, …), **Fishing**, and **CraftBrewery** | Query shape: cone-filtered by heading + 850 m, not an omnidirectional radius search; excludes Cabin/OvernightFacility/NetworkHut/TentSite/RestArea/Lodging entirely, and Water unless co-tagged as an attraction |
| `poi_query` HostApi capability (`plugins.md`) | The existing "JSON POI list into guest buffer" capability | May need a cone-shaped variant/parameter (heading + half-angle + radius) if `poi_query` today is radius-only — see [Host capabilities](#host-capabilities-proposed) |
| `opening-hours` crate (`conditional.rs`, [`crates.md`](../crates.md)) | OSM `opening_hours` evaluation already used for access/maxspeed conditionals | Applied to discovery POIs at query time with device-local clock; not an urgency/alert path |

---

## Cone geometry

| Item | Value |
|---|---|
| Radius (look-ahead distance) | **850 m** |
| Angle | **60°** total width, i.e. **±30°** half-angle from GPS heading — narrower than the existing hazard cone's ±60° (120° total). **Assumption, flag for confirmation:** if "60 degree cone" was meant as the same ±60° half-angle convention the hazard cone uses (120° total), that is a one-constant change from the value above; the two readings are called out explicitly here so they are not silently conflated. |
| Heading source | Same GPS heading as the existing look-ahead cone; isotropic (no directional filter) when heading is unavailable, matching the documented hazard-cone fallback |
| Route requirement | None — works from GPS position + heading alone, same as the existing "Look forward" cone |
| Windowing | Same cell-window pattern as `live_hazard.rs` (`~0.05°` cell + 1 pad) is a reasonable default for a fresh implementation, but is **not** the same cache/index as the hazard cone — a separate compact point set, loaded and windowed the same way, not appended to `LiveHazardIndex` |

Proposed constants (new, not modifying existing ones):

```rust
pub const POI_LOOKAHEAD_CONE_M: f64 = 850.0;
pub const POI_LOOKAHEAD_CONE_HALF_WIDTH_DEG: f64 = 30.0; // ±30° = 60° total; see note above
```

---

## POI category scope

Reuses the `PoiCategory` classification already defined in
[`poi.md`](../poi.md) / `core/src/poi/classifier.rs`. No new tag-matching
scheme — this plugin is a **filtered view** over the existing categories.

### Included

| Category | Included tags (from `poi.md`) | Notes |
|---|---|---|
| **General** (attractions) | `tourism` ∈ attraction, viewpoint, museum, artwork; `amenity` ∈ museum, gallery, zoo, aquarium, viewpoint, picnic_site, cafe, restaurant, fast_food | Full `General` set including OSM [`tourism=artwork`](https://wiki.openstreetmap.org/wiki/Tag:tourism%3Dartwork) (sculpture, mural, installation, etc.). If "attractions … etc" is meant more narrowly (e.g. viewpoint/attraction/museum/artwork only, not cafe/fast_food), that is a filter to apply on top of `General` at implementation time — flagged as an [open question](#open-questions) rather than assumed here. |
| **Fishing** | `leisure` ∈ fishing, fishing_pier; `sport=fishing`; `shop=fishing` | Existing category — piers, spots, and tackle shops ahead in the cone. Outdoor spots often lack `opening_hours`; they follow the [hours-unknown](#opening-hours) rule unless tagged closed. |
| **CraftBrewery** | `microbrewery=yes`; `shop` ∈ alcohol, wine; `craft` ∈ brewery, winery, distillery; `brewery` ∈ cider, wine, mead, beer; `industrial=distillery` | Beer, cider, wine, spirits/distillery producers and alcohol/wine retail. A brewery that also distills matches if it keeps `craft=brewery` and/or `industrial=distillery`. |

### Explicitly excluded

| Category | Reason |
|---|---|
| **Cabin**, **OvernightFacility**, **NetworkHut**, **TentSite** | "Not cabins or huts" — these are shelter/overnight categories (`tourism` ∈ wilderness_hut, alpine_hut, hostel, camp_site, camp_pitch; `amenity=shelter`), not stop-and-look destinations. |
| **RestArea**, **Lodging** | Logistics/overnight categories, same rationale as above; also not requested. |
| **Water** | Excluded **unless** the same OSM node/way also carries an attraction tag (see below). A plain `amenity=drinking_water` / `amenity=fountain` / `amenity=water_point` / `natural=spring` with no `tourism=attraction` (or equivalent) does not appear in this cone. |
| **Restroom** | Not requested; out of scope for this cone (remains available through the existing general-purpose `PoiIndex::nearest` search). |

### Water-as-attraction rule

A water-source object is shown **only** when it also matches the `General`
attraction classification — concretely, the same OSM object carries both a
water tag (`amenity` ∈ drinking_water, fountain, water_point; or
`natural=spring`) **and** `tourism=attraction` (or is otherwise picked up by
the existing `General` tourism/amenity rule). Practically this will be rare —
most tagged water points are plain infrastructure — but it covers cases like a
named historic well or spring that is itself the attraction. Implementation
note: this is a natural fit for `PoiCategory::nearest` (or the cone-filtered
equivalent) returning objects that match **both** `Water` and `General`, and
the plugin surfacing only that intersection rather than all of `Water`.

---

## Opening hours

**Required for v1.** After cone membership and category filtering, each
candidate must be evaluated against OSM `opening_hours` using the device-local
"now" (same `opening-hours` crate Navi already uses for conditional access /
maxspeed in `core/src/routing/conditional.rs`; see [`crates.md`](../crates.md)).

| Case | Behaviour |
|---|---|
| `opening_hours` present and evaluates **open** at local now | Eligible to show |
| `opening_hours` present and evaluates **closed** at local now | **Suppress** — do not show marker/chip |
| `opening_hours` present but **unparseable** (crate declines the string) | Treat like missing hours (below) — do not invent open/closed |
| `opening_hours` **absent** | Still eligible, but label as **hours unknown** (e.g. "650 m — Viewpoint (hours unknown)"). OSM coverage is incomplete; hiding every untagged attraction would empty the cone in many regions. A Settings option for **strict: hide when hours unknown** is allowed as an optional hardening, default off. |

Rules of thumb:

- Re-evaluate on a sensible tick (e.g. when position/heading updates or every
  ~30–60 s) so a place that closes while the driver approaches disappears
  rather than sticking as "open."
- Seasonal / conditional clauses (`Mo-Fr 10:00-18:00; PH off`, etc.) follow
  the crate's evaluation; unparseable fragments decline rather than inventing
  open (same philosophy as `conditional.rs`).
- Timezone: device-local clock via host `clock_read` (or host-evaluated
  open-now boolean). Do not assume UTC in the guest.

### Host / data note (implementation, not this doc pass)

`PoiRecord` already keeps the OSM `tags` map, so `opening_hours` is available
when present on the object — no new classifier is required for hours. The host
must either:

1. Include `opening_hours` (raw string) in the `poi_query` JSON so the guest
   can call a host-side evaluate import, **or**
2. Prefer **host-evaluated** `open_now: true|false|unknown` on each result
   (recommended — keeps the `opening-hours` crate and clock in one place,
   avoids shipping a second OH parser into WASM).

Either way, closed-now POIs must not reach the HUD list.

---

## Presentation

- Quiet, non-blocking marker or HUD chip on the **top right** (opposite the
  yellow children / road-sign warning box on the left) — not the
  `RoadSignWarningBox` approach-phase chrome used for hazards. No urgency
  state, no sound.
- Icon keys reuse the existing semantic keys from `osm_icon_key`
  ([`icons.md`](../icons.md)): `tourism-attraction`, `tourism-viewpoint`,
  `tourism-museum`, `tourism-artwork` (or nearest shipped key until an
  artwork-specific glyph exists), `amenity-*`, fishing keys (`leisure-fishing` /
  `leisure-fishing_pier` / `sport-fishing` / `shop-fishing` as resolved by
  `osm_icon_key`), and `shop-alcohol` for craft brewery / cider.
- Suggested label: distance + name (when named) + category, e.g. "650 m —
  Viewpoint", "400 m — Fishing pier", or "300 m — Fosmoen cider". Append
  "(hours unknown)" when `opening_hours` was missing/unparseable.
- One entry per POI in the cone; when several fall in the same short stretch,
  nearest-first is a reasonable default (no priority/merge logic is needed
  here the way it is for hazard categories, since nothing is being suppressed
  for safety reasons — this is a browsing list, not a single warning slot).
  Closed-now POIs are already filtered out before ordering.

---

## Host capabilities (proposed)

Implemented today (reuse): `log`, `position_read`, `poi_query`.

| Capability | Purpose |
|---|---|
| `position_read` | GPS lat/lon + heading for the cone anchor |
| `poi_query` | Existing capability — JSON POI list into guest buffer. If today's `poi_query` is an omnidirectional radius query only, it needs a **heading + half-angle + radius** query shape (or the guest filters a radius result by bearing itself using the same `bearing_deg`/`angle_diff_deg` math as `live_hazard.rs`, applied to `POI_LOOKAHEAD_CONE_M` / `POI_LOOKAHEAD_CONE_HALF_WIDTH_DEG` rather than the hazard-cone constants). Results must carry open-now / hours fields as above |
| `clock_read` | Local "now" for opening-hours evaluation (or used only by host if open-now is precomputed) |
| `plugin_kv` / `storage` | Persist the enable/disable toggle, optional strict hours-unknown filter, and any "don't show this POI again" dismissal, if that UX is wanted |
| `log` | Diagnostics |

No `voice_speak`, `alert_sound_play`, or `warning_event_subscribe` — this is a
visual browsing surface, not an alert.

---

## Settings

- Master **Nearby attractions** toggle — **default off** (opt-in). No
  motor-profile or first-run logic may flip it on.
- Optional **Hide when hours unknown** (default off) for drivers who only want
  tagged open venues.
- No severity/urgency settings, since there is no urgency model.

---

## Open questions

1. **Exact `General` subset.** Does "attractions … etc" mean the full existing
   `General` category (including cafe/restaurant/fast_food/picnic_site), or a
   narrower attraction-flavoured subset (viewpoint/attraction/museum/artwork/
   gallery/zoo/aquarium only)? The table above defaults to the full set and
   flags the narrower reading as a filter to apply if that's the intent.
2. **Cone angle reading.** Confirm whether "60 degree cone" means 60° total
   width (±30°, as specified above) or the existing hazard-cone convention of
   a 60° **half**-angle (120° total).
3. **Cider / wine / spirits tagging.** Folded into `CraftBrewery` with one
   `shop-alcohol` icon: cider/wine/mead/beer via `brewery=*`, winery/distillery
   via `craft=*`, plus `industrial=distillery` and `shop=wine`. Separate
   category icons can be revisited later.
4. **Named vs unnamed attractions.** Some `tourism=viewpoint` nodes are
   unnamed. Decide whether unnamed attractions still surface (generic
   "Viewpoint ahead") or are filtered out in favour of named ones only.
5. **Data source parity with the hazard cone.** Should this reuse the same
   `live_hazards_cache/<pbf-stem>/` on-disk layer-cache pattern
   ([`road-signs.md` — Offline / pack architecture](../road-signs.md#offline--pack-architecture-decision))
   for its own compact point set, or ride entirely on the existing `PoiIndex`
   without a dedicated cache? The hazard-cone precedent (sparse points, cheap
   to re-scan) suggests a dedicated cache is optional rather than required.
6. **Strict hours-unknown default.** Keep "show with hours unknown" as default
   (recommended for OSM coverage), or start strict and empty the cone more
   often?

## Explicitly out of scope (v1)

- Any audio, urgency, or safety-styled presentation.
- Modifying `live_hazard.rs`, its constants, or its FFI surface.
- Extending the classifier beyond the current `CraftBrewery` OR-set (already
  includes `brewery=cider`).
- Route-corridor (200 m band) scanning — this is the no-route heading cone
  only; a corridor variant, if wanted later, is a separate spec.
- Filtering by ratings, reviews, or other non-OSM popularity signals.

## Testing (when implemented)

- Unit: cone membership — reuse the existing `bearing_deg`/`angle_diff_deg`
  pattern's test shape (`cone_rejects_behind_and_beyond_radius` in
  `live_hazard.rs`) against the new 850 m / ±30° constants.
- Unit: category filter — a `Cabin`-only object never appears; a `Water`-only
  object never appears; a `Water` + `General`(attraction) object does appear;
  a `Fishing` object (pier / spot / shop) does appear when in the cone and not
  known-closed.
- Unit: opening hours — tagged closed-now is suppressed; tagged open-now
  appears; missing hours appears with "hours unknown"; unparseable hours do
  not invent open/closed.
- Unit: `brewery=cider` classifies as `CraftBrewery`.
- Device: no-route cone behaves the same with and without an active plan,
  mirroring the existing `LiveHazardConeVallsetInstrumentedTest` pattern for
  route-independence, but asserting POI content rather than hazard content;
  confirm a venue that closes during approach drops from the chip list.

## References

- [`poi.md`](../poi.md) — `PoiCategory` definitions, OSM tag rules, radii, and
  the "Adding a POI category" checklist.
- [`road-signs.md`](../road-signs.md) — existing route-independent look-ahead
  cone ("Look forward"), 300 m / ±60°, no-route heading-driven mechanism.
- [`icons.md`](../icons.md) — semantic icon keys (`tourism-*`, fishing /
  `shop-fishing`, `shop-alcohol`).
- [`cider-route.md`](../cider-route.md) / [`how-to-use.md`](../how-to-use.md) —
  Norwegian cider-route example; `brewery=cider` is now a `CraftBrewery`
  classifier match (and motor break candidate) as well as searchable by name.
- [`crates.md`](../crates.md) / `core/src/routing/conditional.rs` —
  `opening-hours` crate already used for OSM time conditions.
- `core/src/routing/live_hazard.rs` — cone geometry precedent
  (`bearing_deg`, `angle_diff_deg`, `in_live_cone`), reused for its
  **pattern**, not its constants.
