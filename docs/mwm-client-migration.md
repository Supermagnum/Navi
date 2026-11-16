# Navi client migration — `.rkyv` packs to CoMaps MWM + Navi sidecars

Client-side plan for moving Navi off per-region `.rkyv` graph/POI/wetland packs
onto CoMaps leaf `.mwm` base maps plus Navi `{leaf}.navi-sidecar.v1.zst`
overlays. Server contracts (read before implementing further phases):

| Doc (navi-server `main`) | Role |
|--------------------------|------|
| `docs/client-fetch-mwm.md` | Catalog v2, CoMaps discovery, downloads, fingerprints, release switch |
| `docs/sidecar-format.md` | NVSC / EL02 byte format |
| `docs/client-routing-hooks.md` | Wetland Soft/Hard, boardwalk, elevation, validation routes |
| `docs/client-migration-from-rkyv.md` | Phased acceptance + `.rkyv` retirement rule |
| `docs/examples/catalog-v2.example.json` | Normative catalog subset |

Live host: `https://navigate-me.duckdns.org` (Apache DocumentRoot =
`data/published/`). HTTP paths are `/mwm/…` and `/current.json`, not
`/published/mwm/…`. Filesystem layout on the server still uses `published/mwm/`.

**Feature flag (this repo):** `NAVI_MWM_CLIENT_ENABLED` (default **on** in every
build type and flavour, including F-Droid — unset env and
`BuildConfig.MWM_CLIENT_ENABLED=true`). Set `0` / `false` / `off` (or Gradle
`-Pnavi.mwmClient=false`) to force the classic `.rkyv` path for debugging. With
the flag off, behaviour is identical to today (`.rkyv` downloads, routing, local
PBF fallback). Do not remove the `.rkyv` path until Phase 8.

**Ship gate:** do **not** merge this client as the default store build / tag a
release until Phases **3–6** (CoMaps NDK core, wetland routing hook, search,
rendering/drape) accept. A draft PR may land the gated layer early; production
APKs that default the flag ON are not ready until those phases pass.

---

## 1. Module inventory (today → target)

| Area | Navi modules today | Target |
|------|--------------------|--------|
| Routing graph | `core/src/routing/indexed/graph_pack.rs`, `graph_pack_v8.rs`, `load.rs`, `convert.rs`, `graph/*` | **CoMaps core via NDK** (option A) + JNI/Rust FFI shim; wetland/elevation costs from sidecar |
| Tiles / corridor cache | `routing/indexed/tile_cache.rs`, `corridor_cache.rs`, `corridor_skeleton*` | Unchanged until CoMaps core owns tile I/O; then **CoMaps core** |
| Rendering / drape | Android MapLibre / `basemap/*` PMTiles, optional Mapterhorn hillshade | **Unchanged** basemap path short-term; drape DEM stays client-side (not sidecar). Long-term optional CoMaps drape |
| Search / place index | `search/*`, `pack_server/place_index_after.rs`, Geofabrik PBF → FTS | Phase 5: **CoMaps search** (or keep PBF index until then). PBF fallback **unchanged** while flag off |
| Elevation (routable) | `routing/elevation/*`, graph `delta_h` in `.rkyv` | **Sidecar reader** (`elevation_overrides` EL02) applied on CoMaps altitudes |
| Wetland / boardwalk | `routing/indexed/wetland_pack.rs`, `routing/wetland.rs` | **Sidecar reader** (`wetland_edges`, `boardwalk_flags`); Soft×5 / Hard≈forbid |
| POI / barriers | `routing/indexed/poi_barrier_pack.rs`, `poi/*` | **CoMaps core** POI sections (or keep `.rkyv` POI until search/POI phase) |
| Downloads / catalog | `pack_server/*`, `download/http.rs` | **New** `core/src/mwm/*` (catalog v2, CoMaps mirrors, sidecars). Existing `.rkyv` **unchanged** |
| Region UI | Tools region pills / Geofabrik tree + `current.json` ready ids | Phase 1 data: `country_groups` + `display_name_overrides`; UI wiring later |
| Attribution | Pack-side credits; DATEX/repeaters separate | **Offline** `attribution.json` / `ATTRIBUTION.md` (+ sidecar attribution section) |
| Local PBF fallback | `pack_server/acquisition.rs` → Geofabrik + on-device convert | **Unchanged** while `.rkyv` path lives; kill-switch still falls back |

