//! Trip-bbox pad schedule for adaptive plan widen-retry.
//!
//! Initial pad matches historical `plan_car_route_inner` behaviour; widen doubles
//! until [`PLAN_BBOX_PAD_CAP_DEG`] so RAM stays bounded on Automotive devices.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Runtime floor raised by corridor disconnect widen-retry (0 = inactive).
/// Combined with [`effective_max_plan_tiles_for_stems`] via `max(base, floor)`.
static PLAN_TILE_BUDGET_AT_LEAST: AtomicUsize = AtomicUsize::new(0);

/// Stage B: skip chord-only tile budget caps; select tiles covering the coarse path.
static STAGE_B_ACTIVE: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// Full coarse path (lat, lon) for Stage B hop corridor tile selection.
    static STAGE_B_COARSE_PATH: RefCell<Option<Vec<(f64, f64)>>> = const { RefCell::new(None) };
}

/// Enable/disable Stage B densify mode for the current plan.
pub fn set_stage_b_active(on: bool) {
    STAGE_B_ACTIVE.store(on, Ordering::Relaxed);
    if !on {
        clear_stage_b_coarse_path();
        set_plan_tile_budget_at_least(0);
    } else {
        // No tile-budget truncation: raise floor to the widen cap so every
        // coarse-path-covering tile can load.
        set_plan_tile_budget_at_least(MAX_PLAN_TILES_WIDEN_CAP);
    }
}

pub fn stage_b_active() -> bool {
    STAGE_B_ACTIVE.load(Ordering::Relaxed)
}

pub fn set_stage_b_coarse_path(path: Vec<(f64, f64)>) {
    STAGE_B_COARSE_PATH.with(|c| *c.borrow_mut() = Some(path));
}

pub fn clear_stage_b_coarse_path() {
    STAGE_B_COARSE_PATH.with(|c| *c.borrow_mut() = None);
}

/// Coarse-path samples between hop endpoints (inclusive), for Stage B tile/edge
/// corridor. Falls back to `[start, end]` when Stage B path is unset.
pub fn stage_b_hop_corridor_points(start: (f64, f64), end: (f64, f64)) -> Vec<(f64, f64)> {
    STAGE_B_COARSE_PATH.with(|c| {
        let borrow = c.borrow();
        let Some(path) = borrow.as_ref() else {
            return vec![start, end];
        };
        if path.len() < 2 {
            return vec![start, end];
        }
        // Indices nearest to start/end along the coarse polyline.
        let nearest = |p: (f64, f64)| -> usize {
            path.iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    let da = (a.0 - p.0).hypot(a.1 - p.1);
                    let db = (b.0 - p.0).hypot(b.1 - p.1);
                    da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(i, _)| i)
                .unwrap_or(0)
        };
        let mut i0 = nearest(start);
        let mut i1 = nearest(end);
        if i0 > i1 {
            std::mem::swap(&mut i0, &mut i1);
        }
        let mut out: Vec<(f64, f64)> = path[i0..=i1].to_vec();
        if out.first().copied() != Some(start) {
            out.insert(0, start);
        }
        if out.last().copied() != Some(end) {
            out.push(end);
        }
        // Decimate very dense paths (~every ~0.15°) to keep tile sample cost bounded.
        if out.len() > 64 {
            let step = (out.len() / 48).max(1);
            let mut dec: Vec<_> = out.iter().step_by(step).copied().collect();
            if dec.last() != out.last() {
                if let Some(l) = out.last().copied() {
                    dec.push(l);
                }
            }
            out = dec;
        }
        out
    })
}

/// Initial pad: `span * 0.35` clamped to this band (degrees).
pub const PLAN_BBOX_PAD_MIN_DEG: f64 = 0.35;
pub const PLAN_BBOX_PAD_INITIAL_MAX_DEG: f64 = 2.5;
/// Hard cap for widen-retry (degrees). Not unbounded.
pub const PLAN_BBOX_PAD_CAP_DEG: f64 = 5.0;

/// Build the pad attempt list for an OD pair (degrees of lat/lon pad).
///
/// Always includes the initial pad; then doubles until the cap (inclusive once).
pub fn plan_bbox_pad_schedule(
    start_lat: f64,
    start_lon: f64,
    end_lat: f64,
    end_lon: f64,
) -> Vec<f64> {
    plan_bbox_pad_schedule_points(&[(start_lat, start_lon), (end_lat, end_lon)])
}

/// Pad schedule from the axis-aligned span of all route points (start, vias, end).
pub fn plan_bbox_pad_schedule_points(points: &[(f64, f64)]) -> Vec<f64> {
    let (min_lat, min_lon, max_lat, max_lon) = points_bounds(points);
    let lat_span = (max_lat - min_lat).abs();
    let lon_span = (max_lon - min_lon).abs();
    let mut pad =
        (lat_span.max(lon_span) * 0.35).clamp(PLAN_BBOX_PAD_MIN_DEG, PLAN_BBOX_PAD_INITIAL_MAX_DEG);
    let mut out = Vec::with_capacity(4);
    loop {
        out.push(pad);
        if pad >= PLAN_BBOX_PAD_CAP_DEG - 1e-9 {
            break;
        }
        let next = (pad * 2.0).min(PLAN_BBOX_PAD_CAP_DEG);
        if (next - pad).abs() < 1e-9 {
            break;
        }
        pad = next;
    }
    out
}

pub fn trip_bbox(start_lat: f64, start_lon: f64, end_lat: f64, end_lon: f64, pad: f64) -> [f64; 4] {
    trip_bbox_points(&[(start_lat, start_lon), (end_lat, end_lon)], pad)
}

/// Axis-aligned bbox covering all points, expanded by `pad` degrees.
pub fn trip_bbox_points(points: &[(f64, f64)], pad: f64) -> [f64; 4] {
    let (min_lat, min_lon, max_lat, max_lon) = points_bounds(points);
    [min_lat - pad, min_lon - pad, max_lat + pad, max_lon + pad]
}

/// Max axis-aligned hop (degrees) before a long-trip plan is split into
/// sequential legs. Keeps per-leg pack merges inside Automotive RAM budgets
/// (4 GB device class — peak process RSS must stay well under ~2.5 GB).
pub const LONG_TRIP_CHUNK_DEG: f64 = 1.15;

/// Pad (degrees) around each consecutive OD segment when selecting graph tiles
/// for multi-region corridors. Keeps RAM bounded vs the full trip AABB.
///
/// Kept modest (0.25° ≈ 28 km): densify hops are already ≤ [`LONG_TRIP_CHUNK_DEG`];
/// a 0.40° pad pulled entire neighbour tiles into the first Bevensen→SH hop and
/// LMK'd ~3 GiB RSS on 4 GB Automotive before snap.
pub const CORRIDOR_TILE_PAD_DEG: f64 = 0.30;

/// Chebyshev half-width (degrees) for **edge materialization** along the OD
/// polyline. Tiles are still selected with [`CORRIDOR_TILE_PAD_DEG`], but only
/// edges near the chord are kept — a diagonal hop's AABB otherwise materializes
/// ~1M edges (~1 GiB host / ~3 GiB device) for a ~120 km leg.
///
/// 0.40° (~45 km) leaves room for land-bridge detours (e.g. NI↔SH around
/// Hamburg when the city-state pack is absent) while still excluding the far
/// corners of the hop's diagonal AABB.
pub const CORRIDOR_EDGE_HALF_WIDTH_DEG: f64 = 0.40;

/// Extra half-width at densify / hop **endpoints** so region centroids retain
/// enough clearance-legal network for [`CHUNK_INTERMEDIATE_SNAP_M`] (25 km ≈ 0.25°).
pub const CORRIDOR_ENDPOINT_HALF_WIDTH_DEG: f64 = 0.40;

/// Sample step (degrees) when building the corridor band of small clip boxes.
pub const CORRIDOR_BAND_STEP_DEG: f64 = 0.20;

/// Hard cap on graph tiles merged for one plan/leg on Automotive (4 GB).
/// Large car tiles are 80–150 MB on disk; rkyv materialization peaks higher.
/// Endpoint-covering tiles are always kept even if this is exceeded slightly.
/// Six is enough for one-stem corridors once edge clipping is a corridor band.
pub const MAX_PLAN_TILES: usize = 6;

/// Cross-stem corridors (e.g. Ostlandet→Vestlandet / Raufoss→Bergen) need more
/// than six tiles so midpoint samples keep a connected bridge; six dropped the
/// Vestlandet half and left A* exploring a disconnected Ostlandet component
/// for minutes. Fourteen stays under the ~550 MiB on-disk soft byte cap for
/// typical car corridor tiles (measured ~900 MiB peak RSS on host).
pub const MAX_PLAN_TILES_MULTI_STEM: usize = 14;

/// Memory-aware upper bound when widening a disconnected corridor tile budget.
/// Stays under ~1.5× the multi-stem default; callers must stop and error past this.
pub const MAX_PLAN_TILES_WIDEN_CAP: usize = 20;

/// Soft RSS ceiling (MiB) used only as a **warning** in widen notes. Widen still
/// proceeds up to [`MAX_PLAN_TILES_WIDEN_CAP`] so a truncated corridor can recover;
/// the disconnected graph is dropped before reload.
pub const PLAN_TILE_WIDEN_RSS_CAP_MB: f64 = 2800.0;

/// Raise the effective tile budget floor for the current plan thread/process.
/// Pass `0` to clear. Used by disconnect widen-retry so a forced measure budget
/// of 6 can still recover by loading the Vestlandet bridge.
pub fn set_plan_tile_budget_at_least(n: usize) {
    PLAN_TILE_BUDGET_AT_LEAST.store(n, Ordering::Relaxed);
}

/// Current tile-budget floor (0 when inactive).
pub fn plan_tile_budget_at_least() -> usize {
    PLAN_TILE_BUDGET_AT_LEAST.load(Ordering::Relaxed)
}

/// Next widen step above `current`, or `None` when the memory-aware cap is hit.
pub fn next_plan_tile_budget(current: usize) -> Option<usize> {
    const STEPS: &[usize] = &[6, 10, 14, 18, MAX_PLAN_TILES_WIDEN_CAP];
    STEPS.iter().copied().find(|&s| s > current)
}

/// How many entries of [`plan_bbox_pad_schedule`] chunked long-trip legs keep.
/// Full schedule reaches [`PLAN_BBOX_PAD_CAP_DEG`] (5.0°); the default three
/// stops at 1.4° so per-leg RAM stays bounded. Override at measure time via
/// `NAVI_MEASURE_CHUNK_PAD_TAKE`.
pub const CHUNK_PAD_SCHEDULE_TAKE: usize = 3;

fn measure_override_usize(key: &str) -> Option<usize> {
    std::env::var(key).ok().and_then(|s| s.parse().ok())
}

fn measure_override_f64(key: &str) -> Option<f64> {
    std::env::var(key).ok().and_then(|s| s.parse().ok())
}

/// Effective tile budget (see [`MAX_PLAN_TILES`]).
pub fn effective_max_plan_tiles() -> usize {
    let base = measure_override_usize("NAVI_MEASURE_MAX_PLAN_TILES").unwrap_or(MAX_PLAN_TILES);
    base.max(plan_tile_budget_at_least())
}

/// Tile budget when `extra_stem_count` neighbour stems join the primary.
///
/// Env `NAVI_MEASURE_MAX_PLAN_TILES` sets the **base** (measure campaigns).
/// [`set_plan_tile_budget_at_least`] can raise above that base so a forced
/// 6-tile measure still recovers a cross-stem bridge instead of spinning A*.
pub fn effective_max_plan_tiles_for_stems(extra_stem_count: usize) -> usize {
    let base =
        measure_override_usize("NAVI_MEASURE_MAX_PLAN_TILES").unwrap_or(if extra_stem_count > 0 {
            MAX_PLAN_TILES_MULTI_STEM
        } else {
            MAX_PLAN_TILES
        });
    let floor = plan_tile_budget_at_least();
    base.max(floor)
}

/// Effective corridor-band half-width (see [`CORRIDOR_EDGE_HALF_WIDTH_DEG`]).
pub fn effective_corridor_edge_half_width_deg() -> f64 {
    measure_override_f64("NAVI_MEASURE_CORRIDOR_HALF_WIDTH_DEG")
        .unwrap_or(CORRIDOR_EDGE_HALF_WIDTH_DEG)
}

/// Effective chunk pad-schedule take (see [`CHUNK_PAD_SCHEDULE_TAKE`]).
pub fn effective_chunk_pad_schedule_take() -> usize {
    measure_override_usize("NAVI_MEASURE_CHUNK_PAD_TAKE").unwrap_or(CHUNK_PAD_SCHEDULE_TAKE)
}

/// Soft cap on on-disk tile bytes merged for one plan/leg. Prefer dropping the
/// largest non-essential tiles before exceeding this; endpoints always stay.
/// With corridor-band edge clip, ~280 MB disk stays well under 2.8 GiB RSS.
/// Ostlandet car tiles are ~100–150 MB; a same-stem short hop needs three of
/// them so the mid bridge is not dropped under a tighter cap.
pub const MAX_PLAN_TILE_BYTES: u64 = 550 * 1024 * 1024;

/// Snap budget for densify hop endpoints (region centroids), not user stops.
/// Centroids can sit several km offshore / inland of the nearest clearance-legal
/// road (SH→DK water approaches needed ~11–22 km). Same-stem tile fill prevents
/// the false Skåne→Halland component jump that a large snap used to cause.
///
/// This constant affects **snap search only** (pad ≈ max_m/1e5 degrees). It does
/// **not** widen tile selection or edge materialization — those use
/// [`CORRIDOR_TILE_PAD_DEG`] / [`CORRIDOR_EDGE_HALF_WIDTH_DEG`].
pub const CHUNK_INTERMEDIATE_SNAP_M: f64 = 35_000.0;

/// Effective densify-joint snap budget (see [`CHUNK_INTERMEDIATE_SNAP_M`]).
pub fn effective_chunk_intermediate_snap_m() -> f64 {
    measure_override_f64("NAVI_MEASURE_CHUNK_INTERMEDIATE_SNAP_M")
        .unwrap_or(CHUNK_INTERMEDIATE_SNAP_M)
}

/// Tighter densify snap when both hop ends lie in the same catalog region.
pub const CHUNK_SAME_REGION_SNAP_M: f64 = 8_000.0;

/// Chebyshev-ish span of the point set (max of lat/lon ranges).
pub fn trip_span_deg(points: &[(f64, f64)]) -> f64 {
    let (min_lat, min_lon, max_lat, max_lon) = points_bounds(points);
    (max_lat - min_lat).abs().max((max_lon - min_lon).abs())
}

/// Insert linear midpoints so each consecutive hop's span is ≤ `max_hop_deg`.
/// Prefer [`densify_route_points_via_regions`] for cross-sea corridors.
pub fn densify_route_points(points: &[(f64, f64)], max_hop_deg: f64) -> Vec<(f64, f64)> {
    if points.len() < 2 || max_hop_deg <= 0.0 {
        return points.to_vec();
    }
    let mut out = Vec::with_capacity(points.len() * 2);
    out.push(points[0]);
    for w in points.windows(2) {
        let (a_lat, a_lon) = w[0];
        let (b_lat, b_lon) = w[1];
        let dlat = b_lat - a_lat;
        let dlon = b_lon - a_lon;
        let dist = dlat.abs().max(dlon.abs());
        let n = ((dist / max_hop_deg).ceil() as usize).max(1);
        for i in 1..n {
            let t = i as f64 / n as f64;
            out.push((a_lat + dlat * t, a_lon + dlon * t));
        }
        out.push((b_lat, b_lon));
    }
    out
}

/// Densify using centroids of Ready region packs under `data_dir` that lie
/// along the trip. Avoids geometric midpoints in the Baltic / Kattegat that
/// make densified hops `disconnected`.
pub fn densify_route_points_via_regions(
    points: &[(f64, f64)],
    data_dir: &std::path::Path,
    max_hop_deg: f64,
) -> Vec<(f64, f64)> {
    densify_route_points_via_regions_dirs(points, &[data_dir], max_hop_deg)
}

