# GPX import / export check (issue #241)

Date: **2026-11-25** (Europe/Oslo). Device: Samsung **SM-P613** serial
**`R52TB0JQEDE`**. App `no.navi.app` **0.5.2-beta** (`versionCode` **37**).
Branch used for the check: **`cursor/gpx-import-export-check-de43`** (from
**`dev`**). Issue:
[Supermagnum/Navi#241](https://github.com/Supermagnum/Navi/issues/241).

**No app or library product code was changed.** Import, display-simplify,
export round-trip, and on-device rendering already behaved correctly. An
ignored smoke test was added only on the feature branch
(`core/tests/gpx_import.rs`, `issue241_nordleden_stolavs_waterway_no_jumps`).

Screenshots (host): `/home/haaken/Nedlastinger/gpx-241-issue241/`
(panel / overview / detail for each file). Samples were not committed to
the repository.

---

## Verdict

| File | Source pts | After simplify @6000 | Jumps >= 2.5 km | Length | Device panel | Render |
|---|---|---|---|---|---|---|
| Nordleden | 4795 | 4795 | 0 (worst 198 m) | 156.5 km | 4795 pts, 156.5 km, ascent 4129 m | continuous magenta overlay, no jumps |
| St. Olavsleden hiking 2022 | 5178 | 5178 | 0 (worst 1895 m) | 578.0 km | 5178 pts, 578.0 km | continuous Trondheim → Sundsvall, no jumps |
| outdooractive (St. Olav Waterway cyclists) | 10450 | 6000 | 0 (worst after simplify 1273 m) | 290.2 km | shows original 10450 pts; drape uses simplified | continuous archipelago line, no jumps |

| Check | Result | OK |
|---|---|---|
| Parse (all three) | ok, `unsupported=[]` | yes |
| Display simplify @6000 (app `MAX_POINTS_PER_LINE`) | outdooractive only reduced; length ratio 1.0000 | yes |
| No consecutive jumps >= 2.5 km (raw + simplified) | 0 on all three | yes |
| Export round-trip (`to_gpx` → `parse_gpx`) | point counts match simplified lines | yes |
| On-device import (MediaStore URI + read grant) | all three imported | yes |
| On-device render (GPS follow broken so fit sticks) | continuous overlays, no detours/jumps | yes |

**Overall: PASS.** No product bug found for issue #241.

---

## Test setup

| Item | Value |
|---|---|
| Device | Samsung SM-P613 / `R52TB0JQEDE` |
| App | `no.navi.app` 0.5.2-beta (37) |
| Samples dir | `/tmp/gpx-241` (env `GPX_ISSUE241_DIR`) |
| Nordleden | `Nordleden.gpx` — lat 63.80–64.54, lon 11.56–12.56 |
| St. Olavsleden hiking 2022 | `st-olavsleden-hiking-2022.gpx` — lat 62.35–63.80, lon 10.40–17.32 |
| outdooractive | `outdooractive.gpx` — St. Olav Waterway for cyclists_10_2020; lat 59.92–60.45, lon 19.54–22.40 |
| Host smoke | `cargo test --release -p driver-break-core --test gpx_import issue241 -- --ignored --nocapture` |
| Jump threshold | 2500 m between consecutive points |
| Display cap | 6000 points per line (Douglas–Peucker via `simplify_to_max_points` / UniFFI `gpxParseJson`) |
| Device open path | `content://media/external/file/<id>` with `--grant-read-uri-permission` (DocumentsProvider URI denied without SAF) |

---

## Notes

- UI label uses `original_point_count` (so outdooractive shows 10450); the
  drape line is the simplified geometry (<= 6000).
- DocumentsProvider open-with (`content://com.android.externalstorage…`)
  failed with Permission Denial unless opened through SAF / MediaStore with
  a read grant. That is an Android access path, not a GPX geometry bug.
- Non-ignored `gpx_import` suite: 13 passed, 6 ignored (pre-existing real-file
  smokes).