Legend: **unchanged** = keep as-is for that phase; **CoMaps core via NDK** =
option A from routing-hooks; **sidecar reader** = NVSC; **removed** only after
Phase 8 retirement.

---

## 2. Phases and acceptance checks

### Phase 1 — Catalog + downloads + region UI data (this branch)

- Parse `navi-mwm-catalog/v2`; CoMaps metaserver → mirrors → `countries.txt` +
  Ed25519 (fail closed); country groups (224 / 1159 routable leaves); shared
  leaves; `region-migration.json`; polite MWM + sidecar download; fingerprint;
  atomic release switch; offline attribution store.
- **Acceptance:** unit tests green with flag **on** (CI default); dedicated CI
  job with flag **off** keeps `.rkyv` / `pack_server` green; staging never
  activates a mixed release; World / WorldCoasts are `no_sidecar`.

### Phase 2 — Sidecar reader (EL02) — **done on this branch**

- Implementation: `core/src/mwm/sidecar.rs` (`SidecarReader`).
- Sectioned NVSC v1 + **EL02-only** elevation (legacy i16 rejected); lazy
  section decompress; wetland/boardwalk = sorted `Vec` + binary search
  (~8 B/record); elevation = sorted feature index + flat delta-encoded point
  buffer (decode one feature on lookup); attribution JSON for Licences.
- Golden fixture vendored at
  `core/tests/fixtures/mwm/Norway_Hedmark.navi-sidecar.v1.zst` (same bytes as
  navi-server docs fixture) so CI stays offline and pinned.
- **Acceptance:** Hedmark inspect counts match server meta; unknown optional
  sections skipped; missing required / bad checksum / bad magic / unsupported
  `format_version` / fingerprint mismatch rejected.

### Phase 3 — CoMaps core via NDK + JNI/Rust FFI shim

- Ship / link CoMaps routing (+ map) core; Rust/Kotlin orchestrate downloads and
  call FFI. See **§8 Phase 3 build plan** below (plan only until that work
  starts).
- **Acceptance:** load Østlandet leaf set; route without Navi layers.

### Phase 4 — Routing wetland hook + parity (this branch)

- Soft×5 / Hard≈forbid; boardwalk exempt; elevation overrides on routable
  altitudes; World atomic release switch; optimize / bike-wetland switches;
  turns JSON; ~40-route parity suite vs `.rkyv`.
- Details: `docs/mwm-phase4-parity.md`. Fixture:
  `core/tests/fixtures/mwm/parity-suite-ostlandet.json`.
- **Acceptance (device Samsung `R52TB0JQEDE`):** foot detour **13867 → 16245 m**;
  World switch unit tests; release-build remasure; parity suite MWM 39/40 OK;
  turn-by-turn street names on 3 routes; PreferNonMotorway demo on
  Lillehammer→Drammen. Product defaults left open (see §3 + PR).

### Phase 5 — Search (this branch)

- CoMaps `search::Engine` (free text, category, reverse geocode) behind the
  `navi_comaps_*` C ABI, `core/src/mwm/comaps_search.rs` and UniFFI
  `comaps_search_*`; results merged with Navi's own POI catalogs under
  `comaps` / `navi:*` source labels (no storage change). Flag off keeps the
  `place_index` FTS search.
- Also in this phase: user-confirmed nearest-road snap for
  `START/END_NOT_ON_ROAD` (never automatic), 75 m EL02 band removed, climb for
  eco/diagnostics from CoMaps `ascent_m`/`descent_m` when the flag is ON.
