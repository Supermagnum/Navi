//! Trip-bbox pad schedule for adaptive plan widen-retry.
//!
//! Initial pad matches historical `plan_car_route_inner` behaviour; widen doubles
//! until [`PLAN_BBOX_PAD_CAP_DEG`] so RAM stays bounded on Automotive devices.

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
/// Six leaves room for a two-stem border hop once edge clipping is a corridor
/// band (not a fat diagonal AABB) — count alone no longer dominates RSS.
pub const MAX_PLAN_TILES: usize = 6;

/// Soft cap on on-disk tile bytes merged for one plan/leg. Prefer dropping the
/// largest non-essential tiles before exceeding this; endpoints always stay.
/// With corridor-band edge clip, ~280 MB disk stays well under 2.8 GiB RSS.
/// Soft cap on on-disk tile bytes merged for one plan/leg. Prefer dropping the
/// largest non-essential tiles before exceeding this; endpoints always stay.
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
pub const CHUNK_INTERMEDIATE_SNAP_M: f64 = 25_000.0;

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
    // Keep explicit vias (everything except start/end) as forced anchors.
    for &p in &points[1..points.len() - 1] {
        let t = ((p.0 - start.0) * vlat + (p.1 - start.1) * vlon) / v2;
        anchors.push((t.clamp(0.0, 1.0), p));
    }

    let ready = collect_ready_region_entries_dirs(dirs);
    for (path, bbox) in &ready {
        if densify_skip_country_when_leaves_ready(path, &ready) {
            continue;
        }
        let c = ((bbox[0] + bbox[2]) * 0.5, (bbox[1] + bbox[3]) * 0.5);
        let t = ((c.0 - start.0) * vlat + (c.1 - start.1) * vlon) / v2;
        if t <= 0.02 || t >= 0.98 {
            continue;
        }
        // Reject hinterland centroids that progress in lon/lat mix but sit
        // outside the OD latitude band (e.g. western Niedersachsen after
        // Hamburg on a northbound Stendal→Norway chord).
        let lat_lo = start.0.min(end.0) - 0.25;
        let lat_hi = start.0.max(end.0) + 0.25;
        if c.0 < lat_lo || c.0 > lat_hi {
            continue;
        }
        // Wide pad so lateral corridor regions (Skåne east of the Hamar→Minden
        // chord) stay eligible as land anchors. Pad 2.0 left Skåne's centroid
        // (~13.53°E) outside the trip AABB (max OD lon + 2 ≈ 13.07) and forced
        // Halland→NI geometric mids into Denmark spill.
        let trip = trip_bbox_points(points, CORRIDOR_TILE_PAD_DEG.max(3.0));
        if c.0 < trip[0] || c.0 > trip[2] || c.1 < trip[1] || c.1 > trip[3] {
            continue;
        }
        anchors.push((t, c));
    }

    anchors.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    // Dedup near-duplicates along t, then keep only points that get closer to
    // the destination. Lon-weighted t alone can rank western Niedersachsen
    // after Hamburg and force a southbound chunk hop (then back again).
    let cheb = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs().max((a.1 - b.1).abs());
    let via_set: std::collections::HashSet<(u64, u64)> = points[1..points.len() - 1]
        .iter()
        .map(|p| (p.0.to_bits(), p.1.to_bits()))
        .collect();
    let mut filtered: Vec<(f64, (f64, f64))> = Vec::new();
    let mut last_t = -1.0_f64;
    let mut best_to_end = cheb(start, end);
    for (t, p) in anchors {
        let is_via = via_set.contains(&(p.0.to_bits(), p.1.to_bits()));
        if t - last_t < 0.04 && !filtered.is_empty() && !is_via {
            continue;
        }
        let d_end = cheb(p, end);
        if !is_via && d_end >= best_to_end - 1e-6 {
            continue;
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
    out
}

/// Walk consecutive hops; when span exceeds `max_hop_deg`, insert the Ready
/// region centroid closest to the geometric midpoint (land-only proxy).
fn densify_gaps_with_region_centroids(
    points: &[(f64, f64)],
    dirs: &[&std::path::Path],
    max_hop_deg: f64,
) -> Vec<(f64, f64)> {
    let ready = collect_ready_region_entries_dirs(dirs);
    if ready.is_empty() {
        return points.to_vec();
    }
    // Prefer leaf centroids for gap fill, but keep country boxes for land checks
    // so SH→Skåne still densifies across Jutland (europe/denmark).
    let centroids: Vec<(f64, f64)> = ready
        .iter()
        .filter(|(path, _)| !densify_skip_country_when_leaves_ready(path, &ready))
        .map(|(_, b)| ((b[0] + b[2]) * 0.5, (b[1] + b[3]) * 0.5))
        .collect();
    let mut out = Vec::with_capacity(points.len() * 2);
    out.push(points[0]);
    for w in points.windows(2) {
        let a = w[0];
        let b = w[1];
        insert_land_safe_mids(&mut out, a, b, &centroids, &ready, max_hop_deg);
        out.push(b);
    }
    out
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

/// Country extracts (`europe/denmark`) spill across Öresund into Sweden. Omit the
/// country centroid from densify when Ready leaf packs exist under that country,
/// or when a foreign leaf bbox intersects the country box (DK∩Skåne).
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
) {
    let dlat = b.0 - a.0;
    let dlon = b.1 - a.1;
    let dist = dlat.abs().max(dlon.abs());
    if dist <= max_hop_deg + 1e-9 {
        return;
    }
    let v2 = dlat * dlat + dlon * dlon;
    if v2 < 1e-12 {
        return;
    }
    // Walk forward along AB using 2D projection. Dominant-axis-only t wrongly
    // treated Schleswig-Holstein (north) as between Stendal and Niedersachsen
    // because their longitudes nest.
    let mut best: Option<(f64, (f64, f64))> = None;
    for &c in centroids {
        let t = ((c.0 - a.0) * dlat + (c.1 - a.1) * dlon) / v2;
        if t <= 0.05 || t >= 0.95 {
            continue;
        }
        let proj = (a.0 + t * dlat, a.1 + t * dlon);
        let perp = (c.0 - proj.0).abs() + (c.1 - proj.1).abs();
        if perp > max_hop_deg * 0.85 {
            continue;
        }
        let da = (c.0 - a.0).abs().max((c.1 - a.1).abs());
        let db = (c.0 - b.0).abs().max((c.1 - b.1).abs());
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
        if best.is_none_or(|(bt, _)| t < bt) {
            best = Some((t, c));
        }
    }
    let c = if let Some((_, c)) = best {
        c
    } else {
        // Geometric midpoint when it lies inside a Ready region. Reject mids that
        // sit in multi-country bbox spill (Öresund covered by DK + Skåne) — those
        // snap to the wrong shore and disconnect under a 4 GB tile budget.
        let mid = (a.0 + dlat * 0.5, a.1 + dlon * 0.5);
        let mid_on_land = ready
            .iter()
            .any(|(_, r)| crate::routing::basemap::bbox_covers_point(*r, mid.0, mid.1));
        if !mid_on_land || densify_mid_in_country_spill(mid, a, b, ready) {
            return;
        }
        let da = (mid.0 - a.0).abs().max((mid.1 - a.1).abs());
        let db = (mid.0 - b.0).abs().max((mid.1 - b.1).abs());
        if da < max_hop_deg * 0.2 || db < max_hop_deg * 0.2 {
            return;
        }
        mid
    };
    if let Some(prev) = out.last() {
        if (prev.0 - c.0).abs() < 1e-4 && (prev.1 - c.1).abs() < 1e-4 {
            return;
        }
    }
    if (c.0 - b.0).abs() < 1e-4 && (c.1 - b.1).abs() < 1e-4 {
        return;
    }
    insert_land_safe_mids(out, a, c, centroids, ready, max_hop_deg);
    out.push(c);
    insert_land_safe_mids(out, c, b, centroids, ready, max_hop_deg);
}

fn densify_region_country(path: &str) -> Option<&str> {
    let mut parts = path.split('/');
    let _cont = parts.next()?;
    parts.next()
}

fn densify_point_country(pt: (f64, f64), ready: &[(String, [f64; 4])]) -> Option<&str> {
    let mut best: Option<(f64, &str)> = None;
    for (path, bbox) in ready {
        if !crate::routing::basemap::bbox_covers_point(*bbox, pt.0, pt.1) {
            continue;
        }
        let Some(country) = densify_region_country(path) else {
            continue;
        };
        let area = (bbox[2] - bbox[0]).max(0.0) * (bbox[3] - bbox[1]).max(0.0);
        if best.is_none_or(|(ba, _)| area < ba) {
            best = Some((area, country));
        }
    }
    best.map(|(_, c)| c)
}

/// Reject geometric mids on same-country hops that also fall inside a foreign
/// country box (Öresund: Skåne→Halland mid covered by europe/denmark spill).
/// Cross-country hops (SH→Denmark) must still densify across DE∩DK Baltic boxes.
fn densify_mid_in_country_spill(
    mid: (f64, f64),
    a: (f64, f64),
    b: (f64, f64),
    ready: &[(String, [f64; 4])],
) -> bool {
    let Some(home) = densify_point_country(a, ready) else {
        return false;
    };
    if densify_point_country(b, ready) != Some(home) {
        return false;
    }
    let mut home_covers = false;
    let mut foreign_covers = false;
    for (path, bbox) in ready {
        if !crate::routing::basemap::bbox_covers_point(*bbox, mid.0, mid.1) {
            continue;
        }
        match densify_region_country(path) {
            Some(c) if c == home => home_covers = true,
            Some(_) => foreign_covers = true,
            None => {}
        }
    }
    home_covers && foreign_covers
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
                corridor_band_bboxes(pts, CORRIDOR_EDGE_HALF_WIDTH_DEG, CORRIDOR_BAND_STEP_DEG)
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

#[cfg(test)]
mod tests {
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
                    r#"{{"schema":1,"stem":"{stem}","pbf_filename":"{stem}.osm.pbf","graph_files":{{}},"graph_format_version":8}}"#
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
            .any(|(lat, lon)| (lat - 56.935).abs() < 0.05 && (lon - 12.70).abs() < 0.15);
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
                    r#"{{"schema":1,"stem":"{stem}","pbf_filename":"{stem}.osm.pbf","graph_files":{{}},"graph_format_version":8}}"#
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
            .any(|(lat, lon)| (lat - 55.91).abs() < 0.05 && (lon - 13.525).abs() < 0.15);
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
}
