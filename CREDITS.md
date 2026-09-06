# Credits and third-party assets

## OpenStreetMap Carto POI sprites (offline basemap)

Additional POI icons in `app/src/main/assets/map-styles/protomaps-light/sprites/`
(`alcohol`, `fuel`, `charging_station`, `parking`, `hospital`, `pharmacy`, `hotel`,
`townhall`, `car_repair`, `motorcycle`, `motorcycle_repair`, `bicycle`,
`bicycle_repair`, `bicycle_repair_station`, `police`, `fire_station`,
`place_of_worship`, `christian`, `spring`) are derived from the
[OpenStreetMap Carto](https://github.com/gravitystorm/openstreetmap-carto) symbol
set.

- **License:** [CC0 1.0 Universal (Public Domain Dedication)](https://creativecommons.org/publicdomain/zero/1.0/)
- **Source tree:** `symbols/` in the Carto repository (e.g. `shop/alcohol.svg`,
  `amenity/fuel.svg`, `amenity/town_hall.svg`, `amenity/police.svg`,
  `amenity/firestation.svg`, `amenity/place_of_worship.svg`,
  `religion/christian.svg`, `natural/spring.svg`)
- **Build:** `scripts/build-poi-sprites.sh` (download Carto SVGs, pack with
  [spreet](https://github.com/flother/spreet), merge into the existing Protomaps
  light atlas via `scripts/merge_sprite_atlas.py`)

**Substitutions (no exact Carto file):**

| Navi `kind` / sprite key | Carto source used |
|---|---|
| `bicycle_repair` | `shop/bicycle.svg` (Carto has no `shop/bicycle_repair.svg`) |
| `bicycle_repair_station` | `shop/bicycle.svg` (same) |
| `fire_station` | `amenity/firestation.svg` (Carto filename has no underscore) |
| `spring` | `natural/spring.svg` (Carto stroke `#ffffff` remapped to `#000000` for the light basemap) |

## Railway station sprite (offline basemap)

Protomaps `pois.kind=station` (OSM `railway=station`) uses the Navit
`rail_station.svg` icon packed into the same atlas as sprite key `station`.
OSM Carto renders stations as a generic square, not a pictorial symbol, so
there is no Carto SVG to reuse.

- **Source:** [`core/src/icons/rail_station.svg`](core/src/icons/rail_station.svg)
- **License:** GPL v2 (Navit-derived; same family as other Navi overlay icons —
  see [`docs/icons.md`](docs/icons.md))

The bundled Protomaps light atlas baseline remains from
[protomaps/basemaps-assets](https://github.com/protomaps/basemaps-assets) (v4).

See also [`docs/icons.md`](docs/icons.md) for the separate Navit overlay icon set.
