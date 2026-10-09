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

/// One-shot UI accept for an absurd installed-only detour (cleared when read).
static ACCEPT_ABSURD_DETOUR: AtomicBool = AtomicBool::new(false);

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
        // Do not decimate: a skipped sample drops that tile from the hop graph
        // (follow-up 23: every coarse-path tile stays; hops split if too large).
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

/// Coarse-route / straight-line ratio above which an installed-only path is an
/// absurd detour **when the direct corridor has missing regions**. Fully
/// installed corridors are never blocked for length (fjord/mountain routes
/// often exceed 2× crow-flies).
pub const ABSURD_DETOUR_RATIO: f64 = 2.0;

/// Soft cap on estimated packed graph nodes loaded for one densify hop
/// (path tiles + pad). Replaces a raw tile count: tiles vary hugely by region.
/// Density ≈ archive bytes / 200 (packed car-graph). ~500k nodes ≈ ~1 GiB peak.
/// Estimated packed nodes per densify hop. Sized so a single hop's pack load
/// stays near the ~1 GB planning peak target when skeletons are already warm.
pub const MAX_PATH_NODES_PER_HOP: usize = 900_000;

/// Refuse micro-splits: never insert a hop joint closer than this (metres) even
/// when the node budget is exceeded (overlapping large tiles along a dense
/// coarse path previously produced 1000+ hops).
pub const MIN_HOP_SPLIT_GAP_M: f64 = 40_000.0;

/// Deprecated alias: hop split uses [`MAX_PATH_NODES_PER_HOP`] (estimated nodes).
pub const MAX_PATH_TILES_PER_HOP: usize = 10;

/// Great-circle distance in km between two WGS84 points.
pub fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const R: f64 = 6371.0;
    let (lat1, lon1, lat2, lon2) = (
        lat1.to_radians(),
        lon1.to_radians(),
        lat2.to_radians(),
        lon2.to_radians(),
    );
    let dlat = lat2 - lat1;
    let dlon = lon2 - lon1;
    let a = (dlat * 0.5).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon * 0.5).sin().powi(2);
    2.0 * R * a.sqrt().min(1.0).asin()
}

/// Straight-line km along consecutive waypoints (sum of legs).
pub fn waypoints_straight_km(waypoints: &[(f64, f64)]) -> f64 {
    waypoints
        .windows(2)
        .map(|w| haversine_km(w[0].0, w[0].1, w[1].0, w[1].1))
        .sum()
}

/// True when an installed-only coarse path is an absurd detour **and** at least
/// one direct-corridor region is missing. Never triggers when every region on
/// the outline corridor is installed (fjord/mountain routes may be >>2× straight).
pub fn is_absurd_installed_detour(
    coarse_km: f64,
    waypoints: &[(f64, f64)],
    missing_direct_regions: &[String],
) -> bool {
    if missing_direct_regions.is_empty() {
        return false;
    }
    if coarse_km <= 0.0 || waypoints.len() < 2 {
        return false;
    }
    let straight = waypoints_straight_km(waypoints);
    if straight < 1.0 {
        return false;
    }
    coarse_km / straight > ABSURD_DETOUR_RATIO
}

/// Arm a one-shot UI accept for the next absurd-detour gate check.
pub fn set_accept_absurd_detour(accept: bool) {
    ACCEPT_ABSURD_DETOUR.store(accept, Ordering::SeqCst);
}

/// Debug-only env override. Product UI uses [`set_accept_absurd_detour`];
/// release builds ignore `NAVI_ACCEPT_ABSURD_DETOUR`.
pub fn accept_absurd_detour_env() -> bool {
    if !cfg!(debug_assertions) {
        return false;
    }
    matches!(
        std::env::var("NAVI_ACCEPT_ABSURD_DETOUR")
            .ok()
            .as_deref()
            .map(str::trim),
        Some("1") | Some("true") | Some("yes")
    )
}

/// True when the user accepted the detour in the app, or (debug only) via env.
pub fn accept_absurd_detour() -> bool {
    ACCEPT_ABSURD_DETOUR.swap(false, Ordering::SeqCst) || accept_absurd_detour_env()
}

/// Metres a densify hop end may differ from the intended coarse-path joint.
/// Absorbs printed-coordinate rounding (1e-5° ≈ 1.1 m) and pack vs skeleton
/// float noise. This is not a search radius: a hop end is never moved farther.
pub const HOP_END_MATCH_M: f64 = 50.0;

/// Snap budget for densify hop endpoints (region centroids), not user stops.
/// Centroids can sit several km offshore / inland of the nearest clearance-legal
/// road (SH→DK water approaches needed ~11–22 km). Same-stem tile fill prevents
/// the false Skåne→Halland component jump that a large snap used to cause.
///
/// This constant affects **snap search only** (pad ≈ max_m/1e5 degrees). It does
/// **not** widen tile selection or edge materialization — those use
/// [`CORRIDOR_TILE_PAD_DEG`] / [`CORRIDOR_EDGE_HALF_WIDTH_DEG`].
/// Intermediate densify hop ends use [`HOP_END_MATCH_M`] instead of this value.
pub const CHUNK_INTERMEDIATE_SNAP_M: f64 = 35_000.0;

/// Effective densify-joint snap budget (see [`CHUNK_INTERMEDIATE_SNAP_M`]).
pub fn effective_chunk_intermediate_snap_m() -> f64 {
    measure_override_f64("NAVI_MEASURE_CHUNK_INTERMEDIATE_SNAP_M")
        .unwrap_or(CHUNK_INTERMEDIATE_SNAP_M)
}

/// Chebyshev-ish span of the point set (max of lat/lon ranges).
pub fn trip_span_deg(points: &[(f64, f64)]) -> f64 {
    let (min_lat, min_lon, max_lat, max_lon) = points_bounds(points);
    (max_lat - min_lat).abs().max((max_lon - min_lon).abs())
}

/// Insert linear midpoints so each consecutive hop's span is ≤ `max_hop_deg`.
/// Cross-sea / multi-region trips use Stage B corridor densify instead.
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

/// Skip country-level Ready packs when foreign leaf AABBs intersect their
/// sea-spilling bbox and own leaves are Ready (or proxies would duplicate).
/// Still used by indexed pack load; not part of centroid densify.
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
    #[test]
    fn centroid_densify_and_hardcoded_anchors_removed() {
        let src = include_str!("plan_bbox.rs");
        // Assemble names so this test body does not match its own needles.
        let via = format!("fn densify_route_points{}", "_via_regions");
        let spine = format!("fn norway_e6_spine{}", "_anchors");
        let bias = format!("fn prefer_densify_leaf{}", "_centroid");
        assert!(!src.contains(&via), "centroid densify must stay deleted");
        assert!(
            !src.contains(&spine),
            "hard-coded E6 densify anchors must stay deleted"
        );
        assert!(
            !src.contains(&bias),
            "densify centroid biases must stay deleted"
        );
    }

    use super::*;

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
}
