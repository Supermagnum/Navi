# Navi architecture

File-level “where do I edit X?” map for contributors:
[`codebase-map.md`](codebase-map.md). Callable APIs: [`API.md`](API.md).
Which Rust crates are first-party vs crates.io (unaltered):
[`rust-crates.md`](rust-crates.md).
Canonical future-proofing findings and tracked risk priorities:
[`future-proofing-audit-2026-07.md`](future-proofing-audit-2026-07.md).

## Layout

| Path | Crate / role |
|---|---|
| `core/` | `driver-break-core` — trusted Rust: elevation, routing, POI, search, rest/safety, icons, tracks, ECU hooks, SQLite |
| `navi-ffi/` | UniFFI CDYLIB bridging core ↔ Android (and other hosts) |
| `navi-desktop/` | Linux desktop map shell (WebKitGTK + MapLibre GL JS); see [`build-linux.md`](build-linux.md) |
| `app/` | Kotlin/Compose Android Automotive UI + MapLibre |
| `plugin-host/` | `navi-plugin-host` — wasmtime sandbox |
| `plugin-sdk/` | Guest helpers for HostApi imports |
| `plugins/` | Reference `.wasm` guests (`log-hello`, `busy-loop`) |

Workspace members are declared in the root `Cargo.toml`. Default members:
`core`, `plugin-host`.

## Network principle

Core routing, rest planning, and POI queries work fully offline once a region
extract is on disk. Network access is an **opt-in enhancement layer** (DEM
tiles, Geofabrik update checks, fixture downloads, optional PMTiles basemap
download, live OpenFreeMap tiles when no local PMTiles cover the camera). Map
data is never replaced silently in the background — see
[`docs/osm-updates.md`](docs/osm-updates.md) and [`docs/map-styles.md`](docs/map-styles.md).

## Thread priority tiers

| Tier | Role | Priority |
|---|---|---|
| T0 Sensor | GPS / IMU | Highest |
| T1 ECU | Live energy (WASM / native plugin) | High |
| T2 UI / audio | Compose UI; media must stay smooth | High |
| T3 Routing | Graph build, eco-reweight, A* | Medium (below audio) |
| T4 DB | SQLite | Lowest |

Routing-tier Rayon pools are sized from `std::thread::available_parallelism()` with
headroom reserved for T0–T2 so audio/UI are not starved. See
`driver_break_core::routing::workers::WorkerPoolPlan`.

Country-scale OSM loads are opt-in with a low-RAM warning; regional extracts are
the default working set on ~4 GB devices.

---

## How Rust crates wire together

```text
┌─────────────────────────────────────────────────────────────┐
│  app (Kotlin / Compose)                                      │
│  MapLibre · Drive HUD · search UI · settings sheets          │
└───────────────────────────┬─────────────────────────────────┘
                            │ UniFFI (navi.kt ↔ navi-ffi)
                            ▼
┌─────────────────────────────────────────────────────────────┐
│  navi-ffi                                                    │
│  Thin exports: provision region, corridor route, search,     │
│  icons, config load/save, FfiTrackStore, OSM update helpers  │
└───────────────────────────┬─────────────────────────────────┘
                            │ Rust calls
                            ▼
┌─────────────────────────────────────────────────────────────┐
│  driver-break-core                                           │
│  ┌──────────┐ ┌──────────┐ ┌────────┐ ┌────────┐ ┌───────┐ │
│  │ routing  │ │ elevation│ │  poi   │ │ search │ │tracks │ │
│  │ graph A* │ │ DEM/HGT  │ │ R-tree │ │ FTS5   │ │upsert │ │
│  └────┬─────┘ └────┬─────┘ └───┬────┘ └───┬────┘ └───┬───┘ │
│       │            │           │          │          │     │
│  ┌────┴────────────┴───────────┴──────────┴──────────┴───┐ │
│  │ storage (SQLite) · config · eco · rest · safety · ecu  │ │
│  └────────────────────────────────────────────────────────┘ │
│  bus::WorldSnapshot  (position + profile + live_energy)      │
└─────────────────────────────────────────────────────────────┘

┌──────────────────────┐     HostApi imports      ┌─────────────┐
│ plugin-host          │◄────────────────────────►│ plugin.wasm │
│ (wasmtime + fuel)    │                          │ (guest)     │
└──────────┬───────────┘                          └─────────────┘
           │ callbacks into host process
           │ (position, poi_query/write, log; more planned)
           ▼
     May call UniFFI / core via the Android host — guests never
     open WASI FS or raw network themselves.
```

