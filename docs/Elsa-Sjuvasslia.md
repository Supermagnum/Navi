# Elsa's caravan & galleri (Bugøynes) → Sjuvasslia Camping

Date: **2026-11-19** (JUnit re-verify PASS + DATEX keep). Automotive AVD
**`Navi_8c_4G`** (`emulator-5554`, x86_64). App `no.navi.app` APKs on `dev`
under `compiled/` (see `compiled/SHA256SUMS`):

| File | sha256 |
|---|---|
| `compiled/navi-debug.apk` | `bd8106c9ba9043f6cba4269d502eee4976552c4973e7788ae92a47d0a24e5500` |
| `compiled/navi-release.apk` | `49dec1fcf66c312e76bfb632abf0e8428350135266d25981e48abfd33a21ce24` |

Not added to CI.

Instrumentation driver
`LongTripMobileHomeElsaSjuvassliaUiCampaignTest#elsa_sjuvasslia_ui_plan_campaign`
drives the real Compose UI. ADB is used only for GPS mock, soft DATEX inject
(Penalize), and read-only monitoring. Host: `/tmp/elsa_sjuvass_host.py`
(`logcat -T 1`; inject uses `coordinatesForDisplay` + wide validity).
Artifacts: `/tmp/elsa-sjuvass-campaign/`.

Download / plan path: **MWM-first Long trip** (release **261105**). CoMaps
leaves under `files/mwm/releases/261105`; classic Geofabrik PBF packs are
fallback only (`LongTripMwmSupport`). World overlays stay at the MWM root
(`0011` writable scope); headless `RoutesBuilder` skips World / WorldCoasts
(`0012`) so Car long-trip does not fail with mixed World+country graphs.

---

## Verdict (JUnit re-verify PASS)

`OK (1 test)` on `emulator-5554` with the debug APK above.
`PASS_UI dist=2038.525 etaMin=1588.3 man=81 datex=true breaks=17 wild=64 attr=4`.

| Metric | Product expectation | Measured |
|---|---|---|
| Distance / time / maneuvers | 1600–2300 km, 15–31 h, 1–170 | **2038.5 km**, **26.5 h**, **81** |
| Wild camping | > 0 on-foot | **64** |
| Attractions “several” + viewpoint | ≥ 3, include **viewpoint** | **4** (`tourism-viewpoint=1`, `tourism-attraction=2`, `tourism-artwork=1`) |
| Soft break pins | ~1.5 h spacing | `break_interval_h=1.50`, **17** rest pins |
| Break HUD | **Break in 240 min** at plan (4 h / 15 min fields) | Campaign `soft_break_interval_h=4`, `soft_rest_min=15`; HUD assert 200–280 min |
| DATEX | Inject 3–6 soft Penalize; plan `datex_impacts` in **3–6** | **`datex_impacts=5`** (`penalize=5`), `warning=navi_synth_keep`, `active=5` |
| Ferries | 0 | 0 |

---

## Test setup

| Item | Value |
|---|---|
| Emulator | `Navi_8c_4G` / `emulator-5554` (x86_64) |
| Origin | Elsa's caravan & galleri, Bugøynes **69.9741435, 29.6337571**, elev **4 m** |
| Destination | Sjuvasslia Camping **59.803175, 9.397871** |
| Profile | **Car** (VW Transporter T6 camper limits) |
| Eco | on |
| Soft daily budget | **6.0 h** |
| Break HUD interval | **4.0 h** (Break in 240 min) / rest **15 min** |
| Soft rest-stop pins | ~**1.5 h** corridor spacing when HUD ≥ 3 h |
| Wild camping | on |
| Long trip | on (MWM-first) |
| DATEX overlay | on; soft Penalize inject + `navi-synth` keep |
| Nearby attractions | on |
| Plan path | Compose UI `btn_plan_route` |

---

## Regions (MWM Long trip corridor)

Eight long-trip region ids (Nord-Norge → Finland → Swedish län → Ostlandet),
all Ready before plan. Placement:
`/data/user/0/no.navi.app/files/mwm/releases/261105`.

---

## Fixes that unblock the bars

| Issue | Fix |
|---|---|
| Plan `No route` with World in RoutesBuilder | Quilt `0012` skip World / WorldCoasts; `0011` World scope `wr` |
| Low-zoom / World overlays unreadable at root | `0011-navi-world-writable-scope.patch` |
| Long-trip progress invisible | MWM installs via `MwmMapDownloader.installBlocking` + readiness UI |
| Wild camping = 0 on MWM-only | Camping `waypoint_fallback` + denser polyline samples |
| Attractions = 0 / weak “several” / no viewpoint | Isotropic tally; typed CoMaps tourism seed; ≥8 samples; assert viewpoint |
| Break HUD stuck at 0 mid-trip | `minutesUntilNextBreak` modulo interval; plan sets hook to ~240 with 4 h |
| DATEX inject but `datex_impacts=0` | Synth XML/meta keep + refuse live overwrite; host `coordinatesForDisplay` + validity spanning wall-clock `Utc::now`; 25 km synth margin |

Cross-refs: [`how-to-use.md`](how-to-use.md) (Long trip / Breaks),
[`plugins/datex-plugin.md`](plugins/datex-plugin.md) (navi-synth),
[`plugins/poi-lookahead-cone-spec.md`](plugins/poi-lookahead-cone-spec.md)
(MWM seed), [`mwm-client-migration.md`](mwm-client-migration.md) (patches
0011–0012), [`notes/navi-server-world-storage-rule.md`](notes/navi-server-world-storage-rule.md).

---

## Notes

- Arm64 release APK in `compiled/` is for device sideload; Elsa campaign itself
  is emulator x86_64.
- Campaign host must not replay stale logcat (`adb logcat -T 1`). Synth
  situations need `coordinatesForDisplay` (not `pointByCoordinates`) or parse
  yields zero geometry.
