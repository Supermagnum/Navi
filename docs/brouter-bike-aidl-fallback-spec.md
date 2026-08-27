# Spec: BRouter AIDL fallback for bicycle pack-miss

**Status:** product/integration spec — not implemented.  
**Date:** 2026-08-27  
**Path:** `docs/brouter-bike-aidl-fallback-spec.md`  
**Evidence:** [`brouter-engine-substitution-investigation.md`](brouter-engine-substitution-investigation.md)
(Gaps 1–2 on SM-P613). Car is **out of scope** (seasonal-conditional gate fail).

---

## Goal

On Android, when a **bicycle** plan misses Navi indexed packs, optionally obtain
a route from the installed **BRouter** app via AIDL instead of (or before)
a full local PBF rebuild — only when BRouter + corridor tiles are available.

Do **not** change pack format, pack-hit semantics, or hiking/car miss behaviour.

---

## Non-goals

- Car / truck fallback via BRouter (blocked until departure-date / conditional
  story exists).
- Converting `.rd5` into Navi packs (already rejected).
- Embedding a desktop JVM `RouteServer` in Navi.
- Claiming BRouter answers are `pack_hit` or identical to Navi
  `BikeCapability` filtering.
- Hiking.

---

## Trigger

```text
plan(profile = Bicycle):
  1. existing indexed-pack path → pack_hit (unchanged)
  2. on pack_miss (Android only):
       if BRouterService bindable
          AND required segments4 tiles present for the OD bbox
          AND feature flag / setting allows BRouter bike fallback:
            route = AIDL getTrackFromParams(trekking, lonlats, trackFormat=gpx)
            if ok → return with engine=brouter; stop
       fall through to existing PBF rebuild miss path
```

Desktop / non-Android builds: unchanged miss path only.

---

## AIDL contract

- Package / service: `btools.routingapp` /
  `btools.routingapp.BRouterService`
- Interface: `IBRouterService.getTrackFromParams(Bundle)`
- Bind: `Intent` action `btools.routingapp.IBRouterService` + explicit
  component (OsmAnd-style). Declare `<queries>` for `btools.routingapp` on
  API 30+.
- Params (minimum):
  - `profile` = `trekking` (default bicycle map; later optional map from
    Road/Trekking/Mountain → distinct `.brf` if product wants)
  - `lonlats` = `lon,lat|lon,lat|...`
  - `trackFormat` = `gpx` (required for long tracks; JSON can exceed Binder
    ~1 MiB reply limit — observed on Espa→Atnbrufossen trekking)
  - `maxRunningTime` = string seconds (e.g. `120`)
- Parse GPX into Navi’s existing route/track model; preserve distance /
  ascent if present in metadata.
- Call off the UI thread; timeouts must fail closed to PBF rebuild.

---

## Result signalling

- Set an explicit engine marker (`engine=brouter` / equivalent). **Never**
  set `pack_hit=true`.
- UI: short honesty line that the route came from BRouter offline routing
  and may differ from Navi bicycle suitability (surfaces, tracktype,
  eco/time tradeoffs).
- Analytics (if any): separate from pack_hit / pack_miss rebuild counters.

---

## Provisioning

- **App:** depend on user-installed BRouter (`btools.routingapp`, F-Droid /
  Play). Navi does not vendor the APK in v1 of this fallback.
- **Tiles:** official `segments4` `.rd5` covering the OD. Navi may:
  - detect missing tiles and deep-link / instruct Download Manager, or
  - optional later: push tiles into BRouter’s media baseDir
    (`Android/media/btools.routingapp/brouter/segments4/`) after first-run
    config exists.
- First-run of BRouter (baseDir / `config15.dat`) is **outside** Navi; if
  unbound or unconfigured, skip fallback silently to PBF rebuild.
- Corridor size class: Espa–Atnbrufossen needed ~55 MiB (`E5_N60` +
  `E10_N60`).

---

## Performance expectations (SM-P613 evidence)

| Path | Observed (Espa→Atnbrufossen trekking) |
|---|---|
| Process-cold bind+query | ~1.60–1.67 s (3 repeats) |
| Warm query | ~0.41–0.82 s |
| vs bicycle-class pack-miss rebuild | ≪ ~54 s car-class miss order |

Missing-tile download remains network-bound; only count as “fallback ready”
when tiles are already on device (or after an explicit download step).

---

## Quality bar (from Gap 2)

Acceptable for ship of this fallback:

- Plausible trekking geometry on mixed gravel/track/path ODs (Gap 2 pair 3).
- General corridor ODs not wildly wrong (pairs 1, 5).
- Distances may diverge from Navi (observed up to ~48% longer on mixed
  mountain OD) — document in UI; do not pretend equivalence.

Not required for this bike-only spec:

- Matching Navi `BikeCapability` Road/Trekking/Mountain thresholds exactly.
- `motor_vehicle:conditional` (Navi bicycle already ignores it).

---

## Failure / fallback

Use existing PBF rebuild (or user-visible miss error) when any of:

- BRouter not installed / not configured
- Bind timeout or `RemoteException` / Binder failure
- Profile missing / error string returned instead of GPX
- Empty or unparseable track
- Feature flag off

Do not retry unbound forever; one bind attempt per plan is enough.

---

## Testing

- Instrumented or throwaway harness on SM-P613: cold + warm AIDL, GPX parse.
- At least one mixed-surface OD and one general OD in CI-adjacent manual
  checklist (device required for AIDL).
- Regression: car and hiking miss paths unchanged (no BRouter call).

---

## Open product decisions (block implementation kickoff)

1. Setting default: off vs on when BRouter detected.
2. Whether Navi ever auto-provisions `.rd5` tiles or only documents manual
   BRouter Download Manager.
3. Mapping `BikeCapability` → alternate BRouter profiles beyond `trekking`.

Implementation may start once (1) is decided; (2)/(3) can ship as
follow-ups behind the same `engine=brouter` marker.
