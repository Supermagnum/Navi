//! Offline region-adjacency corridor (default long-trip region selector).
//!
//! Geometry + undirected edges come from `data/region_adjacency.bin`, built by
//! `scripts/generate-region-adjacency.py` (Geofabrik index polys, NE Sweden
//! län, named fixed/legacy links). Rings are never hand-edited except the
//! documented hedmark stub rectangle in that script.
//!
//! [`ordered_needed_regions_for_trip`] is the **default** corridor source:
//! containing-region PIP + shortest hop-count path (centroid-distance
//! tie-break). Router densify ([`super::ordered_needed_regions_along_route`])
//! remains available for an explicit "refine with online routing" path — see
//! the long_trip module docs on wiring spots; no toggle is built here.

use std::cmp::Ordering;
use std::collections::{BTreeSet, BinaryHeap};
use std::sync::OnceLock;

use crate::pack_server::{normalize_region_id, region_ids_match_for_catalog};

const ASSET: &[u8] = include_bytes!("data/region_adjacency.bin");
const COORD_SCALE: f64 = 10_000_000.0;

#[derive(Debug, Clone, PartialEq)]
pub enum MissingCorridor {
    /// No catalog region covers the waypoint.
    UnknownRegion { lat: f64, lon: f64 },
    /// Start and end resolve, but the adjacency graph has no path (isolate /
    /// sea gap without a named fixed link).
    NoPath { from: String, to: String },
}

impl std::fmt::Display for MissingCorridor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownRegion { lat, lon } => {
                write!(f, "no catalog region contains ({lat}, {lon})")
            }
            Self::NoPath { from, to } => {
                write!(f, "no adjacency corridor from {from} to {to}")
            }
        }
    }
}

impl std::error::Error for MissingCorridor {}

#[derive(Clone)]
struct Ring {
    /// Closed ring as (lon, lat).
    pts: Vec<(f64, f64)>,
    min_lon: f64,
    min_lat: f64,
    max_lon: f64,
    max_lat: f64,
}

#[derive(Clone)]
struct Region {
    id: String,
    rings: Vec<Ring>,
    centroid_lon: f64,
    centroid_lat: f64,
    area_deg2: f64,
    min_lon: f64,
    min_lat: f64,
    max_lon: f64,
    max_lat: f64,
}

