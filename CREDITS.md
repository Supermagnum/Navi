# Credits and third-party assets

## OpenStreetMap Carto POI sprites (offline basemap)

Additional POI icons in `app/src/main/assets/map-styles/protomaps-light/sprites/`
(`alcohol`, `fuel`, `charging_station`, `parking`, `hospital`, `pharmacy`, `hotel`,
`townhall`, `car_repair`, `motorcycle`, `motorcycle_repair`, `bicycle`,
`bicycle_repair`, `bicycle_repair_station`) are derived from the
[OpenStreetMap Carto](https://github.com/gravitystorm/openstreetmap-carto) symbol
set.

- **License:** [CC0 1.0 Universal (Public Domain Dedication)](https://creativecommons.org/publicdomain/zero/1.0/)
- **Source tree:** `symbols/` in the Carto repository (e.g. `shop/alcohol.svg`,
  `amenity/fuel.svg`, `amenity/town_hall.svg`)
- **Build:** `scripts/build-poi-sprites.sh` (download Carto SVGs, pack with
  [spreet](https://github.com/flother/spreet), merge into the existing Protomaps
  light atlas via `scripts/merge_sprite_atlas.py`)

**Substitutions (no exact Carto file):**

| Navi `kind` | Carto source used |
|---|---|
| `bicycle_repair` | `shop/bicycle.svg` (Carto has no `shop/bicycle_repair.svg`) |
| `bicycle_repair_station` | `shop/bicycle.svg` (same) |

The bundled Protomaps light atlas baseline remains from
[protomaps/basemaps-assets](https://github.com/protomaps/basemaps-assets) (v4).

See also [`docs/icons.md`](docs/icons.md) for the separate Navit overlay icon set.