- Details, category gaps and acceptance: `docs/mwm-phase5-search.md`; routing
  decisions A1-A3: `docs/mwm-phase4-parity.md`. Fixture:
  `core/tests/fixtures/mwm/search-suite-ostlandet.json` (43 queries).
- **Acceptance:** offline find for Østlandet places without requiring full
  Geofabrik PBF for MWM-flag users. Host fixture checks and Kotlin unit tests
  pass; the on-device suite (`ComapsPhase5SearchInstrumentedTest`) has not been
  run yet.

### Phase 6 — Rendering / drape (done on this branch, with documented decisions)

- **Drape only on the MWM path** (Phase 6b): the CoMaps drape engine is the only map renderer when the
  MWM flag is ON; MapLibre and PMTiles are not initialised there. The drape owns the camera (gestures,
  zoom, rotation, tilt, heading-up, follow, navigation camera) through the native entry points of
  `patches/comaps/0004-*.patch` (905 lines). No per-device zoom offset, no rotation/tilt fallback.
- Navi overlays (route with turn arrows, route points, position, DATEX, weather,
  tracks, camping, markers) are drape route subroutes, route points, user marks and tracks. Items that
  cannot be expressed (estimated / lost puck colours, APRS and weather sprites) are listed with the
  smallest alternative in `mwm-phase6b-drape.md`.
- In-app map download (catalog, mirrors, verified countries.txt, sidecars) and live leaf registration
  without restart; missing-maps dialog naming the leaves; per-segment maxspeed to the speed-limit HUD.
- Flag OFF keeps the Phase 5 UI (MapLibre, PMTiles) unchanged.
- **Acceptance (Samsung SM-P613, releasetest):** `mwm-phase6b-drape.md` (function matrix, screenshots,
  performance, APK size, CAT dry runs). Phase 6 (first pass): `mwm-phase6-drape.md`.

### Phase 7 — Migrating users' existing `.rkyv` downloads (done on this branch, with documented decisions)

- Detection on the first start with the MWM path: `.rkyv` regions with a loadable graph are mapped to leaf sets with
  `region-migration.json` (shared leaves counted once, World / WorldCoasts included). One screen shows what the user has,
  which map areas they get, the download size, the free space needed and available, and that old maps are removed only
  after the new ones are verified. Options: **Migrate now**, **Later**, **Choose regions**. Nothing happens without a tap.
- Wi-Fi only by default for large migrations; background run in a foreground service with progress, pause / resume,
  restart and network-loss recovery; the catalog the plan was made with is pinned, so a release published while it runs
  is finished first and then offered as an update.
- Atomic switch only after every chosen leaf, World and sidecars are downloaded and verified, then a deep verify from
  disk; only then the app asks before deleting the old `.rkyv` (showing the space freed). Declined: the old maps stay and
  Settings / Map storage offers the cleanup. "Map storage" lists installed leaf sets per country group, release version,
  per-leaf delete and the update offer.
- **Acceptance (Samsung SM-P613, releasetest):** `docs/mwm-phase7-migration.md` (end-to-end run with Østlandet +
  Sørlandet through the UI, edge cases, host end-to-end tests with a fake mirror, screenshots, defects found).
  Østlandet and Sørlandet share `Norway_Southern` and `Denmark_North Denmark Region`; both are downloaded once.

### Phase 8 — Retiring `.rkyv`

- Only after MWM client tested and correct. Server stops serving
  `published/packs/` + `current.json` region manifests; then delete tree.
  **Freeze ≠ retirement.** Client may remove `.rkyv` UI only after server cutover.

---

## 3. Open product decisions (options + recommendation)

Do **not** encode these in code yet.

