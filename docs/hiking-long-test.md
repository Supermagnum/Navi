# Hiking long trip — Nashaugsætra → Sandhaug → Lothepus

Date: **2026-11-20** (Europe/Oslo). Emulator **`Navi_8c_4G`** (`emulator-5554`).
App `no.navi.app` **0.5.1-beta** (`versionCode` **36**), worktree branch
**`cursor/hiking-campaign-fail-fixes-d723`** (base tip `a1893ae1d` + uncommitted
tunnel soft-avoid strengthen). Not added to CI.

Driver: `HikingLongTripNashaugsLothepusUiCampaignTest#hiking_nashaugs_lothepus_ui_plan_campaign`
(Compose UI). Host `/tmp/hiking-long-campaign/hiking_long_host.py`: ADB GPS only +
read-only monitoring. Artifacts: `/tmp/hiking-long-campaign/` and
`docs/evidence/hiking-long-test/`.

---

## Verdict

| Check | Result |
|---|---|
| UI wipe → empty leaves | **PASS** (0 leaves after delete) |
| Østlandet first on SD | **PASS** (~88 s) |
| Vestlandet ready on SD | **PASS** — wait-on-in-flight + leaves-on-disk; Ready in **~8 s** corridor wait |
| Unlocked + networked cabins | **ON** (Hiking profile) |
| Plan found | **PASS** — **338.9 km**, 333 maneuvers, route on map |
| Soft-avoid UI progress | **PASS** — `tunnel_avoid_r*` / `mwm_enrich` pct during ~23.7 min plan |
| GPS replay + DNT screenshots | **PASS** (Nashaugsætra, Sandhaug, Lothepus + extras) |
| Ferries | **0** |
| Tunnels | **FAIL — 1** (`unnamed@0.08`, ~77 m). Soft-avoid `9→1` then `tunnel_avoid_fail=true`. Annotate matches soft-avoid (**reconciled**; was 10 vs 1). Ustedalen / Lilletoppen / Tyssedalsvegen cleared. |
| Wild camping sites | **PASS — 12** (`camping_plugin` ON) |
| Nearby attractions | **0** (cone-only @ 850 m; seed empty — no CoMaps report tallies) |
| Water sources | **0** in corridor counts |
| Overall | **FAIL** (one unavoidable micro-tunnel after soft-avoid) |

---

## Test setup

| Item | Value |
|---|---|
| Emulator | `Navi_8c_4G` / `emulator-5554` (~4 GB RAM) |
| Profile | **Hiking** |
| Avoid tunnels | **on** (locked for Hiking) |
| Avoid ferries | **off** |
| Wild camping | **on** |
| Nearby attractions | **on** |
| Long trip | **on** |
| DATEX | **off** |
| From | **Nashaugsætra** — `60.8835, 9.9720` |
| Via | **Sandhaug Turisthytte** (Hardangervidda) — `60.19220, 7.51250` |
| To | **Lothepus camping** — `60.0651, 6.5526` |

---

## Regions downloaded (order, timing, path)

All MWM leaves landed under SD:

`/storage/0000-0000/Android/data/no.navi.app/files/mwm/`  
(`releases/261105/leaves/` + mirrored `261105/`).

| # | Region | Result | Wall time | Notes |
|---|---|---|---|---|
| 1 | **Østlandet** (`europe_norway_ostlandet`) | OK | **88.0 s** download/install | Leaves: Norway_Central, Hedmark, Hordaland, Oppland, Southern, Western (+ sidecars 6). Placement: SD |
| 2 | **Vestlandet** (`europe_norway_vestlandet`) | OK | Corridor wait **8.0 s** | Alias leaves already overlapping Østlandet set + Rogaland → **Ready** |

Final leaf set on SD (World.mwm kept at MWM root):

- Norway_Central, Norway_Hedmark, Norway_Hordaland, Norway_Oppland, Norway_Rogaland, Norway_Southern, Norway_Western

Corridor wait until Ready before Plan: **~8.0 s** (`corridor_wait_ms=7953`).

---

## Route metrics

