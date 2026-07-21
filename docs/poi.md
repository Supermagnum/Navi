# Searchable POI categories

Navi indexes OpenStreetMap features into `PoiCategory` values used by rest /
overnight planning and nearby search. A single OSM object may match **more than
one** category.

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

## Place / address FTS search

Separate from the POI R-tree: the offline `NameIndex` (FTS5) indexes named OSM
features for the **To** / **Via** UI (places, huts, peaks, roads, settlements).
That index is built from the regional `.osm.pbf` via **Tools → Download corridor
region + build place index** (or equivalent provision).

## Icon keys

Craft brewery / alcohol retail maps to semantic icon key `shop-alcohol`. Other
POIs use `amenity-*`, `tourism-*`, etc. See [`icons.md`](icons.md).

## Code

- `core/src/poi/categories.rs` — enum + radii
- `core/src/poi/classifier.rs` — tag rules
- `core/src/poi/index.rs` — R-tree load / `nearest` query
