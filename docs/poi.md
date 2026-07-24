# Searchable POI categories

Navi indexes OpenStreetMap features into `PoiCategory` values used by rest /
overnight planning and nearby search. A single OSM object may match **more than
one** category.

Overnight POI use: truck EC 561 multi-day rests — [`ec-561-truck-rest.md`](ec-561-truck-rest.md)
(**RestArea**); soft car / motorcycle / mobile home / cycle multi-day overnight —
README “Rest and overnight” (**Lodging**, camping tags, **RestArea** fallback).
Jurisdiction scope for legal packs: [`jurisdiction-rules.md`](jurisdiction-rules.md).

Default search radii come from `SafetyConfig` / `defaults.rs` (CraftBrewery uses
the General radius of **15 km**).

| Category | Default radius | OSM match rules (any listed condition) |
|---|---|---|
| **Water** | 2 km | `amenity` = drinking_water, fountain, or water_point; **or** `natural=spring` |
| **Restroom** | General (or safety override) | `amenity=toilets` |
| **Cabin** | 5 km | `tourism` ∈ wilderness_hut, alpine_hut, hostel, camp_site, camp_pitch; **or** `amenity=shelter` |
| **OvernightFacility** | 5 km (same as Cabin) | Assigned together with Cabin for the same overnight tags |
| **NetworkHut** | 25 km | wilderness_hut / alpine_hut **and** `operator` or `network` contains DNT, STF, DAV, SAC, OeAV, or Metsähallitus |
| **General** | 15 km | `amenity` ∈ cafe, restaurant, fast_food, museum, gallery, zoo, aquarium, viewpoint, picnic_site; **or** `tourism` ∈ viewpoint, attraction, museum |
| **CraftBrewery** | 15 km (General) | **OR** of: `microbrewery=yes`, `shop=alcohol`, `craft=brewery` |
| **TentSite** | Cabin radius | `tourism` ∈ camp_site, camp_pitch; **or** `amenity=camping` |
| **Fishing** | 15 km (General) | **OR** of: `leisure=fishing`, `leisure=fishing_pier`, `sport=fishing`, `shop=fishing` — icon: Navit-derived `fish.svg` as `leisure-fishing` ([`icons.md`](icons.md)) |
| **RestArea** | max(General, 20 km) | **OR** of: `highway=rest_area`, `highway=services`, or `amenity=parking` with HGV access (`hgv` / `access:hgv` = yes or designated). Used for truck EC 561 overnight matching and as a fallback for car / motorcycle / mobile home soft multi-day overnight (intentionally simpler than hiking hut scoring — nearest tagged stop within a fixed radius) |
| **Lodging** | max(General, 20 km) | **OR** of: `tourism` ∈ hotel, motel, guest_house, apartment, chalet, hostel. Used for car / motorcycle / mobile home / cycle multi-day overnight matching (nearest within radius; simpler than hiking hut scoring) |

Suggested preference radii and regional nearest-neighbor spacing (for tuning
hiking/cycling search): [`poi-search-defaults.md`](poi-search-defaults.md).

## Place / address FTS search

Separate from the POI R-tree: the offline `NameIndex` (FTS5) indexes named OSM
features for the **To** / **Via** UI (places, huts, peaks, roads, settlements).
That index is built from the regional `.osm.pbf` via **Tools → Download region
+ build place index** (country or region-in-country Geofabrik extract, or
equivalent provision). Named fishing spots that
appear in FTS are searchable by name without a dedicated `PoiCategory`; the
category system below is for **typed nearby / rest planning** queries
(`PoiIndex::nearest`).

## Icon keys

Craft brewery / alcohol retail maps to semantic icon key `shop-alcohol`. Other
POIs use `amenity-*`, `tourism-*`, `leisure-*`, etc. via `osm_icon_key` in
`core/src/poi/icons.rs`. See [`icons.md`](icons.md).

## Adding a POI category (example: fishing)

Use this checklist when you want a new typed POI such as **fishing** (spots,
piers, shops), not only name search.