| Metric | Value |
|---|---|
| Total length | **338.9 km** |
| ETA | **5476 min** (~91.3 h) |
| Soft-break interval | **11.3 km** / **3.04 h** (`chunked_soft_breaks`) |
| Estimated stops | **29** soft rests (+ start/via/end) |
| Ferries | **0** |
| Tunnels (path annotate) | **1** — `unnamed@0.08` (~77 m). Soft-avoid leftover_fp matches. |
| Soft-avoid summary | `tunnels_before=9; tunnels_after=1; midpoints=27; attempts=124; rounds=5; leftover_fp=unnamed@0.08` + `tunnel_avoid_fail=true` |
| Wild camping sites | **12** |
| Water sources (corridor) | **0** |
| Attractions (cone @ 850 m) | **0** (by type: none) |

### Rest places (29)

All named **Hiking rest**, kind `rest`. Sample coords (full list in `device_report.json`):

| # | lat | lon |
|---|---|---|
| 1 | 61.17858 | 9.67115 |
| 10 | 60.64615 | 9.10313 |
| 22 | 60.22031 | 7.55907 |
| 29 | 60.09011 | 6.60652 |

### Navigation instructions

| Type | Count |
|---|---|
| left | 152 |
| right | 171 |
| destination | 10 |
| **Total** | **333** |

---

## Soft-avoid + enrich timing

`planning_done` `duration_ms=1423330` (~23.7 min) — dominated by strengthened tunnel sweeps.

| Phase | Detail |
|---|---|
| Initial MWM plan | ~315.6 km, **9** tunnels (incl. Ustedalen / Lilletoppen / Tyssedalsvegen) |
| Round 1 | Macro offsets → **4** left (`unnamed@0.08\|unnamed@0.47\|Lilletoppen@4.16\|Tyssedalsvegen@1.86`) |
| Round 2 | → **1** left (`unnamed@0.08`, max_run_m≈77) at **338.9 km** |
| Rounds 3–5 | Micro offsets 80–3000 m + entrance/mid/exit anchors; **no further cut** |
| Soft-avoid stop | `reroute_incomplete` + `tunnel_avoid_fail=true` (not `reroute_apply`) |
| Enrich | path annotate **route_tunnel_count=1** (matches soft-avoid after); attractions seed empty @ 850 m |

HUD/logcat showed continuous `tunnel_avoid_r*` progress (no multi-minute stuck at pct=10). Nested trials still skip enrich.

---

## DNT cabin / shelter screenshots

| Place | Path |
|---|---|
| Nashaugsætra | `docs/evidence/hiking-long-test/dnt_cabins/Nashaugsætra.png` |
| Sandhaug Turisthytte | `docs/evidence/hiking-long-test/dnt_cabins/Sandhaug_Turisthytte.png` |
| Lothepus camping | `docs/evidence/hiking-long-test/dnt_cabins/Lothepus_camping.png` |

Also: plan result `docs/evidence/hiking-long-test/screenshots/11_plan_result.png`.

---

## Notes (tunnel diagnosis)

1. **Annotate 10 vs soft-avoid 1 (prior run):** failed nested soft-avoid attempts overwrote `lastTurnsJson`; enrich annotated the last trial (10 tunnels) instead of the accepted corridor (1). Fixed by restoring `lastTurnsJson` to the accepted turns after every miss / before return.
2. **Leftover identity:** not Ustedalen / Lilletoppen / Tyssedalsvegen — those clear by round 2. Final leftover is **`unnamed@0.08` (~77 m)** present from the first-pass corridor and resistant to macro + micro perpendicular detours (124 attempts / 5 rounds).
3. **Fail clearly:** hiking soft-avoid no longer claims `reroute_apply` / OK while tunnels remain; emits `reroute_incomplete` + `tunnel_avoid_fail=true; tunnels_remain=N` with `leftover_fp`.
4. **Tunnel-zero still open:** soft-avoid cannot remove this micro covered passage with via detours; product options are alternate corridor seeds, native tunnel costing, or a short-run policy (not applied here).
5. **Intact:** region-reconcile Ready gate, nested-enrich-skip, `uiProgress` during soft-avoid; attractions stay cone-only (0 @ 850 m).