| Topic | Options | Recommendation |
|-------|---------|----------------|
| Fastest vs shortest wording | (A) Keep “fastest” label with Soft×5 changing foot “best”; (B) Rename to “recommended” / “best”; (C) Explicit fastest vs shortest toggle | **(B)** for foot/hike when wetland hook is on. Phase 4 exposes CoMaps **Time** vs **PreferNonMotorway** (Distance unsupported); owner picks default |
| Wetland penalties for bike | (A) Foot/hike only (server default); (B) Soft only on bike; (C) Same Soft/Hard on bike | **(A)** until field feedback. Phase 4: `comaps_session_set_bike_wetland` (switchable; recommend off) |
| Boardwalk recall ~25% | (A) Ship proxy as-is; (B) Block Soft/Hard until better recall; (C) Optional PBF enrichment later | **(A)** now; track **(C)** as follow-up |
| Android KVM CI / device testing | (A) Host-only Rust unit tests; (B) KVM emulator + NDK; (C) Device farm for six-route | **(A)** for Phase 1; spike **(B)** before Phase 3 merge; **(C)** before Phase 4 sign-off |
| Drape DEM source | (A) Mapterhorn (already in 3D opt-in); (B) CoMaps/other; (C) None | **(A)**; independent of sidecar elevation overrides |
| Display-name overrides beyond Norway | (A) Norway only until review; (B) Add France/UK/… from `likely_outdated_elsewhere` without review; (C) Community PR review per country | **(A)** + **(C)**; never guess **(B)** |

---

## 4. On-device storage layout

See also the `mwm` module docs. Under the app `data_dir`:

```text
{data_dir}/mwm/
  active.json                 # {"version":"261105"} — atomic switch target
  world-release.json          # {"version":"261105"} — must match active for routing
  World.mwm                   # non-routable overlay at storage root (not under version/)
  WorldCoasts.mwm
  releases/{version}/
    catalog.json              # snapshot of catalog used for this install
    attribution.json
    ATTRIBUTION.md
    countries.txt             # verified copy (optional cache)
    leaves/{Leaf}.mwm         # country leaves only (World promoted out on activate)
    sidecars/{Leaf}.navi-sidecar.v1.zst
    leaf-state.json           # per-leaf verify + fingerprint status
  staging/{version}/          # incomplete download; never active
  migration-cache/
    state.json                # Phase 7 migration state (queued / running / paused / ... / verified / done)
    catalog-{version}.json    # the catalog the migration was planned with (pinned)
    later.json                # "Later" reminder, three days
    kept.json                 # migrated .rkyv regions the user kept (never offered again)
  catalog-cache.json          # last catalog fetched for Map storage (offline summary)
```

Rules: never mix `version` values under `active`; shared leaves are paths under
`releases/{version}/leaves/` (downloaded once); on failed switch, delete
`staging/{version}` and keep previous `active.json` **and** root World files.
`World.mwm` / `WorldCoasts.mwm` are downloaded into staging `leaves/`, verified
with the release, then promoted to the MWM root on `activate_release` together
with `world-release.json`. Refuse CoMaps routing when
`assert_world_matches_active` fails (version mismatch or missing World files).
Do **not** leave World overlays under `releases/{version}/leaves/` — that
registers them as versioned countries and breaks Joints cross-MWM routing.

---

## 5. HTTP path note (docs vs live)

`client-fetch-mwm.md` §2 tables write `GET …/published/mwm/current.json`. The
same doc §10 and `client-fetch.md` DocumentRoot contract use `GET …/mwm/…`.
Live `navigate-me.duckdns.org` serves `/mwm/current.json` (200) and 404s
`/published/mwm/…`. Navi resolves MWM URLs as DocumentRoot-relative `/mwm/`,
matching the existing `.rkyv` `/current.json` client.

---

## 6. navi-server doc mismatches to fix upstream

Report these against Supermagnum/navi-server docs (do not guess around them in
the client beyond DocumentRoot-relative URLs already used for `.rkyv`):