### Crate responsibilities

| Crate | Depends on | Owns |
|---|---|---|
| `driver-break-core` | rusqlite, osmpbf, geo, pathfinding, rayon, … | All trusted algorithms + DB schema for app config / elevation jobs / routes; in-memory `TrackStore`; graph cache files |
| `navi-ffi` | `driver-break-core`, uniffi | Stable ABI for Kotlin; no UI |
| `navi-plugin-host` | wasmtime, serde | Manifest load, capability gate, fuel/timeout |
| `navi-plugin-sdk` | (minimal) | Guest-side wrappers |
| `plugins/*` | plugin-sdk | Example guests only |

### Typical data paths

| User action | Path |
|---|---|
| Provision region | App → UniFFI `provisionRegionData` → download/parse `.pbf` + DEM → graph build/reweight → cache on disk |
| Download basemap | App Tools → `pmtilesQueueRegion` / `pmtilesRunJob` → `{dataDir}/pmtiles/*.pmtiles` → MapLibre `pmtiles://file://` |
| Download terrain DEM | App Tools → `pmtilesQueueDemRegion` / `pmtilesRunJob` → `{dataDir}/pmtiles/{region}_dem.pmtiles` (Mapterhorn) |
| Search place | App → `searchPlaces` → `NameIndex` FTS5 DB |
| Route corridor | App → `runCarCorridorPipeline` → `RouteGraph` A* (+ eco weights) → polyline + POI back to MapLibre |
| Save drive settings | App → `saveCarRestSettings` / `saveFuelConfig` / `saveEbikeConfig` / `saveEvCarConfig` → `ConfigStore` → `app_config` rows |
| Moving icon | Test/host → `FfiTrackStore.upsert` → `TrackStore` → Compose overlay |
| Future APRS SDR | Host + `rtl-sdr-rs` → DSP → upsert tracks (see [`docs/APRS-SDR.md`](docs/APRS-SDR.md)) |
| Future CAT auto-tune | Host CAT + repeater DB → VFO 1 (see [`docs/CAT.md`](docs/CAT.md)) |

### Core modules (`driver-break-core`)

| Module | Role |
|---|---|
| `routing` | OSM graph build, eco reweight, A*, workers, OSM updates, elevation, PMTiles basemap jobs |
| `poi` | Categories, classifier, R-tree index, icons |
| `search` | FTS5 name index + saved `routes` table helpers |
| `storage` | SQLite `Storage`, migrations, config + elevation + pmtiles job stores |
| `download` | Shared `DownloadControl` (pause / resume / cancel) |
| `config` | Profiles, rest/eco/safety/fuel/vehicle limits |
| `tracks` | APRS-style station upsert / range / timeout |
| `ecu` | `LiveEnergyProvider` / `refine_energy_cost` (no polling yet) |
| `bus` | `WorldSnapshot` shared between tiers |
| `sensors` | Position sample stubs |
| `icons` | SVG/SVGZ → PNG rasterization |

---

## Database

Persistence is **SQLite** (rusqlite, bundled). There are several files / schemas
on purpose so a corrupt FTS index cannot wipe elevation job state.

### 1. App / config DB (`Storage`)

Opened via `driver_break_core::storage::Storage::open(path)`. Migration in
`storage/schema.rs` creates:

| Table | Purpose |
|---|---|
| `app_config` | Key/JSON blob store for rest, safety, eco, vehicle limits, fuel (`ConfigStore`) |
| `elevation_jobs` | DEM download job metadata (bbox, status, progress) |
| `elevation_job_tiles` | Per-tile status, etag, local path (FK → jobs) |
| `routes` | Saved route endpoints, profile, via JSON, break/overnight hints |

`ConfigStore` keys (JSON values): `rest_config`, `safety_config`, `eco_config`,
`vehicle_limits`, `fuel_config`, `ebike_config`, `ev_car_config`, `truck_driving_history` (EC 561 day rows /
extensions — see [`ec-561-truck-rest.md`](ec-561-truck-rest.md)).

Access is serialized with `Arc<Mutex<Connection>>` (T4). UniFFI load/save
helpers used by Drive settings write through this store.

### 2. Place name index (FTS5)

`search::NameIndex` uses its **own** SQLite file (or in-memory):

| Table | Purpose |
|---|---|
| `name_entries` | `osm_id`, name, kind, lat, lon |
| `name_fts` | FTS5 virtual table over names (`content='name_entries'`) |

Built from the region `.pbf` (`load_from_pbf`). Queries use prefix FTS
(`query*`) and return `NameHit` for the search UI.

### 3. Graph cache (not SQLite)

Eco-reweighted graphs are stored as a **binary file** (`NAVIGPH1` magic +
bincode) via `routing/graph/cache.rs` so warm starts skip full reweight. This is
separate from SQLite for size and mmap-friendly load.

### 4. Tracks

`TrackStore` is **in-memory** (HashMap) with timeout + Haversine `visible()`.
Persistence of APRS stations to SQLite is not required for the current moving-
icons tests; a future APRS plugin may add a stations table.

### 5. DEM tiles

Elevation rasters live as files on disk (HGT / COG under the elevation cache
dir). SQLite only tracks **job progress**, not pixel data.

### Wiring diagram (storage)

```text
navi.db (Storage)
├── app_config          ← Drive settings, limits, fuel
├── elevation_jobs      ← DEM download UX
├── elevation_job_tiles
└── routes              ← saved corridors

places.db (NameIndex)   ← FTS search (path chosen by host)
└── name_entries + name_fts

*.navi-graph-*.rkyv / *.navi-poi-barrier.rkyv / *.navi-manifest.json
                        ← indexed region packs (plan fast path; M5+)
elevation/              ← tile files
```

`.navigph` trip-bbox caches are **deprecated** (M5): planners no longer read or
write them; missing packs fall back to a cold PBF rebuild.

Exact filenames are host-chosen under the Android app data directory; UniFFI
APIs take `dataDir` / explicit paths.

---

## Plugins

See [`docs/plugins.md`](docs/plugins.md) for HostApi, manifest format, isolation,
and the planned beneficial plugins (APRS, weather, road info, CAT, ECU/EV,
voice guidance).

## ECU / live energy (T1)

Live OBD-II / J1939 / MegaSquirt polling is **not** in the trusted core. The
`ecu` module exposes `LiveEnergyProvider` + `LiveEnergySnapshot`; graph reweight
calls `refine_energy_cost` when a snapshot is present. Protocol details:
[`docs/ECU.md`](docs/ECU.md).

## APRS / tracks (T0 display; RF planned)

- **Implemented:** `tracks::TrackStore` upsert, timeout, Haversine range clamp
  50–150 km, map moving icons. See [`docs/APRS.md`](docs/APRS.md).
- **Planned RF path:** IQ via [`rtl-sdr-rs`](https://crates.io/crates/rtl-sdr-rs),
  AFSK/AX.25 DSP in [`docs/APRS-SDR.md`](docs/APRS-SDR.md).

## CAT / repeaters (planned)

Auto-tune VFO 1 from onboard OSM/RepeaterBook NFM sites within 150 km:
[`docs/CAT.md`](docs/CAT.md) (includes Innlandsnettet relation
[18780801](https://www.openstreetmap.org/relation/18780801) as a network model).
