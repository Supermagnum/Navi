# Expanded local fuzz campaign (issue #167)

Date: **2026-11-25 → 2026-11-27** (Europe/Oslo). Host: self-hosted Linux.
Toolchain: **`nightly-2026-06-15`**. Branch tip: **`4620161a7`** (`fuzz`),
later worktree `Navi-fuzz-campaign` (detached same rev) so main-repo checkouts
could not interrupt the run.

## Verdict (new targets)

| Target | Duration | Result | Notes |
|---|---|---|---|
| `benshi_rf_ch` | 1 h | PASS | 0 oom/timeout/crash |
| `benshi_settings_patch` | 1 h | PASS | |
| `benshi_dev_info` | 1 h | PASS | |
| `benshi_ht_status` | 1 h | PASS | |
| `benshi_freq_mode_notif` | 1 h | PASS | |
| `rigctld_reply` | 1 h | PASS | |
| `repeater_checksums` | 1 h | PASS | |
| `parse_mhz` | 1 h | PASS | |
| `maidenhead` | 1 h | PASS | |
| `osm_repeater_relation_xml` | 1 h | PASS | prior OOB fix held |
| `repeater_csv_bytes` | 1 h | PASS | |
| `repeater_kml_point` | 1 h | PASS | |
| `mwm_catalog_json` | 1 h | PASS | |
| `mwm_countries_txt` | 1 h | PASS | |
| `navi_manifest_json` | 1 h | PASS | |
| `pbf_extract_gate` | 1 h | PASS | |
| `osm_maxspeed` | 1 h | PASS | |
| `osm_conditional_maxspeed` | 1 h | **CRASH** (~1193 s) | see below |
| `plugin_manifest` | 1 h | PASS | |
| `mwm_sidecar` | **4 h** | PASS | 0/0/0 |
| `flat_graph_pack` | **4 h** | PASS | 0/0/0 |

Earlier seven targets already had a clean 1 h campaign:
[`fuzzing-issue-167.md`](fuzzing-issue-167.md).

## Finding: `osm_conditional_maxspeed`

Artifact:
`fuzz/artifacts/osm_conditional_maxspeed/crash-ce8ba363865788f6f71f4bb168254882b1f41c8e`

Input (hex): `36 00 27 40 32 3a 30 30 2d 30 36 2d 34 30 3b …`

`opening-hours-syntax` 2.1.6 hits
`debug_assert!(!err.is_implementation_error())` in its parser
(`GrammarLogic { rule: hour_minutes, invariant: "cannot be empty" }`), which
panics under fuzz / debug-assertion builds even though `FromStr` would
otherwise return `Err`.

**Fix:** `oh_condition_matches_at` wraps `OpeningHours::from_str` in
`catch_unwind` and treats panics as unparseable (`None`). Regression test
`oh_malformed_hour_minutes_does_not_panic`.

## Operational note

Two earlier sequential campaigns aborted mid-run when the primary checkout
switched to `dev`/`main` (bins disappeared). The finishing campaign ran from
an isolated worktree and completed through the 4 h targets.