| Topic | Docs say | Live / observed |
|-------|----------|-----------------|
| Catalog / sidecar HTTP path | §2 tables often `…/published/mwm/…` | DocumentRoot serves **`/mwm/…`** (200); `/published/mwm/…` 404. §10 already documents `/mwm/`. |
| Hedmark sidecar example | Example bytes `261797`, sha `593e2e…` | Live release `261105`: **`267737`** bytes, sha **`4feb60…`** (MWM fingerprint still matches the example MWM). |
| `countries.txt` body | Described as text | Live body is **JSON** (sha matches catalog). |
| Metaserver `/servers` | Ordered list wording | Live returns a **JSON array** of mirror URLs. |

---

## 7. F-Droid — CoMaps native core from source + download disclosure

F-Droid builds of Navi already compile Rust (`navi-ffi`) and MapLibre-related
native bits from source with NDK **r27c** (see
`tools/fdroid-check/metadata/no.navi.app.yml`). When Phase 3 lands CoMaps core,
the recipe must stay **fully from source**:

### 7.1 Exact from-source recipe (Phase 3)

Pin file: `third_party/comaps.pin.json` (tag `v2026.11.07-16` /
`fd11cc6f…`, Codeberg `comaps/comaps`).

```bash
# 1) Fetch (network only here). Submodule SHAs are fixed by the pin checkout.
./scripts/fetch-comaps.sh
# records: git rev-parse HEAD + git submodule status

# 2) Apply quilt series (wetland callback hook + Android RoutesBuilder)
#    then NDK r27c CMake build of target `organicmaps` → liborganicmaps.so
./scripts/build-comaps-android.sh   # ABI=arm64-v8a; stages into jniLibs (gitignored)

# CMake flags (same as script):
#   -DANDROID_ABI=arm64-v8a -DANDROID_PLATFORM=android-24 -DANDROID_STL=c++_static
#   -DSKIP_TESTS=ON -DSKIP_TOOLS=ON
#   NDK: 27.2.12479018 (r27c) — overrides CoMaps' baked 28.2
```

| Item | Requirement |
|------|-------------|
| CoMaps tree | `srclibs` / `third_party/comaps` at pin SHA; `configure.sh` with `SKIP_MAP_DOWNLOAD=1` |
| Native build | NDK **r27c** + CMake/Ninja target **`organicmaps`** (+ Navi `navi_comaps_*` C ABI) |
| Prebuilt `.so` | **Forbidden** in git and in the F-Droid recipe — `app/src/main/jniLibs/**/*.so` already gitignored |
| ABI | **arm64-v8a** for devices; `x86_64` only if needed for host unit tests — never AVD |
| Patches | `patches/comaps/series` (~134 lines wetland + RoutesBuilder-on-Android) |
| Cannot exclude | **drape** / GLES — stock `organicmaps` → `map` PUBLIC-links drape (Phase 6 uses or replaces rendering) |
| Rust flag | Leave `NAVI_MWM_CLIENT_ENABLED` unset (default ON) |
| CI | Prefer **nightly** arm64 CoMaps native job (full tree ~minutes–tens of minutes); PR CI keeps flag ON/OFF Rust jobs green without rebuilding CoMaps every push |

F-Droid `Builds:` should call `fetch-comaps.sh` + `build-comaps-android.sh` (or
inline the same steps) after the existing `build-android-native.sh` Rust/Hamlib
pass, then package the freshly built `liborganicmaps.so`.

### 7.2 Store / AntiFeatures text (downloads)

Optional map downloads talk to hosts that are not trivial to replace without
changing the app. Draft wording for `AntiFeatures` / full description (extend
the existing TetheredNet note):

> After install, Navi may download offline map leaves (`.mwm`) from **CoMaps
> mirror servers** discovered via the CoMaps metaserver, and Navi sidecar /
> catalog / attribution files from **https://navigate-me.duckdns.org** (`/mwm/…`,
> release series such as `261105`). Routing and map display are intended to run
> offline once those files are on the device. The classic Geofabrik-derived
> `.rkyv` pack host remains available when `NAVI_MWM_CLIENT_ENABLED=0`.

Also list OSM (ODbL), CoMaps/mirrors, Mapterhorn (hillshade/DEM), and Statens
vegvesen (DATEX) in the About / Licences screen once offline attribution files
from `/mwm/{version}/attribution.json` are shown in the UI.