struct Graph {
    regions: Vec<Region>,
    /// Parallel `'static` ids (same order as `regions`).
    id_static: Vec<&'static str>,
    /// Undirected adjacency list (region indices).
    adj: Vec<Vec<usize>>,
    named_links: Vec<(usize, usize, &'static str)>,
}

fn decode_asset(bytes: &[u8]) -> Result<Graph, String> {
    if bytes.len() < 16 {
        return Err("asset too short".into());
    }
    if &bytes[0..8] != b"NAVIRADJ" {
        return Err("bad magic".into());
    }
    let version = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    if version != 1 {
        return Err(format!("unsupported version {version}"));
    }
    let n = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let mut off = 16usize;
    let mut regions = Vec::with_capacity(n);
    for _ in 0..n {
        if off + 3 > bytes.len() {
            return Err("truncated id header".into());
        }
        let id_len = u16::from_le_bytes(bytes[off..off + 2].try_into().unwrap()) as usize;
        let _source = bytes[off + 2];
        off += 3;
        if off + id_len + 8 + 2 > bytes.len() {
            return Err("truncated id/centroid".into());
        }
        let id = String::from_utf8(bytes[off..off + id_len].to_vec())
            .map_err(|e| format!("utf8 id: {e}"))?;
        off += id_len;
        let lon_i = i32::from_le_bytes(bytes[off..off + 4].try_into().unwrap());
        let lat_i = i32::from_le_bytes(bytes[off + 4..off + 8].try_into().unwrap());
        off += 8;
        let n_rings = u16::from_le_bytes(bytes[off..off + 2].try_into().unwrap()) as usize;
        off += 2;
        let mut rings = Vec::with_capacity(n_rings);
        let mut min_lon = f64::INFINITY;
        let mut min_lat = f64::INFINITY;
        let mut max_lon = f64::NEG_INFINITY;
        let mut max_lat = f64::NEG_INFINITY;
        let mut area = 0.0;
        for _ in 0..n_rings {
            if off + 4 > bytes.len() {
                return Err("truncated ring len".into());
            }
            let n_pts = u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap()) as usize;
            off += 4;
            if off + n_pts * 8 > bytes.len() {
                return Err("truncated ring pts".into());
            }
            let mut pts = Vec::with_capacity(n_pts);
            for _ in 0..n_pts {
                let lon = i32::from_le_bytes(bytes[off..off + 4].try_into().unwrap()) as f64
                    / COORD_SCALE;
                let lat = i32::from_le_bytes(bytes[off + 4..off + 8].try_into().unwrap()) as f64
                    / COORD_SCALE;
                off += 8;
                min_lon = min_lon.min(lon);
                max_lon = max_lon.max(lon);
                min_lat = min_lat.min(lat);
                max_lat = max_lat.max(lat);
                pts.push((lon, lat));
            }
            area += ring_area_deg2(&pts);
            let (rmin_lon, rmin_lat, rmax_lon, rmax_lat) = ring_bbox(&pts);
            rings.push(Ring {
                pts,
                min_lon: rmin_lon,
                min_lat: rmin_lat,
                max_lon: rmax_lon,
                max_lat: rmax_lat,
            });
        }
        regions.push(Region {
            id,
            rings,
            centroid_lon: lon_i as f64 / COORD_SCALE,
            centroid_lat: lat_i as f64 / COORD_SCALE,
            area_deg2: area.abs(),
            min_lon,
            min_lat,
            max_lon,
            max_lat,
        });
    }
    if off + 4 > bytes.len() {
        return Err("truncated edge count".into());
    }
    let n_edges = u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap()) as usize;
    off += 4;
    let mut adj = vec![Vec::new(); n];
    for _ in 0..n_edges {
        if off + 5 > bytes.len() {
            return Err("truncated edge".into());
        }
        let u = u16::from_le_bytes(bytes[off..off + 2].try_into().unwrap()) as usize;
        let v = u16::from_le_bytes(bytes[off + 2..off + 4].try_into().unwrap()) as usize;
        let _kind = bytes[off + 4];
        off += 5;
        if u >= n || v >= n {
            return Err("edge index OOB".into());
        }
        adj[u].push(v);
        adj[v].push(u);
    }
    for nbrs in &mut adj {
        nbrs.sort_unstable();
        nbrs.dedup();
    }
    if off + 4 > bytes.len() {
        return Err("truncated named count".into());
    }
    let n_named = u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap()) as usize;
    off += 4;
    let mut named_raw: Vec<(usize, usize, String)> = Vec::with_capacity(n_named);
    for _ in 0..n_named {
        if off + 7 > bytes.len() {
            return Err("truncated named header".into());
        }
        let u = u16::from_le_bytes(bytes[off..off + 2].try_into().unwrap()) as usize;
        let v = u16::from_le_bytes(bytes[off + 2..off + 4].try_into().unwrap()) as usize;
        let _kind = bytes[off + 4];
        let note_len = u16::from_le_bytes(bytes[off + 5..off + 7].try_into().unwrap()) as usize;
        off += 7;
        if off + note_len > bytes.len() {
            return Err("truncated named note".into());
        }
        let note = String::from_utf8(bytes[off..off + note_len].to_vec())
            .map_err(|e| format!("utf8 note: {e}"))?;
        off += note_len;
        named_raw.push((u, v, note));
    }
    let _ = off;
    let id_static: Vec<&'static str> = regions
        .iter()
        .map(|r| Box::leak(r.id.clone().into_boxed_str()) as &'static str)
        .collect();
    let named_links: Vec<(usize, usize, &'static str)> = named_raw
        .into_iter()
        .map(|(u, v, note)| (u, v, Box::leak(note.into_boxed_str()) as &'static str))
        .collect();
    Ok(Graph {
        regions,
        id_static,
        adj,
        named_links,
    })
}

fn ring_bbox(pts: &[(f64, f64)]) -> (f64, f64, f64, f64) {
    let mut min_lon = f64::INFINITY;
    let mut min_lat = f64::INFINITY;
    let mut max_lon = f64::NEG_INFINITY;
    let mut max_lat = f64::NEG_INFINITY;
    for &(lon, lat) in pts {
        min_lon = min_lon.min(lon);
        max_lon = max_lon.max(lon);
        min_lat = min_lat.min(lat);
        max_lat = max_lat.max(lat);
    }
    (min_lon, min_lat, max_lon, max_lat)
}

fn ring_area_deg2(pts: &[(f64, f64)]) -> f64 {
    if pts.len() < 3 {
        return 0.0;
    }
    let mut a = 0.0;
    for i in 0..pts.len() - 1 {
        a += pts[i].0 * pts[i + 1].1 - pts[i + 1].0 * pts[i].1;
    }
    a * 0.5
}

fn point_in_ring(pts: &[(f64, f64)], lon: f64, lat: f64) -> bool {
    let mut inside = false;
    if pts.len() < 3 {
        return false;
    }
    let n = pts.len() - 1;
    for i in 0..n {
        let (x1, y1) = pts[i];
        let (x2, y2) = pts[i + 1];
        let intersect = ((y1 > lat) != (y2 > lat))
            && (lon < (x2 - x1) * (lat - y1) / (y2 - y1 + f64::EPSILON) + x1);
        if intersect {
            inside = !inside;
        }
    }
    inside
}

fn region_covers(r: &Region, lat: f64, lon: f64) -> bool {
    if lon < r.min_lon || lon > r.max_lon || lat < r.min_lat || lat > r.max_lat {
        return false;
    }
    r.rings.iter().any(|ring| {
        if lon < ring.min_lon || lon > ring.max_lon || lat < ring.min_lat || lat > ring.max_lat {
            return false;
        }
        point_in_ring(&ring.pts, lon, lat)
    })
}

fn graph() -> &'static Graph {
    static G: OnceLock<Graph> = OnceLock::new();
    G.get_or_init(|| decode_asset(ASSET).expect("region_adjacency.bin decode"))
}

/// Number of regions in the compiled adjacency asset.
pub fn adjacency_region_count() -> usize {
    graph().regions.len()
}

/// Number of undirected edges in the corridor graph.
pub fn adjacency_edge_count() -> usize {
    graph().adj.iter().map(|n| n.len()).sum::<usize>() / 2
}

/// Region ids with degree 0 (isolates).
pub fn adjacency_isolates() -> Vec<&'static str> {
    let g = graph();
    g.regions
        .iter()
        .enumerate()
        .filter(|(i, _)| g.adj[*i].is_empty())
        .map(|(i, _)| g.id_static[i])
        .collect()
}

/// Named fixed / legacy links as `(a, b, note)`.
pub fn adjacency_named_links() -> Vec<(&'static str, &'static str, &'static str)> {
    let g = graph();
    g.named_links
        .iter()
        .map(|&(u, v, note)| (g.id_static[u], g.id_static[v], note))
        .collect()
}

fn country_catalog_prefix(iso: &str) -> String {
    match iso.trim().to_ascii_lowercase().as_str() {
        "us" | "usa" => "north-america/us".into(),
        "no" | "nor" => "europe/norway".into(),
        "se" | "swe" => "europe/sweden".into(),
        "de" | "deu" => "europe/germany".into(),
        "dk" | "dnk" => "europe/denmark".into(),
        other => format!("country/{other}"),
    }
}

fn allowed(id: &str, country_iso: Option<&str>) -> bool {
    let Some(iso) = country_iso else {
        return true;
    };
    let prefix = country_catalog_prefix(iso);
    if prefix == "north-america/us" {
        return id == "north-america/us" || id.starts_with("north-america/us/");
    }
    id.starts_with(&prefix) || id == prefix.trim_end_matches('/')
}

/// Finest catalog region covering `(lat, lon)`, optionally restricted by ISO.
pub fn region_containing(lat: f64, lon: f64, country_iso: Option<&str>) -> Option<&'static str> {
    let g = graph();
    let mut best_i: Option<usize> = None;
    for (i, r) in g.regions.iter().enumerate() {
        if !allowed(&r.id, country_iso) {
            continue;
        }
        if !region_covers(r, lat, lon) {
            continue;
        }
        match best_i {
            None => best_i = Some(i),
            Some(cur) => {
                let cr = &g.regions[cur];
                if r.area_deg2 < cr.area_deg2 || (r.area_deg2 == cr.area_deg2 && r.id < cr.id) {
                    best_i = Some(i);
                }
            }
        }
    }
    best_i.map(|i| g.id_static[i])
}

fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6371.0;
    let p1 = lat1.to_radians();
    let p2 = lat2.to_radians();
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

#[derive(Copy, Clone)]
struct State {
    hops: u32,
    cost_x100: u64,
    node: usize,
}

impl PartialEq for State {
    fn eq(&self, other: &Self) -> bool {
        self.hops == other.hops && self.cost_x100 == other.cost_x100 && self.node == other.node
    }
}
impl Eq for State {}
impl PartialOrd for State {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for State {
    fn cmp(&self, other: &Self) -> Ordering {
        // Min-heap via reverse.
        other
            .hops
            .cmp(&self.hops)
            .then_with(|| other.cost_x100.cmp(&self.cost_x100))
            .then_with(|| other.node.cmp(&self.node))
    }
}

fn shortest_path_indices(from: usize, to: usize, country_iso: Option<&str>) -> Option<Vec<usize>> {
    let g = graph();
    if from == to {
        return Some(vec![from]);
    }
    if !allowed(&g.regions[from].id, country_iso) || !allowed(&g.regions[to].id, country_iso) {
        return None;
    }
    let n = g.regions.len();
    let mut best_hops = vec![u32::MAX; n];
    let mut best_cost = vec![u64::MAX; n];
    let mut parent = vec![None; n];
    let mut heap = BinaryHeap::new();
    best_hops[from] = 0;
    best_cost[from] = 0;
    heap.push(State {
        hops: 0,
        cost_x100: 0,
        node: from,
    });
    while let Some(State {
        hops,
        cost_x100,
        node: u,
    }) = heap.pop()
    {
        if hops > best_hops[u] || (hops == best_hops[u] && cost_x100 > best_cost[u]) {
            continue;
        }
        if u == to {
            break;
        }
        for &v in &g.adj[u] {
            if !allowed(&g.regions[v].id, country_iso) {
                continue;
            }
            let step = haversine_km(
                g.regions[u].centroid_lat,
                g.regions[u].centroid_lon,
                g.regions[v].centroid_lat,
                g.regions[v].centroid_lon,
            );
            let nd = hops + 1;
            let nc = cost_x100 + (step * 100.0).round() as u64;
            if nd < best_hops[v] || (nd == best_hops[v] && nc < best_cost[v]) {
                best_hops[v] = nd;
                best_cost[v] = nc;
                parent[v] = Some(u);
                heap.push(State {
                    hops: nd,
                    cost_x100: nc,
                    node: v,
                });
            }
        }
    }
    if best_hops[to] == u32::MAX {
        return None;
    }
    let mut path = Vec::new();
    let mut cur = Some(to);
    while let Some(i) = cur {
        path.push(i);
        cur = parent[i];
    }
    path.reverse();
    Some(path)
}

fn is_installed(id: &str, installed: &[String]) -> bool {
    installed
        .iter()
        .any(|inst| region_ids_match_for_catalog(inst, id))
}

/// Default long-trip corridor: PIP + adjacency hop path for each consecutive
/// waypoint pair, concatenated and de-duplicated in first-crossing order.
///
/// Installed regions are dropped from the result (same contract as densify).
pub fn ordered_needed_regions_for_trip(
    waypoints: &[(f64, f64)],
    installed: &[String],
    country_iso: Option<&str>,
) -> Result<Vec<String>, MissingCorridor> {
    if waypoints.is_empty() {
        return Ok(Vec::new());
    }
    let g = graph();
    let mut region_ids: Vec<String> = Vec::new();
    for &(lat, lon) in waypoints {
        let Some(id) = region_containing(lat, lon, country_iso) else {
            return Err(MissingCorridor::UnknownRegion { lat, lon });
        };
        region_ids.push(normalize_region_id(id));
    }

    let mut out: Vec<String> = Vec::new();
    let mut seen = BTreeSet::new();
    let push = |id: &str, out: &mut Vec<String>, seen: &mut BTreeSet<String>| {
        if is_installed(id, installed) {
            return;
        }
        if seen.insert(id.to_string()) {
            out.push(id.to_string());
        }
    };

    if region_ids.len() == 1 {
        push(&region_ids[0], &mut out, &mut seen);
        return Ok(out);
    }

    for w in region_ids.windows(2) {
        let a = &w[0];
        let b = &w[1];
        let ia = g.regions.iter().position(|r| r.id == *a);
        let ib = g.regions.iter().position(|r| r.id == *b);
        let (Some(ia), Some(ib)) = (ia, ib) else {
            return Err(MissingCorridor::NoPath {
                from: a.clone(),
                to: b.clone(),
            });
        };
        let Some(path) = shortest_path_indices(ia, ib, country_iso) else {
            return Err(MissingCorridor::NoPath {
                from: a.clone(),
                to: b.clone(),
            });
        };
        for idx in path {
            push(&g.regions[idx].id, &mut out, &mut seen);
        }
    }
    Ok(out)
}

/// Warm the static graph (decode). Returns region count.
pub fn warm_region_adjacency() -> usize {
    adjacency_region_count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_loads_and_has_named_links() {
        let n = warm_region_adjacency();
        assert!(n >= 100, "expected full catalog, got {n}");
        assert!(adjacency_edge_count() > 100);
        let links = adjacency_named_links();
        assert!(
            links.iter().any(|(a, b, note)| {
                (*a == "europe/denmark" || *b == "europe/denmark")
                    && note.to_ascii_lowercase().contains("oresund")
            }),
            "missing Oresund named link in {links:?}"
        );
        assert!(
            links.iter().any(|(a, b, note)| {
                (*a == "europe/norway/hedmark" || *b == "europe/norway/hedmark")
                    && note.contains("catalog legacy")
            }),
            "missing hedmark legacy link in {links:?}"
        );
    }

    #[test]
    fn expected_isolates_include_gotland_and_overseas() {
        let iso: BTreeSet<_> = adjacency_isolates().into_iter().collect();
        for want in [
            "europe/sweden/gotland",
            "europe/norway/svalbard-janmayen",
            "north-america/us/alaska",
            "north-america/us/hawaii",
            "north-america/us/puerto-rico",
            "north-america/us/us-virgin-islands",
        ] {
            assert!(iso.contains(want), "expected isolate {want}, have {iso:?}");
        }
    }

    #[test]
    fn klecken_and_innlandet_pip() {
        assert_eq!(
            region_containing(53.334, 10.045, None),
            Some("europe/germany/niedersachsen")
        );
        assert_eq!(
            region_containing(61.593, 10.332, None),
            Some("europe/norway/ostlandet")
        );
    }

    #[test]
    fn gotland_trip_is_missing_corridor() {
        let err =
            ordered_needed_regions_for_trip(&[(57.63, 18.29), (59.33, 18.07)], &[], Some("se"))
                .unwrap_err();
        assert!(matches!(err, MissingCorridor::NoPath { .. }), "got {err}");
    }
}
