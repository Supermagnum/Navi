# Suggested POI search defaults (hiking & cycling)

Suggested defaults for **Hiking** and **Cycling** profiles when searching for
huts and when preferring official trails. Radii tune optional network-hut
preference and related POI search; they are not hard routing limits.

Legacy compile-time constants (`core/src/config/defaults.rs`) still exist for
`SafetyConfig` fallbacks; Drive settings persist per-profile
`ProfilePoiRadii` (defaults below match the slider floors / mid-band):

| Setting | Default (persisted table before first Drive save) |
|---|---|
| Hiking / bicycle / electric cycle search & cabin | 10.5 km |
| Hiking / bicycle network hut (table default) | 25 km |
| After Drive slider save (hike/cycle) | search = cabin = network = preference = slider km |
| Car / motorcycle / truck / mobile home search | 3 h × 80 km/h = 240 km |
| Motor cabin / preference (until slider save) | legacy cabin / preference constants |

Each menu travel profile (Car, Motorcycle, Truck, Mobile home, Bicycle,
Electric cycle, Hiking) stores its own radii via UniFFI
`loadProfilePoiRadii` / `saveProfilePoiRadii`. Drive-settings slider ranges:

| Profile | Slider | Notes |
|---|---|---|
| Hiking | **10.5–20 km** | Cabin / pause search; also auto-via lateral + detour floor |
| Bicycle / Electric cycle | **10.5–28 km** | Cabin / pause search |
| Car / motorcycle / mobile home / truck | **2–4 hours** | Converted at ~80 km/h to metres for the planner; truck HOS clocks stay jurisdiction-locked |

**Car, motorcycle, truck, and mobile home** always require POIs to be linked to
the road network (or the planned corridor within ~1 km). Hiking and cycling
prefer path/trail-linked stops but may allow a short off-path fallback unless
“Require path / trail link” is enabled.

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

Operator samples (size only): Germany DAV ~22; Switzerland SAC/CAS ~66
(denser Alps); Austria OeAV ~22 (denser Alps).

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

## Path / road connectivity

Prefer POIs that are connected to paths, trails, tracks, or small roads in the
routing graph. Motor profiles (car, motorcycle, truck, mobile home) **require**
road-linked pause and overnight candidates. Hiking and cycling prefer
path-linked huts; a configurable “require path / trail link” switch can harden
that for the active profile.
