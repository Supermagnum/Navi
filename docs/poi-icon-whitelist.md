# Offline Protomaps POI icon whitelist

Current-state reference for **what draws on the offline basemap** and why.
This is the MapLibre `pois` layer in
`app/src/main/assets/map-styles/protomaps-light/style.template.json`, not the
rest/overnight [`poi.md`](poi.md) `PoiIndex`.

Re-verify against `style.template.json` and
`app/src/main/assets/map-styles/protomaps-light/sprites/light.json` whenever
this document is read long after it was written. The whitelist and atlas
change independently of this file.

Sprite keys below were checked against `light.json` at the time of writing.
Icon-image fallback in the style is `townspot`; **no currently whitelisted
kind uses that fallback**.

## 1. Whitelisted kinds with dedicated icons

Every kind in the offline `pois` allow-list maps to a named sprite in
`light.json` (not `townspot`). Shared art is noted.

| kind | sprite | notes |
|---|---|---|
| `school` | `school` | |
| `university` | `university` | |
| `college` | `university` | shared university sprite |
| `kindergarten` | `school` | shared school sprite |
| `fuel` | `fuel` | OSM Carto |
| `charging_station` | `charging_station` | OSM Carto |
| `convenience` | `convenience` | bundled Protomaps atlas |
| `supermarket` | `supermarket` | bundled Protomaps atlas |
| `grocery` | `supermarket` | shared supermarket sprite |
| `alcohol` | `alcohol` | OSM Carto; `shop=alcohol` (Vinmonopolet) |
| `car_repair` | `car_repair` | OSM Carto; see shop-vs-amenity note in section 3 |
| `motorcycle_repair` | `motorcycle_repair` | OSM Carto |
| `motorcycle` | `motorcycle` | OSM Carto; see shop-vs-amenity note in section 3 |
| `bicycle` | `bicycle` | OSM Carto; routing-related amenity/shop, not the held-back retail audit row |
| `bicycle_repair` | `bicycle_repair` | OSM Carto `shop/bicycle.svg` (no Carto bicycle_repair) |
| `bicycle_repair_station` | `bicycle_repair_station` | same Carto `shop/bicycle.svg` |
| `bus_stop` | `bus_stop` | |
| `station` | `station` | Protomaps `railway=station`; kind floor z12; sprite from Navit `rail_station.svg` (GPL v2) |
| `parking` | `parking` | OSM Carto |
| `attraction` | `attraction` | |
| `museum` | `museum` | |
| `cafe` | `cafe` | |
| `restaurant` | `restaurant` | |
| `fast_food` | `fast_food` | |
| `hospital` | `hospital` | OSM Carto |
| `pharmacy` | `pharmacy` | OSM Carto |
| `library` | `library` | |
| `post_office` | `post_office` | |
| `toilets` | `toilets` | |
| `drinking_water` | `drinking_water` | |
| `bench` | `bench` | |
| `playground` | `park` | shared park sprite |
| `park` | `park` | |
| `zoo` | `zoo` | |
| `theatre` | `theatre` | |
| `cinema` | `theatre` | shared theatre sprite |
| `hotel` | `hotel` | OSM Carto |
| `townhall` | `townhall` | OSM Carto; kind floor z15 |
| `peak` | `peak` | kind floor z13 (OSM Carto / osm.org peak floor; extract maxzoom is 15) |
| `hill` | `peak` | shared peak sprite; kind floor z13 |
| `glacier` | `peak` | icon opacity 0; name-only from z12 |
| `wetland` | `park` | icon opacity 0; name-only from z12 |

Default kind floor is **z16**. Exceptions: `glacier` / `wetland` / `station` →
12, `peak` / `hill` → 13, `townhall` → 15. Peaks cannot stay at the default
z16 floor: offline region extracts are native maxzoom 15, so a z16 gate never
shows. Railway stations use z12 so named stops (e.g. Hamar, Lillehammer)
appear at town overview zoom; tile `min_zoom` from Protomaps QRank still
hides minor stops until their packed floor.

## 2. Whitelisted kinds still using generic fallback

None. After the OSM Carto sprite pass, every allow-listed kind has a dedicated
(or deliberately shared) atlas key. `townspot` remains the `icon-image` default
for unknown kinds and is unused by the current whitelist.

