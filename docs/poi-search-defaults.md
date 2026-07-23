# Suggested POI search defaults (hiking & cycling)

Suggested defaults for **Hiking** and **Cycling** profiles when searching for
huts and when preferring official trails. Radii tune optional network-hut
preference and related POI search; they are not hard routing limits.

Code constants today (`core/src/config/defaults.rs` / `SafetyConfig`):

| Setting | Default |
|---|---|
| Network hut search radius (`POI_RADIUS_NETWORK_HUT_M`) | 25 km |
| Network hut preference radius (`POI_NETWORK_HUT_PREFERENCE_RADIUS_M`) | 11 km |

Category rules (which OSM tags count as NetworkHut vs Cabin): [`poi.md`](poi.md).

## DNT / network hut priority

Optional preference for network huts with a configurable search radius. Set
radius from typical nearest-neighbor spacing; raise toward the max in remote
areas.

Networked cabin spacing (sample nearest-neighbor stats, for search-radius
tuning):

| Region | Sample | Avg | Median | Max |
|---|---|---|---|---|
| Norway (DNT, OSM relation 1110420) | 449 huts | 10.56 km | 8.83 km | 100.45 km |
| Sweden (wilderness/alpine hut) | 439 | 12.31 km | 8.24 km | 83.85 km |
| Sweden STF only | 42 | 14.47 km | 11.50 km | 83.85 km |
| Finland (wilderness/alpine hut) | 324 | 11.72 km | 6.68 km | 64.32 km |
| Finland Metsähallitus only | 108 | 16.05 km | 5.31 km | 247 km |
| Germany (wilderness/alpine hut) | 261 | 12.98 km | 9.72 km | 119.76 km |
| Switzerland (wilderness/alpine hut) | 328 | 4.40 km | 3.82 km | 23.70 km |
| Austria (wilderness/alpine hut) | 330 | 5.30 km | 3.56 km | 102.51 km |

Open / non-networked huts (no network tag; not DNT/STF/DAV/SAC/OeAV/Metsähallitus
etc.):

| Region | Sample | Avg | Median | Max |
|---|---|---|---|---|
| Germany | 235 | 14.29 km | 10.22 km | 119.76 km |
| Switzerland | 261 | 4.93 km | 4.03 km | 23.70 km |
| Austria | 287 | 5.71 km | 3.65 km | 102.51 km |
| Sweden | 395 | 12.45 km | 7.85 km | 64.86 km |
| Finland | 206 | 17.00 km | 12.33 km | 75.75 km |

Practical ranges: ~5–15 km in the Alps; ~10–20 km in
Scandinavia/Germany/Finland for open huts. Use at least typical spacing
(~10–12 km); raise toward max remotely.

## Hiking / pilgrimage priority

Optional preference for official hiking and pilgrimage routes when validating
or suggesting stops.
