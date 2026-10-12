# PR #160 emulator evidence — sticky typed From

- Device: `emulator-5554` only (Navi_8c_4G_128G AVD / x86_64)
- Branch tip at run: `65783feb9f1f04e6fc88c23b9014d4264e8ec4d8` (plus this evidence commit)
- Instrumented class: `no.navi.app.FromStickyGpsInstrumentedTest`
- JVM: `RouteStartGpsPolicyTest` (BUILD SUCCESSFUL)

## Case 1 — Typed Hamar ~1 km from GPS stays sticky

**PASS**

- Mock GPS ~903 m west of Hamar (60.79446, 11.0621 vs Hamar 60.79446, 11.07872)
- From set to typed/picked **Hamar** with `fromFollowsGps=false`
- Planned short corridor; sustained off-route inject
- After off-route window: `fromFollowsGps=false`, From label still **Hamar**, no auto-replan
- Log: `PASS sticky Hamar fromFollowsGps=false from=Hamar summary=From: Hamar | To: Near Hamar ... startToGpsM≈902.75`

Screenshot: [01_typed_hamar_sticky_after_offroute.png](01_typed_hamar_sticky_after_offroute.png)

## Case 2 — Use GPS / My position follows GPS

**PASS**

- Tapped **Use GPS as from**; `fromFollowsGps=true`
- Same off-route inject path; auto-replan fired (`reroutes=1`)
- From updated to live GPS coords after replan
- Log: `PASS use-gps followsGps=true from=60.79446, 11.06210 reroutes=1 summary=From: GPS (60.85863, 11.08498) ...`

Screenshot: [02_use_gps_follows_after_replan.png](02_use_gps_follows_after_replan.png)

## How to reproduce

```bash
export ANDROID_SERIAL=emulator-5554
./gradlew :app:testDebugUnitTest --tests 'no.navi.app.RouteStartGpsPolicyTest' -PnaviAbis=x86_64
./gradlew :app:connectedDebugAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=no.navi.app.FromStickyGpsInstrumentedTest \
  -PnaviAbis=x86_64
```
