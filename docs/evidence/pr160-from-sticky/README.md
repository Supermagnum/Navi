# PR #160 — sticky typed From vs Use GPS (emulator)

Device: `emulator-5554` only (`Navi_8c_4G_128G`). Not SM-P613.

Runner: `FromStickyGpsInstrumentedTest`

- `01_typed_hamar_sticky_after_offroute.png` — Hamar typed as From (~1 km from GPS fix) stays Hamar after plan + off-route; no GPS auto-replan.
- `02_use_gps_follows_after_replan.png` — Use GPS / My position follows GPS on auto-replan.
