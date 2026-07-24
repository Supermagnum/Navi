# Closing pass report (2026-07-21)

Combined status for menu verification, plugin host, docs, README disclaimer, craft-brewery POI, and multi-zoom screenshots.

## 1. Menu verification

Verified against `MainActivity.kt` Compose UI (reachable controls), not planning docs alone.

| Item | Status | Notes |
|---|---|---|
| Profile menu (Car, Bicycle, Hiking, Motorcycle) | **present and working** | Chips filtered by UniFFI `travelProfileMenuFocus`; Truck + electric variants in enum (caption + core/FFI), not primary chips. |
| Eco-mode toggle | **present and working** | Toggleable for Car/Motorcycle; locked on + disabled switch for Bicycle/Hiking. |
| To / Via search | **present and working** | To/Via + Place/Address chips call `searchPlaces`. Hut names (Jammerdalsbu-style) resolve once the place index is built from a region that contains them; not re-probed on-device in this pass. |
| Route list (visible + deletable) | **present and working** | Scrollable Saved routes panel with Refresh/Delete + save current To/Via. |
| Start from GPS | **present but broken** | Button present; `lastGpsFix()` is a **stub** demo coordinate, not Android fused location. |
| Continue from last stop | **present and working** | Uses saved-route `last_break_*` when set; otherwise falls back to Via. |
| Avoid motorways/trunk/primary | **present but broken** | Switch + report text in UI; priority-path share uses **demo constants**, not live route validation metrics. |
| Vehicle dimension/weight inputs | **present and working** | Axle / height / width fields; persist via `saveVehicleLimits` / load on start (below fold — scroll the overlay). |

UniFFI regenerate (this pass): `./scripts/build-android-native.sh` exit 0; Kotlin binding includes `travelProfileMenuFocus` (and related eco helpers).

## 2. Plugin handler

| Check | Result |
|---|---|
| `plugin-host` load → call → return (`log-hello`) | **PASS** — `load_call_log_hello_end_to_end` |
| Manifest capability check before load | **PASS** — `manifest_capability_checked_before_load` |
| Busy-loop fuel/timeout kill without blocking host | **PASS** — `busy_loop_is_killed_by_fuel_or_timeout_without_blocking_host` |
| Docs | [`docs/plugins.md`](docs/plugins.md); crates `plugin-host/`, `plugin-sdk/`, refs under `plugins/` |

Command: `cargo test -p navi-plugin-host --test isolation -- --nocapture` → `3 passed; 0 failed`.

## 3. Documentation / README table

| Document | Exists |
|---|---|
| `docs/architecture.md` | yes |
| `docs/plugins.md` | yes (plugin system) |
| `docs/icons.md` | yes (icon system) |
| `docs/API.md` | yes |
| `docs/PROTOCOLS.md` | yes |
| `docs/APRS.md` | yes |
| `docs/CAT.md` | yes |
| `docs/test-results.md` | yes |
| `docs/android-test-results.md` | yes |

README Documents table lists the above; no broken links found in this audit.

## 4. AI-use disclaimer

**Live** at the top of [`README.md`](README.md) under `# AI assistance`, above the product overview. Factual disclosure of Claude assistance + author review/direction.

## 5. Craft brewery / alcohol POI

| Check | Result |
|---|---|
| Category | `PoiCategory::CraftBrewery` |
| Tags (OR) | `microbrewery=yes` OR `shop=alcohol` OR `craft=brewery` |
| Default radius | General POI radius (~15 km) |
| Icon mapping | `shop-alcohol` (see `docs/icons.md`) |
| Unit tests | **PASS** — `craft_brewery_matches_any_of_three_tag_styles`, `craft_brewery_does_not_require_all_three_tags`, `craft_brewery_surfaces_in_nearest_query` |

## 6. Multi-zoom screenshots

Center **58.991547, 6.138377**. Captured via `ZoomPoiScreenshotTest` + device `screencap`, pulled to host fixtures.

| Zoom | Path | Bytes | Dimensions |
|---|---|---|---|
| 6.5 | `core/target/integration-fixtures/zoom_z6_5.png` | 364529 | 1280×720 PNG |
| 11.0 | `core/target/integration-fixtures/zoom_z11.png` | 245493 | 1280×720 PNG |
| 16.0 | `core/target/integration-fixtures/zoom_z16.png` | 223775 | 1280×720 PNG |

Chrome hidden during capture (`NaviMapTestHooks.hideUiChrome`) so OpenFreeMap liberty basemap / standard POI icons are visible. Center **58.991547, 6.138377**.
