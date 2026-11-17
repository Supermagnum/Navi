# Phase 8 — client `.rkyv` retirement

**Status:** partial merged via #221; mainline G3 version is now `0.5.0-beta` / vc35
(renumbered from the mistaken `0.4.0-beta` / vc34 draft; CAT tags `v0.4.0-beta` /
`v0.4.1-beta` stay untouched).

## Done on this branch

1. Removed CI job `rust-lib-tests-mwm-off` (flag-OFF classic path no longer CI-gated).
2. Product flag forced ON:
   - Kotlin `MwmClientBootstrap.isEnabled()` always `true` (`NAVI_MWM_CLIENT_ENABLED=1`).
   - Gradle `BuildConfig.MWM_CLIENT_ENABLED` hard-coded `true` (no `-Pnavi.mwmClient=false`).
   - Rust `mwm_client_enabled()` defaults ON; test override may still force OFF for fixtures.
3. UI map path forced to CoMaps drape (`mwmDrapeMap = remember { true }` in `MainActivity`).
4. Phase 7 migration + `region-migration.json` / on-disk `.rkyv` detection **kept**.

## Explicitly NOT done yet (still in tree)

| Area | Status |
|------|--------|
| MapLibre GLES + PMTiles Kotlin / native | **Still packaged** (~12 MiB `libmaplibre.so` arm64). Dead at runtime when drape SDK is present, but not deleted (full deletion stalled on `MainActivity` blast radius). |
| `pack_server` classic `.rkyv` download / catalog client | Still in `core` + UniFFI; MWM download reuses shared HTTP helpers. |
| Graph / tile / wetland `.rkyv` loaders + `planCarRoute` | Still in `libnavi`; production planner uses CoMaps when MWM path is active. |
| FTS5 `place_index` / `core/src/search` | **Kept on purpose** — not fully replaced. |

## FTS5 evidence (do not remove)

- `ComapsSearchBridge` merges CoMaps hits with `place_index` FTS via `comapsSearchMerge`
  (`navi:place_index` source label). Empty CoMaps results leave the FTS list unchanged.
- `core/src/mwm/comaps_search.rs` and `navi-ffi` docs: flag/session off keeps FTS.
- Production From/Via/To still call place-index ranking then merge (`MainActivity` search path).

## Expected APK impact when deletion completes

- Drop MapLibre native (~4.5–12 MiB compressed / ~12 MiB arm64 `.so`).
- Further `libnavi.so` shrink after loader deletion (measure with before/after APK).

## Gates

| Gate | Notes |
|------|-------|
| G2 | Met (PR #222). |
| G3 | `0.5.0-beta` / vc35 on `dev`/`main` (renumbered; CAT `v0.4.x-beta` tags untouched). Tag/publish still need go-ahead. |
| G4 | This PR — owner chooses this-release vs next-release. |
