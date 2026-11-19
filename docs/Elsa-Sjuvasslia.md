# Elsa's caravan & galleri (Bugøynes) → Sjuvasslia Camping

Date: **2026-11-19** (fixes + docs refresh). Automotive AVD **`Navi_8c_4G`**
(`emulator-5554`, x86_64). App `no.navi.app` APKs on `dev` under `compiled/`
(see `compiled/SHA256SUMS`):

| File | sha256 |
|---|---|
| `compiled/navi-debug.apk` | `650783fe6c51c23352f92c1900cf4fc7e679446198e661611932cc6792c7d005` |
| `compiled/navi-release.apk` | `e44a58c7a2486ffc82a513aae68793f827f04a64d2e96dc8e278a1573c0e8443` |

Not added to CI.

Instrumentation driver
`LongTripMobileHomeElsaSjuvassliaUiCampaignTest#elsa_sjuvasslia_ui_plan_campaign`
drives the real Compose UI. ADB is used only for GPS mock, soft DATEX inject
(Penalize), and read-only monitoring. Host: `/tmp/elsa_sjuvass_host.py`.
Artifacts: `/tmp/elsa-sjuvass-campaign/`.

Download / plan path: **MWM-first Long trip** (release **261105**). CoMaps
leaves under `files/mwm/releases/261105`; classic Geofabrik PBF packs are
fallback only (`LongTripMwmSupport`). World overlays stay at the MWM root
(`0011` writable scope); headless `RoutesBuilder` skips World / WorldCoasts
(`0012`) so Car long-trip does not fail with mixed World+country graphs.

---

## Verdict (product bars after fixes)

| Metric | Product expectation | Fix / note |
|---|---|---|
| Distance / time / maneuvers | 1600–2300 km, 15–31 h, 1–170 | Unchanged corridor bands |
| Wild camping | > 0 on-foot | `waypoint_fallback` + denser samples when PBF absent |
| Attractions “several” | ≥ 3, include **viewpoint** | CoMaps tourism seed maps feature/name → `tourism=viewpoint|museum|artwork|attraction`; up to 8 corridor bias points; tally by `icon_key` |
| Soft break pins | ~1.5 h spacing | Soft pin interval prefers 1.5 h when HUD interval ≥ 3 h |
| Break HUD | **Break in 240 min** at plan (4 h field) | `minutesUntilNextBreak` wraps; campaign sets `breakIntervalHours=4.0`, rest **15** min |
| DATEX | Inject 3–6 soft Penalize; plan `datex_impacts` in **3–6** | `navi-synth` meta/XML keep; wider synth corridor; host writes `datex-cache.json`; snap by seed/corridor (not minutes_before as km) |
| Ferries | 0 | Land corridor |

Prior false PASS claimed `attr=4` (`by_type.general=4`, origin-only) and
`datex_impacts=0` (`hosts_unreachable`) while soft pins already used 1.5 h.
Those gaps are addressed in product + host + campaign asserts; re-run the
instrumented campaign on `emulator-5554` after installing the rebuilt APK to
confirm JUnit `PASS_UI` with the new asserts.

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
| DATEX inject but `datex_impacts=0` | Synth XML/meta keep; hosts-down fallback; 25 km synth margin; host `datex-cache.json` + correct snap |

Cross-refs: [`how-to-use.md`](how-to-use.md) (Long trip / Breaks),
[`plugins/datex-plugin.md`](plugins/datex-plugin.md) (navi-synth),
[`plugins/poi-lookahead-cone-spec.md`](plugins/poi-lookahead-cone-spec.md)
(MWM seed), [`mwm-client-migration.md`](mwm-client-migration.md) (patches
0011–0012), [`notes/navi-server-world-storage-rule.md`](notes/navi-server-world-storage-rule.md).

---

## Honest gaps / follow-up

- Re-verify full Elsa JUnit on emulator after installing the rebuilt
  `compiled/navi-debug.apk` (x86_64) with the asserts above.
- DATEX live duckdns NPRA cache must not overwrite campaign synth when
  fingerprint is `navi-synth-*` (product keep path).
- Arm64 release APK in `compiled/` is for device sideload; Elsa campaign itself
  is emulator x86_64.