/// Same as [`densify_route_points_via_regions`], scanning Ready packs across
/// multiple directories (internal Tools root + long-trip pack dir).
pub fn densify_route_points_via_regions_dirs(
    points: &[(f64, f64)],
    dirs: &[&std::path::Path],
    max_hop_deg: f64,
) -> Vec<(f64, f64)> {
    if points.len() < 2 {
        return points.to_vec();
    }
    let start = points[0];
    let end = *points.last().unwrap();
    let vlat = end.0 - start.0;
    let vlon = end.1 - start.1;
    let v2 = vlat * vlat + vlon * vlon;
    if v2 < 1e-12 {
        return points.to_vec();
    }

    let mut anchors: Vec<(f64, (f64, f64))> = Vec::new();
    let ready = collect_ready_region_entries_dirs(dirs);
    // Drammen→Berlevåg-class: OD vector is NE so 2D projection ranks Gudbrandsdalen
    // west-dip towns (Dombås) before Hamar and produces southbound densify hops.
    // Norway E6 spine applies sequence-ordered t; skip landsdel centroids then
    // (their 2D/lat t would re-interleave Hamar/Oppdal / skip Karasjok).
    let spine_anchors = norway_e6_spine_anchors(start, end, &ready);
    let inbound_anchors = if spine_anchors.is_empty() {
        norway_e6_inbound_anchors(points, &ready)
    } else {
        Vec::new()
    };
    let ottadal_west_anchors = norway_ottadal_westbound_anchors(points, &ready);
    let use_spine = !spine_anchors.is_empty();
    let progress_t = |p: (f64, f64)| -> f64 {
        (((p.0 - start.0) * vlat + (p.1 - start.1) * vlon) / v2).clamp(0.0, 1.0)
    };
    // Keep explicit vias (everything except start/end) as forced anchors.
    for &p in &points[1..points.len() - 1] {
        let t = if use_spine {
            // Place vias by latitude into the spine t range roughly.
            let lat_t = if (end.0 - start.0).abs() > 1e-9 {
                ((p.0 - start.0) / (end.0 - start.0)).clamp(0.0, 1.0)
            } else {
                progress_t(p)
            };
            0.04 + 0.92 * lat_t
        } else {
            progress_t(p)
        };
        anchors.push((t, p));
    }
    if !use_spine {
        for (path, bbox) in &ready {
            if densify_skip_country_when_leaves_ready(path, &ready) {
                continue;
            }
            if densify_skip_east_baltic_hinterland_leaf(path, start, end, &ready) {
                continue;
            }
            if densify_skip_sorlandet_west_loop_leaf(path, start, end) {
                continue;
            }
            let c = ((bbox[0] + bbox[2]) * 0.5, (bbox[1] + bbox[3]) * 0.5);
            let c = prefer_densify_leaf_centroid(c, *bbox, path, &ready);
            let t = progress_t(c);
            if t <= 0.02 || t >= 0.98 {
                continue;
            }
            let lat_lo = start.0.min(end.0) - 0.25;
            let lat_hi = start.0.max(end.0) + 0.25;
            if c.0 < lat_lo || c.0 > lat_hi {
                continue;
            }
            let trip = trip_bbox_points(points, CORRIDOR_TILE_PAD_DEG.max(3.0));
            if c.0 < trip[0] || c.0 > trip[2] || c.1 < trip[1] || c.1 > trip[3] {
                continue;
            }
            anchors.push((t, c));
        }
    }

    // Norway landsdel packs are huge; catalog centroids alone miss the E6
    // (Gudbrandsdalen dips west to Dombås ≈9.13°E). Inject trunk waypoints so
    // Stay-in-Country northbound densify follows the highway spine.
    anchors.extend(spine_anchors.iter().copied());
    anchors.extend(inbound_anchors.iter().copied());
    anchors.extend(ottadal_west_anchors.iter().copied());

    anchors.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    // Dedup near-duplicates along t, then keep only points that get closer to
    // the *current* progress target (next explicit via, else the destination).
    // Chebyshev-to-final-dest dropped Västra Götaland on Bevensen→Ottadal→
    // Dalsøren: Gothenburg is east of Halland while dest is west, so the filter
    // skipped the E6 land bridge and gap-fill hopped Halland→Kattegat (DK
    // t2_3/t3_4 + Halland t1_0). Lon-weighted t alone can still rank western
    // Niedersachsen after Hamburg; the per-segment Chebyshev check blocks that.
    // Norway E6 spine waypoints are forced (like explicit vias): Gudbrandsdalen
    // dips west (Dombås ≈9.13°E) which temporarily worsens Chebyshev-to-end.
    let cheb = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs().max((a.1 - b.1).abs());
    let via_set: std::collections::HashSet<(u64, u64)> = points[1..points.len() - 1]
        .iter()
        .map(|p| (p.0.to_bits(), p.1.to_bits()))
        .collect();
    let spine_set: std::collections::HashSet<(u64, u64)> = spine_anchors
        .iter()
        .chain(inbound_anchors.iter())
        .chain(ottadal_west_anchors.iter())
        .map(|(_, p)| (p.0.to_bits(), p.1.to_bits()))
        .collect();
    // Explicit vias with their densify t (same order basis as anchors).
    let via_ts: Vec<(f64, (f64, f64))> = anchors
        .iter()
        .copied()
        .filter(|(_, p)| via_set.contains(&(p.0.to_bits(), p.1.to_bits())))
        .collect();
    let next_progress_target = |t: f64| -> (f64, f64) {
        via_ts
            .iter()
            .find(|(tv, _)| *tv > t + 1e-9)
            .map(|(_, p)| *p)
            .unwrap_or(end)
    };
    // Forced joints with densify t — used to block leaf centroids that would
    // force a south jog to hit an upcoming Oslo/Hamar/Otta inbound hop.
    let forced_joints: Vec<(f64, (f64, f64))> = anchors
        .iter()
        .copied()
        .filter(|(_, p)| {
            via_set.contains(&(p.0.to_bits(), p.1.to_bits()))
                || spine_set.contains(&(p.0.to_bits(), p.1.to_bits()))
        })
        .collect();
    let mut filtered: Vec<(f64, (f64, f64))> = Vec::new();
    let mut last_t = -1.0_f64;
    let mut target = next_progress_target(-1.0);
    let mut best_to_end = cheb(start, target);
    for (t, p) in anchors {
        let is_via = via_set.contains(&(p.0.to_bits(), p.1.to_bits()));
        let is_spine = spine_set.contains(&(p.0.to_bits(), p.1.to_bits()));
        let forced = is_via || is_spine;
        if t - last_t < 0.04 && !filtered.is_empty() && !forced {
            // OD progress_t can cluster far-apart leaves (SH vs Skåne on a NW
            // chord). Keep the new point when it is a real geographic hop.
            let prev = filtered.last().map(|(_, q)| *q).unwrap_or(start);
            let geo = cheb(prev, p);
            if geo < LONG_TRIP_CHUNK_DEG * 0.75 {
                continue;
            }
        }
        let seg_target = next_progress_target(t);
        if (seg_target.0 - target.0).abs() > 1e-12 || (seg_target.1 - target.1).abs() > 1e-12 {
            target = seg_target;
            let prev = filtered.last().map(|(_, q)| *q).unwrap_or(start);
            best_to_end = cheb(prev, target);
        }
        let d_end = cheb(p, target);
        if !forced && d_end >= best_to_end - 1e-6 {
            continue;
        }
        // Inbound E6 joints use lat-based t; leaf centroids use OD progress_t.
        // A leaf at ~60.5°N can rank before Oslo (59.9°N) and force A* south
        // then north again (~140 km on Bevensen→Dalsøren). Drop non-forced hops
        // that sit north of a still-ahead forced spine/inbound joint.
        if !forced && end.0 > start.0 + 2.0 {
            let south_jog = forced_joints
                .iter()
                .any(|(ta, pa)| *ta > t + 1e-9 && pa.0 + 0.05 < p.0);
            if south_jog {
                continue;
            }
        }
        // Chebyshev-to-via can prefer Mecklenburg (further north, far east)
        // over SH/Fehmarn. Reject that hinterland on a northbound DE→Scandinavia
        // trip — not an extra via and not a hop stub.
        if !forced && densify_east_baltic_hinterland_point(start, end, p) {
            continue;
        }
        if !forced && densify_sorlandet_west_loop_point(start, end, p) {
            continue;
        }
        if !forced && densify_west_mjosa_wrong_side_point(start, end, p) {
            continue;
        }
        // Skip densify centroids that overshoot an *imminent* explicit via on
        // the approach axis. Vestlandet (60.75, 6.25) ranks just before
        // Sognefjell on NW OD progress_t but lies west of the via, forcing a
        // fjord loop then NE back (~350–520 km excess on Bevensen→Dalsøren).
        // Far vias (e.g. Lillehammer) must still allow Skåne/Halland land-bridge
        // swings that temporarily leave the via's meridian.
        if !forced {
            let prev = filtered.last().map(|(_, q)| *q).unwrap_or(start);
            if let Some(&(t_via, next_via)) = via_ts.iter().find(|(tv, _)| *tv > t + 1e-9) {
                if t_via - t < 0.15 && densify_centroid_overshoots_via(prev, p, next_via) {
                    continue;
                }
            }
        }
        filtered.push((t, p));
        last_t = t;
        if d_end < best_to_end {
            best_to_end = d_end;
        }
    }

    let mut out = Vec::with_capacity(filtered.len() + 2);
    out.push(start);
    out.extend(filtered.iter().map(|(_, p)| *p));
    out.push(end);

    // Subdivide remaining long gaps using Ready region centroids only — never
    // raw geometric midpoints (those land in Kattegat / Baltic and fail snap).
    if max_hop_deg > 0.0 {
        out = densify_gaps_with_region_centroids(&out, dirs, max_hop_deg);
    }
    // Pull leaf AABB centers that spike far off the neighbor envelope back
    // toward the land corridor (Skåne 13.5°E between Zealand/Halland ~12.7°E).
    // Soft pull — not OD-chord clamp — so Öresund fringe stays intact.
    let ready = densify_ready_with_leaf_proxies(collect_ready_region_entries_dirs(dirs));
    let mut forced = via_set;
    forced.extend(spine_set.iter().copied());
    smooth_densify_secondary_spikes(&mut out, &forced, &ready);
    out
}

/// Walk consecutive hops; when span exceeds `max_hop_deg`, insert the Ready
/// region centroid closest to the geometric midpoint (land-only proxy).
fn densify_gaps_with_region_centroids(
    points: &[(f64, f64)],
    dirs: &[&std::path::Path],
    max_hop_deg: f64,
) -> Vec<(f64, f64)> {
    let ready = densify_ready_with_leaf_proxies(collect_ready_region_entries_dirs(dirs));
    if ready.is_empty() {
        return points.to_vec();
    }
    // Prefer leaf centroids for gap fill, but keep country boxes for land checks
    // so SH→Skåne still densifies across Jutland (europe/denmark).
    let trip_start = points[0];
    let trip_end = *points.last().unwrap_or(&points[0]);
    let centroids: Vec<(f64, f64)> = ready
        .iter()
        .filter(|(path, _)| !densify_skip_country_when_leaves_ready(path, &ready))
        .filter(|(path, _)| {
            !densify_skip_east_baltic_hinterland_leaf(path, trip_start, trip_end, &ready)
        })
        .filter(|(path, _)| !densify_skip_sorlandet_west_loop_leaf(path, trip_start, trip_end))
        .map(|(path, b)| {
            let c = ((b[0] + b[2]) * 0.5, (b[1] + b[3]) * 0.5);
            prefer_densify_leaf_centroid(c, *b, path, &ready)
        })
        .filter(|c| !densify_east_baltic_hinterland_point(trip_start, trip_end, *c))
        .filter(|c| !densify_sorlandet_west_loop_point(trip_start, trip_end, *c))
        .collect();
    let mut out = Vec::with_capacity(points.len() * 2);
    out.push(points[0]);
    for w in points.windows(2) {
        let a = w[0];
        let b = w[1];
        insert_land_safe_mids(&mut out, a, b, &centroids, &ready, max_hop_deg, 0);
        out.push(b);
    }
    out
}

/// When a country pack is Ready but skipped because foreign leaves intersect its
/// sea-spilling AABB (and no own leaves are Ready), inject catalog leaf bboxes as
/// densify-only waypoints. Pack load still uses the country extract; this does not
/// require leaf downloads or force bridges/ferries.
fn densify_ready_with_leaf_proxies(mut ready: Vec<(String, [f64; 4])>) -> Vec<(String, [f64; 4])> {
    let snapshot = ready.clone();
    let mut seen: std::collections::HashSet<String> =
        snapshot.iter().map(|(p, _)| p.clone()).collect();
    for (path, _) in &snapshot {
        let mut parts = path.split('/');
        let (Some(_), Some(_), None) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        if !densify_skip_country_when_leaves_ready(path, &snapshot) {
            continue;
        }
        let own_prefix = format!("{path}/");
        if snapshot.iter().any(|(p, _)| p.starts_with(&own_prefix)) {
            continue;
        }
        for (leaf, bbox) in crate::routing::basemap::catalog_leaf_bboxes_under(path) {
            // Skip border-spill leaves that intersect a foreign Ready leaf AABB
            // (e.g. Hovedstaden∩Skåne). Those centroids pull densify onto
            // coastal/Øresund chords that snap-fail under the intermediate budget.
            if densify_leaf_intersects_foreign_ready(leaf, bbox, &snapshot) {
                continue;
            }
            if seen.insert(leaf.to_string()) {
                ready.push((leaf.to_string(), bbox));
            }
        }
    }
    ready
}

fn densify_leaf_intersects_foreign_ready(
    leaf_path: &str,
    leaf_bbox: [f64; 4],
    ready: &[(String, [f64; 4])],
) -> bool {
    let Some(home) = densify_region_country(leaf_path) else {
        return false;
    };
    // Skip leaves whose *centroid* sits on a foreign Ready leaf fringe
    // (Hovedstaden hugging Skåne). Full AABB intersection is too aggressive —
    // Sjælland's box nicks Skåne but its centroid is inland on Zealand.
    const BORDER_FRINGE_DEG: f64 = 0.25;
    let c = (
        (leaf_bbox[0] + leaf_bbox[2]) * 0.5,
        (leaf_bbox[1] + leaf_bbox[3]) * 0.5,
    );
    ready.iter().any(|(p, bb)| {
        if densify_region_country(p) == Some(home) || p.matches('/').count() < 2 {
            return false;
        }
        // MV's catalog AABB swallows the Baltic up to ~54.98°N / 14.4°E.
        // Sjælland's centroid (55.18°N) sits 0.19° north of that edge and was
        // treated as a "fringe" — the leaf proxy vanished, then SH→Skåne
        // even-split walked Lolland / Kalvehave instead of E47/Farø.
        if p.contains("mecklenburg-vorpommern") {
            return false;
        }
        let lat = c.0.clamp(bb[0], bb[2]);
        let lon = c.1.clamp(bb[1], bb[3]);
        (c.0 - lat).abs().max((c.1 - lon).abs()) < BORDER_FRINGE_DEG
    })
}

fn collect_ready_region_entries_dirs(dirs: &[&std::path::Path]) -> Vec<(String, [f64; 4])> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for data_dir in dirs {
        let Ok(entries) = std::fs::read_dir(data_dir) else {
            continue;
        };
        for ent in entries.flatten() {
            let name = ent.file_name();
            let name = name.to_string_lossy();
            let Some(stem) = name.strip_suffix(".navi-manifest.json") else {
                continue;
            };
            let Some(path) = crate::routing::basemap::pbf_stem_to_geofabrik_path(stem) else {
                continue;
            };
            if !seen.insert(path.clone()) {
                continue;
            }
            let Some(bbox) = crate::routing::basemap::region_bbox(&path) else {
                continue;
            };
            out.push((path, bbox));
        }
    }
    out
}

/// True when densify centroid `c` lies past explicit via `via` relative to `prev`
/// on the approach axis (or a large secondary-axis reverse).
///
/// progress_t on the OD chord can rank a sideways landsdel centroid *before* a
/// user via that is the better corridor anchor (Vestlandet before Sognefjell on
/// Bevensen→Dalsøren). Inserting that centroid forces a reverse hop back to the via.
///
/// Modest land-bridge dips on a secondary axis (Schleswig-Holstein west of a
/// near-meridian Lillehammer via on a lat-dominant Stendal→NO leg) must still
/// be allowed.
fn densify_centroid_overshoots_via(prev: (f64, f64), c: (f64, f64), via: (f64, f64)) -> bool {
    const MIN_OVERSHOOT_DEG: f64 = 0.35;
    const LARGE_SECONDARY_DEG: f64 = 1.0;
    let dlat = via.0 - prev.0;
    let dlon = via.1 - prev.1;
    let lon_over = (c.1 - via.1).abs();
    let lat_over = (c.0 - via.0).abs();
    let lon_past = dlon * (c.1 - via.1) > 0.0 && lon_over > MIN_OVERSHOOT_DEG;
    let lat_past = dlat * (c.0 - via.0) > 0.0 && lat_over > MIN_OVERSHOOT_DEG;
    if !lon_past && !lat_past {
        return false;
    }
    let primary_lon = dlon.abs() >= dlat.abs();
    if primary_lon && lon_past {
        return true;
    }
    if !primary_lon && lat_past {
        return true;
    }
    // Secondary-axis reverse past the via: only large absolute overshoots
    // (Vestlandet ~1.8° west of Sognefjell), not SH's ~0.6° land-bridge dip.
    if lon_past && lon_over > LARGE_SECONDARY_DEG && lon_over > dlon.abs() * 0.5 {
        return true;
    }
    if lat_past && lat_over > LARGE_SECONDARY_DEG && lat_over > dlat.abs() * 0.5 {
        return true;
    }
    false
}