## 3. Audited-but-not-whitelisted shop kinds

Retail `shop=*` kinds confirmed in eight Østlandet z15 tiles (Gjøvik, Hamar,
Oslo, Lillehammer, Lillestrøm, Fredrikstad, Tønsberg, Elverum on Protomaps
planet `20260722`) and **held back** to keep town-centre density down. Counts
are named hits in that sample, not a full-region census.

Held back on purpose — not a missing-sprite bug:

| kind | named hits | notes |
|---|---|---|
| `kiosk` | 23 | e.g. Narvesen at CC Gjøvik |
| `mall` | 10 | e.g. CC Gjøvik; source `min_zoom` as low as 13 (style floor would still hold to z16) |
| `chemist` | 12 | not `pharmacy` (already whitelisted) |
| `doityourself` / `hardware` | 5 / 3 | Clas Ohlson, Jernia |
| `bakery` | 9 | |
| `butcher` | 2 | |
| `greengrocer` | 5 | |
| `clothes` | 72 | densest by far |
| `hairdresser` | 38 | `min_zoom` often 17 |
| `beauty` | 13 | atlas already has unused `beauty` sprite |
| `mobile_phone` | 13 | |
| `electronics` | 10 | atlas already has unused `electronics` sprite |
| `sports` | 11 | |
| `books` | 12 | atlas already has unused `books` sprite |

Smaller-count set also present in that sample, still not whitelisted:
`optician`, `furniture`, `houseware`, `florist`, `shoes`, `jewelry`, `gift`,
`toys`, `bicycle` *(shop kind in the audit — distinct from the now-whitelisted
routing-related `bicycle` icon)*, `cosmetics`, `tattoo`, `paint`, `computer`,
`pet`, `confectionery`, `variety_store`, `department_store`, `garden_centre`,
`copyshop`, `laundry`, `dry_cleaning`, `car`, `second_hand`, `ticket`, `hifi`,
`photo`, `fabric`.

Unused-but-present atlas sprites that would cover a subset if the whitelist
is ever extended: `beauty`, `books`, `clothes`, `electronics`.

### Shop vs amenity tagging (same kind string)

Protomaps `Pois.java` maps several OSM keys onto one `kind` string. The
Østlandet **shop** audit and the later **whitelist** are not the same source
tag:

- `car_repair` — **absent** as `shop=car_repair` in the eight-tile sample;
  later **whitelisted** from a different OSM tag that still tiles as
  `kind=car_repair` (typically `amenity=car_repair` / similar). Do not treat
  the audit “absent” row as a contradiction of the whitelist.
- `motorcycle` — same pattern: missing as a **shop** kind in that sample;
  **whitelisted** from amenity/shop-motorcycle tagging that tiles as
  `kind=motorcycle`.
- `bicycle` — the audit’s shop-kind row was held back for density; the
  whitelist `bicycle` / `bicycle_repair` / `bicycle_repair_station` icons are
  the later Carto pass for cycle amenities, not a decision to open all
  `shop=bicycle` retail.

Confirmed **absent** from that sampled extract (no need to whitelist yet):
`watches`, `stationery`, `travel_agency`, `funeral_directors`,
`car_repair` *(as a shop kind)*, `motorcycle` *(as a shop kind)*, `tobacco`,
`wholesale`.

## 4. Non-shop core whitelist

These were the original amenity / civic / nature allow-list, not part of the
retail audit. They remain long-standing:

`school`, `university`, `college`, `kindergarten`, `fuel`, `charging_station`,
`bus_stop`, `station`, `parking`, `attraction`, `museum`, `cafe`, `restaurant`,
`fast_food`, `hospital`, `pharmacy`, `library`, `post_office`, `toilets`,
`drinking_water`, `bench`, `playground`, `park`, `zoo`, `theatre`, `cinema`,
`hotel`, `townhall`, `peak`, `hill`, `glacier`, `wetland`.

Grocery (`convenience`, `supermarket`, `grocery`) was already in that core
set before the alcohol/shop audit. `alcohol` and the vehicle-repair kinds
were added later; see [`map-styles.md`](map-styles.md). `station` (railway
station names/icons) was missing from the allow-list until a later pass —
tiles already carried `pois.kind=station` from OSM `railway=station`.