---

## 8. Phase 3 — CoMaps native core + FFI shim

**Status on `feat/mwm-client`:** source pin + arm64 `liborganicmaps.so` build
scripted; wetland hook is a **callback** into Rust `SidecarReader` (no zstd
inside CoMaps); thin C ABI in `native/comaps_shim/`; Rust facade
`mwm::comaps_router` (feature `comaps-native`). Device route table filled when
Samsung verification runs.

Pin matches navi-server’s generator pin (`config/mwm` / catalog `generator`):

| Item | Value |
|------|-------|
| CoMaps tag | `v2026.11.07-16` |
| Commit | `fd11cc6f2256443d4d69112a3c22299d4e0664ce` |
| NDK | r27c (same as F-Droid / `build-android-native.sh`) |
| Device ABIs | **`arm64-v8a` required**; `x86_64` only if needed for **host** unit tests — **never** for emulators/AVDs |

### 8.1 Build from source (NDK / CMake)

1. `srclibs` / git submodule: CoMaps tree at the pin above (no floating `main`).
2. CMake (or upstream Android entry) produces shared libs for each packaged ABI.
3. **No prebuilt `.so`** in the repo or recipe (F-Droid §7.1).
4. Link into the app via Gradle `jniLibs` / CMake `EXTERNAL_NATIVE_BUILD` the same
   way MapLibre/native pieces are already wired.

### 8.2 Targets needed

| Area | CoMaps / omim pieces (indicative) | Why |
|------|-----------------------------------|-----|
| Routing | `routing`, cross-MWM graph, vehicle models | Pathfinding |
| Storage / MWM I/O | `indexer` / `storage` / MWM reader | Load leaves |
| Altitudes | altitude section readers | Base heights before EL02 overrides |
| Search | search index | Phase 5 |
| Drape / rendering | drape + optional DEM | Phase 6 map display |

### 8.3 Wetland hook as a patch

Carry navi-server’s validation hook
(`scripts/mwm/hook/navi_wetland_cost.{hpp,cpp}` + `apply-wetland-hook.sh`,
~212+ lines of cost logic) as a **quilt/patch** on the pinned CoMaps tree so
Soft×5 / Hard≈forbid + boardwalk exemption run inside the same edge-cost path
the server used for the 13867 → 16245 m foot detour. Phase 4 lands the behaviour;
Phase 3 lands the ability to call routing without Navi layers first.

### 8.4 JNI / Rust FFI shim (design)

```
Kotlin UI / download orchestration
        │
        ▼
navi-ffi (UniFFI)  ── owns pack/MWM paths, SidecarReader, fingerprint gate
        │
        ▼
thin JNI/C++ shim  ── open MWM set, set sidecar dir env / callbacks, route()
        │
        ▼
CoMaps native core (+ wetland patch)
```

- Rust keeps catalog/download/sidecar (Phases 1–2); does **not** reimplement the
  CoMaps graph.
- Kotlin never talks to CoMaps C++ except through UniFFI / a tiny JNI bridge.
- On fingerprint mismatch: call router **without** loading Navi layers (Phase 1
  warning string already defined).

### 8.5 First Samsung verification (physical device only)

Standing rule: **ADB to the connected Samsung only** — no emulator/AVD. Use
`adb devices -l`, then `-s <serial>` on every command.

1. Install a debug build with MWM flag on; download Østlandet leaf MWMs +
   sidecars (release `261105`).
2. Route **Hamar → Elverum** (six-route suite entry) — expect `NoError`.
3. Foot detour `61.0500,11.4700` → `61.0800,11.5200`: **13867 m** without Soft/Hard,
   **16245 m** with the hook (Phase 4 sign-off; Phase 3 may only prove “route
   without Navi layers” first).

### 8.6 Packaging (Gradle + assets)

- `scripts/build-comaps-android.sh` builds `liborganicmaps.so` (arm64-v8a) into
  `app/src/main/jniLibs/` (gitignored).