### 1. Decide OSM tags

Pick the OpenStreetMap tags that should match. For fishing, common choices
include (OR any that you care about):

| Tag | Meaning (typical) |
|---|---|
| `leisure=fishing` | Fishing spot / area |
| `sport=fishing` | Sport fishing |
| `shop=fishing` | Fishing tackle shop |
| `leisure=fishing_pier` | Pier used for fishing (where mapped) |

Confirm against [OSM Taginfo](https://taginfo.openstreetmap.org/) / the wiki so
you do not invent tags the extract will never contain.

### 2. Add `PoiCategory` + default radius

In [`core/src/poi/categories.rs`](../core/src/poi/categories.rs):

1. Add a variant, e.g. `Fishing`.
2. Map it in `default_radius_m` — either reuse an existing safety radius
   (`poi_radius_general_m`) or add a dedicated field on `SafetyConfig`
   ([`core/src/config/safety.rs`](../core/src/config/safety.rs) +
   [`core/src/config/defaults.rs`](../core/src/config/defaults.rs)).

Example radius choice: **5–15 km** for “nearby on a drive”; tighter (e.g. 2 km)
if the POI is only useful when almost on-route.

### 3. Classify OSM tags

In [`core/src/poi/classifier.rs`](../core/src/poi/classifier.rs), extend
`classify_tags` so matching objects push `PoiCategory::Fishing` (objects may
already push other categories too).

```rust
let leisure = tags.get("leisure").map(String::as_str);
let sport = tags.get("sport").map(String::as_str);
let shop = tags.get("shop").map(String::as_str);

if leisure == Some("fishing")
    || leisure == Some("fishing_pier")
    || sport == Some("fishing")
    || shop == Some("fishing")
{
    out.push(PoiCategory::Fishing);
}
```

Add unit tests next to the existing craft-brewery tests (match each tag style;
assert non-fishing tags do not match).

### 4. Icon key (optional but recommended)

`osm_icon_key` already returns `leisure-fishing` / `shop-fishing` from those
tags. For a stable product key (like brewery → `shop-alcohol`), add an early
branch in [`core/src/poi/icons.rs`](../core/src/poi/icons.rs), e.g. return
`"fishing"`, then provide:

- `core/src/icons/fishing.svg` (and/or alias in the icon resolver), and
- the same file under `app/src/main/assets/icons/` if the Android lean pack must
  show it on-device.

Authoring steps: [`icons.md`](icons.md).

### 5. Wire consumers

Anything that **queries by category** must know the new variant:

| Area | What to update |
|---|---|
| Rest / overnight / corridor helpers | Call `poi.nearest(PoiCategory::Fishing, …)` where fishing should influence planning |
| Integration tests | Assert hits near a known fishing node in a fixture extract |
| UniFFI / Android | Expose the category if the host UI lists POI types (`navi-ffi`, Kotlin bindings) |
| This doc | Add a row to the table above |

Rebuild the POI index after code changes (re-run region provision / graph+POI
pipeline). Old on-disk indexes do not gain new categories until rebuilt from
`.osm.pbf`.

### 6. Verify

```bash
cargo test -p driver-break-core poi::
# plus any ignored integration test that loads a real extract
```

On device: provision a region that contains fishing tags, then confirm
`nearest(Fishing, …)` (or the host UI that uses it) returns expected places.

### What you usually do **not** need

- Changing MapLibre basemap styles (vector “fishing” icons on the tile layer are
  separate from Navi’s `PoiIndex`).
- FTS place search — names still resolve via the place index if the OSM object
  is named; a category is only required for typed “find fishing near me / on
  route” logic.

## Code

- `core/src/poi/categories.rs` — enum + radii
- `core/src/poi/classifier.rs` — tag rules
- `core/src/poi/icons.rs` — OSM → icon key
- `core/src/poi/index.rs` — R-tree load / `nearest` query
- `core/src/config/safety.rs` / `defaults.rs` — default search radii
