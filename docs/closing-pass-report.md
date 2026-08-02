# Closing pass report (point-in-time)

**Original pass date:** 2026-07-21  
**Last verified against:** commit `fe88039` (2026-07-26) — documentation and GPS
sections re-checked against the tree and live-GPS evidence already on record.
This file is a **snapshot**, not a living design doc. Prefer
[`docs/codebase-map.md`](codebase-map.md), [`docs/API.md`](API.md), and feature
docs for current behaviour; update the “Last verified” line when you re-audit.

Combined status for menu verification, plugin host, docs, README disclaimer,
craft-brewery POI, and multi-zoom screenshots (as of the original pass, with
stale rows corrected below).

## 1. Menu verification

Verified against `MainActivity.kt` Compose UI (reachable controls), not planning docs alone.

| Item | Status | Notes |
|---|---|---|
| Profile menu (Car, Bicycle, Hiking, Motorcycle) | **present and working** | Chips filtered by UniFFI `travelProfileMenuFocus`; Truck + electric variants in enum (caption + core/FFI), not primary chips. |
| Eco-mode toggle | **present and working** | Toggleable for Car/Motorcycle; locked on + disabled switch for Bicycle/Hiking. |
| To / Via search | **present and working** | To/Via + Place/Address chips call `searchPlaces`. Hut names (Jammerdalsbu-style) resolve once the place index is built from a region that contains them; not re-probed on-device in this pass. |
| Route list (visible + deletable) | **present and working** | Scrollable Saved routes panel with Refresh/Delete + save current To/Via. |
| Start from GPS | **present and working** | Android `LocationManager` feeds `mapState.gpsLat/Lon` and mirrors each fix via UniFFI `updateGpsFix` so `lastGpsFix()` is never a demo stub (unavailable until the first push). Confirmed on live emulator GPS: `LiveMultiDayDayCardsInstrumentedTest` refuses a hardcoded start and parses `adb shell dumpsys location` (gps/fused); truck corridor Minnesund belt → Bodø is on record in [`pictures.md`](pictures.md) / README rest table. |
| Continue from last stop | **present and working** | Uses saved-route `last_break_*` when set; otherwise falls back to Via. |
| Avoid motorways | **present and working** | Switch feeds `planCarRoute`. Report shows **plan-derived** non-motorway road share (`CorridorRouteResult.priorityPathSharePct` = 100% minus motorway length). Toggle no longer invents 72.5 / 41.0. Trunk/primary are not excluded. |
| Vehicle dimension/weight inputs | **present and working** | Axle / height / width fields; persist via `saveVehicleLimits` / load on start (below fold — scroll the overlay). |

UniFFI regenerate (original pass): `./scripts/build-android-native.sh` exit 0; Kotlin binding includes `travelProfileMenuFocus` (and related eco helpers). Approach thresholds remain UniFFI `approachAppearM` / `approachUrgencyM` / `approachHideM` (metres).

## 2. Plugin handler

| Check | Result |
|---|---|
| `plugin-host` load → call → return (`log-hello`) | **PASS** — `load_call_log_hello_end_to_end` |
| Manifest capability check before load | **PASS** — `manifest_capability_checked_before_load` |
| Busy-loop fuel/timeout kill without blocking host | **PASS** — `busy_loop_is_killed_by_fuel_or_timeout_without_blocking_host` |
| Docs | [`docs/plugins.md`](plugins.md); crates `plugin-host/`, `plugin-sdk/`, refs under `plugins/` |
| Product content plugins | Still **not shipped** (intentional) — camping, resupply, cluster, i18n, animated icons, etc. remain specs only |

Command: `cargo test -p navi-plugin-host --test isolation -- --nocapture` → `3 passed; 0 failed` (as of original pass; re-run if touching the host).

## 3. Documentation / README table

| Document | Exists |
|---|---|
| `docs/architecture.md` | yes |
| `docs/plugins.md` | yes (plugin system + roadmap specs) |
| `docs/icons.md` | yes (icon system; static Inkscape steps) |
| `docs/API.md` | yes |
| `docs/PROTOCOLS.md` | yes |
| `docs/APRS.md` | yes |
| `docs/CAT.md` | yes |
| `docs/test-results.md` | yes |
| `docs/android-test-results.md` | yes |
| `docs/current-street.md` / `docs/unicode-road-names.md` | yes (added after original pass) |
| `docs/plugins/*-spec.md` | yes (specs only; not product plugins) |

README Documents table lists the core set; feature docs added later are linked from README / Norwegian indexes. No broken links found in the original audit.

## 4. AI-use disclaimer

**Live** at the top of [`README.md`](../README.md) under `# AI assistance`, above the product overview. Factual disclosure of Claude assistance + author review/direction.

## 5. Craft brewery / alcohol POI

| Check | Result |
|---|---|
| Category | `PoiCategory::CraftBrewery` |
| Tags (OR) | `microbrewery=yes` OR `shop=alcohol` OR `craft=brewery` |
| Default radius | General POI radius (~15 km) |
| Icon mapping | `shop-alcohol` (see `docs/icons.md`) |
| Unit tests | **PASS** — `craft_brewery_matches_any_of_three_tag_styles`, `craft_brewery_does_not_require_all_three_tags`, `craft_brewery_surfaces_in_nearest_query` |

## 6. Multi-zoom screenshots

Center **58.991547, 6.138377**. Captured via `ZoomPoiScreenshotTest` + device `screencap`, pulled to host fixtures (local under `core/target/integration-fixtures/` — not the GitHub `docs/images/` allowlist).

| Zoom | Path | Bytes | Dimensions |
|---|---|---|---|
| 6.5 | `core/target/integration-fixtures/zoom_z6_5.png` | 364529 | 1280×720 PNG |
| 11.0 | `core/target/integration-fixtures/zoom_z11.png` | 245493 | 1280×720 PNG |
| 16.0 | `core/target/integration-fixtures/zoom_z16.png` | 223775 | 1280×720 PNG |

Chrome hidden during capture (`NaviMapTestHooks.hideUiChrome`) so OpenFreeMap liberty basemap / standard POI icons are visible.

## Stale items found and corrected (2026-07-26 review)

| Item | Was | Now |
|---|---|---|
| Start from GPS | “broken” / stub `lastGpsFix` | Working; LocationManager + `updateGpsFix` mirror; live Minnesund→Bodø evidence |
| Avoid motorways | “broken” / then “incomplete” (demo %) | Plan switch + **real** non-motorway share from path edges |
| Header | Undated “current” implication | Explicit point-in-time + last-verified commit |
| Plugin section | Host-only | Noted product plugins still deferred |
| Doc inventory | 2026-07-21 set only | Noted later docs (`current-street`, plugin specs) |