- `scripts/build-android-native.sh` enables `navi-ffi` feature `comaps-native`
  and links `-lorganicmaps` when that `.so` is present.
- `scripts/stage-comaps-data-assets.sh` copies classificator / types /
  visibility / `countries.txt` / `packed_polygons.bin` / categories / colors /
  patterns and **all** `drules_proto*.bin` from the pinned CoMaps `data/` into
  `app/src/main/assets/comaps-data/` at assemble time (gitignored; ~17.8 MiB
  raw). Init copies them into the writable root (Android `GetReader` scope `w`).
- Headless init must call `GetStyleReader().SetCurrentStyle(kDefaultMapStyle)`
  before routing — a bare `classificator::Load()` fills style slot 0 while
  Android `kDefaultMapStyle` is `MapStyleDefaultLight` (19).
- Phase 1 download: every release also fetches **World** + **WorldCoasts**
  (`leaf_ids_with_world_overlays` / `download_world_overlays`; `no_sidecar`,
  SHA-1 from `countries.txt`). Keep World overlays at the writable root (or APK
  assets scope `r`); do **not** place them under `261105/` as versioned country
  files — without cross-mwm sections they break Joints cross-MWM routing.
- Phase 3 device gate (Samsung `R52TB0JQEDE`): four Østlandet routes with/without
  wetland hook returned `OK`; foot detour matched 13867 m / 16245 m.

---

## 9. Phase 6 decision record -- CoMaps drape over MapLibre

**Decision (owner-approved, implemented in Phase 6b):** CoMaps drape is the only map renderer on the
MWM path. Phase 6a shipped a hybrid (drape basemap under a transparent MapLibre view); Phase 6b moved
the camera, gestures and every overlay to the drape and retired MapLibre / PMTiles / DEM there. Details,
device results and the remaining decisions: `docs/mwm-phase6b-drape.md` (and `docs/mwm-phase6-drape.md`
for the first pass).

### Why drape

- `liborganicmaps.so` already **PUBLIC-links drape / GLES** for the Android
  `organicmaps` target -- the binary cost is paid even for headless routing.
- Drape renders the same `.mwm` leaves Phase 1 already downloads -- fully offline, no
  second tile pipeline (and it carries contour lines and 3D buildings in the leaf data).
- MapLibre would need a **separate** vector-tile (or PMTiles) source that does not reuse
  CoMaps leaf geometry; that doubles download size and keeps two map stacks warm.

### What landed

| Piece | Where |
|-------|-------|
| `:comaps-sdk` Android library (Java, res, assets of the pinned checkout, **no CMake**) | `comaps-sdk/`, included by `settings.gradle.kts` only when the gitignored checkout exists |
| `ComapsDrape` facade (no SDK types, safe without the checkout) | `app/src/main/.../ComapsDrape.kt` |
| `ComapsDrapeBootstrap` (OrganicMaps Platform + Framework on top of the headless session) | `app/src/drape/java/` (compiled only with `:comaps-sdk`) |
| `ComapsDrapeMap` Compose host (camera, overlays, placeholder, missing maps) | `app/src/main/.../ComapsDrapeMap.kt` |
| Native camera / follow / overlay API | `patches/comaps/0004-navi-drape-camera-overlays-and-leaf-lock.patch`, JNI class `NaviDrape` |
| In-app download + install | `core/src/mwm/install.rs`, `navi-ffi/src/mwm_install_ffi.rs`, `MwmMapDownloader.kt` |
| Symbol atlas generation | `scripts/generate-comaps-symbols.py` (run by `comaps-sdk/build.gradle.kts`) |

### What still uses MapLibre

Only the flag OFF path (`CorridorMapView`, Basemap*, Mapterhorn*, PMTiles / DEM / region downloads).
Section 5 of `mwm-phase6b-drape.md` lists the files. Retirement of that code belongs with Phase 8.

### Open items

See section 10 of `mwm-phase6b-drape.md`.
