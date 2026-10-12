# From sticky / Use GPS — emulator results

**Device:** emulator-5554 (`Navi_8c_4G_128G` / sdk_gphone64_x86_64). **Not** SM-P613.

**Runner:** `FromStickyGpsInstrumentedTest` — both methods PASS.

| Check | Result |
|---|---|
| Typed From = Hamar, GPS fix ~903 m away | stays `From: Hamar`, `fromFollowsGps=false` after plan |
| Off-route while sticky | no auto-replan (`autoRerouteTriggeredCount` unchanged) |
| Use GPS / My position | `fromFollowsGps=true`; auto-replan follows GPS (`reroutes=1`) |

```
PASS sticky Hamar fromFollowsGps=false from=Hamar summary=From: Hamar  |  To: Near Hamar  |  Via: (none) gps=60.85863,11.08498 startToGpsM≈902.75
PASS use-gps followsGps=true from=… reroutes=1 summary=From: GPS (60.85863, 11.08498)  |  To: Near Hamar  |  Via: (none)
```

Screenshots: `01_typed_hamar_sticky_after_offroute.png`, `02_use_gps_follows_after_replan.png`.