/// Country extracts (`europe/denmark`) spill across Öresund into Sweden. Omit the
/// country centroid from densify when Ready leaf packs exist under that country,
/// or when a foreign leaf bbox intersects the country box (DK∩Skåne).
/// Skip Mecklenburg-Vorpommern as a densify leaf when SH + Scandinavia packs
/// are Ready and the trip is already west of Fehmarn heading north. The MV
/// AABB centroid (~54.04N, 12.51E) shrinks Chebyshev-to-via versus Bevensen
/// (further north wins) then gap-fill walks the Baltic hinterland (~1000 km
/// east loop). SH/Fehmarn (~11.0E) stays; this does not invent ferries or vias.
fn densify_skip_east_baltic_hinterland_leaf(
    path: &str,
    start: (f64, f64),
    end: (f64, f64),
    ready: &[(String, [f64; 4])],
) -> bool {
    if !path.contains("mecklenburg-vorpommern") {
        return false;
    }
    let sh_ready = ready.iter().any(|(p, _)| p.contains("schleswig-holstein"));
    sh_ready && densify_east_baltic_hinterland_trip(start, end)
}

fn densify_east_baltic_hinterland_trip(start: (f64, f64), end: (f64, f64)) -> bool {
    start.0 < 54.2 && start.1 < 11.25 && end.0 > 56.0
}

/// True when `p` is east of Fehmarn while still **south** of the SH/Fehmarn
/// joint on a northbound DE→Scandinavia densify (MV/Rostock hinterland).
/// Lolland/Falster (~54.56N, 11.7E) must stay — that is the Fehmarn Belt
/// corridor, not the east German loop.
fn densify_east_baltic_hinterland_point(start: (f64, f64), end: (f64, f64), p: (f64, f64)) -> bool {
    if !(p.0 < 54.25 && p.1 > 11.38) {
        return false;
    }
    densify_east_baltic_hinterland_trip(start, end)
        || (start.0 < 54.5 && start.1 < 11.25 && end.0 > start.0 + 0.2)
}

/// Skip Sørlandet as a densify leaf on DE→Innlandet northbound trips.
/// The landsdel AABB centroid (~58.65N, 7.75E) shrinks Chebyshev-to-via versus
/// Västra Götaland (further north wins) then A* walks Setesdal / Sørlandet
/// (~700 km west of E6) before climbing back to Ottadal/Vågå.
fn densify_skip_sorlandet_west_loop_leaf(path: &str, start: (f64, f64), end: (f64, f64)) -> bool {
    path.contains("norway/sorlandet") && densify_sorlandet_west_loop_trip(start, end)
}

fn densify_sorlandet_west_loop_trip(start: (f64, f64), end: (f64, f64)) -> bool {
    start.0 < 54.5 && start.1 < 12.0 && end.0 > 60.0
}

/// True when `p` sits in Sørlandet west of the E6 Oslo approach on a hop that
/// is already northbound from Sweden/DE toward Innlandet. Skien (~9.6E) and
/// Drammen stay; Kristiansand / Setesdal (~8E) do not.
fn densify_sorlandet_west_loop_point(start: (f64, f64), end: (f64, f64), p: (f64, f64)) -> bool {
    if !(p.0 >= 57.7 && p.0 <= 59.55 && p.1 < 9.35) {
        return false;
    }
    densify_sorlandet_west_loop_trip(start, end)
        || (start.0.max(end.0) > 60.0 && start.1.max(end.1) > 10.5)
}

/// True when `p` parks on the west shore of Mjøsa (Gjøvik / Rv4) on a
/// northbound DE/SE→Innlandet densify. E6 / bad-luster stay east (Hamar).
/// Lillehammer (~61.12N, 10.47E) is north of this band and is allowed.
fn densify_west_mjosa_wrong_side_point(start: (f64, f64), end: (f64, f64), p: (f64, f64)) -> bool {
    if !(p.0 >= 60.35 && p.0 <= 60.95 && p.1 >= 10.30 && p.1 <= 10.78) {
        return false;
    }
    densify_sorlandet_west_loop_trip(start, end) || (start.0 < 59.0 && end.0 > 60.5)
}

