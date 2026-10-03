# Densify joint detour (Bergen→Stavanger and long corridors)

Branch: `fix/densify-detour` (from `dev` after #140). Client only. No merge.

## Problem

Geometric densify joints (chord midpoints snapped within 35 km) forced routes
off the best corridor. Bergen→Stavanger centre: single-shot **206.88 km** vs
densified **230.20 km** (~+11%). Joint was the chord mid
`(59.679072, 5.533872)` — not on E39. Dig tablet densify matched **228.21 km**
for the same reason. Real E39-class distance is ~207–210 km.

Same-stem coastal densify (restored in #140) remains required so tablet plans
never hold the ~259k-node Vestlandet single-shot owned graph (~985 MiB VmHWM).

## STEP 1 — Host measure (before fix)

Pack: `.packs/long-trip-packs` (vestlandet Ready). Host harness:
`directed-snap-diag` (stub PBF disables overlay; pack ferries used).

| Case | mode | distance_km | ETA min | ferries | Δ vs single-shot |
| --- | --- | ---: | ---: | --- | ---: |
| Bergen→Stav centre | single-shot (`NAVI_FORCE_SINGLE_SHOT=1`) | **206.88** | 181.4 | Halhjem@21.32\|Arsvågen@9.15 | — |
| Bergen→Stav centre | densify (geometric mid) | **230.20** | 209.3 | same pair (split across hops) | **+23.3 km / +11.3%** |
| Bergen→Stav station | single-shot | **205.51** | 179.6 | same | — |
| Bergen→Stav station | densify (geometric mid) | **228.83** | 207.6 | same | **+23.3 km / +11.4%** |

Diverge: hop joint at chord mid west of E39; leg1/leg2 stitch through coastal
roads instead of the single-shot Halhjem→Arsvågen corridor.

Other matrix / long trips (Raufoss→Tromsø, Oslo→Trondheim, …) need full
landsdel packs on host; vestlandet-only pack dir could not run them here.
Largest working host sub-trip without extra packs: Vestlandet coastal OD above
(single-shot peaks ~259k nodes — fine on host, not on tablet).

## STEP 2 — Fix: skeleton coarse path joints

Option **(a)** implemented: coarse pre-pass on a **reduced** graph
(motorway/trunk/primary/secondary + links + car ferries), place densify joints
on that A* path, refine hops on the full corridor as today.

- Filter runs during tile materialize (`with_densify_skeleton_only`) so peak
  RSS stays under a full corridor.
- Fallback: previous geometric/region densify if skeleton load or path fails.
- Same-stem coastal densify retained for memory; joints now track the best route.

## STEP 3 — Workaround

Kept same-stem coastal densify for tablet RSS. With skeleton joints, densified
distance matches single-shot within measurement noise (see below) — no need to
delete the densify gate.

## Host acceptance (after fix)

| Case | densified km | single-shot km | Δ% | ferries |
| --- | ---: | ---: | ---: | --- |
| Bergen→Stav centre (lt off) | **206.88** | 206.88 | **0.0** | Halhjem\|Arsvågen |
| Bergen→Stav centre (lt on) | **206.88** | (densify) | **0.0** | same |
| Bergen→Stav station | **205.51** | 205.51 | **0.0** | same |

Wall (host cold-ish, directed-snap-diag): centre densify ~4.1 s / lt ~5.2 s /
station ~4.4 s (includes skeleton pre-pass). Prior geometric densify was ~2.3–3.0 s.

## Tablet (SM-P613 R52TB0JQEDE) — pending re-run

Expected after install of this branch:

- Bergen→Stavanger centre ~207 km long-trip on and off; same ferries as above
- Peak RSS matrix max ≤933 MiB (densify hops; skeleton pre-pass filtered)
- Non-densified dig-matching cases (Raufoss→Bergen lt-off, Dombås, Førde)
  unchanged geom/distance
- Tromsø: still 4 days / 3 overnight; report new distance vs dig 1766.89

## Harness

```bash
cargo run -p navi-ffi --release --bin directed-snap-diag -- \
  --pack-dir .packs/long-trip-packs
```
