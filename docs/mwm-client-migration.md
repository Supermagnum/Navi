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

### Phase 2 — Sidecar reader (EL02)

- Sectioned NVSC load; wetland / boardwalk / elevation_overrides / attribution.
- **Acceptance:** Hedmark inspect counts match server; unknown sections skipped;
  unsupported `format_version` rejected.

### Phase 3 — CoMaps core via NDK + JNI/Rust FFI shim

- Ship / link CoMaps routing (+ map) core; Rust/Kotlin orchestrate downloads and
  call FFI.
- **Acceptance:** load Østlandet leaf set; route without Navi layers.

### Phase 4 — Routing wetland hook

- Soft×5 / Hard≈forbid; boardwalk exempt; elevation overrides on routable
  altitudes.
- **Acceptance:** foot detour `61.0500,11.4700` → `61.0800,11.5200`: **13867 m**
  without Soft/Hard, **16245 m** with; six-route suite NoError on release
  `261105` (see `client-routing-hooks.md`).

### Phase 5 — Search

- CoMaps search index or interim hybrid; place UX parity for pilot region.
- **Acceptance:** offline find for Østlandet places without requiring full
  Geofabrik PBF for MWM-flag users.

### Phase 6 — Rendering / drape

- Decide MapLibre vs CoMaps drape; DEM source independent of sidecar.
- **Acceptance:** map + hillshade usable offline for downloaded leaves.

### Phase 7 — Migrating users' existing `.rkyv` downloads

- One-tap: `region_to_leaves[bake_id]` / `region-migration.json` → download leaf
  union; keep `.rkyv` until user confirms.
- **Acceptance:** Østlandet → documented leaf set; Sørlandet shares
  `Norway_Southern`; shared leaves stored once.

### Phase 8 — Retiring `.rkyv`

- Only after MWM client tested and correct. Server stops serving
  `published/packs/` + `current.json` region manifests; then delete tree.
  **Freeze ≠ retirement.** Client may remove `.rkyv` UI only after server cutover.

---

## 3. Open product decisions (options + recommendation)

Do **not** encode these in code yet.

| Topic | Options | Recommendation |
|-------|---------|----------------|
| Fastest vs shortest wording | (A) Keep “fastest” label with Soft×5 changing foot “best”; (B) Rename to “recommended” / “best”; (C) Explicit fastest vs shortest toggle | **(B)** for foot/hike when wetland hook is on; keep a shortest toggle later if needed |
| Wetland penalties for bike | (A) Foot/hike only (server default); (B) Soft only on bike; (C) Same Soft/Hard on bike | **(A)** until field feedback |
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
  releases/{version}/
    catalog.json              # snapshot of catalog used for this install
    attribution.json
    ATTRIBUTION.md
    countries.txt             # verified copy (optional cache)
    leaves/{Leaf}.mwm         # shared across region picks; one file per leaf
    sidecars/{Leaf}.navi-sidecar.v1.zst
    leaf-state.json           # per-leaf verify + fingerprint status
  staging/{version}/          # incomplete download; never active
  migration-cache/
    region-migration.json     # last fetched mapping for one-tap migrate UI
```

Rules: never mix `version` values under `active`; shared leaves are paths under
`releases/{version}/leaves/` (downloaded once); on failed switch, delete
`staging/{version}` and keep previous `active.json`.

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

### 7.1 Recipe expectations (Phase 3+)

| Item | Requirement |
|------|-------------|
| CoMaps / Organic Maps tree | `srclibs` (or git submodule) pinned to an **exact tag/commit**; no floating `main` |
| Native build | **NDK + CMake/Ninja** (or the upstream Android.mk/CMake entry the pin documents) inside `prebuild`/`build`, producing `lib*` for each ABI Navi ships |
| Prebuilt `.so` | **Forbidden** in the APK and in the recipe — no downloading binary CoMaps libraries |
| ABI set | Same `naviAbis` filter as today (`arm64-v8a` / `x86_64` as packaged) |
| Rust flag | Leave `NAVI_MWM_CLIENT_ENABLED` **unset** (default ON) or set `1`; do not bake a silent OFF |
| Gradle | No product flavour required; `BuildConfig.MWM_CLIENT_ENABLED` defaults true. Optional debug-only `-Pnavi.mwmClient=false` must not be used in the F-Droid `Builds:` entry |

Until Phase 3 merges, the F-Droid metadata does **not** need a CoMaps `srclib`;
this section is the contract for that future recipe change.

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
