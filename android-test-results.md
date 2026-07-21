# Android + corridor test results (full on-device wiring)

Date: 2026-07-21 (follow-up pass)

## Environment

| Item | Value |
|---|---|
| Emulator | `emulator-5554` / xtrons AVD API 35 |
| ABI | `x86_64` |
| Fixture HTTP | `http://10.0.2.2:8765/` (host `python3 -m http.server`) |
| Corridor PBF | `espa-atnbrufossen-corridor.osm.pbf` (~58 MiB cut from Ostlandet) |
| DEM seed | `elevation-corridor.tar` via in-app provisioner |
| Screenshot | `core/target/integration-fixtures/route_map.png` |

## Data source policy (item 0)

| Test | `TEST_KIND` | `DATA_SOURCE` | Validates |
|---|---|---|---|
| `smokeTest_ffiLinkageOnly_labeledSmoke` | **SMOKE** | `none` | UniFFI + `WorkerPoolPlan` only |
| `realPipeline_provisionsViaDownload_thenRoutes` | **REAL_PIPELINE** | `real_pbf` | parse → graph → eco reweight → cache → POI → route |
| `iconRasterization_producesNonEmptyBitmaps` | **ICON_RASTER** | `real_svg` | usvg/resvg bitmaps |
| `mapIsVisible_withRouteOverlay_andScreenshot` | (uses REAL_PIPELINE + UI) | `real_pbf` | MapLibre basemap + route + marker + screencap |

Silence on `DATA_SOURCE` must be treated as smoke/stub. This suite labels every report explicitly.

## Connected suite result

```
tests=4 failures=0 errors=0 skipped=0 time≈35.8s
```

| Test | Result | Notes |
|---|---|---|
| `smokeTest_ffiLinkageOnly_labeledSmoke` | **PASS** | Labeled SMOKE; no routing claim |
| `realPipeline_provisionsViaDownload_thenRoutes` | **PASS** | HTTP provision (no manual `adb push`); `DATA_SOURCE=real_pbf`; cache hit on 2nd load |
| `iconRasterization_producesNonEmptyBitmaps` | **PASS** | fuel, nav_straight, status_routing, eco-mode + country_NO.svgz |
| `mapIsVisible_withRouteOverlay_andScreenshot` | **PASS** | Screenshot evidence below |

## Item 1 — Automated provisioning (no manual adb push)

- In-app / FFI: `provision_region_data(data_dir, pbf_url, pbf_filename, elevation_tar_url?)`
- Emulator reaches host fixtures at `http://10.0.2.2:8765/...`
- Scripts: `scripts/cut-corridor-extract.py`, `scripts/prepare-android-fixtures.sh`, `scripts/serve-android-fixtures.sh`
- Instrumented tests call the same provisioner the UI uses

## Item 2 — Reweighted graph cache

- `core/src/routing/graph/cache.rs` (`NAVIGPH1` + bincode)
- Pipeline runs cold wipe → build/save → warm `load_or_build_reweighted` and asserts `cache_hit=true` and warm << cold (when cold > 2 s)

## Item 3 — Icon rasterization

- `usvg`/`resvg` + `flate2` for `.svgz` in `core/src/icons`
- FFI: `rasterize_icon_png` / `rasterize_icon_check`
- On-device: non-empty PNG/RGBA for POI, nav, status, eco; flag key resolves `.svgz`

## Item 4 — Visible map (screenshot evidence)

File: [`core/target/integration-fixtures/route_map.png`](core/target/integration-fixtures/route_map.png)

Captured on-device via `screencap -p /data/local/tmp/navi_route_map.png` during the instrumented map test (pulled with `adb pull`).

Confirmed in the image:

- MapLibre basemap (Norway demotiles, MapLibre logo)
- Red route line overlay (Espa → Atnbrufossen corridor geometry)
- Rasterized POI marker + label along the route
- Launcher uses `appicon.svg` (dock icon)

## Item 5 — Stub / gap audit (reachable + related)

| Location | Status |
|---|---|
| `core/src/bus/mod.rs` | Minimal stubs (inter-thread snapshots) — not corridor-critical |
| `core/src/sensors/mod.rs` | Placeholders; GPS/IMU out of scope |
| `core/src/lib.rs` / ECU | Live ECU/OBD out of scope (extension point only) |
| `core/src/icons` `unknown.svg` | Intentional fallback placeholder when a POI `icon_key` has no matching asset in the lean on-device pack |
| No `todo!()` / `unimplemented!()` | in corridor / FFI / map paths for this pass |

Known gaps after this pass (acknowledged, not claimed PASS):

- On-device icon asset pack is a **lean subset** of `core/src/icons` (full Navit set remains on disk in core; not all keys are bundled into the APK).
- Corridor extract is bbox-cut from Ostlandet (~58 MiB), not a full country download UX with progress UI polish.
- Ferrostar SDK is not used; MapLibre is wired directly (matches “Ferrostar hands rendering to MapLibre” split).
- Sensor/ECU/bus modules remain stubs as above.

## Commands (no `tee`)

```bash
python3 scripts/cut-corridor-extract.py
# serve fixtures:
python3 -m http.server 8765 --bind 0.0.0.0 --directory core/target/integration-fixtures
./scripts/build-android-native.sh x86_64-linux-android release
./gradlew :app:installDebug :app:installDebugAndroidTest
./gradlew :app:connectedDebugAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=no.navi.app.CorridorInstrumentedTest
adb pull /data/local/tmp/navi_route_map.png core/target/integration-fixtures/route_map.png
```

## Item 6 — Multi-zoom POI screenshots (closing pass)

Center: `58.991547, 6.138377` (basemap / standard map POIs, not route overlay).

Instrumented test: `no.navi.app.ZoomPoiScreenshotTest` via `NaviMapTestHooks.pendingCamera`.

| Zoom | Host path | Size |
|---|---|---|
| 6.5 (regional) | [`core/target/integration-fixtures/zoom_z6_5.png`](core/target/integration-fixtures/zoom_z6_5.png) | 117355 bytes |
| 11.0 (town) | [`core/target/integration-fixtures/zoom_z11.png`](core/target/integration-fixtures/zoom_z11.png) | 108077 bytes |
| 16.0 (street) | [`core/target/integration-fixtures/zoom_z16.png`](core/target/integration-fixtures/zoom_z16.png) | 96775 bytes |

```bash
./gradlew :app:connectedDebugAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=no.navi.app.ZoomPoiScreenshotTest
adb pull /data/local/tmp/navi_zoom_6_5.png core/target/integration-fixtures/zoom_z6_5.png
adb pull /data/local/tmp/navi_zoom_11.png core/target/integration-fixtures/zoom_z11.png
adb pull /data/local/tmp/navi_zoom_16.png core/target/integration-fixtures/zoom_z16.png
```

Connected result: `tests=1 failures=0` (2026-07-21 closing pass).