pub(crate) fn densify_skip_country_when_leaves_ready(
    path: &str,
    ready: &[(String, [f64; 4])],
) -> bool {
    let mut parts = path.split('/');
    let (Some(cont), Some(country), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    let own_prefix = format!("{cont}/{country}/");
    if ready.iter().any(|(p, _)| p.starts_with(&own_prefix)) {
        return true;
    }
    let Some((_, country_bbox)) = ready.iter().find(|(p, _)| p == path) else {
        return false;
    };
    ready.iter().any(|(p, leaf_bbox)| {
        if p.matches('/').count() < 2 || p.starts_with(&own_prefix) {
            return false;
        }
        leaf_bbox[0] <= country_bbox[2]
            && leaf_bbox[2] >= country_bbox[0]
            && leaf_bbox[1] <= country_bbox[3]
            && leaf_bbox[3] >= country_bbox[1]
    })
}

fn insert_land_safe_mids(
    out: &mut Vec<(f64, f64)>,
    a: (f64, f64),
    b: (f64, f64),
    centroids: &[(f64, f64)],
    ready: &[(String, [f64; 4])],
    max_hop_deg: f64,
    depth: u32,
) {
    let dlat = b.0 - a.0;
    let dlon = b.1 - a.1;
    let dist = dlat.abs().max(dlon.abs());
    if depth > 16 {
        return;
    }
    // Under-chunk Chebyshev hops can still be open-water chords (Øresund-class:
    // southern Zealand→Skåne is ~0.99° < 1.15° chunk). Corridor-band clip then
    // follows the sea line and drops the land approach (Køge / Copenhagen), so
    // O/D stay on different components despite Øresundsbron edges in both packs.
    // Split those water chords via an **origin-shore** land mid even when
    // dist ≤ max_hop_deg. Dest-shore even-splits (Lolland / Kalvehave / Møn)
    // were the regression that walked E47 off Farø after this session's
    // under-chunk patch — keep those hops unsplit so the band follows the
    // motorway/ferry instead of island coasts.
    let chord_mid = (a.0 + dlat * 0.5, a.1 + dlon * 0.5);
    let water_chord = densify_point_in_multi_country_spill(chord_mid, ready)
        || densify_cross_sea_chord(a, b, chord_mid, ready);
    if dist <= max_hop_deg + 1e-9 && !water_chord {
        return;
    }
    let v2 = dlat * dlat + dlon * dlon;
    if v2 < 1e-12 {
        return;
    }
    let cheb = |p: (f64, f64), q: (f64, f64)| (p.0 - q.0).abs().max((p.1 - q.1).abs());
    let proj_t = |c: (f64, f64)| ((c.0 - a.0) * dlat + (c.1 - a.1) * dlon) / v2;
    let perp_of = |c: (f64, f64), t: f64| {
        let proj = (a.0 + t * dlat, a.1 + t * dlon);
        (c.0 - proj.0).abs() + (c.1 - proj.1).abs()
    };

    // Walk forward along AB using 2D projection. Dominant-axis-only t wrongly
    // treated Schleswig-Holstein (north) as between Stendal and Niedersachsen
    // because their longitudes nest.
    let mut best: Option<(f64, (f64, f64))> = None;
    for &c in centroids {
        let t = proj_t(c);
        if t <= 0.05 || t >= 0.95 {
            continue;
        }
        if perp_of(c, t) > max_hop_deg * 0.85 {
            continue;
        }
        let da = cheb(c, a);
        let db = cheb(c, b);
        if da < max_hop_deg * 0.05
            || db < max_hop_deg * 0.05
            || da >= dist * 0.98
            || db >= dist * 0.98
        {
            continue;
        }
        // Must progress toward b — otherwise a hinterland centroid (e.g. western
        // Niedersachsen after Hamburg) can send a chunk hop south against the trip.
        if db >= dist * 0.98 {
            continue;
        }
        if densify_point_in_multi_country_spill(c, ready)
            && !densify_mid_on_origin_shore(c, a, ready)
        {
            continue;
        }
        if !densify_point_has_leaf_cover(c, ready) {
            continue;
        }
        if densify_reverses_hop_axis(a, b, c) {
            continue;
        }
        if densify_east_baltic_hinterland_point(a, b, c) {
            continue;
        }
        if densify_sorlandet_west_loop_point(a, b, c) {
            continue;
        }
        if best.is_none_or(|(bt, _)| t < bt) {
            best = Some((t, c));
        }
    }

    let Some(c) = best.map(|(_, c)| c).or_else(|| {
        // Geometric midpoint when it lies inside a Ready region. Reject mids that
        // sit in multi-country bbox spill (Baltic / Kattegat / Öresund) — catalog
        // AABBs cover water between shores; those endpoints disconnect under a
        // 4 GB tile budget. Applies to every OD, not only same-country hops.
        // Destination-leaf AABBs also cover the sea (Skåne⊃Øresund); those mids
        // look "on land" to spill/leaf checks and must not skip land-bridge.
        let mid = (a.0 + dlat * 0.5, a.1 + dlon * 0.5);
        let covering: Vec<[f64; 4]> = ready
            .iter()
            .filter(|(_, r)| crate::routing::basemap::bbox_covers_point(*r, mid.0, mid.1))
            .map(|(_, r)| *r)
            .collect();
        if !covering.is_empty()
            && !densify_point_in_multi_country_spill(mid, ready)
            && densify_point_has_leaf_cover(mid, ready)
            && !densify_cross_sea_chord(a, b, mid, ready)
            && !densify_intra_leaf_open_water_even_split(a, b, mid, ready)
        {
            // Large landsdel boxes (e.g. Nord-Norge) treat mountain plateaus as
            // "on land". For NE Stay-in-Country climbs, insert a due-north mid at
            // the western endpoint's longitude first so densify follows the coastal
            // meridian before swinging east.
            let mid = prefer_north_then_east_mid(mid, a, b, &covering);
            if !densify_point_in_multi_country_spill(mid, ready)
                && !densify_cross_sea_chord(a, b, mid, ready)
            {
                let da = cheb(mid, a);
                let db = cheb(mid, b);
                if da >= max_hop_deg * 0.2 && db >= max_hop_deg * 0.2 {
                    return Some(mid);
                }
            }
        }
        // Chord mid is open water / multi-country spill (or unusable). Prefer a
        // land-safe Ready/leaf candidate off the chord rather than inventing a
        // sea endpoint.
        densify_land_bridge_mid(a, b, centroids, ready, max_hop_deg)
    }) else {
        return;
    };

    // Require a strict Chebyshev split so recursion cannot micro-step forever.
    let da = cheb(c, a);
    let db = cheb(c, b);
    if da.max(db) >= dist - 1e-6 {
        return;
    }
    // Under-chunk water chords may only split onto the origin shore (Køge on
    // Zealand→Skåne). An even-split on the dest shore (Rødbyhavn, Sakskøbing,
    // Kalvehave, Råbylille Strand) pulls the 0.40° band off E47/Farø.
    if dist <= max_hop_deg + 1e-9 && water_chord && !densify_mid_on_origin_shore(c, a, ready) {
        return;
    }

    if let Some(prev) = out.last() {
        if (prev.0 - c.0).abs() < 1e-4 && (prev.1 - c.1).abs() < 1e-4 {
            return;
        }
    }
    if (c.0 - b.0).abs() < 1e-4 && (c.1 - b.1).abs() < 1e-4 {
        return;
    }
    insert_land_safe_mids(out, a, c, centroids, ready, max_hop_deg, depth + 1);
    out.push(c);
    insert_land_safe_mids(out, c, b, centroids, ready, max_hop_deg, depth + 1);
}

/// When a water chord mid is rejected, pick a land-safe densify candidate that
/// shortens the Chebyshev hop. Uses Ready/leaf centroids plus a coarse grid
/// inside leaf boxes (any country) so sea-spilling country AABBs are not the only
/// gap-fill option. Does not inject bridges, ferries, or road segments.
fn densify_land_bridge_mid(
    a: (f64, f64),
    b: (f64, f64),
    centroids: &[(f64, f64)],
    ready: &[(String, [f64; 4])],
    max_hop_deg: f64,
) -> Option<(f64, f64)> {
    let dlat = b.0 - a.0;
    let dlon = b.1 - a.1;
    let dist = dlat.abs().max(dlon.abs());
    let v2 = dlat * dlat + dlon * dlon;
    if v2 < 1e-12 {
        return None;
    }
    let cheb = |p: (f64, f64), q: (f64, f64)| (p.0 - q.0).abs().max((p.1 - q.1).abs());
    let mut candidates: Vec<(f64, f64)> = centroids.to_vec();
    candidates.extend(densify_leaf_grid_samples(a, b, ready, max_hop_deg));

    // Prefer the candidate that most evenly splits the hop (min max(da,db)),
    // allowing larger cross-track than the tight chord search — the OD chord is
    // the water line we are avoiding.
    let mut best_split: Option<(f64, f64, (f64, f64))> = None; // (max_sub, perp, c)
    for &c in &candidates {
        if densify_point_in_multi_country_spill(c, ready)
            && !densify_mid_on_origin_shore(c, a, ready)
        {
            continue;
        }
        if !densify_point_has_leaf_cover(c, ready) {
            continue;
        }
        if densify_reverses_hop_axis(a, b, c) {
            continue;
        }
        if densify_east_baltic_hinterland_point(a, b, c) {
            continue;
        }
        if densify_sorlandet_west_loop_point(a, b, c) {
            continue;
        }
        let t = ((c.0 - a.0) * dlat + (c.1 - a.1) * dlon) / v2;
        if t <= 0.02 || t >= 0.98 {
            continue;
        }
        let proj = (a.0 + t * dlat, a.1 + t * dlon);
        let perp = (c.0 - proj.0).abs() + (c.1 - proj.1).abs();
        if perp > max_hop_deg * 2.5 {
            continue;
        }
        // Grid samples of a sea-spilling leaf (sjaelland⊃Lolland/Møn) follow
        // the SH→Skåne water chord. Keep them near the hop like centroid
        // search does, so Kalvehave / Rødvig even-splits lose to E47/Farø.
        if !centroids
            .iter()
            .any(|q| (q.0 - c.0).abs() < 1e-6 && (q.1 - c.1).abs() < 1e-6)
            && perp > max_hop_deg * 0.85
        {
            continue;
        }
        let da = cheb(c, a);
        let db = cheb(c, b);
        let sub = da.max(db);
        // Require a *meaningful* split — tiny Chebyshev gains with reverse on a
        // secondary axis (e.g. south of SH while heading north) used to park
        // chunk ends on coastal grid samples and thrash A*.
        if sub >= dist * 0.92 || da < max_hop_deg * 0.15 || db < max_hop_deg * 0.15 {
            continue;
        }
        if best_split
            .is_none_or(|(bs, bp, _)| sub < bs - 1e-9 || ((sub - bs).abs() < 1e-9 && perp < bp))
        {
            best_split = Some((sub, perp, c));
        }
    }
    if let Some((_, _, c)) = best_split {
        return Some(c);
    }

    // Greedy land step from `a`: one chunk toward `b` along AB (0<t<1), preferring
    // the candidate closest to `b`. Used when no even split exists (long sea chords
    // where the first land entry is a hinterland detour in Chebyshev).
    let mut best_step: Option<(f64, f64, (f64, f64))> = None; // (db, t, c)
    for &c in &candidates {
        if densify_point_in_multi_country_spill(c, ready)
            && !densify_mid_on_origin_shore(c, a, ready)
        {
            continue;
        }
        if !densify_point_has_leaf_cover(c, ready) {
            continue;
        }
        if densify_reverses_hop_axis(a, b, c) {
            continue;
        }
        if densify_east_baltic_hinterland_point(a, b, c) {
            continue;
        }
        if densify_sorlandet_west_loop_point(a, b, c) {
            continue;
        }
        let t = ((c.0 - a.0) * dlat + (c.1 - a.1) * dlon) / v2;
        if t <= 0.02 || t >= 0.98 {
            continue;
        }
        let da = cheb(c, a);
        let db = cheb(c, b);
        if da < max_hop_deg * 0.15 || da > max_hop_deg + 1e-9 {
            continue;
        }
        if best_step
            .is_none_or(|(bdb, bt, _)| db < bdb - 1e-9 || ((db - bdb).abs() < 1e-9 && t > bt))
        {
            best_step = Some((db, t, c));
        }
    }
    best_step.map(|(_, _, c)| c)
}

/// True when `c` reverses past `a` on the hop's **dominant** axis.
///
/// Blocks southbound densify on a northbound land bridge (and the symmetric
/// cases). Secondary-axis reverse is allowed: Øresund-class water chords are
/// slightly east of Zealand, so the land mid (Køge) is west of `a` while the
/// hop's primary travel is north. Checking both axes dropped that land split
/// and left the sea chord in the 0.40° corridor band.
fn densify_reverses_hop_axis(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> bool {
    const AXIS_MIN: f64 = 0.25;
    const SLACK: f64 = 0.02;
    let dlat = b.0 - a.0;
    let dlon = b.1 - a.1;
    if dlat.abs() < AXIS_MIN && dlon.abs() < AXIS_MIN {
        return false;
    }
    if dlat.abs() >= dlon.abs() {
        let step = c.0 - a.0;
        if dlat > 0.0 && step < -SLACK {
            return true;
        }
        if dlat < 0.0 && step > SLACK {
            return true;
        }
    } else {
        let step = c.1 - a.1;
        if dlon > 0.0 && step < -SLACK {
            return true;
        }
        if dlon < 0.0 && step > SLACK {
            return true;
        }
    }
    false
}

/// Coarse interior samples of Ready **leaf** boxes that intersect hop AB.
/// Country-level AABBs are skipped (they often cover open sea).
fn densify_leaf_grid_samples(
    a: (f64, f64),
    b: (f64, f64),
    ready: &[(String, [f64; 4])],
    max_hop_deg: f64,
) -> Vec<(f64, f64)> {
    let hop = trip_bbox_points(&[a, b], max_hop_deg.max(0.5));
    let step = (max_hop_deg * 0.45).clamp(0.35, 0.55);
    let mut out = Vec::new();
    for (path, bbox) in ready {
        if path.matches('/').count() < 2 {
            continue;
        }
        if bbox[0] > hop[2] || bbox[2] < hop[0] || bbox[1] > hop[3] || bbox[3] < hop[1] {
            continue;
        }
        let mut lat = bbox[0] + step * 0.5;
        while lat < bbox[2] {
            let mut lon = bbox[1] + step * 0.5;
            while lon < bbox[3] {
                let p = (lat, lon);
                if densify_east_baltic_hinterland_point(a, b, p) {
                    lon += step;
                    continue;
                }
                if densify_sorlandet_west_loop_point(a, b, p) {
                    lon += step;
                    continue;
                }
                if crate::routing::basemap::bbox_covers_point(hop, p.0, p.1)
                    && !densify_point_in_multi_country_spill(p, ready)
                {
                    out.push(p);
                }
                lon += step;
            }
            lat += step;
        }
    }
    out
}

/// Coastal densify helpers for large landsdel catalog boxes.
///
/// Catalog centroids for Nord-Norge / Trøndelag sit inland; straight chord mids
/// land on mountain plateaus where chunk snap collapses (`zero_length_leg`) or
/// exceeds [`CHUNK_INTERMEDIATE_SNAP_M`]. Pad / tile widening peaks at 2.3–3.3 GiB
/// RSS and still fails — densify must prefer the E6 / coastal-highway spine.
///
/// Northern FI/SE transit packs that unlock the Bugøynes→Østlandet land bridge
/// (Pajala / Umeå class). Southern leftovers like Västra Götaland must not
/// match — those previously false-disabled the E6 spine.
#[allow(dead_code)]
fn ready_has_northern_scandinavia_transit(ready: &[(String, [f64; 4])]) -> bool {
    ready.iter().any(|(p, _)| {
        let p = p.as_str();
        p.contains("europe/finland")
            || p.contains("/norrbotten")
            || p.contains("/vasterbotten")
            || p.contains("/vasternorrland")
            || p.contains("/jamtland")
    })
}

/// Fair Bugøynes→Østlandet land corridor via Finnish Lapland then Norrbotten /
/// Västerbotten (Pajala–Umeå class, ~1944 km). Used when northern SE/FI packs
/// are Ready so densify does not chord Finnmark plateaus (geometric midpoints
/// `disconnected`) and does not force the coastal E6 Stay-in-Country spine
/// (~2670 km).
///
/// Sampled ~45 km along `geojson-routes/elsa-sjuvass.geojson` so consecutive
/// Chebyshev gaps stay ≤ ~0.90° (under [`LONG_TRIP_CHUNK_DEG`]) **and** each
/// hop stays near the real coastal/inland highway. A sparse spine whose first
/// joint sat ~1.1° off the Bugøynes exit chord left corridor-band A*
/// `disconnected` / `bbox_exhausted` on `chunk_leg1`. Finnish Lapland joints
/// require `europe/finland` Ready (adjacency PIP currently holes that area).
#[allow(dead_code)]
fn scandinavia_se_transit_spine_anchors(
    start: (f64, f64),
    end: (f64, f64),
) -> Vec<(f64, (f64, f64))> {
    let _ = (start, end);
    Vec::new()
}

/// Removed: place-specific inbound Oslo/Hamar/Lillehammer/Otta lists. Joint
/// cuts + path-vs-chord cover hop terminals.
fn norway_e6_inbound_anchors(
    points: &[(f64, f64)],
    ready: &[(String, [f64; 4])],
) -> Vec<(f64, (f64, f64))> {
    let _ = (points, ready);
    Vec::new()
}

/// Removed: Ottadal Lom / Sognefjell hard-coded densify anchors.
fn norway_ottadal_westbound_anchors(
    points: &[(f64, f64)],
    ready: &[(String, [f64; 4])],
) -> Vec<(f64, (f64, f64))> {
    let _ = (points, ready);
    Vec::new()
}

/// Removed: E6 Hamar–Tana and FI/SE transit spines.
fn norway_e6_spine_anchors(
    start: (f64, f64),
    end: (f64, f64),
    ready: &[(String, [f64; 4])],
) -> Vec<(f64, (f64, f64))> {
    let _ = (start, end, ready);
    Vec::new()
}

fn landsdel_box_needs_coastal_bias(bbox: [f64; 4]) -> bool {
    let lat_span = (bbox[2] - bbox[0]).abs();
    let lon_span = (bbox[3] - bbox[1]).abs();
    lat_span >= 2.5 && lon_span >= 4.0
}

/// Leaf densify centroid after geography-specific soft biases (Norway E6 spine,
/// Fehmarn entry on SH when Scandinavia packs are Ready).
fn prefer_densify_leaf_centroid(
    c: (f64, f64),
    bbox: [f64; 4],
    path: &str,
    ready: &[(String, [f64; 4])],
) -> (f64, f64) {
    let c = prefer_coastal_centroid(c, bbox, path);
    prefer_west_coast_e6_centroid(c, bbox, path, ready)
}

/// Østlandet AABB center (~60.65N, 10.50E) sits on the **west** shore of Mjøsa
/// (Gjøvik). bad-luster / E6 run the **east** shore (Hamar ~11.07E) then
/// Lillehammer. Bias the leaf densify centroid onto that motorway when SE packs
/// are Ready — densify geometry, not an extra via.
#[allow(dead_code)]
fn prefer_ostlandet_e6_centroid(
    c: (f64, f64),
    bbox: [f64; 4],
    path: &str,
    ready: &[(String, [f64; 4])],
) -> (f64, f64) {
    if !(path.contains("ostlandet") || path.ends_with("/ostlandet")) {
        return c;
    }
    if !scandinavia_leaf_ready(ready) {
        return c;
    }
    let lon_span = (bbox[3] - bbox[1]).abs();
    let lat_span = (bbox[2] - bbox[0]).abs();
    if lon_span < 2.0 || lat_span < 1.5 {
        return c;
    }
    // ~11.07°E / ~60.80°N: Hamar E6 east of Mjøsa (west edge 7.5 + 0.595*6).
    let e6_lon = bbox[1] + lon_span * 0.595;
    let e6_lat = bbox[0] + lat_span * 0.535;
    let lon = e6_lon.clamp(bbox[1] + 0.35, bbox[3] - 0.35);
    let lat = e6_lat.clamp(bbox[0] + 0.35, bbox[2] - 0.35);
    (lat, lon)
}

/// Skåne / Halland / Västra Götaland AABB centers sit inland of E6/E20
/// (Kristianstad E22 ~13.5°E, Borås/E45 ~12.9°E). A 0.40° corridor band around
/// those centroids excludes Gothenburg E6 (~11.97°E), so chunk A* is forced onto
/// hinterland roads (~200+ km vs the west-coast motorway). Bias west toward the
/// E6 spine when Scandinavia packs are Ready — same class of land-corridor
/// anchor as Fehmarn on SH, not an extra via.
#[allow(dead_code)]
fn prefer_west_coast_e6_centroid(
    c: (f64, f64),
    bbox: [f64; 4],
    path: &str,
    ready: &[(String, [f64; 4])],
) -> (f64, f64) {
    if !scandinavia_leaf_ready(ready) {
        return c;
    }
    let lon_frac = if path.contains("/sweden/skane") || path.ends_with("/skane") {
        // ~13.05°E: Malmö–Helsingborg E6, east of Öresund water (~12.45–12.70).
        0.28
    } else if path.contains("halland") {
        // ~12.50°E: Halmstad–Varberg E6, west of the AABB center (~12.70).
        0.38
    } else if path.contains("vastra_gotaland") || path.contains("vastra-gotaland") {
        // ~11.99°E: Gothenburg E6. Raw AABB center is ~12.88°E (Borås).
        0.27
    } else {
        return c;
    };
    let lon_span = (bbox[3] - bbox[1]).abs();
    if lon_span < 0.8 {
        return c;
    }
    let west_target = bbox[1] + lon_span * lon_frac;
    let lon = c.1.min(west_target).max(bbox[1] + 0.15);
    (c.0, lon.min(bbox[3] - 0.15))
}

/// Sjælland AABB center (~11.70°E) sits west of E47/Farø (~12.0°E) on Lolland
/// local roads (Sakskøbing). East-edge bias (~12.50°E, Stevns / Kalvehave) was
/// the opposite failure: corridor-band A* walked north-Zealand coasts instead
/// of E47. When SE packs are Ready, park on the Farø–Køge Bugt motorway so
/// Fehmarn→Øresund hops stay on the land/ferry trunk — not extra vias.
#[allow(dead_code)]
fn prefer_oresund_entry_centroid(
    c: (f64, f64),
    bbox: [f64; 4],
    path: &str,
    ready: &[(String, [f64; 4])],
) -> (f64, f64) {
    if !scandinavia_leaf_ready(ready) {
        return c;
    }
    let lon_span = (bbox[3] - bbox[1]).abs();
    let lat_span = (bbox[2] - bbox[0]).abs();
    if path.contains("sjaelland") {
        if lon_span < 1.0 || lat_span < 0.5 {
            return c;
        }
        // ~12.04°E / ~55.11°N: Farø–south Zealand E47, east of Lolland AABB
        // center, west of Stevns/Møn coasts, south of north-Zealand beaches.
        let e47_lon = bbox[1] + lon_span * 0.70;
        let e47_lat = bbox[0] + lat_span * 0.45;
        let lon = e47_lon.clamp(bbox[1] + 0.20, bbox[3] - 0.35);
        let lat = e47_lat.clamp(bbox[0] + 0.20, bbox[2] - 0.25);
        return (lat, lon);
    }
    if path.contains("hovedstaden") {
        // Kastrup / Øresundsbron approach (SE of the Hovedstaden box).
        let lat = bbox[0] + lat_span * 0.12;
        let lon = bbox[3] - 0.08;
        return (
            lat.clamp(bbox[0] + 0.02, bbox[2] - 0.02),
            lon.max(bbox[1] + 0.05),
        );
    }
    c
}

fn scandinavia_leaf_ready(ready: &[(String, [f64; 4])]) -> bool {
    ready.iter().any(|(p, _)| {
        p.contains("/sweden/")
            || p.contains("/norway/")
            || p == "europe/sweden"
            || p == "europe/norway"
    })
}

/// Move a northern landsdel centroid onto the E6 / coastal-highway spine.
///
/// Uses ~0.55 of a capped lon span from the west edge — not the far-west
/// third (0.35), which put Trøndelag anchors at ~10.25°E (Fosen fjords) while
/// E6 runs Steinkjer≈11.5°E. Cap the span so ocean-wide Nord-Norge boxes do
/// not pull anchors into the Norwegian Sea.
fn prefer_coastal_centroid(c: (f64, f64), bbox: [f64; 4], path: &str) -> (f64, f64) {
    // Only northern Norway landsdel packs — Ostlandet's west edge is Vestlandet
    // mountains/fjords, not the E6 spine. Empty `path` is allowed for gap-fill
    // covering lookups when the bbox itself is a northern landsdel box (lat≥62).
    let northern = path.contains("nord-norge")
        || path.contains("trondelag")
        || (path.is_empty() && bbox[0] >= 62.0);
    if !northern || !landsdel_box_needs_coastal_bias(bbox) {
        return c;
    }
    let lon_span = (bbox[3] - bbox[1]).abs();
    let spine_target = bbox[1] + lon_span.min(5.0) * 0.55;
    let floor = c.1 - 6.0;
    (c.0, c.1.min(spine_target).max(floor).max(bbox[1] + 0.5))
}

/// Schleswig-Holstein AABB center (~9.85°E near Kiel) steers densify onto the
/// Jutland/Funen Great Belt land bridge. When SE/NO packs are Ready (DE→
/// Scandinavia densify), bias the SH leaf east toward Fehmarn (~11.2°E) so the
/// corridor band can materialize the Puttgarden→Rødby ferry instead of a
/// ~180 km land detour. Pure DK-Jutland corridors (no SE/NO Ready) keep Kiel.
#[allow(dead_code)]
fn prefer_fehmarn_entry_centroid(
    c: (f64, f64),
    bbox: [f64; 4],
    path: &str,
    ready: &[(String, [f64; 4])],
) -> (f64, f64) {
    if !path.contains("schleswig-holstein") {
        return c;
    }
    if !scandinavia_leaf_ready(ready) {
        return c;
    }
    let lon_span = (bbox[3] - bbox[1]).abs();
    if lon_span < 1.0 {
        return c;
    }
    // ~0.90 of lon span ≈ 11.03°E — east of Kiel, approaching Fehmarn (11.23°E).
    let east_target = bbox[1] + lon_span * 0.90;
    let lon = c.1.max(east_target).min(bbox[3] - 0.05);
    (c.0, lon)
}

/// For NE gap-fill inside a large landsdel box, climb north while drifting
/// toward the E6 spine — not freezing at the western endpoint (that parked
/// hops on Fosen and `bbox_exhausted` / disconnected under Stay-in-Country).
///
/// Must not rewrite west/south approaches (Ottadal→Dalsøren class): applying
/// the 20%-of-dlon blend on negative `dlat` left a half-step mid (~8.78°E) that
/// forced a second mountain densify joint and a ~5× road/GC micro-hop
/// (~100–140 km excess on Bevensen→Dalsøren).
fn prefer_north_then_east_mid(
    geometric: (f64, f64),
    a: (f64, f64),
    b: (f64, f64),
    covering: &[[f64; 4]],
) -> (f64, f64) {
    let Some(bbox) = covering
        .iter()
        .min_by(|x, y| {
            let aa = (x[2] - x[0]).abs() * (x[3] - x[1]).abs();
            let bb = (y[2] - y[0]).abs() * (y[3] - y[1]).abs();
            aa.partial_cmp(&bb).unwrap_or(std::cmp::Ordering::Equal)
        })
        .copied()
    else {
        return geometric;
    };
    if !landsdel_box_needs_coastal_bias(bbox) {
        return geometric;
    }
    let dlat = b.0 - a.0;
    let dlon = b.1 - a.1;
    // Northbound climbs only (positive dlat). Westbound / southbound gap-fill
    // must keep the geometric mid so Chebyshev splits evenly.
    if dlat < 0.15 {
        return geometric;
    }
    let spine_lon = prefer_coastal_centroid(a, bbox, "").1;
    // Already primarily eastbound (Finnmark finale): keep geometric mid.
    if dlon.abs() >= dlat && a.1.min(b.1) >= spine_lon - 0.5 {
        return geometric;
    }
    // Climb north; blend toward the E6 spine and a fraction of geometric dlon
    // so the corridor progresses east without the inland catalog-centroid chord.
    let mid_lat = a.0 + dlat * 0.5;
    let mid_lon = a.1 + (spine_lon - a.1) * 0.65 + dlon * 0.20;
    (mid_lat, mid_lon)
}

fn densify_region_country(path: &str) -> Option<&str> {
    let mut parts = path.split('/');
    let _cont = parts.next()?;
    parts.next()
}

/// Soft-pull densify joints that spike far outside the lat/lon envelope of their
/// neighbors (catalog AABB centers sitting deep inland of a coastal land bridge).
///
/// Full OD-chord centroid bias was tried and reverted: Bevensen→Ottadal's chord
/// clamps to Skåne's west edge (Öresund) and disconnected chunk A*. Neighbor
/// envelope uses the already-chosen land hops (Zealand / Halland), so the pull
/// stays on the E6-side interior without forcing bridges or road segments.
fn smooth_densify_secondary_spikes(
    points: &mut [(f64, f64)],
    forced: &std::collections::HashSet<(u64, u64)>,
    ready: &[(String, [f64; 4])],
) {
    const SPIKE_DEG: f64 = 0.45;
    const EDGE_PAD: f64 = 0.25;
    if points.len() < 3 || ready.is_empty() {
        return;
    }
    for i in 1..points.len() - 1 {
        let cur = points[i];
        if forced.contains(&(cur.0.to_bits(), cur.1.to_bits())) {
            continue;
        }
        let prev = points[i - 1];
        let next = points[i + 1];
        let lon_lo = prev.1.min(next.1) - EDGE_PAD;
        let lon_hi = prev.1.max(next.1) + EDGE_PAD;
        let lat_lo = prev.0.min(next.0) - EDGE_PAD;
        let lat_hi = prev.0.max(next.0) + EDGE_PAD;
        let mut lat = cur.0;
        let mut lon = cur.1;
        let mut changed = false;
        // Soft half-step toward the envelope edge — enough to cut tens of km of
        // zigzag without parking on the multi-country spill fringe.
        if lon > lon_hi + SPIKE_DEG {
            lon = (lon + lon_hi) * 0.5;
            changed = true;
        } else if lon < lon_lo - SPIKE_DEG {
            lon = (lon + lon_lo) * 0.5;
            changed = true;
        }
        if lat > lat_hi + SPIKE_DEG {
            lat = (lat + lat_hi) * 0.5;
            changed = true;
        } else if lat < lat_lo - SPIKE_DEG {
            lat = (lat + lat_lo) * 0.5;
            changed = true;
        }
        if !changed {
            continue;
        }
        let p = (lat, lon);
        if densify_point_in_multi_country_spill(p, ready) || !densify_point_has_leaf_cover(p, ready)
        {
            continue;
        }
        points[i] = p;
    }
}

/// True when a Ready **leaf** (path depth ≥ 2) covers `pt`. Country AABBs alone
/// often include open water; densify endpoints must sit in a leaf box.
fn densify_point_has_leaf_cover(pt: (f64, f64), ready: &[(String, [f64; 4])]) -> bool {
    ready.iter().any(|(path, bbox)| {
        path.matches('/').count() >= 2
            && crate::routing::basemap::bbox_covers_point(*bbox, pt.0, pt.1)
    })
}

/// Reject densify points that sit in multi-country catalog bbox spill.
///
/// Country/leaf AABBs routinely cover open water between shores (Baltic,
/// Kattegat, Öresund, Channel approaches, etc.). A point covered by two or more
/// **countries** is treated as a water chord / disconnected mid for **any** OD —
/// not only same-country hops. Cross-sea densify must use land-safe leaf
/// centroids or grid samples instead of inventing geometric sea endpoints.
///
/// When a Ready **leaf** covers the point, foreign **country** AABBs that spill
/// over that leaf (Denmark⊃western Skåne) must not count — otherwise land-bridge
/// densify cannot place joints on the approached shore and Öresund-class hops
/// stay as one disconnected sea chord.
/// Keep origin-shore densify mids when the destination leaf AABB overlaps them
/// (Skåne⊃Kastrup). Those points are land on the approached country, not sea.
fn densify_mid_on_origin_shore(c: (f64, f64), a: (f64, f64), ready: &[(String, [f64; 4])]) -> bool {
    let ca = densify_leaf_countries_at(a, ready);
    let cc = densify_leaf_countries_at(c, ready);
    !ca.is_empty() && !ca.is_disjoint(&cc)
}

fn densify_leaf_countries_at(
    pt: (f64, f64),
    ready: &[(String, [f64; 4])],
) -> std::collections::HashSet<&str> {
    let mut out = std::collections::HashSet::new();
    for (path, bbox) in ready {
        if path.matches('/').count() < 2 {
            continue;
        }
        if !crate::routing::basemap::bbox_covers_point(*bbox, pt.0, pt.1) {
            continue;
        }
        if let Some(c) = densify_region_country(path) {
            out.insert(c);
        }
    }
    out
}

/// True when A/B sit in different Ready leaf countries and the chord mid is not
/// still on the origin shore. Skåne's leaf AABB spills west over Øresund water,
/// so [`densify_point_in_multi_country_spill`] treats that mid as "on land".
fn densify_cross_sea_chord(
    a: (f64, f64),
    b: (f64, f64),
    mid: (f64, f64),
    ready: &[(String, [f64; 4])],
) -> bool {
    let ca = densify_leaf_countries_at(a, ready);
    let cb = densify_leaf_countries_at(b, ready);
    if ca.is_empty() || cb.is_empty() || !ca.is_disjoint(&cb) {
        return false;
    }
    let cm = densify_leaf_countries_at(mid, ready);
    cm.is_disjoint(&ca)
}

/// Same-leaf catalog AABBs swallow intra-leaf sea (Lolland–Zealand inside
/// `europe/denmark/sjaelland`). A geometric even-split then looks "on land"
/// and parks hops on island coasts instead of the trunk (E47/Farø). Skip that
/// even-split so gap-fill uses leaf centroids. Large northern landsdel boxes
/// keep geometric/coastal mids via [`landsdel_box_needs_coastal_bias`].
fn densify_intra_leaf_open_water_even_split(
    a: (f64, f64),
    b: (f64, f64),
    mid: (f64, f64),
    ready: &[(String, [f64; 4])],
) -> bool {
    let hop = (a.0 - b.0).abs().max((a.1 - b.1).abs());
    if hop < 0.55 {
        return false;
    }
    ready.iter().any(|(path, bbox)| {
        if path.matches('/').count() < 2 {
            return false;
        }
        if landsdel_box_needs_coastal_bias(*bbox) {
            return false;
        }
        let lat_span = (bbox[2] - bbox[0]).abs();
        let lon_span = (bbox[3] - bbox[1]).abs();
        if lat_span.min(lon_span) < 0.8 {
            return false;
        }
        let covers = |p: (f64, f64)| crate::routing::basemap::bbox_covers_point(*bbox, p.0, p.1);
        covers(mid) && covers(a) && covers(b)
    })
}

fn densify_point_in_multi_country_spill(pt: (f64, f64), ready: &[(String, [f64; 4])]) -> bool {
    let mut leaf_countries = std::collections::HashSet::new();
    let mut country_only = std::collections::HashSet::new();
    for (path, bbox) in ready {
        if !crate::routing::basemap::bbox_covers_point(*bbox, pt.0, pt.1) {
            continue;
        }
        let Some(c) = densify_region_country(path) else {
            continue;
        };
        if path.matches('/').count() >= 2 {
            leaf_countries.insert(c);
        } else {
            country_only.insert(c);
        }
    }
    if !leaf_countries.is_empty() {
        return leaf_countries.len() >= 2;
    }
    country_only.len() >= 2
}

/// One padded bbox per consecutive point pair (start→via→…→end).
pub fn corridor_segment_bboxes(points: &[(f64, f64)], pad: f64) -> Vec<[f64; 4]> {
    if points.len() < 2 {
        return Vec::new();
    }
    points
        .windows(2)
        .map(|w| trip_bbox_points(&[w[0], w[1]], pad))
        .collect()
}

/// True when `tile` intersects any corridor segment bbox.
pub fn tile_intersects_corridor(tile: [f64; 4], segments: &[[f64; 4]]) -> bool {
    segments
        .iter()
        .any(|seg| tile[0] <= seg[2] && tile[2] >= seg[0] && tile[1] <= seg[3] && tile[3] >= seg[1])
}

/// How plan-time graph loads choose edge clip boxes.
///
/// [`Self::CorridorBand`] is the RAM-safe default (fixed half-width along the
/// OD chord). Pad schedule widens only the trip AABB used for **tile** picks —
/// it does **not** grow the band — so land-bridge detours outside
/// [`CORRIDOR_EDGE_HALF_WIDTH_DEG`] stay missing across every pad. After a
/// `disconnected` A* on that stable band, callers should retry with
/// [`Self::TripAabb`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlanEdgeClipMode {
    /// Small boxes along the OD polyline ([`corridor_band_bboxes`]).
    #[default]
    CorridorBand,
    /// Single trip AABB (`clip_bbox` / pad-expanded). Includes far cross-track
    /// detours that the band excludes.
    TripAabb,
}

/// Edge clip boxes for a plan load.
///
/// - [`PlanEdgeClipMode::CorridorBand`]: band along `route_points` when ≥2 stops;
///   otherwise `clip_bbox` alone.
/// - [`PlanEdgeClipMode::TripAabb`]: `clip_bbox` only (ignore band), so pad
///   widen actually expands materialization.
pub fn plan_edge_clips(
    route_points: Option<&[(f64, f64)]>,
    clip_bbox: Option<[f64; 4]>,
    mode: PlanEdgeClipMode,
) -> Option<Vec<[f64; 4]>> {
    match mode {
        PlanEdgeClipMode::TripAabb => clip_bbox.map(|b| vec![b]),
        PlanEdgeClipMode::CorridorBand => route_points
            .filter(|pts| pts.len() >= 2)
            .map(|pts| {
                corridor_band_bboxes(
                    pts,
                    effective_corridor_edge_half_width_deg(),
                    CORRIDOR_BAND_STEP_DEG,
                )
            })
            .filter(|b| !b.is_empty())
            .or_else(|| clip_bbox.map(|b| vec![b])),
    }
}

/// Build a corridor **band** of small axis-aligned clip boxes along `points`.
///
/// Unlike [`corridor_segment_bboxes`] (one fat AABB per hop), this samples the
/// polyline every `step_deg` and emits a `2 * half_width_deg` square at each
/// sample. Edge materialization that keeps edges touching **any** box therefore
/// follows the chord instead of filling the diagonal rectangle — critical for
/// 4 GB Automotive densify hops.
///
/// Endpoints use [`CORRIDOR_ENDPOINT_HALF_WIDTH_DEG`] so densify centroids keep
/// enough network for the intermediate snap budget.
///
/// **Not** parameterized by plan pad: widening [`plan_bbox_pad_schedule`] does
/// not change these boxes (see [`PlanEdgeClipMode::TripAabb`] fallback).
pub fn corridor_band_bboxes(
    points: &[(f64, f64)],
    half_width_deg: f64,
    step_deg: f64,
) -> Vec<[f64; 4]> {
    if points.is_empty() || half_width_deg <= 0.0 {
        return Vec::new();
    }
    let step = step_deg.max(1e-3);
    let end_w = CORRIDOR_ENDPOINT_HALF_WIDTH_DEG.max(half_width_deg);
    let mut out = Vec::new();
    let push = |out: &mut Vec<[f64; 4]>, lat: f64, lon: f64, w: f64| {
        out.push([lat - w, lon - w, lat + w, lon + w]);
    };
    push(&mut out, points[0].0, points[0].1, end_w);
    for w in points.windows(2) {
        let (a_lat, a_lon) = w[0];
        let (b_lat, b_lon) = w[1];
        let dlat = b_lat - a_lat;
        let dlon = b_lon - a_lon;
        let dist = dlat.abs().max(dlon.abs());
        let n = ((dist / step).ceil() as usize).max(1);
        for i in 1..n {
            let t = i as f64 / n as f64;
            push(&mut out, a_lat + dlat * t, a_lon + dlon * t, half_width_deg);
        }
        push(&mut out, b_lat, b_lon, end_w);
    }
    out
}

/// True when a `disconnected` A* on corridor-band materialization should retry
/// with [`PlanEdgeClipMode::TripAabb`]. Pad widen alone cannot fix that case:
/// band boxes ignore the pad schedule.
pub fn should_fallback_to_trip_aabb(mode: PlanEdgeClipMode, terminate: &str) -> bool {
    mode == PlanEdgeClipMode::CorridorBand && terminate == "disconnected"
}

#[cfg(test)]
mod trip_aabb_fallback_tests {
    use super::*;

    #[test]
    fn disconnected_corridor_band_falls_back_once() {
        assert!(should_fallback_to_trip_aabb(
            PlanEdgeClipMode::CorridorBand,
            "disconnected"
        ));
        assert!(
            !should_fallback_to_trip_aabb(PlanEdgeClipMode::TripAabb, "disconnected"),
            "second fail must not schedule another AABB/pad"
        );
    }
}

/// Perpendicular distance (degrees, Chebyshev-ish) from `point` to the infinite
/// line through `a`→`b`. Used to classify cross-track detours vs band half-width.
pub fn cross_track_deg(a: (f64, f64), b: (f64, f64), point: (f64, f64)) -> f64 {
    let (ax, ay) = (a.1, a.0); // lon, lat as x,y
    let (bx, by) = (b.1, b.0);
    let (px, py) = (point.1, point.0);
    let dx = bx - ax;
    let dy = by - ay;
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-18 {
        return (py - ay).abs().max((px - ax).abs());
    }
    // Distance from point to infinite line in lon/lat degrees.
    ((dx * (ay - py) - (ax - px) * dy).abs()) / len2.sqrt()
}

/// True when a point lies inside any clip box.
pub fn point_in_any_bbox(lat: f64, lon: f64, clips: &[[f64; 4]]) -> bool {
    clips
        .iter()
        .any(|b| lat >= b[0] && lat <= b[2] && lon >= b[1] && lon <= b[3])
}

fn points_bounds(points: &[(f64, f64)]) -> (f64, f64, f64, f64) {
    let mut min_lat = f64::INFINITY;
    let mut min_lon = f64::INFINITY;
    let mut max_lat = f64::NEG_INFINITY;
    let mut max_lon = f64::NEG_INFINITY;
    for &(lat, lon) in points {
        min_lat = min_lat.min(lat);
        min_lon = min_lon.min(lon);
        max_lat = max_lat.max(lat);
        max_lon = max_lon.max(lon);
    }
    if !min_lat.is_finite() {
        (0.0, 0.0, 0.0, 0.0)
    } else {
        (min_lat, min_lon, max_lat, max_lon)
    }
}

/// Sample densify joints along a road polyline so consecutive Chebyshev hops
/// stay ≤ `max_hop_deg`. Keeps endpoints.
pub fn sample_densify_joints_along_path(path: &[(f64, f64)], max_hop_deg: f64) -> Vec<(f64, f64)> {
    if path.len() < 2 || max_hop_deg <= 0.0 {
        return path.to_vec();
    }
    let cheb = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs().max((a.1 - b.1).abs());
    let mut out = Vec::with_capacity(8);
    out.push(path[0]);
    let mut last = path[0];
    let end = *path.last().unwrap();
    for &p in &path[1..path.len() - 1] {
        if cheb(last, p) >= max_hop_deg * 0.85 {
            out.push(p);
            last = p;
        }
    }
    // If the remaining hop to the destination is still too long, keep adding
    // the farthest path vertex that stays within budget until we can finish.
    while cheb(last, end) > max_hop_deg + 1e-9 {
        let mut best: Option<(f64, (f64, f64))> = None;
        for &p in path {
            let d_from = cheb(last, p);
            let d_end = cheb(p, end);
            if d_from < max_hop_deg * 0.4 || d_from > max_hop_deg + 1e-9 {
                continue;
            }
            if d_end >= cheb(last, end) - 1e-9 {
                continue;
            }
            if best.is_none_or(|(bd, _)| d_end < bd) {
                best = Some((d_end, p));
            }
        }
        let Some((_, nxt)) = best else {
            // No on-path progress — fall back to geometric densify of remainder.
            let rem = densify_route_points(&[last, end], max_hop_deg);
            out.extend(rem.into_iter().skip(1));
            return out;
        };
        out.push(nxt);
        last = nxt;
    }
    if out.last().copied() != Some(end) {
        out.push(end);
    }
    out
}

/// Build densify hops from a major-road + ferry coarse path on Ready packs.
/// Returns `None` when the skeleton graph cannot connect O→D.
pub fn try_densify_joints_via_skeleton_path(
    path_nodes_latlon: &[(f64, f64)],
    max_hop_deg: f64,
) -> Option<Vec<(f64, f64)>> {
    if path_nodes_latlon.len() < 2 {
        return None;
    }
    let hops = sample_densify_joints_along_path(path_nodes_latlon, max_hop_deg);
    if hops.len() < 2 {
        return None;
    }
    Some(hops)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::indexed::GRAPH_FORMAT_VERSION;

    #[test]
    fn schedule_starts_clamped_and_caps() {
        let pads = plan_bbox_pad_schedule(60.0, 10.0, 60.01, 10.01);
        assert!((pads[0] - PLAN_BBOX_PAD_MIN_DEG).abs() < 1e-9);
        assert!(*pads.last().unwrap() <= PLAN_BBOX_PAD_CAP_DEG + 1e-9);
        assert!(pads.windows(2).all(|w| w[1] >= w[0]));
    }

    #[test]
    fn schedule_is_finite() {
        let pads = plan_bbox_pad_schedule(50.0, 5.0, 70.0, 25.0);
        assert!(pads.len() <= 8);
        assert!((pads[0] - PLAN_BBOX_PAD_INITIAL_MAX_DEG).abs() < 1e-9);
    }

    #[test]
    fn points_bbox_includes_via() {
        let pts = [(60.0, 10.0), (61.0, 11.0), (60.5, 12.0)];
        let b = trip_bbox_points(&pts, 0.1);
        assert!((b[0] - 59.9).abs() < 1e-9);
        assert!((b[1] - 9.9).abs() < 1e-9);
        assert!((b[2] - 61.1).abs() < 1e-9);
        assert!((b[3] - 12.1).abs() < 1e-9);
    }

    #[test]
    fn corridor_segments_tighter_than_trip_aabb() {
        let pts = [
            (52.605766, 11.859277),
            (61.114545, 10.467007),
            (61.514623, 8.852972),
        ];
        let full = trip_bbox_points(&pts, CORRIDOR_TILE_PAD_DEG);
        let segs = corridor_segment_bboxes(&pts, CORRIDOR_TILE_PAD_DEG);
        assert_eq!(segs.len(), 2);
        let lon_full = full[3] - full[1];
        let lon_segs: f64 = segs.iter().map(|s| s[3] - s[1]).sum::<f64>() / segs.len() as f64;
        assert!(
            lon_segs < lon_full,
            "per-segment lon span {lon_segs} should beat full AABB {lon_full}"
        );
        // Magdeburg-ish tile far west of corridor must not match either segment.
        let west = [51.0, 6.0, 52.0, 7.0];
        assert!(!tile_intersects_corridor(west, &segs));
        // Tile near the Stendal→Lillehammer chord.
        let mid = [56.5, 10.5, 57.5, 11.5];
        assert!(tile_intersects_corridor(mid, &segs));
    }

    #[test]
    fn corridor_band_excludes_diagonal_aabb_corners() {
        // Bevensen-class NW hop: fat AABB covers NE/SW corners the band must not.
        let pts = [(53.079686, 10.587198), (54.168273, 9.728913)];
        let aabb = trip_bbox_points(&pts, CORRIDOR_TILE_PAD_DEG);
        let band = corridor_band_bboxes(&pts, CORRIDOR_EDGE_HALF_WIDTH_DEG, CORRIDOR_BAND_STEP_DEG);
        assert!(!band.is_empty());
        // NE corner of the hop AABB (east of Bevensen, north of start).
        let ne_lat = aabb[2] - 0.01;
        let ne_lon = aabb[3] - 0.01;
        assert!(
            !point_in_any_bbox(ne_lat, ne_lon, &band),
            "NE AABB corner ({ne_lat},{ne_lon}) must fall outside corridor band"
        );
        // Midpoint on the chord must stay inside.
        let mid = ((pts[0].0 + pts[1].0) * 0.5, (pts[0].1 + pts[1].1) * 0.5);
        assert!(
            point_in_any_bbox(mid.0, mid.1, &band),
            "chord midpoint must stay inside corridor band"
        );
    }

    #[test]
    fn pad_widen_does_not_expand_corridor_band_but_aabb_clip_does() {
        // Leg13-class hop (Sognefjell densify): land-bridge detour beyond
        // CORRIDOR_EDGE_HALF_WIDTH_DEG (0.40°) but still inside the trip AABB.
        let start = (61.375314, 8.657898);
        let end = (61.617086, 8.043864);
        let pts = [start, end];
        let pads = plan_bbox_pad_schedule_points(&pts);
        assert!(pads.len() >= 2);
        let aabb0 = trip_bbox_points(&pts, pads[0]);

        let mid = ((start.0 + end.0) * 0.5, (start.1 + end.1) * 0.5);
        let dlat = end.0 - start.0;
        let dlon = end.1 - start.1;
        let len = f64::sqrt(dlat * dlat + dlon * dlon);
        let (px, py) = (-dlat / len, dlon / len);
        // Pick the perp side that stays inside the narrowest pad AABB at 0.50°.
        let mut detour = None;
        for sign in [1.0_f64, -1.0_f64] {
            let cand = (mid.0 + py * 0.50 * sign, mid.1 + px * 0.50 * sign);
            let xt = cross_track_deg(start, end, cand);
            let in_aabb = cand.0 >= aabb0[0]
                && cand.0 <= aabb0[2]
                && cand.1 >= aabb0[1]
                && cand.1 <= aabb0[3];
            if xt > CORRIDOR_EDGE_HALF_WIDTH_DEG && in_aabb {
                detour = Some((cand, xt));
                break;
            }
        }
        let (detour, xt) = detour.expect("need a >0.40° cross-track point inside trip AABB");
        assert!(
            (xt - 0.50).abs() < 0.02,
            "expected ~0.50° cross-track, got {xt}"
        );

        let band = corridor_band_bboxes(&pts, CORRIDOR_EDGE_HALF_WIDTH_DEG, CORRIDOR_BAND_STEP_DEG);
        assert!(
            !point_in_any_bbox(detour.0, detour.1, &band),
            "cross-track detour must fall outside 0.40° corridor band"
        );

        // Pad schedule widens trip AABB but band boxes are identical (no pad arg).
        let band_again =
            corridor_band_bboxes(&pts, CORRIDOR_EDGE_HALF_WIDTH_DEG, CORRIDOR_BAND_STEP_DEG);
        assert_eq!(
            band, band_again,
            "corridor band must not depend on pad widen"
        );
        for &pad in &pads {
            let clips = plan_edge_clips(
                Some(&pts),
                Some(trip_bbox_points(&pts, pad)),
                PlanEdgeClipMode::CorridorBand,
            )
            .expect("band clips");
            assert!(
                !point_in_any_bbox(detour.0, detour.1, &clips),
                "pad={pad}: corridor-band mode still excludes detour"
            );
        }

        let aabb_clips =
            plan_edge_clips(Some(&pts), Some(aabb0), PlanEdgeClipMode::TripAabb).expect("aabb");
        assert_eq!(aabb_clips.len(), 1);
        assert!(
            point_in_any_bbox(detour.0, detour.1, &aabb_clips),
            "TripAabb fallback must materialize the cross-track detour"
        );
        assert!(should_fallback_to_trip_aabb(
            PlanEdgeClipMode::CorridorBand,
            "disconnected"
        ));
        assert!(!should_fallback_to_trip_aabb(
            PlanEdgeClipMode::TripAabb,
            "disconnected"
        ));
        assert!(!should_fallback_to_trip_aabb(
            PlanEdgeClipMode::CorridorBand,
            "snap_failed"
        ));
    }

    #[test]
    fn densify_skips_denmark_country_when_swedish_leaves_ready() {
        let dir = tempfile::tempdir().expect("tmpdir");
        for stem in [
            "denmark-latest",
            "skane-latest",
            "halland-latest",
            "vastra_gotaland-latest",
            "ostlandet-latest",
            "sachsen-anhalt-latest",
            "niedersachsen-latest",
            "hamburg-latest",
            "schleswig-holstein-latest",
        ] {
            let path = dir.path().join(format!("{stem}.navi-manifest.json"));
            std::fs::write(
                &path,
                format!(
                    r#"{{"schema":1,"stem":"{stem}","pbf_filename":"{stem}.osm.pbf","graph_files":{{}},"graph_format_version":{GRAPH_FORMAT_VERSION}}}"#
                ),
            )
            .unwrap();
        }
        let pts = [
            (52.605766, 11.859277), // Stendal
            (61.114545, 10.467007), // Lillehammer
            (61.514623, 8.852972),  // Bessheim
        ];
        let hops = densify_route_points_via_regions(&pts, dir.path(), LONG_TRIP_CHUNK_DEG);
        let oresund_mid = (56.08076_f64, 12.60140_f64);
        let hit = hops.iter().any(|(lat, lon)| {
            (lat - oresund_mid.0).abs() < 0.02 && (lon - oresund_mid.1).abs() < 0.05
        });
        assert!(
            !hit,
            "densify must not insert Skåne↔Denmark Öresund mid; hops={hops:?}"
        );
        let has_halland = hops
            .iter()
            .any(|(lat, lon)| (lat - 56.935).abs() < 0.05 && (lon - 12.50).abs() < 0.22);
        assert!(
            has_halland,
            "expected Halland leaf centroid on corridor; hops={hops:?}"
        );
        // SH→Skåne must still densify across Denmark (not one 3° hop).
        let sh = (54.210_f64, 9.845_f64);
        let sk = (55.910_f64, 13.525_f64);
        let between = hops.iter().any(|(lat, lon)| {
            *lat > sh.0 + 0.2 && *lat < sk.0 - 0.2 && *lon > sh.1 + 0.3 && *lon < sk.1 - 0.3
        });
        assert!(
            between,
            "expected Jutland/Zealand densify points between SH and Skåne; hops={hops:?}"
        );
    }

    /// Drammen→Berlevåg (Stay-in-Country / NO-only packs): large landsdel
    /// boxes used to accept inland geometric mids (mountain plateaus), which
    /// collapsed chunk snaps (`zero_length_leg`) or exceeded the intermediate
    /// snap budget — and raising pad to 2.8°/5.0° peaked at 2.3–3.3 GiB
    /// RSS. Coastal bias must keep Nordland densify mids west of the inland
    /// catalog chord while still subdividing the span.
    #[test]
    fn densify_drammen_berlevag_prefers_coastal_gap_fill() {
        let dir = tempfile::tempdir().expect("tmpdir");
        for stem in ["ostlandet-latest", "trondelag-latest", "nord-norge-latest"] {
            let path = dir.path().join(format!("{stem}.navi-manifest.json"));
            std::fs::write(
                &path,
                format!(
                    r#"{{"schema":1,"stem":"{stem}","pbf_filename":"{stem}.osm.pbf","graph_files":{{}},"graph_format_version":{GRAPH_FORMAT_VERSION}}}"#
                ),
            )
            .unwrap();
        }
        let start = (59.7401977_f64, 10.2015629_f64);
        let end = (70.8578156_f64, 29.0860363_f64);
        let hops = densify_route_points_via_regions(&[start, end], dir.path(), LONG_TRIP_CHUNK_DEG);
        assert!(
            hops.len() >= 10,
            "NO-only densify must subdivide this span; hops={}",
            hops.len()
        );
        let hard_spine = hops.iter().any(|(lat, lon)| {
            (*lat - 62.075).abs() < 0.02 && (*lon - 9.128).abs() < 0.02
                || (*lat - 69.472).abs() < 0.02 && (*lon - 25.511).abs() < 0.02
        });
        assert!(
            !hard_spine,
            "must not inject Hamar–Tana E6 town anchors; hops={hops:?}"
        );
        let fosen_trap = hops
            .iter()
            .any(|(lat, lon)| *lat > 63.5 && *lat < 65.0 && *lon < 10.7);
        assert!(
            !fosen_trap,
            "densify must not park hops west of E6 (Fosen) in Trøndelag; hops={hops:?}"
        );
        // Chunk legs: CHUNK_PAD_SCHEDULE_TAKE of the short-hop schedule → max pad 1.4° (not 5.0).
        let short = plan_bbox_pad_schedule(64.0, 11.5, 64.25, 12.08);
        let chunk: Vec<f64> = short.into_iter().take(CHUNK_PAD_SCHEDULE_TAKE).collect();
        assert_eq!(chunk.len(), CHUNK_PAD_SCHEDULE_TAKE);
        assert!((chunk[0] - PLAN_BBOX_PAD_MIN_DEG).abs() < 1e-9);
        assert!((chunk[2] - 1.4).abs() < 1e-9);
        assert!(
            *chunk.last().unwrap() < PLAN_BBOX_PAD_CAP_DEG - 1.0,
            "chunk take({CHUNK_PAD_SCHEDULE_TAKE}) must stop well below the full {PLAN_BBOX_PAD_CAP_DEG}° cap; got {chunk:?}"
        );
    }

    /// Bugøynes→Sjuvasslia (southbound Stay-in-Country): without the E6 spine
    /// the geometric chord cuts Finnmark plateaus and chunk legs disconnect
    /// (`bbox_exhausted` / `disconnected`). Spine must reverse for southbound
    /// when only Norway packs are Ready.
    #[test]
    fn densify_bugoynes_sjuvasslia_southbound_uses_e6_spine() {
        let dir = tempfile::tempdir().expect("tmpdir");
        // Include a leftover *southern* foreign Ready pack — must not suppress the spine.
        for stem in [
            "ostlandet-latest",
            "trondelag-latest",
            "nord-norge-latest",
            "vastra_gotaland-latest",
        ] {
            let path = dir.path().join(format!("{stem}.navi-manifest.json"));
            std::fs::write(
                &path,
                format!(
                    r#"{{"schema":1,"stem":"{stem}","pbf_filename":"{stem}.osm.pbf","graph_files":{{}},"graph_format_version":{GRAPH_FORMAT_VERSION}}}"#
                ),
            )
            .unwrap();
        }
        // Elsa's caravan & galleri (Bugøynes) → Sjuvasslia Camping.
        let start = (69.9741435_f64, 29.6337571_f64);
        let end = (59.803175_f64, 9.397871_f64);
        let hops = densify_route_points_via_regions(&[start, end], dir.path(), LONG_TRIP_CHUNK_DEG);
        assert!(
            hops.len() >= 10,
            "southbound NO densify must subdivide this span; hops={}",
            hops.len()
        );
        let alta_anchor = hops
            .iter()
            .any(|(lat, lon)| (*lat - 69.969).abs() < 0.02 && (*lon - 23.272).abs() < 0.02);
        let narvik_anchor = hops
            .iter()
            .any(|(lat, lon)| (*lat - 68.438).abs() < 0.02 && (*lon - 17.427).abs() < 0.02);
        assert!(
            !alta_anchor && !narvik_anchor,
            "must not inject E6 Alta/Narvik town anchors; hops={hops:?}"
        );
        assert_eq!(hops.first().copied(), Some(start));
        assert_eq!(hops.last().copied(), Some(end));
    }

    /// With northern Sweden Ready, Bugøynes→Sjuvasslia must use the SE transit
    /// spine (Pajala / Umeå class), not the coastal E6 Alta/Narvik anchors.
    #[test]
    fn densify_bugoynes_sjuvasslia_skips_e6_when_northern_se_ready() {
        let dir = tempfile::tempdir().expect("tmpdir");
        for stem in [
            "ostlandet-latest",
            "trondelag-latest",
            "nord-norge-latest",
            "norrbotten-latest",
            "vasterbotten-latest",
            "jamtland-latest",
        ] {
            let path = dir.path().join(format!("{stem}.navi-manifest.json"));
            std::fs::write(
                &path,
                format!(
                    r#"{{"schema":1,"stem":"{stem}","pbf_filename":"{stem}.osm.pbf","graph_files":{{}},"graph_format_version":{GRAPH_FORMAT_VERSION}}}"#
                ),
            )
            .unwrap();
        }
        let start = (69.9741435_f64, 29.6337571_f64);
        let end = (59.803175_f64, 9.397871_f64);
        let hops = densify_route_points_via_regions(&[start, end], dir.path(), LONG_TRIP_CHUNK_DEG);
        assert!(
            hops.len() >= 6,
            "SE-ready densify must still subdivide; hops={}",
            hops.len()
        );
        // E6 Alta / Narvik forced anchors must not appear once SE transit is Ready.
        let alta = hops
            .iter()
            .any(|(lat, lon)| *lat > 69.7 && *lat < 70.2 && *lon > 22.5 && *lon < 24.0);
        let narvik = hops
            .iter()
            .any(|(lat, lon)| *lat > 68.2 && *lat < 68.7 && *lon > 16.8 && *lon < 18.0);
        assert!(
            !alta && !narvik,
            "must not inject E6 Alta/Narvik town anchors; hops={hops:?}"
        );
        let pajala_anchor = hops
            .iter()
            .any(|(lat, lon)| (*lat - 67.20546).abs() < 0.03 && (*lon - 23.43596).abs() < 0.03);
        assert!(
            !pajala_anchor,
            "must not inject FI/SE transit spine joints; hops={hops:?}"
        );
    }

    #[test]
    fn densify_hamar_minden_keeps_skane_land_bridge() {
        let dir = tempfile::tempdir().expect("tmpdir");
        for stem in [
            "denmark-latest",
            "skane-latest",
            "halland-latest",
            "vastra_gotaland-latest",
            "ostlandet-latest",
            "niedersachsen-latest",
            "schleswig-holstein-latest",
            "detmold-regbez-latest",
        ] {
            let path = dir.path().join(format!("{stem}.navi-manifest.json"));
            std::fs::write(
                &path,
                format!(
                    r#"{{"schema":1,"stem":"{stem}","pbf_filename":"{stem}.osm.pbf","graph_files":{{}},"graph_format_version":{GRAPH_FORMAT_VERSION}}}"#
                ),
            )
            .unwrap();
        }
        let pts = [
            (60.7945, 11.0680), // Hamar
            (52.2885, 8.9167),  // Minden
        ];
        let hops = densify_route_points_via_regions(&pts, dir.path(), LONG_TRIP_CHUNK_DEG);
        let has_skane = hops
            .iter()
            .any(|(lat, lon)| *lat > 55.70 && *lat < 56.20 && *lon > 12.90 && *lon < 14.60);
        assert!(
            has_skane,
            "Skåne must stay on Hamar→Minden densify (pad≥3°); hops={hops:?}"
        );
        // No consecutive hop whose only land cover is Denmark country spill
        // between Halland and SH without a Swedish leaf endpoint.
        let halland = (56.935_f64, 12.70_f64);
        let has_halland_to_denmark_skip = hops.windows(2).any(|w| {
            let a_h = (w[0].0 - halland.0).abs() < 0.05 && (w[0].1 - halland.1).abs() < 0.15;
            let b_in_dk_only = w[1].0 > 54.5
                && w[1].0 < 57.5
                && w[1].1 > 8.0
                && w[1].1 < 12.5
                && (w[1].0 - 55.91).abs() > 0.3;
            a_h && b_in_dk_only
        });
        assert!(
            !has_halland_to_denmark_skip,
            "must not jump Halland→Denmark without Skåne; hops={hops:?}"
        );
    }

    /// Bevensen→Dalsøren with Sognefjell via: Vestlandet landsdel centroid
    /// (60.75, 6.25) ranks before the via on NW OD progress_t and used to force
    /// a west fjord loop then NE back (~350–520 km excess). Densify must keep
    /// the via as the corridor anchor and not insert that west reverse.
    #[test]
    fn densify_bevensen_dalsoren_skips_vestlandet_before_sognefjell_via() {
        let dir = tempfile::tempdir().expect("tmpdir");
        for stem in [
            "niedersachsen-latest",
            "mecklenburg-vorpommern-latest",
            "schleswig-holstein-latest",
            "denmark-latest",
            "skane-latest",
            "halland-latest",
            "vastra_gotaland-latest",
            "ostlandet-latest",
            "vestlandet-latest",
        ] {
            let path = dir.path().join(format!("{stem}.navi-manifest.json"));
            std::fs::write(
                &path,
                format!(
                    r#"{{"schema":1,"stem":"{stem}","pbf_filename":"{stem}.osm.pbf","graph_files":{{}},"graph_format_version":{GRAPH_FORMAT_VERSION}}}"#
                ),
            )
            .unwrap();
        }
        let bevensen = (53.079686_f64, 10.587198_f64);
        let sognefjell = (61.6170857_f64, 8.0438639_f64);
        let dalsoren = (61.4433766_f64, 7.4614016_f64);
        let vestlandet = (60.75_f64, 6.25_f64);

        let hops = densify_route_points_via_regions(
            &[bevensen, sognefjell, dalsoren],
            dir.path(),
            LONG_TRIP_CHUNK_DEG,
        );
        let has_vestlandet = hops.iter().any(|(lat, lon)| {
            (lat - vestlandet.0).abs() < 0.05 && (lon - vestlandet.1).abs() < 0.15
        });
        assert!(
            !has_vestlandet,
            "must not insert Vestlandet centroid before Sognefjell via; hops={hops:?}"
        );
        let has_via = hops.iter().any(|(lat, lon)| {
            (lat - sognefjell.0).abs() < 1e-6 && (lon - sognefjell.1).abs() < 1e-6
        });
        assert!(
            has_via,
            "Sognefjell via must remain an anchor; hops={hops:?}"
        );
        // No hop that swings west of the via then returns NE to it.
        let west_then_back = hops.windows(3).any(|w| {
            let mid_west = w[1].1 < sognefjell.1 - 0.35;
            let ends_at_via =
                (w[2].0 - sognefjell.0).abs() < 1e-6 && (w[2].1 - sognefjell.1).abs() < 1e-6;
            mid_west && ends_at_via && w[0].1 > sognefjell.1
        });
        assert!(
            !west_then_back,
            "densify must not create west reverse into Sognefjell; hops={hops:?}"
        );

        // Campaign plan vias (Landskrona → Ängelholm → Gothenburg → Sognefjell).
        let landskrona = (55.870_f64, 12.830_f64);
        let angelholm = (56.243_f64, 12.864_f64);
        let gothenburg = (57.708_f64, 11.975_f64);
        let hops_campaign = densify_route_points_via_regions(
            &[
                bevensen, landskrona, angelholm, gothenburg, sognefjell, dalsoren,
            ],
            dir.path(),
            LONG_TRIP_CHUNK_DEG,
        );
        let has_vestlandet_campaign = hops_campaign.iter().any(|(lat, lon)| {
            (lat - vestlandet.0).abs() < 0.05 && (lon - vestlandet.1).abs() < 0.15
        });
        assert!(
            !has_vestlandet_campaign,
            "campaign vias must not densify Vestlandet before Sognefjell; hops={hops_campaign:?}"
        );

        // User counterexample via (Ottadal corridor) must likewise not pull west.
        let ottadal = (61.8691419_f64, 9.1055130_f64);
        let hops_otta = densify_route_points_via_regions(
            &[bevensen, ottadal, dalsoren],
            dir.path(),
            LONG_TRIP_CHUNK_DEG,
        );
        let has_vestlandet_otta = hops_otta.iter().any(|(lat, lon)| {
            (lat - vestlandet.0).abs() < 0.05 && (lon - vestlandet.1).abs() < 0.15
        });
        assert!(
            !has_vestlandet_otta,
            "must not insert Vestlandet before Ottadal via; hops={hops_otta:?}"
        );
    }

    /// Gap-fill must not invent the SH→Halland Baltic geometric chord
    /// `(54.89125, 10.55875)` (multi-country DE∩DK water). Leaf proxies +
    /// land-bridge retry keep densify on land-safe candidates for any OD that
    /// would otherwise mid-hop the sea.
    #[test]
    fn densify_rejects_baltic_water_chord_mid_on_de_se_corridor() {
        let dir = tempfile::tempdir().expect("tmpdir");
        for stem in [
            "niedersachsen-latest",
            "mecklenburg-vorpommern-latest",
            "schleswig-holstein-latest",
            "denmark-latest",
            "skane-latest",
            "halland-latest",
            "vastra_gotaland-latest",
            "ostlandet-latest",
            "vestlandet-latest",
            "sorlandet-latest",
        ] {
            let path = dir.path().join(format!("{stem}.navi-manifest.json"));
            std::fs::write(
                &path,
                format!(
                    r#"{{"schema":1,"stem":"{stem}","pbf_filename":"{stem}.osm.pbf","graph_files":{{}},"graph_format_version":{GRAPH_FORMAT_VERSION}}}"#
                ),
            )
            .unwrap();
        }
        let bevensen = (53.079686_f64, 10.587198_f64);
        let otta = (61.8691419_f64, 9.1055130_f64);
        let dalsoren = (61.4433766_f64, 7.4614016_f64);
        let hops = densify_route_points_via_regions(
            &[bevensen, otta, dalsoren],
            dir.path(),
            LONG_TRIP_CHUNK_DEG,
        );
        let baltic_mid = hops
            .iter()
            .any(|(lat, lon)| (lat - 54.89125).abs() < 1e-4 && (lon - 10.55875).abs() < 1e-4);
        assert!(
            !baltic_mid,
            "must not insert SH→Halland Baltic water mid; hops={hops:?}"
        );
        // Must not park hops on the geometric SH→Halland sea chord (the former
        // failing mid and its recursive halves). Fehmarn entry bias moves the
        // SH densify joint east (~11.03°E).
        let sh = (54.21_f64, 11.025_f64);
        let halland = (56.935_f64, 12.7_f64);
        let baltic_chain = [
            (54.89125_f64, 10.55875_f64),
            (55.5725, 11.2725),
            (56.25375, 11.98625),
        ];
        for &(blat, blon) in &baltic_chain {
            let hit = hops
                .iter()
                .any(|(lat, lon)| (lat - blat).abs() < 1e-3 && (lon - blon).abs() < 1e-3);
            assert!(
                !hit,
                "must not insert SH→Halland water chord point ({blat}, {blon}); hops={hops:?}"
            );
        }
        // Land-bridge must not step south of SH on a northbound corridor.
        let south_of_sh = hops.windows(2).any(|w| {
            (w[0].0 - sh.0).abs() < 1e-3 && (w[0].1 - sh.1).abs() < 0.08 && w[1].0 < sh.0 - 0.05
        });
        assert!(
            !south_of_sh,
            "must not densify south of SH on northbound trip; hops={hops:?}"
        );
        let hovedstaden = hops
            .iter()
            .any(|(lat, lon)| (lat - 55.855).abs() < 0.05 && (lon - 12.35).abs() < 0.15);
        assert!(
            !hovedstaden,
            "border-spill leaf proxies (Hovedstaden∩Skåne) must not be densify hops; hops={hops:?}"
        );
        let _ = (sh, halland);
        let sh_fehmarn_bias = hops
            .iter()
            .any(|(lat, lon)| (*lat - 54.21).abs() < 0.05 && (*lon - 11.025).abs() < 0.08);
        assert!(
            !sh_fehmarn_bias,
            "must not inject SH Fehmarn lon-bias densify; hops={hops:?}"
        );
        let faro_frac = hops
            .iter()
            .any(|(lat, lon)| (*lat - 55.11).abs() < 0.04 && (*lon - 12.04).abs() < 0.08);
        assert!(
            !faro_frac,
            "must not inject Farø AABB-fraction densify; hops={hops:?}"
        );
        let has_skane = hops
            .iter()
            .any(|(lat, lon)| *lat > 55.32 && *lat < 56.50 && *lon > 12.45 && *lon < 14.60);
        assert!(
            has_skane,
            "Skåne leaf must survive t-dedup on NW OD densify; hops={hops:?}"
        );
        // Neighbor-envelope soft-pull must not leave the raw AABB center
        // (13.525°E) when Zealand/Halland neighbors sit near ~12.7°E.
        let deep_skane_center = hops
            .iter()
            .any(|(lat, lon)| (lat - 55.91).abs() < 0.05 && (lon - 13.525).abs() < 0.08);
        assert!(
            !deep_skane_center,
            "Skåne densify must soft-pull off raw AABB center; hops={hops:?}"
        );
        assert!(
            hops.len() >= 6,
            "corridor must still densify into multiple land hops; hops={}",
            hops.len()
        );
        // Øresund-class: must not leave a single >chunk hop from Zealand to raw
        // Skåne center — land-bridge must use Skåne leaf interior despite DK spill.
        let long_sea = hops.windows(2).any(|w| {
            let dlat = (w[1].0 - w[0].0).abs();
            let dlon = (w[1].1 - w[0].1).abs();
            let cheb = dlat.max(dlon);
            cheb > LONG_TRIP_CHUNK_DEG + 1e-6
                && w[0].0 > 55.4
                && w[0].0 < 56.0
                && w[0].1 > 11.8
                && w[0].1 < 12.5
                && w[1].0 > 55.7
                && w[1].1 > 13.2
        });
        assert!(
            !long_sea,
            "must densify Zealand→Skåne across leaf land, not one sea chord; hops={hops:?}"
        );
        // Under-chunk Chebyshev water chords (southern Zealand→soft-pulled Skåne
        // ≈0.99° < 1.15°) must still pick a land mid so corridor band is not a
        // sea line that clips Køge (xt≈0.52° > 0.40° half-width).
        // Exception: origin already on E47/Farø (~12.04°E) — that chord keeps
        // Køge inside the 0.40° band; splitting it re-introduced dest-shore
        // Kalvehave/Skåne-SW even-splits.
        let under_chunk_sea = hops.windows(2).any(|w| {
            let dlat = (w[1].0 - w[0].0).abs();
            let dlon = (w[1].1 - w[0].1).abs();
            let cheb = dlat.max(dlon);
            let origin_on_e47_faro =
                w[0].0 > 54.85 && w[0].0 < 55.40 && w[0].1 > 11.85 && w[0].1 < 12.25;
            cheb <= LONG_TRIP_CHUNK_DEG + 1e-6
                && cheb > 0.6
                && w[0].0 > 54.7
                && w[0].0 < 55.4
                && w[0].1 > 11.9
                && w[0].1 < 12.6
                && w[1].0 > 55.7
                && w[1].1 > 12.9
                && !origin_on_e47_faro
        });
        assert!(
            !under_chunk_sea,
            "must split under-chunk Øresund water chords via land densify; hops={hops:?}"
        );
        // After Øresund, densify must stay on Skåne/Halland/VG land (E6). The
        // campaign FAIL loaded DK t2_3/t2_4/t3_3/t3_4 + Halland t1_0 because
        // Chebyshev-to-Dalsøren dropped VG and gap-fill hopped Halland west
        // into Kattegat (~57.85N, 11.78E) — not Helsingborg, not a missing
        // Frederikshavn ferry, not a pack node-id mismatch.
        let kattegat_jump = hops.windows(2).any(|w| {
            let halland_end =
                |p: (f64, f64)| p.0 > 56.32 && p.0 < 57.55 && (p.1 - 12.70).abs() < 0.40;
            let kattegat = |p: (f64, f64)| p.0 > 56.5 && p.0 < 58.2 && p.1 > 10.5 && p.1 < 11.90;
            (halland_end(w[0]) && kattegat(w[1])) || (kattegat(w[0]) && halland_end(w[1]))
        });
        assert!(
            !kattegat_jump,
            "must not hop Halland→Kattegat; follow VG/E6 land; hops={hops:?}"
        );
        let has_vg_land = hops
            .iter()
            .any(|(lat, lon)| *lat > 57.15 && *lat < 59.36 && *lon > 11.70 && *lon < 12.40);
        assert!(
            has_vg_land,
            "Västra Götaland land hop required on west-coast E6 (not Borås/E45); hops={hops:?}"
        );
        let inland_e45 = hops
            .iter()
            .any(|(lat, lon)| *lat > 57.2 && *lat < 59.1 && *lon > 12.55);
        assert!(
            !inland_e45,
            "must not densify VG inland of E6 (E45/Borås); hops={hops:?}"
        );
        let skane_e22 = hops
            .iter()
            .any(|(lat, lon)| *lat > 55.5 && *lat < 56.4 && *lon > 13.25);
        assert!(
            !skane_e22,
            "must not densify Skåne onto E22/AABB inland; hops={hops:?}"
        );
        // Corridor-band clip follows densify hops. Raw VG AABB (12.88°E) left
        // Gothenburg E6 ~0.8° west of the chord (outside 0.40° half-width).
        let band =
            corridor_band_bboxes(&hops, CORRIDOR_EDGE_HALF_WIDTH_DEG, CORRIDOR_BAND_STEP_DEG);
        let gothenburg_e6 = (57.7089_f64, 11.9746_f64);
        assert!(
            point_in_any_bbox(gothenburg_e6.0, gothenburg_e6.1, &band),
            "west-coast densify must keep Gothenburg E6 inside the 0.40° band; hops={hops:?}"
        );
        let boras_e45 = (57.7210_f64, 12.9401_f64);
        assert!(
            !point_in_any_bbox(boras_e45.0, boras_e45.1, &band),
            "inland Borås/E45 must stay outside the E6 corridor band; hops={hops:?}"
        );
        let mv_east = hops.iter().any(|(lat, lon)| *lat < 54.25 && *lon > 11.38);
        assert!(
            !mv_east,
            "must not densify east of Fehmarn through Mecklenburg; hops={hops:?}"
        );
        let sorlandet_west = hops
            .iter()
            .any(|(lat, lon)| *lat >= 57.7 && *lat <= 59.55 && *lon < 9.35);
        assert!(
            !sorlandet_west,
            "must not densify Sørlandet west of E6 on DE→Ottadal; hops={hops:?}"
        );
    }

    /// Bevensen→Vågå densify must keep the Fehmarn–E47–Farø trunk. A reverted
    /// MV-fringe skip dropped the Sjælland leaf proxy; SH→Skåne even-split then
    /// parked hops on Lolland (Sakskøbing–Rødbyhavn) and Kalvehave / Rødvig /
    /// north-Zealand strand. Production densify must reject that hop shape.
    #[test]
    fn densify_bevensen_vagaa_does_not_even_split_onto_zealand_coasts() {
        let dir = tempfile::tempdir().expect("tmpdir");
        for stem in [
            "niedersachsen-latest",
            "mecklenburg-vorpommern-latest",
            "schleswig-holstein-latest",
            "denmark-latest",
            "skane-latest",
            "halland-latest",
            "vastra_gotaland-latest",
            "ostlandet-latest",
            "vestlandet-latest",
            "sorlandet-latest",
        ] {
            let path = dir.path().join(format!("{stem}.navi-manifest.json"));
            std::fs::write(
                &path,
                format!(
                    r#"{{"schema":1,"stem":"{stem}","pbf_filename":"{stem}.osm.pbf","graph_files":{{}},"graph_format_version":{GRAPH_FORMAT_VERSION}}}"#
                ),
            )
            .unwrap();
        }
        let hops = densify_route_points_via_regions(
            &[
                (53.079686_f64, 10.587198_f64),
                (61.8691419, 9.1055130),
                (61.4433766, 7.4614016),
            ],
            dir.path(),
            LONG_TRIP_CHUNK_DEG,
        );
        let lolland_loop = hops
            .iter()
            .any(|(lat, lon)| *lat > 54.58 && *lat < 54.88 && *lon > 11.15 && *lon < 11.72);
        assert!(
            !lolland_loop,
            "must not even-split onto Lolland west of E47; hops={hops:?}"
        );
        let kalvehave = hops
            .iter()
            .any(|(lat, lon)| *lat > 54.88 && *lat < 55.22 && *lon > 12.12 && *lon < 12.55);
        assert!(
            !kalvehave,
            "must not even-split onto Kalvehave/Møn/Rødvig; hops={hops:?}"
        );
        let north_zealand_strand = hops
            .iter()
            .any(|(lat, lon)| *lat > 55.85 && *lat < 56.22 && *lon > 11.85 && *lon < 12.45);
        assert!(
            !north_zealand_strand,
            "must not densify Rågeleje/north-Zealand strand; hops={hops:?}"
        );
        let faro_frac = hops
            .iter()
            .any(|(lat, lon)| (*lat - 55.11).abs() < 0.04 && (*lon - 12.04).abs() < 0.08);
        assert!(
            !faro_frac,
            "must not inject Farø AABB-fraction densify; hops={hops:?}"
        );
        assert!(hops.len() >= 8, "Bevensen densify must subdivide; hops={}", hops.len());
    }

    /// Bevensen→Vågå densify must follow E6 Oslo→Otta east of Mjøsa (bad-luster),
    /// not the Østlandet AABB west-shore centroid (Gjøvik). Otta appears only as
    /// densify geometry so Otta→Vågå stays on Ottadalsvegen — not a user via.
    #[test]
    fn densify_bevensen_vagaa_follows_e6_east_of_mjosa_to_otta() {
        let dir = tempfile::tempdir().expect("tmpdir");
        for stem in [
            "niedersachsen-latest",
            "mecklenburg-vorpommern-latest",
            "schleswig-holstein-latest",
            "denmark-latest",
            "skane-latest",
            "halland-latest",
            "vastra_gotaland-latest",
            "ostlandet-latest",
            "vestlandet-latest",
        ] {
            let path = dir.path().join(format!("{stem}.navi-manifest.json"));
            std::fs::write(
                &path,
                format!(
                    r#"{{"schema":1,"stem":"{stem}","pbf_filename":"{stem}.osm.pbf","graph_files":{{}},"graph_format_version":{GRAPH_FORMAT_VERSION}}}"#
                ),
            )
            .unwrap();
        }
        let bevensen = (53.079686_f64, 10.587198_f64);
        let vagaa = (61.8691419_f64, 9.1055130_f64);
        let dalsoren = (61.4433766_f64, 7.4614016_f64);
        let hops = densify_route_points_via_regions(
            &[bevensen, vagaa, dalsoren],
            dir.path(),
            LONG_TRIP_CHUNK_DEG,
        );
        let inbound = hops.iter().any(|(lat, lon)| {
            (*lat - 59.910).abs() < 0.02 && (*lon - 10.750).abs() < 0.02
                || (*lat - 60.795).abs() < 0.02 && (*lon - 11.068).abs() < 0.02
                || (*lat - 61.115).abs() < 0.02 && (*lon - 10.466).abs() < 0.02
                || (*lat - 61.772).abs() < 0.02 && (*lon - 9.420).abs() < 0.02
        });
        assert!(
            !inbound,
            "must not inject Oslo/Hamar/Lillehammer/Otta inbound anchors; hops={hops:?}"
        );
        let via_i = hops
            .iter()
            .position(|(lat, lon)| (lat - vagaa.0).abs() < 1e-6 && (lon - vagaa.1).abs() < 1e-6)
            .expect("Vågå via must remain an anchor");
        let after = &hops[via_i..];
        let lom = after
            .iter()
            .any(|(lat, lon)| (*lat - 61.838).abs() < 0.02 && (*lon - 8.569).abs() < 0.02);
        assert!(
            !lom,
            "must not inject Lom Ottadal densify; after_via={after:?} hops={hops:?}"
        );
        // Explicit user vias are only Vågå — Otta/Lom are densify joints only.
        let via_count = hops
            .iter()
            .filter(|(lat, lon)| (lat - vagaa.0).abs() < 1e-6 && (lon - vagaa.1).abs() < 1e-6)
            .count();
        assert_eq!(via_count, 1, "Vågå via must appear once; hops={hops:?}");
    }

    /// Ottadal→Dalsøren must keep Ottadalsvegen through Lom (bad-luster), not a
    /// south-of-valley even-split chord and not the former NE-climb half-step
    /// mid (~8.78°E) that produced a ~5× road/GC micro-hop.
    #[test]
    fn densify_ottadal_dalsoren_keeps_ottadalsvegen_through_lom() {
        let dir = tempfile::tempdir().expect("tmpdir");
        for stem in [
            "niedersachsen-latest",
            "mecklenburg-vorpommern-latest",
            "schleswig-holstein-latest",
            "denmark-latest",
            "skane-latest",
            "halland-latest",
            "vastra_gotaland-latest",
            "ostlandet-latest",
            "vestlandet-latest",
        ] {
            let path = dir.path().join(format!("{stem}.navi-manifest.json"));
            std::fs::write(
                &path,
                format!(
                    r#"{{"schema":1,"stem":"{stem}","pbf_filename":"{stem}.osm.pbf","graph_files":{{}},"graph_format_version":{GRAPH_FORMAT_VERSION}}}"#
                ),
            )
            .unwrap();
        }
        let bevensen = (53.079686_f64, 10.587198_f64);
        let ottadal = (61.8691419_f64, 9.1055130_f64);
        let dalsoren = (61.4433766_f64, 7.4614016_f64);
        let hops = densify_route_points_via_regions(
            &[bevensen, ottadal, dalsoren],
            dir.path(),
            LONG_TRIP_CHUNK_DEG,
        );
        let via_i = hops
            .iter()
            .position(|(lat, lon)| (lat - ottadal.0).abs() < 1e-6 && (lon - ottadal.1).abs() < 1e-6)
            .expect("Ottadal via must remain an anchor");
        let after = &hops[via_i..];
        let lom = after
            .iter()
            .any(|(lat, lon)| (*lat - 61.838).abs() < 0.02 && (*lon - 8.569).abs() < 0.02);
        assert!(
            !lom,
            "must not inject Lom Ottadal densify; after_via={after:?} hops={hops:?}"
        );
        // Reject the former NE-climb half-step mid (~8.777°E).
        let half_step = after
            .iter()
            .any(|(lat, lon)| (lat - 61.65626).abs() < 0.02 && (lon - 8.77669).abs() < 0.05);
        assert!(
            !half_step,
            "must not insert NE-climb half-step mid on westbound via→dest; after_via={after:?}"
        );
        assert_eq!(after.last().copied(), Some(dalsoren));
    }

    /// Bevensen→Ottadal densify must soft-pull Skåne off the raw AABB center
    /// (~13.53°E) toward the Zealand/Halland neighbor envelope without parking
    /// on the Öresund west fringe (that OD-chord bias disconnected chunk A*).
    #[test]
    fn densify_bevensen_ottadal_soft_pulls_skane_off_aabb_center() {
        let dir = tempfile::tempdir().expect("tmpdir");
        for stem in [
            "niedersachsen-latest",
            "mecklenburg-vorpommern-latest",
            "schleswig-holstein-latest",
            "denmark-latest",
            "skane-latest",
            "halland-latest",
            "vastra_gotaland-latest",
            "ostlandet-latest",
            "vestlandet-latest",
        ] {
            let path = dir.path().join(format!("{stem}.navi-manifest.json"));
            std::fs::write(
                &path,
                format!(
                    r#"{{"schema":1,"stem":"{stem}","pbf_filename":"{stem}.osm.pbf","graph_files":{{}},"graph_format_version":{GRAPH_FORMAT_VERSION}}}"#
                ),
            )
            .unwrap();
        }
        let bevensen = (53.079686_f64, 10.587198_f64);
        let ottadal = (61.8691419_f64, 9.1055130_f64);
        let dalsoren = (61.4433766_f64, 7.4614016_f64);
        let hops = densify_route_points_via_regions(
            &[bevensen, ottadal, dalsoren],
            dir.path(),
            LONG_TRIP_CHUNK_DEG,
        );
        // Catalog-centroid class hop (not the Øresund approach mid near 12.6°E).
        let skane_centerish: Vec<_> = hops
            .iter()
            .copied()
            .filter(|(lat, lon)| *lat > 55.70 && *lat < 56.20 && *lon > 12.80 && *lon < 13.40)
            .collect();
        assert!(
            !skane_centerish.is_empty(),
            "expected a Skåne interior densify hop; hops={hops:?}"
        );
        assert!(
            skane_centerish
                .iter()
                .all(|(_, lon)| (*lon - 13.525).abs() > 0.08),
            "Skåne hop must not sit on raw AABB center 13.525; skane={skane_centerish:?} hops={hops:?}"
        );
        // Soft-pull must stay inland of the Öresund west fringe (~12.65) that
        // disconnected corridor-centroid bias, while still west of the AABB center.
        assert!(
            skane_centerish
                .iter()
                .all(|(_, lon)| *lon > 12.85 && *lon < 13.20),
            "Skåne densify must sit on E6 west of E22, east of Öresund water; skane={skane_centerish:?}"
        );
        let has_halland = hops
            .iter()
            .any(|(lat, lon)| *lat > 56.32 && *lat < 57.55 && (*lon - 12.70).abs() < 0.35);
        assert!(
            has_halland,
            "Halland leaf must remain on corridor; hops={hops:?}"
        );
    }

    /// Geography-agnostic: a point covered by two country boxes is spill/water.
    #[test]
    fn densify_multi_country_spill_is_geography_agnostic() {
        let ready = vec![
            ("europe/alpha".to_string(), [10.0_f64, 10.0, 20.0, 20.0]),
            ("europe/beta".to_string(), [15.0_f64, 15.0, 25.0, 25.0]),
            (
                "europe/alpha/leaf".to_string(),
                [10.0_f64, 10.0, 14.0, 14.0],
            ),
            ("europe/beta/leaf".to_string(), [16.0_f64, 16.0, 24.0, 24.0]),
        ];
        // Overlap of alpha∩beta country boxes with no leaf cover.
        assert!(densify_point_in_multi_country_spill((15.5, 15.5), &ready));
        // Exclusive alpha leaf interior — foreign beta country AABB must not
        // mark leaf-interior land as spill.
        assert!(!densify_point_in_multi_country_spill((12.0, 12.0), &ready));
        // Two foreign leaves covering the same point → still spill.
        let dual_leaf = vec![
            (
                "europe/alpha/leaf".to_string(),
                [10.0_f64, 10.0, 18.0, 18.0],
            ),
            ("europe/beta/leaf".to_string(), [15.0_f64, 15.0, 25.0, 25.0]),
        ];
        assert!(densify_point_in_multi_country_spill(
            (16.0, 16.0),
            &dual_leaf
        ));
    }

    /// Second corridor: Skåne↔Halland must not mid-hop Öresund water, and
    /// Stendal→Lillehammer must still densify without Baltic chord mids.
    #[test]
    fn densify_skane_halland_and_stendal_avoid_water_chord_mids() {
        let dir = tempfile::tempdir().expect("tmpdir");
        for stem in [
            "denmark-latest",
            "skane-latest",
            "halland-latest",
            "vastra_gotaland-latest",
            "ostlandet-latest",
            "sachsen-anhalt-latest",
            "niedersachsen-latest",
            "hamburg-latest",
            "schleswig-holstein-latest",
        ] {
            let path = dir.path().join(format!("{stem}.navi-manifest.json"));
            std::fs::write(
                &path,
                format!(
                    r#"{{"schema":1,"stem":"{stem}","pbf_filename":"{stem}.osm.pbf","graph_files":{{}},"graph_format_version":{GRAPH_FORMAT_VERSION}}}"#
                ),
            )
            .unwrap();
        }
        let skane = (55.91_f64, 13.525_f64);
        let halland = (56.935_f64, 12.7_f64);
        // Direct densify of a short SE hop that previously accepted DK-spill mids.
        let se_hops =
            densify_route_points_via_regions(&[skane, halland], dir.path(), LONG_TRIP_CHUNK_DEG);
        let oresundish = se_hops.iter().any(|(lat, lon)| {
            *lat > 55.9 && *lat < 56.3 && *lon > 12.4 && *lon < 12.9 && {
                // Geometric mid of Skåne→Halland is ~ (56.42, 13.11) — reject
                // anything sitting on that chord with multi-country cover.
                let mid_lat = (skane.0 + halland.0) * 0.5;
                let mid_lon = (skane.1 + halland.1) * 0.5;
                (lat - mid_lat).abs() < 0.15 && (lon - mid_lon).abs() < 0.15
            }
        });
        assert!(
            !oresundish,
            "Skåne→Halland must not densify onto Öresund mid; hops={se_hops:?}"
        );

        let stendal = (52.605766_f64, 11.859277_f64);
        let lillehammer = (61.114545_f64, 10.467007_f64);
        let no_hops = densify_route_points_via_regions(
            &[stendal, lillehammer],
            dir.path(),
            LONG_TRIP_CHUNK_DEG,
        );
        let baltic = no_hops
            .iter()
            .any(|(lat, lon)| (lat - 54.89125).abs() < 1e-4 && (lon - 10.55875).abs() < 1e-4);
        assert!(
            !baltic,
            "Stendal→Lillehammer must not use Baltic water mid; hops={no_hops:?}"
        );
        assert!(
            no_hops.len() >= 4,
            "northbound DE→NO must still densify; hops={}",
            no_hops.len()
        );
    }

    #[test]
    fn densify_centroid_overshoots_via_detects_vestlandet_past_sognefjell() {
        let prev = (60.65_f64, 10.50_f64); // Ostlandet-ish
        let vestlandet = (60.75_f64, 6.25_f64);
        let sognefjell = (61.617_f64, 8.044_f64);
        assert!(densify_centroid_overshoots_via(
            prev, vestlandet, sognefjell
        ));
        let ostlandet = (60.65_f64, 10.50_f64);
        let gothenburg = (57.71_f64, 11.97_f64);
        assert!(!densify_centroid_overshoots_via(
            gothenburg, ostlandet, sognefjell
        ));
        // Lat-dominant Stendal→Lillehammer: SH west dip is a land bridge, not a reverse.
        let stendal = (52.606_f64, 11.859_f64);
        let sh = (54.210_f64, 9.845_f64);
        let lillehammer = (61.115_f64, 10.467_f64);
        assert!(!densify_centroid_overshoots_via(stendal, sh, lillehammer));
    }

    /// Geography-agnostic gap-fill: overlapping country AABBs must not receive
    /// the geometric chord mid; a leaf-covered land centroid must be preferred.
    #[test]
    fn densify_insert_land_safe_mids_skips_synthetic_sea_spill() {
        let ready = vec![
            ("europe/westland".to_string(), [0.0_f64, 0.0, 10.0, 10.0]),
            ("europe/eastland".to_string(), [0.0_f64, 5.0, 10.0, 15.0]),
            (
                "europe/westland/coast".to_string(),
                [2.0_f64, 1.0, 8.0, 4.5],
            ),
            (
                "europe/eastland/coast".to_string(),
                [2.0_f64, 10.5, 8.0, 14.0],
            ),
        ];
        let west_c = (5.0_f64, 2.75_f64);
        let east_c = (5.0_f64, 12.25_f64);
        let centroids = vec![west_c, east_c];
        let a = (5.0_f64, 2.0_f64);
        let b = (5.0_f64, 13.0_f64);
        let mid = ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5);
        assert!(
            densify_point_in_multi_country_spill(mid, &ready),
            "synthetic chord mid must sit in westland∩eastland spill"
        );
        let mut out = Vec::new();
        insert_land_safe_mids(&mut out, a, b, &centroids, &ready, 4.0, 0);
        assert!(
            !out.iter()
                .any(|(lat, lon)| { (lat - mid.0).abs() < 1e-6 && (lon - mid.1).abs() < 1e-6 }),
            "must not insert multi-country spill mid; out={out:?}"
        );
        assert!(
            !out.is_empty(),
            "must still densify via a land-safe leaf centroid; out={out:?}"
        );
        for &p in &out {
            assert!(
                !densify_point_in_multi_country_spill(p, &ready),
                "inserted hop must not be spill; p={p:?} out={out:?}"
            );
            assert!(
                densify_point_has_leaf_cover(p, &ready),
                "inserted hop must have leaf cover; p={p:?} out={out:?}"
            );
        }
    }

    /// Non-Baltic corridor: Brussels→London must not densify onto English
    /// Channel water covered by France∩UK country AABBs.
    #[test]
    fn densify_rejects_channel_water_mid_on_be_uk_corridor() {
        let dir = tempfile::tempdir().expect("tmpdir");
        for stem in [
            "belgium-latest",
            "france-latest",
            "england-latest",
            "greater-london-latest",
        ] {
            let path = dir.path().join(format!("{stem}.navi-manifest.json"));
            std::fs::write(
                &path,
                format!(
                    r#"{{"schema":1,"stem":"{stem}","pbf_filename":"{stem}.osm.pbf","graph_files":{{}},"graph_format_version":{GRAPH_FORMAT_VERSION}}}"#
                ),
            )
            .unwrap();
        }
        let brussels = (50.8503_f64, 4.3517_f64);
        let london = (51.5074_f64, -0.1278_f64);
        let hops =
            densify_route_points_via_regions(&[brussels, london], dir.path(), LONG_TRIP_CHUNK_DEG);
        let channel_mid = ((brussels.0 + london.0) * 0.5, (brussels.1 + london.1) * 0.5);
        let on_channel_chord = hops.iter().any(|(lat, lon)| {
            (lat - channel_mid.0).abs() < 0.2 && (lon - channel_mid.1).abs() < 0.35
        });
        assert!(
            !on_channel_chord,
            "Brussels→London must not densify onto Channel water mid; hops={hops:?}"
        );
        // Overshoot-via remains geometry-agnostic: a point past an imminent via
        // on the approach axis is skipped; a land-bridge dip is kept.
        let calais = (50.95_f64, 1.85_f64);
        let dover = (51.13_f64, 1.31_f64);
        let past_dover = (51.20_f64, 0.50_f64);
        assert!(densify_centroid_overshoots_via(calais, past_dover, dover));
        assert!(!densify_centroid_overshoots_via(brussels, calais, london));
    }
}
