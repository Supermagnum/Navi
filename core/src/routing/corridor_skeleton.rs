//! Persistent major-road corridor skeleton (Follow-up 13/16 Stage A).
//!
//! Built from installed graph packs: motorway, trunk, primary, ferries, plus
//! pier/approach edges that touch a ferry terminal (any highway class — same
//! rule as pack densify pier stubs), secondary that touch a ferry terminal or
//! border-crossing node, and secondary approaches to borders. Skeletons join
//! across regions on shared OSM node ids (land borders) and on ferries whose
//! terminals lie in two different regions.
//!
//! Coarse search uses the same [`crate::routing::graph::RouteOptions`] as the
//! detailed profile (directed edges, `base_weight` travel time, ferries from
//! duration+boarding, tolls/tunnels allowed by default).

use crate::routing::elevation::country_iso_at;
use crate::routing::eta::motor_path_minutes_from_edges;
use crate::routing::graph::{
    ferry_crossing_and_wait_min, time_base_weight_for_edge, GraphEdge, RouteGraph, RouteOptions,
    RoutingProfile, SurfaceQuality, SurfaceRoutingMode,
};
use crate::routing::indexed::{densify_skeleton_edge, FlatGraphPack, GRAPH_FORMAT_VERSION};
use geo_types::Coord;
use osm4routing::{Node, NodeId};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// On-disk skeleton format version (independent of pack GRAPH_FORMAT_VERSION).
/// v2 adds road ref/name, oneway, and per-node border/ferry flags.
pub const CORRIDOR_SKELETON_FORMAT_VERSION: u32 = 2;

/// Filename stem written next to region packs: `{stem}.navi-corridor-skeleton.json`.
pub fn skeleton_filename(leaf_stem: &str) -> String {
    format!("{leaf_stem}.navi-corridor-skeleton.json")
}

pub fn skeleton_path(dir: &Path, leaf_stem: &str) -> PathBuf {
    dir.join(skeleton_filename(leaf_stem))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorridorSkeletonFile {
    pub format_version: u32,
    pub pack_format_version: u32,
    pub region_id: String,
    pub leaf_stem: String,
    pub profile: String,
    pub node_count: u32,
    pub edge_count: u32,
    pub ferry_edge_count: u32,
    pub secondary_edge_count: u32,
    pub border_node_count: u32,
    pub ferry_terminal_count: u32,
    pub build_ms: u64,
    /// OSM node ids kept in the skeleton.
    pub node_ids: Vec<i64>,
    pub node_lats: Vec<f64>,
    pub node_lons: Vec<f64>,
    /// 1 = land-border crossing (shared OSM id with another region).
    #[serde(default)]
    pub node_is_border: Vec<u8>,
    /// 1 = ferry terminal.
    #[serde(default)]
    pub node_is_ferry_terminal: Vec<u8>,
    pub edge_src: Vec<u32>,
    pub edge_tgt: Vec<u32>,
    pub edge_length_m: Vec<f64>,
    /// Drive-equivalent metres (same as pack `base_weight`: time cost for A*).
    pub edge_base_weight: Vec<f64>,
    pub edge_highway: Vec<String>,
    #[serde(default)]
    pub edge_name: Vec<String>,
    #[serde(default)]
    pub edge_road_ref: Vec<String>,
    #[serde(default)]
    pub edge_is_oneway: Vec<u8>,
    pub edge_is_ferry: Vec<u8>,
    pub edge_is_tunnel: Vec<u8>,
    pub edge_is_toll: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct SkeletonBuildStats {
    pub region_id: String,
    pub leaf_stem: String,
    pub nodes: usize,
    pub edges: usize,
    pub ferry_edges: usize,
    pub secondary_edges: usize,
    pub border_nodes: usize,
    pub ferry_terminals: usize,
    pub build_ms: u64,
    pub file_bytes: u64,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoarseJoint {
    pub lat: f64,
    pub lon: f64,
    pub osm_id: i64,
    /// `start` | `end` | `via` | `border_crossing` | `ferry_terminal`
    pub joint_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoarseFerryLeg {
    pub from_lat: f64,
    pub from_lon: f64,
    pub to_lat: f64,
    pub to_lon: f64,
    pub from_terminal: String,
    pub to_terminal: String,
    pub km: f64,
    pub crossing_min: f64,
    pub boarding_min: f64,
    pub total_ferry_min: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoarseCountrySlice {
    pub iso: String,
    pub km: f64,
    pub driving_min: f64,
    pub ferry_min: f64,
    pub roads: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoarseRouteReport {
    pub name: String,
    pub profile: String,
    pub total_km: f64,
    pub driving_min: f64,
    pub ferry_min: f64,
    pub total_min: f64,
    /// Sum of edge `base_weight` along the path (A* objective; drive-equiv metres).
    pub astar_cost_m: f64,
    pub search_ms: u64,
    pub path_nodes: usize,
    pub merged_nodes: usize,
    pub merged_edges: usize,
    pub peak_rss_mb: u64,
    pub countries: Vec<CoarseCountrySlice>,
    pub joints: Vec<CoarseJoint>,
    pub ferries: Vec<CoarseFerryLeg>,
    pub note: String,
}

/// True when a secondary (or link) edge should stay because it touches a ferry
/// terminal or a border-crossing node already in the major skeleton.
pub fn secondary_connects_anchor(
    highway: &str,
    src: u32,
    tgt: u32,
    anchors: &HashSet<u32>,
) -> bool {
    if !matches!(highway, "secondary" | "secondary_link") {
        return false;
    }
    anchors.contains(&src) || anchors.contains(&tgt)
}

/// True when a non-ferry edge touches a ferry terminal (pier / harbour approach).
/// Matches pack densify: keep the stub regardless of highway class so terminals
/// are not left reachable only via the water edge.
pub fn pier_connects_ferry_terminal(
    is_ferry: bool,
    src: u32,
    tgt: u32,
    ferry_terminals: &HashSet<u32>,
) -> bool {
    if is_ferry {
        return false;
    }
    ferry_terminals.contains(&src) || ferry_terminals.contains(&tgt)
}

/// OSM ids of nodes that appear in at least two region id-sets (land borders
/// and shared ferry terminals).
pub fn shared_osm_ids_across_regions(region_node_ids: &[HashSet<i64>]) -> HashSet<i64> {
    let mut seen: HashMap<i64, u8> = HashMap::new();
    let mut shared = HashSet::new();
    for set in region_node_ids {
        for &oid in set {
            let e = seen.entry(oid).or_insert(0);
            *e = e.saturating_add(1);
            if *e == 2 {
                shared.insert(oid);
            }
        }
    }
    shared
}

/// Collect OSM node ids incident to major (densify-skeleton) or ferry edges.
pub fn major_node_osm_ids(pack: &FlatGraphPack) -> HashSet<i64> {
    let mut out = HashSet::new();
    for i in 0..pack.edge_src.len() {
        let hw = pack.edge_highway[i].as_str();
        let ferry = pack.edge_is_ferry.get(i).copied().unwrap_or(0) != 0;
        if !densify_skeleton_edge(hw, ferry) {
            continue;
        }
        let s = pack.edge_src[i] as usize;
        let t = pack.edge_tgt[i] as usize;
        out.insert(pack.node_ids[s]);
        out.insert(pack.node_ids[t]);
    }
    out
}

/// Max land hops when growing pier approaches from a ferry terminal to the
/// major (motorway/trunk/primary) skeleton. Harbour stubs are often 2–4 edges
/// of service/unclassified before they meet a primary.
pub const FERRY_APPROACH_MAX_HOPS: usize = 12;

/// If a ferry terminal still has no land path to a major road after pier growth,
/// stitch a synthetic approach to the nearest major node within this radius.
/// Puttgarden pier OSM nodes sit ~1.5 km from B 207 / Fährhafenstraße when the
/// intervening service chain is missing from pack+overlay, so 750 m was too tight.
pub const FERRY_STITCH_MAX_M: f64 = 2_500.0;

/// Dual-carriageway motorway/trunk pairs sit about this far apart. Snaps that
/// land on the opposing oneway force multi-ten-km ring detours in directed
/// coarse search; stitch a short bidirectional link when headings oppose.
pub const OPPOSITE_CARRIAGEWAY_MIN_M: f64 = 8.0;
pub const OPPOSITE_CARRIAGEWAY_MAX_M: f64 = 45.0;

/// Degrees inward from the region AABB where secondary roads are kept so
/// landsdel / län cuts that lack shared primary OSM ids still meet (FU23
/// jamtland↔dalarna ~25 km primary-only gap).
pub const BORDER_BAND_SECONDARY_DEG: f64 = 0.40;

fn node_near_region_rim(lat: f64, lon: f64, bbox: [f64; 4], band: f64) -> bool {
    lat <= bbox[0] + band
        || lat >= bbox[2] - band
        || lon <= bbox[1] + band
        || lon >= bbox[3] - band
}

/// Build skeleton membership for one flat pack tile/region.
///
/// Pass 1: motorway/trunk/primary + ferry (same as densify skeleton).
/// Pass 2: mark ferry terminals and nodes whose OSM ids are in `border_osm_ids`.
/// Pass 3: grow land approaches from each ferry terminal through any non-ferry
///         highway until a major-skeleton node is reached (hop-capped).
/// Pass 4: secondary edges that touch ferry or border anchors.
/// Pass 5: secondary edges with an endpoint in the region-rim band (closes
///         pack cuts where primary networks do not share OSM ids).
pub fn select_skeleton_edge_indices(
    pack: &FlatGraphPack,
    border_osm_ids: &HashSet<i64>,
) -> (HashSet<usize>, HashSet<u32>, HashSet<u32>) {
    select_skeleton_edge_indices_for_region(pack, border_osm_ids, None)
}

/// Like [`select_skeleton_edge_indices`] with optional region id for rim secondary.
pub fn select_skeleton_edge_indices_for_region(
    pack: &FlatGraphPack,
    border_osm_ids: &HashSet<i64>,
    region_id: Option<&str>,
) -> (HashSet<usize>, HashSet<u32>, HashSet<u32>) {
    let n = pack.edge_src.len();
    let mut major: HashSet<usize> = HashSet::new();
    let mut ferry_terminals: HashSet<u32> = HashSet::new();
    let mut major_nodes: HashSet<u32> = HashSet::new();
    for i in 0..n {
        let hw = pack.edge_highway[i].as_str();
        let ferry = pack.edge_is_ferry.get(i).copied().unwrap_or(0) != 0;
        if densify_skeleton_edge(hw, ferry) {
            major.insert(i);
            if ferry {
                ferry_terminals.insert(pack.edge_src[i]);
                ferry_terminals.insert(pack.edge_tgt[i]);
            } else {
                major_nodes.insert(pack.edge_src[i]);
                major_nodes.insert(pack.edge_tgt[i]);
            }
        }
    }
    let mut border_nodes: HashSet<u32> = HashSet::new();
    for (idx, &oid) in pack.node_ids.iter().enumerate() {
        if border_osm_ids.contains(&oid) {
            border_nodes.insert(idx as u32);
        }
    }
    let mut anchors = ferry_terminals.clone();
    anchors.extend(border_nodes.iter().copied());
    let mut keep = major;

    // Undirected adjacency of low-class pier approaches only (service /
    // unclassified / residential / tertiary / …). Do not grow through
    // secondary — those use the explicit anchor rule below.
    let mut land_adj: HashMap<u32, Vec<(u32, usize)>> = HashMap::new();
    for i in 0..n {
        if pack.edge_is_ferry.get(i).copied().unwrap_or(0) != 0 {
            continue;
        }
        let hw = pack.edge_highway[i].as_str();
        if densify_skeleton_edge(hw, false) || matches!(hw, "secondary" | "secondary_link") {
            continue;
        }
        let s = pack.edge_src[i];
        let t = pack.edge_tgt[i];
        land_adj.entry(s).or_default().push((t, i));
        land_adj.entry(t).or_default().push((s, i));
    }
    // BFS from each ferry terminal through land edges until major nodes.
    for &term in &ferry_terminals {
        let mut visited: HashSet<u32> = HashSet::from([term]);
        let mut queue: VecDeque<(u32, usize)> = VecDeque::from([(term, 0usize)]);
        while let Some((u, hops)) = queue.pop_front() {
            if hops >= FERRY_APPROACH_MAX_HOPS {
                continue;
            }
            let Some(neigh) = land_adj.get(&u) else {
                continue;
            };
            for &(v, ei) in neigh {
                keep.insert(ei);
                if major_nodes.contains(&v) {
                    // Reached the densify skeleton; stop along this branch.
                    continue;
                }
                if visited.insert(v) {
                    queue.push_back((v, hops + 1));
                }
            }
        }
    }
    // Still keep one-hop pier stubs explicitly (covers terminals with no major
    // within hop budget that only have a single approach edge).
    for i in 0..n {
        if keep.contains(&i) {
            continue;
        }
        let ferry = pack.edge_is_ferry.get(i).copied().unwrap_or(0) != 0;
        if pier_connects_ferry_terminal(
            ferry,
            pack.edge_src[i],
            pack.edge_tgt[i],
            &ferry_terminals,
        ) {
            keep.insert(i);
        }
    }
    for i in 0..n {
        if keep.contains(&i) {
            continue;
        }
        let hw = pack.edge_highway[i].as_str();
        if secondary_connects_anchor(hw, pack.edge_src[i], pack.edge_tgt[i], &anchors) {
            keep.insert(i);
        }
    }
    // Pass 5: rim secondary — keep secondary near the region AABB so adjacent
    // packs meet when primary networks do not share OSM ids at the cut.
    if let Some(rid) = region_id {
        if let Some(bbox) = crate::routing::basemap::region_bbox(rid) {
            for i in 0..n {
                if keep.contains(&i) {
                    continue;
                }
                if pack.edge_is_ferry.get(i).copied().unwrap_or(0) != 0 {
                    continue;
                }
                let hw = pack.edge_highway[i].as_str();
                // Include secondary/tertiary/unclassified/residential on the rim
                // so Geofabrik cuts that omit shared primary OSM ids still meet
                // within metres (jamtland↔dalarna was ~7 km primary-only).
                if !matches!(
                    hw,
                    "secondary"
                        | "secondary_link"
                        | "tertiary"
                        | "tertiary_link"
                        | "unclassified"
                        | "residential"
                ) {
                    continue;
                }
                let s = pack.edge_src[i] as usize;
                let t = pack.edge_tgt[i] as usize;
                let near = node_near_region_rim(
                    pack.node_lats[s],
                    pack.node_lons[s],
                    bbox,
                    BORDER_BAND_SECONDARY_DEG,
                ) || node_near_region_rim(
                    pack.node_lats[t],
                    pack.node_lons[t],
                    bbox,
                    BORDER_BAND_SECONDARY_DEG,
                );
                if near {
                    keep.insert(i);
                }
            }
        }
    }
    (keep, ferry_terminals, border_nodes)
}

pub fn build_skeleton_from_pack(
    pack: &FlatGraphPack,
    region_id: &str,
    leaf_stem: &str,
    profile: &str,
    border_osm_ids: &HashSet<i64>,
) -> CorridorSkeletonFile {
    let t0 = Instant::now();
    let (keep, ferry_terminals, border_nodes) =
        select_skeleton_edge_indices_for_region(pack, border_osm_ids, Some(region_id));
    let mut used_nodes: HashMap<u32, u32> = HashMap::new();
    let mut node_ids = Vec::new();
    let mut node_lats = Vec::new();
    let mut node_lons = Vec::new();
    let mut node_is_border = Vec::new();
    let mut node_is_ferry_terminal = Vec::new();
    let mut remap = |used: &mut HashMap<u32, u32>, old: u32| -> u32 {
        if let Some(&n) = used.get(&old) {
            return n;
        }
        let n = node_ids.len() as u32;
        used.insert(old, n);
        let i = old as usize;
        node_ids.push(pack.node_ids[i]);
        node_lats.push(pack.node_lats[i]);
        node_lons.push(pack.node_lons[i]);
        node_is_border.push(u8::from(border_nodes.contains(&old)));
        node_is_ferry_terminal.push(u8::from(ferry_terminals.contains(&old)));
        n
    };
    let mut edge_src = Vec::new();
    let mut edge_tgt = Vec::new();
    let mut edge_length_m = Vec::new();
    let mut edge_base_weight = Vec::new();
    let mut edge_highway = Vec::new();
    let mut edge_name = Vec::new();
    let mut edge_road_ref = Vec::new();
    let mut edge_is_oneway = Vec::new();
    let mut edge_is_ferry = Vec::new();
    let mut edge_is_tunnel = Vec::new();
    let mut edge_is_toll = Vec::new();
    let mut ferry_edge_count = 0u32;
    let mut secondary_edge_count = 0u32;
    let mut keep_sorted: Vec<usize> = keep.into_iter().collect();
    keep_sorted.sort_unstable();
    for i in keep_sorted {
        let s = remap(&mut used_nodes, pack.edge_src[i]);
        let t = remap(&mut used_nodes, pack.edge_tgt[i]);
        edge_src.push(s);
        edge_tgt.push(t);
        edge_length_m.push(pack.edge_length_m[i]);
        edge_base_weight.push(pack.edge_base_weight[i]);
        let hw = pack.edge_highway[i].clone();
        if matches!(hw.as_str(), "secondary" | "secondary_link") {
            secondary_edge_count += 1;
        }
        let ferry = pack.edge_is_ferry.get(i).copied().unwrap_or(0);
        if ferry != 0 {
            ferry_edge_count += 1;
        }
        edge_highway.push(hw);
        edge_name.push(
            pack.edge_name
                .get(i)
                .cloned()
                .unwrap_or_default(),
        );
        edge_road_ref.push(
            pack.edge_road_ref
                .get(i)
                .cloned()
                .unwrap_or_default(),
        );
        edge_is_oneway.push(pack.edge_is_oneway.get(i).copied().unwrap_or(0));
        edge_is_ferry.push(ferry);
        edge_is_tunnel.push(pack.edge_is_tunnel.get(i).copied().unwrap_or(0));
        edge_is_toll.push(pack.edge_is_toll.get(i).copied().unwrap_or(0));
    }
    let build_ms = t0.elapsed().as_millis() as u64;
    let mut skel = CorridorSkeletonFile {
        format_version: CORRIDOR_SKELETON_FORMAT_VERSION,
        pack_format_version: GRAPH_FORMAT_VERSION,
        region_id: region_id.to_string(),
        leaf_stem: leaf_stem.to_string(),
        profile: profile.to_string(),
        node_count: node_ids.len() as u32,
        edge_count: edge_src.len() as u32,
        ferry_edge_count,
        secondary_edge_count,
        border_node_count: border_nodes.len() as u32,
        ferry_terminal_count: ferry_terminals.len() as u32,
        build_ms,
        node_ids,
        node_lats,
        node_lons,
        node_is_border,
        node_is_ferry_terminal,
        edge_src,
        edge_tgt,
        edge_length_m,
        edge_base_weight,
        edge_highway,
        edge_name,
        edge_road_ref,
        edge_is_oneway,
        edge_is_ferry,
        edge_is_tunnel,
        edge_is_toll,
    };
    stitch_orphaned_ferry_terminals(&mut skel, FERRY_STITCH_MAX_M);
    stitch_opposite_carriageways(
        &mut skel,
        OPPOSITE_CARRIAGEWAY_MIN_M,
        OPPOSITE_CARRIAGEWAY_MAX_M,
    );
    skel
}

/// When pier stubs never meet motorway/trunk/primary (common at ferry harbours
/// where overlay approaches and pack primaries share no OSM id), add a short
/// synthetic land edge from the terminal to the nearest major node.
pub fn stitch_orphaned_ferry_terminals(skel: &mut CorridorSkeletonFile, max_m: f64) -> u32 {
    let n = skel.node_ids.len();
    if n == 0 || skel.edge_src.is_empty() {
        return 0;
    }
    let mut major_nodes: HashSet<usize> = HashSet::new();
    let mut land_und: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..skel.edge_src.len() {
        let s = skel.edge_src[i] as usize;
        let t = skel.edge_tgt[i] as usize;
        let fer = skel.edge_is_ferry.get(i).copied().unwrap_or(0) != 0;
        if !fer {
            land_und.entry(s).or_default().push(t);
            land_und.entry(t).or_default().push(s);
            let hw = skel.edge_highway.get(i).map(|s| s.as_str()).unwrap_or("");
            if densify_skeleton_edge(hw, false) {
                major_nodes.insert(s);
                major_nodes.insert(t);
            }
        }
    }
    let terms: Vec<usize> = (0..n)
        .filter(|&i| skel.node_is_ferry_terminal.get(i).copied().unwrap_or(0) != 0)
        .collect();
    let mut stitched = 0u32;
    for &term in &terms {
        // Land BFS: already reaches a major?
        let mut seen = HashSet::from([term]);
        let mut q = VecDeque::from([term]);
        let mut reaches_major = major_nodes.contains(&term);
        while let Some(u) = q.pop_front() {
            if major_nodes.contains(&u) {
                reaches_major = true;
                break;
            }
            for &v in land_und.get(&u).into_iter().flatten() {
                if seen.insert(v) {
                    q.push_back(v);
                }
            }
        }
        if reaches_major {
            continue;
        }
        let tlat = skel.node_lats[term];
        let tlon = skel.node_lons[term];
        let mut best: Option<(f64, usize)> = None;
        for &mj in &major_nodes {
            let d = haversine_m(tlat, tlon, skel.node_lats[mj], skel.node_lons[mj]);
            if d <= max_m && best.map(|(bd, _)| d < bd).unwrap_or(true) {
                best = Some((d, mj));
            }
        }
        let Some((len_m, mj)) = best else {
            continue;
        };
        // Bidirectional synthetic approach (service).
        for (s, t) in [(term as u32, mj as u32), (mj as u32, term as u32)] {
            skel.edge_src.push(s);
            skel.edge_tgt.push(t);
            skel.edge_length_m.push(len_m);
            skel.edge_base_weight.push(len_m);
            skel.edge_highway.push("service".into());
            skel.edge_name.push("ferry_terminal_stitch".into());
            skel.edge_road_ref.push(String::new());
            skel.edge_is_oneway.push(0);
            skel.edge_is_ferry.push(0);
            skel.edge_is_tunnel.push(0);
            skel.edge_is_toll.push(0);
        }
        land_und.entry(term).or_default().push(mj);
        land_und.entry(mj).or_default().push(term);
        stitched += 1;
    }
    if stitched > 0 {
        skel.edge_count = skel.edge_src.len() as u32;
        skel.node_count = skel.node_ids.len() as u32;
    }
    stitched
}

fn is_dual_carriageway_highway(hw: &str) -> bool {
    matches!(hw, "motorway" | "trunk")
}

fn normalize_road_ref(raw: &str) -> String {
    raw.split(';')
        .next()
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn bearing_rad(lat0: f64, lon0: f64, lat1: f64, lon1: f64) -> f64 {
    let dlon = (lon1 - lon0).to_radians();
    let la0 = lat0.to_radians();
    let la1 = lat1.to_radians();
    let y = dlon.sin() * la1.cos();
    let x = la0.cos() * la1.sin() - la0.sin() * la1.cos() * dlon.cos();
    y.atan2(x)
}

fn headings_oppose(a: f64, b: f64) -> bool {
    let mut d = (a - b).abs();
    if d > std::f64::consts::PI {
        d = 2.0 * std::f64::consts::PI - d;
    }
    // > ~100° apart ⇒ opposing carriageways, not the same-direction lane.
    d > 100.0_f64.to_radians()
}

/// Link opposite oneway motorway/trunk carriageways that lie within
/// `[min_m, max_m]`. Place-agnostic: any dual carriageway where a snap lands on
/// the wrong oneway otherwise forces ring-road detours in directed search.
pub fn stitch_opposite_carriageways(
    skel: &mut CorridorSkeletonFile,
    min_m: f64,
    max_m: f64,
) -> u32 {
    let n = skel.node_ids.len();
    if n < 2 || skel.edge_src.is_empty() {
        return 0;
    }
    // Outbound heading + road ref for motorway/trunk endpoints (oneway only).
    let mut heading: Vec<Option<f64>> = vec![None; n];
    let mut node_ref: Vec<String> = vec![String::new(); n];
    let mut dual_nodes: Vec<usize> = Vec::new();
    let mut already: HashSet<(u32, u32)> = HashSet::new();
    for i in 0..skel.edge_src.len() {
        let hw = skel.edge_highway.get(i).map(|s| s.as_str()).unwrap_or("");
        if !is_dual_carriageway_highway(hw) {
            continue;
        }
        let s = skel.edge_src[i] as usize;
        let t = skel.edge_tgt[i] as usize;
        already.insert((s as u32, t as u32));
        let r = normalize_road_ref(skel.edge_road_ref.get(i).map(|s| s.as_str()).unwrap_or(""));
        if r.is_empty() {
            continue;
        }
        let oneway = skel.edge_is_oneway.get(i).copied().unwrap_or(0) != 0;
        if !oneway {
            continue;
        }
        let br = bearing_rad(
            skel.node_lats[s],
            skel.node_lons[s],
            skel.node_lats[t],
            skel.node_lons[t],
        );
        // Travel heading applies at both ends of the oneway (traffic continues
        // through the destination). Destinations need a heading so they can
        // pair with the opposite carriageway ~20 m away.
        for &n in &[s, t] {
            if heading[n].is_none() {
                heading[n] = Some(br);
                dual_nodes.push(n);
            }
            if node_ref[n].is_empty() {
                node_ref[n] = r.clone();
            }
        }
    }
    if dual_nodes.len() < 2 {
        return 0;
    }
    // Grid bucket ~ max_m so candidates share a cell or neighbour.
    let cell = max_m.max(1.0);
    let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    for &idx in &dual_nodes {
        // Approximate metres: 1° lat ≈ 111_320 m; lon scaled by cos(lat).
        let lat = skel.node_lats[idx];
        let lon = skel.node_lons[idx];
        let y = (lat * 111_320.0 / cell).floor() as i32;
        let x = (lon * 111_320.0 * lat.to_radians().cos() / cell).floor() as i32;
        grid.entry((x, y)).or_default().push(idx);
    }
    let mut pairs: Vec<(usize, usize, f64)> = Vec::new();
    let mut paired: HashSet<(u32, u32)> = HashSet::new();
    for &a in &dual_nodes {
        let Some(ha) = heading[a] else {
            continue;
        };
        let ra = node_ref[a].as_str();
        if ra.is_empty() {
            continue;
        }
        let lat = skel.node_lats[a];
        let lon = skel.node_lons[a];
        let y0 = (lat * 111_320.0 / cell).floor() as i32;
        let x0 = (lon * 111_320.0 * lat.to_radians().cos() / cell).floor() as i32;
        let mut best: Option<(f64, usize)> = None;
        for dx in -1..=1 {
            for dy in -1..=1 {
                let Some(bucket) = grid.get(&(x0 + dx, y0 + dy)) else {
                    continue;
                };
                for &b in bucket {
                    if b <= a {
                        continue;
                    }
                    if node_ref[b] != ra {
                        continue;
                    }
                    let Some(hb) = heading[b] else {
                        continue;
                    };
                    if !headings_oppose(ha, hb) {
                        continue;
                    }
                    let d = haversine_m(lat, lon, skel.node_lats[b], skel.node_lons[b]);
                    if d < min_m || d > max_m {
                        continue;
                    }
                    if already.contains(&(a as u32, b as u32))
                        || already.contains(&(b as u32, a as u32))
                    {
                        continue;
                    }
                    if best.map(|(bd, _)| d < bd).unwrap_or(true) {
                        best = Some((d, b));
                    }
                }
            }
        }
        if let Some((d, b)) = best {
            let key = if a < b {
                (a as u32, b as u32)
            } else {
                (b as u32, a as u32)
            };
            if paired.insert(key) {
                pairs.push((a, b, d));
            }
        }
    }
    let mut stitched = 0u32;
    for (a, b, len_m) in pairs {
        // Slight penalty vs true ramps so existing motorway_link stays preferred.
        let weight = len_m * 1.25;
        for (s, t) in [(a as u32, b as u32), (b as u32, a as u32)] {
            skel.edge_src.push(s);
            skel.edge_tgt.push(t);
            skel.edge_length_m.push(len_m);
            skel.edge_base_weight.push(weight);
            skel.edge_highway.push("motorway_link".into());
            skel.edge_name.push("skeleton_carriageway_link".into());
            skel.edge_road_ref.push(String::new());
            skel.edge_is_oneway.push(0);
            skel.edge_is_ferry.push(0);
            skel.edge_is_tunnel.push(0);
            skel.edge_is_toll.push(0);
        }
        stitched += 1;
    }
    if stitched > 0 {
        skel.edge_count = skel.edge_src.len() as u32;
    }
    stitched
}

/// Merge several skeleton fragments (e.g. per-tile) that share the same region.
pub fn merge_skeleton_files(parts: Vec<CorridorSkeletonFile>) -> Option<CorridorSkeletonFile> {
    if parts.is_empty() {
        return None;
    }
    if parts.len() == 1 {
        let mut skel = parts.into_iter().next().unwrap();
        stitch_orphaned_ferry_terminals(&mut skel, FERRY_STITCH_MAX_M);
        stitch_opposite_carriageways(
            &mut skel,
            OPPOSITE_CARRIAGEWAY_MIN_M,
            OPPOSITE_CARRIAGEWAY_MAX_M,
        );
        return Some(skel);
    }
    let first = &parts[0];
    let mut pack = FlatGraphPack {
        has_delta_h: false,
        node_ids: Vec::new(),
        node_lats: Vec::new(),
        node_lons: Vec::new(),
        edge_src: Vec::new(),
        edge_tgt: Vec::new(),
        edge_length_m: Vec::new(),
        edge_base_weight: Vec::new(),
        edge_delta_h_m: Vec::new(),
        edge_start_lat: Vec::new(),
        edge_start_lon: Vec::new(),
        edge_end_lat: Vec::new(),
        edge_end_lon: Vec::new(),
        edge_highway: Vec::new(),
        edge_maxspeed_kmh: Vec::new(),
        edge_maxspeed_practical_kmh: Vec::new(),
        edge_maxspeed_advisory_kmh: Vec::new(),
        edge_maxspeed_type: Vec::new(),
        edge_maxspeed_variable: Vec::new(),
        edge_minspeed_kmh: Vec::new(),
        edge_name: Vec::new(),
        edge_road_ref: Vec::new(),
        edge_is_motorroad: Vec::new(),
        edge_is_expressway: Vec::new(),
        edge_is_oneway: Vec::new(),
        edge_lanes: Vec::new(),
        edge_maxweight_t: Vec::new(),
        edge_maxaxleload_t: Vec::new(),
        edge_maxbogieweight_t: Vec::new(),
        edge_maxheight_m: Vec::new(),
        edge_maxwidth_m: Vec::new(),
        edge_maxlength_m: Vec::new(),
        edge_is_toll: Vec::new(),
        edge_is_ferry: Vec::new(),
        edge_is_tunnel: Vec::new(),
        edge_is_roundabout: Vec::new(),
        edge_is_boardwalk: Vec::new(),
        edge_shape_offsets: vec![0],
        edge_shape_lons: Vec::new(),
        edge_shape_lats: Vec::new(),
        edge_motor_vehicle_conditional: Vec::new(),
        edge_access_conditional: Vec::new(),
        edge_maxspeed_conditional: Vec::new(),
        edge_access_forbidden: Vec::new(),
        edge_surface_quality: Vec::new(),
        node_access_blocked: Vec::new(),
    };
    let mut osm_to_idx: HashMap<i64, u32> = HashMap::new();
    let mut border_osm: HashSet<i64> = HashSet::new();
    let mut build_ms = 0u64;
    for part in &parts {
        build_ms += part.build_ms;
        for (i, &oid) in part.node_ids.iter().enumerate() {
            if part.node_is_border.get(i).copied().unwrap_or(0) != 0 {
                border_osm.insert(oid);
            }
            if osm_to_idx.contains_key(&oid) {
                continue;
            }
            let idx = pack.node_ids.len() as u32;
            osm_to_idx.insert(oid, idx);
            pack.node_ids.push(oid);
            pack.node_lats.push(part.node_lats[i]);
            pack.node_lons.push(part.node_lons[i]);
            pack.node_access_blocked.push(0);
        }
        for e in 0..part.edge_src.len() {
            let s_osm = part.node_ids[part.edge_src[e] as usize];
            let t_osm = part.node_ids[part.edge_tgt[e] as usize];
            let (Some(&s), Some(&t)) = (osm_to_idx.get(&s_osm), osm_to_idx.get(&t_osm)) else {
                continue;
            };
            pack.edge_src.push(s);
            pack.edge_tgt.push(t);
            pack.edge_length_m.push(part.edge_length_m[e]);
            pack.edge_base_weight.push(part.edge_base_weight[e]);
            pack.edge_start_lat.push(part.node_lats[part.edge_src[e] as usize]);
            pack.edge_start_lon.push(part.node_lons[part.edge_src[e] as usize]);
            pack.edge_end_lat.push(part.node_lats[part.edge_tgt[e] as usize]);
            pack.edge_end_lon.push(part.node_lons[part.edge_tgt[e] as usize]);
            pack.edge_highway.push(part.edge_highway[e].clone());
            pack.edge_name.push(
                part.edge_name
                    .get(e)
                    .cloned()
                    .unwrap_or_default(),
            );
            pack.edge_road_ref.push(
                part.edge_road_ref
                    .get(e)
                    .cloned()
                    .unwrap_or_default(),
            );
            pack.edge_is_oneway
                .push(part.edge_is_oneway.get(e).copied().unwrap_or(0));
            pack.edge_is_ferry
                .push(part.edge_is_ferry.get(e).copied().unwrap_or(0));
            pack.edge_is_tunnel
                .push(part.edge_is_tunnel.get(e).copied().unwrap_or(0));
            pack.edge_is_toll
                .push(part.edge_is_toll.get(e).copied().unwrap_or(0));
            pack.edge_maxspeed_kmh.push(f64::NAN);
            pack.edge_maxspeed_practical_kmh.push(f64::NAN);
            pack.edge_maxspeed_advisory_kmh.push(f64::NAN);
            pack.edge_maxspeed_type.push(String::new());
            pack.edge_maxspeed_variable.push(0);
            pack.edge_minspeed_kmh.push(f64::NAN);
            pack.edge_is_motorroad.push(0);
            pack.edge_is_expressway.push(0);
            pack.edge_lanes.push(0);
            pack.edge_maxweight_t.push(f64::NAN);
            pack.edge_maxaxleload_t.push(f64::NAN);
            pack.edge_maxbogieweight_t.push(f64::NAN);
            pack.edge_maxheight_m.push(f64::NAN);
            pack.edge_maxwidth_m.push(f64::NAN);
            pack.edge_maxlength_m.push(f64::NAN);
            pack.edge_is_roundabout.push(0);
            pack.edge_is_boardwalk.push(0);
            pack.edge_motor_vehicle_conditional.push(String::new());
            pack.edge_access_conditional.push(String::new());
            pack.edge_maxspeed_conditional.push(String::new());
            pack.edge_access_forbidden.push(0);
            pack.edge_surface_quality.push(0);
            let off = pack.edge_shape_offsets.last().copied().unwrap_or(0);
            pack.edge_shape_offsets.push(off);
        }
    }
    let mut skel = build_skeleton_from_pack(
        &pack,
        &first.region_id,
        &first.leaf_stem,
        &first.profile,
        &border_osm,
    );
    // Re-mark borders from union (build_skeleton_from_pack only sees pack-local indices).
    for (i, oid) in skel.node_ids.iter().enumerate() {
        if border_osm.contains(oid) {
            if i < skel.node_is_border.len() {
                skel.node_is_border[i] = 1;
            }
        }
    }
    skel.border_node_count = skel.node_is_border.iter().filter(|&&b| b != 0).count() as u32;
    skel.build_ms = build_ms;
    stitch_orphaned_ferry_terminals(&mut skel, FERRY_STITCH_MAX_M);
    stitch_opposite_carriageways(
        &mut skel,
        OPPOSITE_CARRIAGEWAY_MIN_M,
        OPPOSITE_CARRIAGEWAY_MAX_M,
    );
    Some(skel)
}

pub fn write_skeleton_file(dir: &Path, skel: &CorridorSkeletonFile) -> std::io::Result<u64> {
    let path = skeleton_path(dir, &skel.leaf_stem);
    let f = File::create(&path)?;
    let mut w = BufWriter::new(f);
    serde_json::to_writer(&mut w, skel).map_err(std::io::Error::other)?;
    w.flush()?;
    Ok(std::fs::metadata(path)?.len())
}

pub fn read_skeleton_file(path: &Path) -> std::io::Result<CorridorSkeletonFile> {
    let f = File::open(path)?;
    let r = BufReader::new(f);
    serde_json::from_reader(r).map_err(std::io::Error::other)
}

/// Build + write one region skeleton; returns stats for the Stage A report.
pub fn build_and_write_region_skeleton(
    pack: &FlatGraphPack,
    out_dir: &Path,
    region_id: &str,
    leaf_stem: &str,
    profile: &str,
    border_osm_ids: &HashSet<i64>,
) -> std::io::Result<SkeletonBuildStats> {
    let skel = build_skeleton_from_pack(pack, region_id, leaf_stem, profile, border_osm_ids);
    let file_bytes = write_skeleton_file(out_dir, &skel)?;
    Ok(SkeletonBuildStats {
        region_id: region_id.to_string(),
        leaf_stem: leaf_stem.to_string(),
        nodes: skel.node_count as usize,
        edges: skel.edge_count as usize,
        ferry_edges: skel.ferry_edge_count as usize,
        secondary_edges: skel.secondary_edge_count as usize,
        border_nodes: skel.border_node_count as usize,
        ferry_terminals: skel.ferry_terminal_count as usize,
        build_ms: skel.build_ms,
        file_bytes,
        path: skeleton_path(out_dir, leaf_stem),
    })
}

/// Convert a persistent skeleton into a directed [`RouteGraph`] for coarse A*.
pub fn skeleton_to_route_graph(skel: &CorridorSkeletonFile, profile: RoutingProfile) -> RouteGraph {
    let mut nodes: HashMap<NodeId, Node> = HashMap::with_capacity(skel.node_ids.len());
    for i in 0..skel.node_ids.len() {
        let id = NodeId(skel.node_ids[i]);
        nodes.insert(
            id,
            Node {
                id,
                coord: Coord {
                    x: skel.node_lons[i],
                    y: skel.node_lats[i],
                },
                uses: 2,
            },
        );
    }
    let mut edges = Vec::with_capacity(skel.edge_src.len());
    for i in 0..skel.edge_src.len() {
        let src = NodeId(skel.node_ids[skel.edge_src[i] as usize]);
        let tgt = NodeId(skel.node_ids[skel.edge_tgt[i] as usize]);
        let hw = skel.edge_highway.get(i).map(|s| s.as_str()).unwrap_or("");
        let name = skel.edge_name.get(i).map(|s| s.as_str()).unwrap_or("");
        let road_ref = skel.edge_road_ref.get(i).map(|s| s.as_str()).unwrap_or("");
        let ferry = skel.edge_is_ferry.get(i).copied().unwrap_or(0) != 0;
        edges.push(GraphEdge {
            id: format!("skel-{}-{}-{}", src.0, tgt.0, i),
            source: src,
            target: tgt,
            length_m: skel.edge_length_m[i],
            base_weight: skel.edge_base_weight[i],
            cost_mult: 1.0,
            eco_weight: Some(skel.edge_base_weight[i]),
            start_lat: skel.node_lats[skel.edge_src[i] as usize],
            start_lon: skel.node_lons[skel.edge_src[i] as usize],
            end_lat: skel.node_lats[skel.edge_tgt[i] as usize],
            end_lon: skel.node_lons[skel.edge_tgt[i] as usize],
            shape: vec![
                (
                    skel.node_lons[skel.edge_src[i] as usize],
                    skel.node_lats[skel.edge_src[i] as usize],
                ),
                (
                    skel.node_lons[skel.edge_tgt[i] as usize],
                    skel.node_lats[skel.edge_tgt[i] as usize],
                ),
            ],
            highway: if hw.is_empty() {
                None
            } else {
                Some(hw.to_string())
            },
            maxspeed_kmh: None,
            maxspeed_practical_kmh: None,
            maxspeed_advisory_kmh: None,
            maxspeed_type: None,
            maxspeed_variable: false,
            minspeed_kmh: None,
            name: if name.is_empty() {
                None
            } else {
                Some(name.to_string())
            },
            road_ref: if road_ref.is_empty() {
                None
            } else {
                Some(road_ref.to_string())
            },
            is_motorroad: false,
            is_expressway: false,
            is_oneway: skel.edge_is_oneway.get(i).copied().unwrap_or(0) != 0,
            lanes: None,
            maxweight_t: None,
            maxaxleload_t: None,
            maxbogieweight_t: None,
            maxheight_m: None,
            maxwidth_m: None,
            maxlength_m: None,
            is_toll: skel.edge_is_toll.get(i).copied().unwrap_or(0) != 0,
            is_ferry: ferry,
            ferry_interval_min: None,
            is_tunnel: skel.edge_is_tunnel.get(i).copied().unwrap_or(0) != 0,
            is_boardwalk_crossing: false,
            is_roundabout: false,
            motor_vehicle_conditional: None,
            access_conditional: None,
            maxspeed_conditional: None,
            access_forbidden: false,
            surface_quality: SurfaceQuality::Good,
        });
    }
    RouteGraph::from_parts(nodes, edges, profile)
}

/// Max distance for synthetic land links when Geofabrik / skeleton cuts leave
/// no shared OSM id. Border joins must be real road cuts (metres), not km-scale
/// bridges. Missing low-class border roads should be pulled into the skeleton
/// instead of inventing long stitch edges (FU24).
pub const ADJACENT_SKELETON_STITCH_MAX_M: f64 = 250.0;

/// Intra-region weak-component cuts (missing secondary inside one extract) may
/// still need a slightly longer bridge than a border cut.
pub const INTRA_SKELETON_STITCH_MAX_M: f64 = 2_000.0;

/// Merge region skeletons on shared OSM node ids (land borders + shared ferry
/// terminals). Directed edges and `base_weight` travel-time costs are preserved.
/// After the OSM-id merge, bridge short Geofabrik cuts: (1) within one region
/// when the skeleton has multiple weak components whose nearest nodes are
/// within [`ADJACENT_SKELETON_STITCH_MAX_M`] (FU23 jamtland ~8.8 km), and
/// (2) between bbox-adjacent regions that share no OSM ids and are not already
/// weakly connected.
pub fn merge_skeletons_to_route_graph(
    skels: &[CorridorSkeletonFile],
    profile: RoutingProfile,
) -> RouteGraph {
    let graphs: Vec<RouteGraph> = skels
        .iter()
        .map(|s| skeleton_to_route_graph(s, profile))
        .collect();
    let mut graph = crate::routing::indexed::merge_tile_graphs(graphs, profile);
    let n_intra =
        stitch_intra_skeleton_component_gaps(&mut graph, skels, INTRA_SKELETON_STITCH_MAX_M);
    let n_adj = stitch_adjacent_skeleton_gaps(&mut graph, skels, ADJACENT_SKELETON_STITCH_MAX_M);
    if n_intra + n_adj > 0 {
        log::info!(
            target: "NaviPlan",
            "skeleton_adjacent_stitch intra={n_intra} adjacent={n_adj} \
             adj_max_m={ADJACENT_SKELETON_STITCH_MAX_M} intra_max_m={INTRA_SKELETON_STITCH_MAX_M}"
        );
    }
    graph
}

/// Push a bidirectional trunk-equivalent stitch edge pair onto `graph.edges`.
fn push_skeleton_stitch_edges(
    graph: &mut RouteGraph,
    id_a: i64,
    id_b: i64,
    alat: f64,
    alon: f64,
    blat: f64,
    blon: f64,
    d: f64,
    tag: &str,
) {
    let na = NodeId(id_a);
    let nb = NodeId(id_b);
    for (src, tgt, sla, slo, ela, elo) in [
        (na, nb, alat, alon, blat, blon),
        (nb, na, blat, blon, alat, alon),
    ] {
        graph.edges.push(GraphEdge {
            id: format!("{tag}-{src}-{tgt}", src = src.0, tgt = tgt.0),
            source: src,
            target: tgt,
            length_m: d,
            base_weight: d,
            cost_mult: 1.0,
            eco_weight: Some(d),
            start_lat: sla,
            start_lon: slo,
            end_lat: ela,
            end_lon: elo,
            shape: vec![(slo, sla), (elo, ela)],
            highway: Some("trunk".into()),
            maxspeed_kmh: Some(70.0),
            maxspeed_practical_kmh: None,
            maxspeed_advisory_kmh: None,
            maxspeed_type: None,
            maxspeed_variable: false,
            minspeed_kmh: None,
            name: Some("skeleton_adjacent_stitch".into()),
            road_ref: None,
            is_motorroad: false,
            is_expressway: false,
            is_oneway: false,
            lanes: None,
            maxweight_t: None,
            maxaxleload_t: None,
            maxbogieweight_t: None,
            maxheight_m: None,
            maxwidth_m: None,
            maxlength_m: None,
            is_toll: false,
            is_ferry: false,
            ferry_interval_min: None,
            is_tunnel: false,
            is_boardwalk_crossing: false,
            is_roundabout: false,
            motor_vehicle_conditional: None,
            access_conditional: None,
            maxspeed_conditional: None,
            access_forbidden: false,
            surface_quality: SurfaceQuality::Good,
        });
    }
}

fn rebuild_graph_adjacency(graph: &mut RouteGraph) {
    let profile = graph.profile();
    let nodes = std::mem::take(&mut graph.nodes);
    let edges = std::mem::take(&mut graph.edges);
    *graph = RouteGraph::from_parts(nodes, edges, profile);
}

/// Bridge weak-component cuts inside a single region skeleton (missing secondary
/// / Geofabrik edge drops). Only links components with ≥10 nodes each.
fn stitch_intra_skeleton_component_gaps(
    graph: &mut RouteGraph,
    skels: &[CorridorSkeletonFile],
    max_m: f64,
) -> u32 {
    let mut added = 0u32;
    // Ignore tiny islands; Elsa jamtland cut is 3342↔177 nodes.
    const MIN_COMP_NODES: usize = 50;
    for skel in skels {
        let skel_set: HashSet<i64> = skel.node_ids.iter().copied().collect();
        let mut parent: HashMap<i64, i64> = skel
            .node_ids
            .iter()
            .map(|&oid| (oid, oid))
            .collect();
        fn find(parent: &mut HashMap<i64, i64>, x: i64) -> i64 {
            let mut root = x;
            while parent[&root] != root {
                root = parent[&root];
            }
            let mut cur = x;
            while cur != root {
                let next = parent[&cur];
                parent.insert(cur, root);
                cur = next;
            }
            root
        }
        fn union(parent: &mut HashMap<i64, i64>, a: i64, b: i64) {
            let ra = find(parent, a);
            let rb = find(parent, b);
            if ra != rb {
                parent.insert(rb, ra);
            }
        }
        for e in &graph.edges {
            if skel_set.contains(&e.source.0) && skel_set.contains(&e.target.0) {
                union(&mut parent, e.source.0, e.target.0);
            }
        }
        let mut comps: HashMap<i64, Vec<usize>> = HashMap::new();
        for (i, &oid) in skel.node_ids.iter().enumerate() {
            let r = find(&mut parent, oid);
            comps.entry(r).or_default().push(i);
        }
        let mut large: Vec<(i64, Vec<usize>)> = comps
            .into_iter()
            .filter(|(_, idxs)| idxs.len() >= MIN_COMP_NODES)
            .collect();
        if large.len() < 2 {
            continue;
        }
        large.sort_by_key(|(r, _)| *r);
        for ci in 0..large.len() {
            for cj in (ci + 1)..large.len() {
                let ra = large[ci].0;
                let rb = large[cj].0;
                let ia = &large[ci].1;
                let ib = &large[cj].1;
                let na0 = NodeId(skel.node_ids[ia[0]]);
                let nb0 = NodeId(skel.node_ids[ib[0]]);
                if graph.same_weak_component(na0, nb0) {
                    continue;
                }
                let step_a = (ia.len() / 200).max(1);
                let step_b = (ib.len() / 200).max(1);
                let mut best: Option<(f64, i64, i64, f64, f64, f64, f64)> = None;
                for &a_i in ia.iter().step_by(step_a) {
                    for &b_i in ib.iter().step_by(step_b) {
                        let d = haversine_m(
                            skel.node_lats[a_i],
                            skel.node_lons[a_i],
                            skel.node_lats[b_i],
                            skel.node_lons[b_i],
                        );
                        if d > max_m {
                            continue;
                        }
                        if best.is_none_or(|b| d < b.0) {
                            best = Some((
                                d,
                                skel.node_ids[a_i],
                                skel.node_ids[b_i],
                                skel.node_lats[a_i],
                                skel.node_lons[a_i],
                                skel.node_lats[b_i],
                                skel.node_lons[b_i],
                            ));
                        }
                    }
                }
                let Some((d, id_a, id_b, alat, alon, blat, blon)) = best else {
                    continue;
                };
                if graph.same_weak_component(NodeId(id_a), NodeId(id_b)) {
                    continue;
                }
                push_skeleton_stitch_edges(
                    graph,
                    id_a,
                    id_b,
                    alat,
                    alon,
                    blat,
                    blon,
                    d,
                    "skeleton_intra_stitch",
                );
                added += 1;
                log::info!(
                    target: "NaviPlan",
                    "skeleton_intra_stitch {} d_m={d:.0} nodes={id_a}/{id_b} comps={ra}/{rb}",
                    skel.leaf_stem
                );
                rebuild_graph_adjacency(graph);
            }
        }
    }
    added
}

/// For each pair of bbox-adjacent skeletons, if their nearest nodes are within
/// `max_m` and not already the same OSM id, add a bidirectional link.
fn stitch_adjacent_skeleton_gaps(
    graph: &mut RouteGraph,
    skels: &[CorridorSkeletonFile],
    max_m: f64,
) -> u32 {
    if skels.len() < 2 {
        return 0;
    }
    let mut bboxes: Vec<Option<[f64; 4]>> = skels
        .iter()
        .map(|s| crate::routing::basemap::region_bbox(&s.region_id))
        .collect();
    // Fallback: node AABB when catalog bbox missing.
    for (i, s) in skels.iter().enumerate() {
        if bboxes[i].is_some() || s.node_lats.is_empty() {
            continue;
        }
        let min_lat = s.node_lats.iter().copied().fold(f64::INFINITY, f64::min);
        let max_lat = s.node_lats.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let min_lon = s.node_lons.iter().copied().fold(f64::INFINITY, f64::min);
        let max_lon = s.node_lons.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        bboxes[i] = Some([min_lat, min_lon, max_lat, max_lon]);
    }
    let mut added = 0u32;
    for i in 0..skels.len() {
        for j in (i + 1)..skels.len() {
            let (Some(a), Some(b)) = (bboxes[i], bboxes[j]) else {
                continue;
            };
            if !crate::long_trip::regions_bbox_adjacent(&a, &b, 0.20) {
                continue;
            }
            // Nearest pair (sampled). Do NOT skip when already same weak
            // component: a long installed-only detour (Turku) can unify the
            // graph while the adjacent Geofabrik cut still has zero shared
            // OSM ids — that local cut must be bridged or A* keeps the
            // absurd detour as the only cheap corridor.
            let sa = &skels[i];
            let sb = &skels[j];
            // True Geofabrik join already present.
            let share: HashSet<i64> = sa.node_ids.iter().copied().collect();
            if sb.node_ids.iter().any(|id| share.contains(id)) {
                continue;
            }
            let step_a = (sa.node_ids.len() / 400).max(1);
            let step_b = (sb.node_ids.len() / 400).max(1);
            let mut best: Option<(f64, i64, i64, f64, f64, f64, f64)> = None;
            for ia in (0..sa.node_ids.len()).step_by(step_a) {
                let na = NodeId(sa.node_ids[ia]);
                if !graph.nodes.contains_key(&na) {
                    continue;
                }
                for ib in (0..sb.node_ids.len()).step_by(step_b) {
                    let nb = NodeId(sb.node_ids[ib]);
                    if !graph.nodes.contains_key(&nb) {
                        continue;
                    }
                    if sa.node_ids[ia] == sb.node_ids[ib] {
                        continue;
                    }
                    let d = haversine_m(
                        sa.node_lats[ia],
                        sa.node_lons[ia],
                        sb.node_lats[ib],
                        sb.node_lons[ib],
                    );
                    if d > max_m {
                        continue;
                    }
                    if best.is_none_or(|b| d < b.0) {
                        best = Some((
                            d,
                            sa.node_ids[ia],
                            sb.node_ids[ib],
                            sa.node_lats[ia],
                            sa.node_lons[ia],
                            sb.node_lats[ib],
                            sb.node_lons[ib],
                        ));
                    }
                }
            }
            let Some((d, id_a, id_b, alat, alon, blat, blon)) = best else {
                continue;
            };
            push_skeleton_stitch_edges(
                graph,
                id_a,
                id_b,
                alat,
                alon,
                blat,
                blon,
                d,
                "skeleton_adjacent_stitch",
            );
            added += 1;
            log::info!(
                target: "NaviPlan",
                "skeleton_adjacent_stitch {}↔{} d_m={d:.0} nodes={id_a}/{id_b}",
                skels[i].leaf_stem,
                skels[j].leaf_stem
            );
            rebuild_graph_adjacency(graph);
        }
    }
    added
}

/// Union of OSM ids marked as border nodes across skeletons.
pub fn border_osm_from_skeletons(skels: &[CorridorSkeletonFile]) -> HashSet<i64> {
    let mut out = HashSet::new();
    for s in skels {
        for (i, &oid) in s.node_ids.iter().enumerate() {
            if s.node_is_border.get(i).copied().unwrap_or(0) != 0 {
                out.insert(oid);
            }
        }
    }
    // Also treat OSM ids present in ≥2 skeletons as borders even if unmarked.
    let sets: Vec<HashSet<i64>> = skels
        .iter()
        .map(|s| s.node_ids.iter().copied().collect())
        .collect();
    out.extend(shared_osm_ids_across_regions(&sets));
    out
}

fn haversine_m(alat: f64, alon: f64, blat: f64, blon: f64) -> f64 {
    let r = 6_371_000.0_f64;
    let dlat = (blat - alat).to_radians();
    let dlon = (blon - alon).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + alat.to_radians().cos() * blat.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

fn road_label(e: &GraphEdge) -> String {
    let r = e.road_ref.as_deref().unwrap_or("").trim();
    let n = e.name.as_deref().unwrap_or("").trim();
    match (r.is_empty(), n.is_empty()) {
        (false, false) => format!("{r} ({n})"),
        (false, true) => r.to_string(),
        (true, false) => n.to_string(),
        (true, true) => e
            .highway
            .clone()
            .unwrap_or_else(|| if e.is_ferry { "ferry".into() } else { "?".into() }),
    }
}

fn terminal_label(lat: f64, lon: f64, edge_name: Option<&str>) -> String {
    // Prefer OSM ferry name when it looks like "A - B".
    if let Some(n) = edge_name.map(str::trim).filter(|s| !s.is_empty()) {
        return n.to_string();
    }
    // Known corridor terminals (coarse report naming only).
    const KNOWN: &[(&str, f64, f64, f64)] = &[
        ("Puttgarden", 54.5028, 11.2282, 2500.0),
        ("Rodby", 54.6543, 11.3508, 2500.0),
        ("Helsingor", 56.0330, 12.6160, 2500.0),
        ("Helsingborg", 56.0433, 12.6915, 2500.0),
        ("Kvanndal", 60.4718, 6.6124, 2500.0),
        ("Utne", 60.4241, 6.6218, 2500.0),
        ("Kinsarvik", 60.3750, 6.7200, 4000.0),
    ];
    for (name, kla, klo, rad) in KNOWN {
        if haversine_m(lat, lon, *kla, *klo) <= *rad {
            return (*name).to_string();
        }
    }
    format!("{lat:.5},{lon:.5}")
}

/// Split an OSM ferry name "A - B" / "A – B" into terminals matching travel direction.
fn ferry_terminal_names(e: &GraphEdge) -> (String, String) {
    let raw = e.name.as_deref().unwrap_or("").trim();
    for sep in [" – ", " - ", " — ", "–", "-"] {
        if let Some((a, b)) = raw.split_once(sep) {
            let a = a.trim();
            let b = b.trim();
            if !a.is_empty() && !b.is_empty() {
                return (a.to_string(), b.to_string());
            }
        }
    }
    (
        terminal_label(e.start_lat, e.start_lon, None),
        terminal_label(e.end_lat, e.end_lon, None),
    )
}

fn ferry_minutes(e: &GraphEdge) -> (f64, f64, f64) {
    let (crossing, wait) = ferry_crossing_and_wait_min(e);
    (crossing, wait, crossing + wait)
}

/// Extract joints: user vias + region border crossings + ferry terminals on path.
/// No evenly spaced hop_deg samples.
///
/// A border crossing is emitted only when the path steps across a node in
/// `border_osm` **and** the country ISO (or ferry leg) changes — not at every
/// shared OSM id that happens to lie on the path inland.
pub fn extract_coarse_joints(
    graph: &RouteGraph,
    path: &[NodeId],
    edge_indices: &[usize],
    border_osm: &HashSet<i64>,
    vias: &[(f64, f64)],
    via_snap_m: f64,
) -> Vec<CoarseJoint> {
    let mut out = Vec::new();
    if path.is_empty() {
        return out;
    }
    let ferry_edge: HashSet<usize> = edge_indices
        .iter()
        .copied()
        .filter(|&i| graph.edges.get(i).is_some_and(|e| e.is_ferry))
        .collect();

    let push = |out: &mut Vec<CoarseJoint>, id: NodeId, joint_type: &str| {
        let Some(n) = graph.nodes.get(&id) else {
            return;
        };
        if out.last().is_some_and(|j| {
            j.osm_id == id.0
                && (j.joint_type == joint_type
                    || (j.joint_type == "ferry_terminal" && joint_type == "border_crossing")
                    || (j.joint_type == "border_crossing" && joint_type == "ferry_terminal"))
        }) {
            return;
        }
        out.push(CoarseJoint {
            lat: n.coord.y,
            lon: n.coord.x,
            osm_id: id.0,
            joint_type: joint_type.to_string(),
        });
    };

    push(&mut out, path[0], "start");
    for (ei, &idx) in edge_indices.iter().enumerate() {
        let Some(e) = graph.edges.get(idx) else {
            continue;
        };
        let arrive = e.target;
        if path.last() == Some(&arrive) && ei + 1 == edge_indices.len() {
            continue;
        }
        if ferry_edge.contains(&idx) {
            push(&mut out, e.source, "ferry_terminal");
            push(&mut out, e.target, "ferry_terminal");
        }
        // Border: shared OSM node where country ISO changes across this edge.
        if border_osm.contains(&arrive.0) {
            let iso_a = country_iso_for_edge(e);
            let iso_b = edge_indices
                .get(ei + 1)
                .and_then(|&j| graph.edges.get(j))
                .map(country_iso_for_edge)
                .unwrap_or_else(|| iso_a.clone());
            if iso_a != iso_b || e.is_ferry {
                push(&mut out, arrive, "border_crossing");
            }
        }
    }
    // One joint per user via: nearest path node within snap radius.
    for &(vlat, vlon) in vias {
        let mut best: Option<(NodeId, f64)> = None;
        for &id in path {
            let Some(n) = graph.nodes.get(&id) else {
                continue;
            };
            let d = haversine_m(n.coord.y, n.coord.x, vlat, vlon);
            if d > via_snap_m {
                continue;
            }
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some((id, d));
            }
        }
        if let Some((id, _)) = best {
            push(&mut out, id, "via");
        }
    }
    if let Some(&last) = path.last() {
        push(&mut out, last, "end");
    }
    out
}

fn country_iso_for_edge(e: &GraphEdge) -> String {
    let lat = (e.start_lat + e.end_lat) * 0.5;
    let lon = (e.start_lon + e.end_lon) * 0.5;
    country_iso_at(lat, lon)
        .unwrap_or_else(|| "XX".into())
        .to_ascii_uppercase()
}

/// Build the Stage A export for one coarse path.
pub fn build_coarse_route_report(
    name: &str,
    graph: &RouteGraph,
    path: &[NodeId],
    edge_indices: &[usize],
    border_osm: &HashSet<i64>,
    vias: &[(f64, f64)],
    search_ms: u64,
    peak_rss_mb: u64,
    note: &str,
) -> CoarseRouteReport {
    let mut total_km = 0.0;
    let mut ferry_min = 0.0;
    let mut astar_cost_m = 0.0;
    let mut ferries = Vec::new();
    let mut by_iso: HashMap<String, (f64, f64, f64, Vec<String>)> = HashMap::new();
    let mut on_ferry = false;
    for &idx in edge_indices {
        let e = &graph.edges[idx];
        astar_cost_m += time_base_weight_for_edge(e);
        let km = e.length_m / 1000.0;
        total_km += km;
        let iso = country_iso_for_edge(e);
        let ent = by_iso.entry(iso).or_insert_with(|| (0.0, 0.0, 0.0, Vec::new()));
        ent.0 += km;
        let label = road_label(e);
        if ent.3.last().map(|s| s.as_str()) != Some(label.as_str()) {
            ent.3.push(label);
        }
        if e.is_ferry {
            let (cross, board, tot) = ferry_minutes(e);
            let mins = if on_ferry { cross } else { tot };
            ferry_min += mins;
            ent.2 += mins;
            let (from_t, to_t) = ferry_terminal_names(e);
            ferries.push(CoarseFerryLeg {
                from_lat: e.start_lat,
                from_lon: e.start_lon,
                to_lat: e.end_lat,
                to_lon: e.end_lon,
                from_terminal: from_t,
                to_terminal: to_t,
                km,
                crossing_min: cross,
                boarding_min: if on_ferry { 0.0 } else { board },
                total_ferry_min: mins,
            });
            on_ferry = true;
        } else {
            on_ferry = false;
            let drive = motor_path_minutes_from_edges(graph, &[idx]);
            ent.1 += drive;
        }
    }
    let driving_min = motor_path_minutes_from_edges(graph, edge_indices) - ferry_min;
    let driving_min = driving_min.max(0.0);
    let total_min = driving_min + ferry_min;
    let mut countries: Vec<CoarseCountrySlice> = by_iso
        .into_iter()
        .map(|(iso, (km, dmin, fmin, roads))| CoarseCountrySlice {
            iso,
            km: (km * 100.0).round() / 100.0,
            driving_min: (dmin * 10.0).round() / 10.0,
            ferry_min: (fmin * 10.0).round() / 10.0,
            roads,
        })
        .collect();
    countries.sort_by(|a, b| a.iso.cmp(&b.iso));
    let joints = extract_coarse_joints(graph, path, edge_indices, border_osm, vias, 2_500.0);
    CoarseRouteReport {
        name: name.to_string(),
        profile: match graph.profile() {
            RoutingProfile::Truck => "truck".into(),
            RoutingProfile::Foot => "foot".into(),
            RoutingProfile::Bicycle => "bicycle".into(),
            RoutingProfile::Car => "car".into(),
        },
        total_km: (total_km * 1000.0).round() / 1000.0,
        driving_min: (driving_min * 10.0).round() / 10.0,
        ferry_min: (ferry_min * 10.0).round() / 10.0,
        total_min: (total_min * 10.0).round() / 10.0,
        astar_cost_m: (astar_cost_m * 10.0).round() / 10.0,
        search_ms,
        path_nodes: path.len(),
        merged_nodes: graph.nodes.len(),
        merged_edges: graph.edges.len(),
        peak_rss_mb,
        countries,
        joints,
        ferries,
        note: note.to_string(),
    }
}

/// Directed travel-time coarse path through optional vias.
///
/// Applies the caller's avoid flags (ferries / tolls / tunnels / motorways) so
/// the Stage B corridor matches detailed hop planning. Surface-transition state
/// stays off: on a major-road skeleton it bloated expansions and steered free
/// A* off Fehmarn onto HH.
pub fn coarse_shortest_path(
    graph: &mut RouteGraph,
    waypoints: &[(f64, f64)],
    snap_m: f64,
    route_options: &RouteOptions,
) -> Option<(Vec<NodeId>, Vec<usize>, f64)> {
    if waypoints.len() < 2 {
        return None;
    }
    graph.ensure_directed_snap_labels();
    let opts = RouteOptions {
        surface_routing_mode: Some(SurfaceRoutingMode::Offroad),
        avoid_motorways: route_options.avoid_motorways,
        toll_policy: route_options.toll_policy,
        avoid_ferries: route_options.avoid_ferries,
        avoid_tunnels: route_options.avoid_tunnels,
        vehicle: route_options.vehicle.clone(),
        departure_local: route_options.departure_local,
        allowed_countries: route_options.allowed_countries.clone(),
        ..RouteOptions::default()
    };
    let mut full_path: Vec<NodeId> = Vec::new();
    let mut full_edges: Vec<usize> = Vec::new();
    let mut total_cost = 0.0;
    for w in waypoints.windows(2) {
        let (olat, olon) = w[0];
        let (dlat, dlon) = w[1];
        let oopts = RouteOptions {
            snap_role: crate::routing::graph::SnapRole::Origin,
            ..opts.clone()
        };
        let dopts = RouteOptions {
            snap_role: crate::routing::graph::SnapRole::Destination,
            ..opts.clone()
        };
        let (sid, _) = graph
            .nearest_routable_with_options_max(olat, olon, &oopts, false, snap_m)
            .ok()?;
        let (gid, _) = graph
            .nearest_routable_with_options_max(dlat, dlon, &dopts, false, snap_m)
            .ok()?;
        let (path, edges, cost) = graph.shortest_path_with_options(sid, gid, false, &opts)?;
        if full_path.is_empty() {
            full_path = path;
            full_edges = edges;
        } else {
            if path.len() > 1 {
                full_path.extend(path.into_iter().skip(1));
            }
            full_edges.extend(edges);
        }
        total_cost += cost;
    }
    Some((full_path, full_edges, total_cost))
}

/// Inter-region ferry edges: ferry whose endpoints' nearest skeleton regions differ,
/// or whose OSM endpoints appear in two different skeletons.
pub fn inter_region_ferry_edges(
    skels: &[CorridorSkeletonFile],
) -> Vec<(String, String, f64, f64, f64, f64, f64)> {
    let mut osm_region: HashMap<i64, String> = HashMap::new();
    for s in skels {
        for &oid in &s.node_ids {
            osm_region
                .entry(oid)
                .and_modify(|e| {
                    if e != &s.region_id {
                        *e = format!("{e}|{}", s.region_id);
                    }
                })
                .or_insert_with(|| s.region_id.clone());
        }
    }
    let mut out = Vec::new();
    for s in skels {
        for i in 0..s.edge_src.len() {
            if s.edge_is_ferry.get(i).copied().unwrap_or(0) == 0 {
                continue;
            }
            let s_osm = s.node_ids[s.edge_src[i] as usize];
            let t_osm = s.node_ids[s.edge_tgt[i] as usize];
            let rs = osm_region.get(&s_osm).cloned().unwrap_or_default();
            let rt = osm_region.get(&t_osm).cloned().unwrap_or_default();
            let multi = rs.contains('|') || rt.contains('|') || (rs != rt && !rs.is_empty() && !rt.is_empty());
            if !multi {
                continue;
            }
            out.push((
                s.region_id.clone(),
                format!("{s_osm}->{t_osm}"),
                s.node_lats[s.edge_src[i] as usize],
                s.node_lons[s.edge_src[i] as usize],
                s.node_lats[s.edge_tgt[i] as usize],
                s.node_lons[s.edge_tgt[i] as usize],
                s.edge_length_m[i],
            ));
        }
    }
    out
}

/// Stage B densify result: hop joints + full coarse path for tile selection.
#[derive(Debug, Clone)]
pub struct StageBDensify {
    /// Ordered hop waypoints: origin, joints (border/ferry/via), destination.
    pub hops: Vec<(f64, f64)>,
    /// Full coarse path node lat/lon (for corridor tiles + pad).
    pub coarse_path: Vec<(f64, f64)>,
    pub total_min: f64,
    pub total_km: f64,
    pub ferries: Vec<String>,
    pub note: String,
}

fn load_skeletons_from_dirs(dirs: &[&Path]) -> Vec<CorridorSkeletonFile> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with(".navi-corridor-skeleton.json"))
            })
            .collect();
        paths.sort();
        for p in paths {
            let Ok(s) = read_skeleton_file(&p) else {
                continue;
            };
            if seen.insert(s.leaf_stem.clone()) {
                out.push(s);
            }
        }
    }
    out
}

fn forbid_ferry_edges_matching_leg(graph: &mut RouteGraph, leg: &CoarseFerryLeg, match_m: f64) {
    for e in graph.edges.iter_mut() {
        if !e.is_ferry {
            continue;
        }
        let fwd = haversine_m(e.start_lat, e.start_lon, leg.from_lat, leg.from_lon) < match_m
            && haversine_m(e.end_lat, e.end_lon, leg.to_lat, leg.to_lon) < match_m;
        let rev = haversine_m(e.start_lat, e.start_lon, leg.to_lat, leg.to_lon) < match_m
            && haversine_m(e.end_lat, e.end_lon, leg.from_lat, leg.from_lon) < match_m;
        if fwd || rev {
            e.access_forbidden = true;
        }
    }
}

fn clear_access_forbidden(graph: &mut RouteGraph) {
    for e in &mut graph.edges {
        e.access_forbidden = false;
    }
}

fn path_latlon(graph: &RouteGraph, path: &[NodeId]) -> Vec<(f64, f64)> {
    path.iter()
        .filter_map(|id| graph.nodes.get(id).map(|n| (n.coord.y, n.coord.x)))
        .collect()
}

fn hops_from_report(report: &CoarseRouteReport, start: (f64, f64), end: (f64, f64)) -> Vec<(f64, f64)> {
    let mut hops = Vec::new();
    hops.push(start);
    for j in &report.joints {
        if j.joint_type == "start" || j.joint_type == "end" {
            continue;
        }
        // Keep border, ferry terminal, and via joints only.
        if matches!(
            j.joint_type.as_str(),
            "border_crossing" | "ferry_terminal" | "via"
        ) {
            let p = (j.lat, j.lon);
            if hops
                .last()
                .is_none_or(|&q| haversine_m(q.0, q.1, p.0, p.1) > 500.0)
            {
                hops.push(p);
            }
        }
    }
    if hops
        .last()
        .is_none_or(|&q| haversine_m(q.0, q.1, end.0, end.1) > 500.0)
    {
        hops.push(end);
    } else if let Some(last) = hops.last_mut() {
        *last = end;
    }
    hops
}

/// Tile bboxes + estimated node counts from Ready manifests under [dirs].
/// Node estimate = archive bytes / 72 (empirical packed car-graph density).
fn load_profile_tile_bboxes(
    dirs: &[&Path],
    profile: RoutingProfile,
) -> Vec<(String, [f64; 4], usize)> {
    use crate::routing::indexed::NaviManifest;
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(dir) else {
            continue;
        };
        for ent in rd.flatten() {
            let path = ent.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.ends_with(".navi-manifest.json") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(man) = serde_json::from_str::<NaviManifest>(&text) else {
                continue;
            };
            let Some(tiles) = man.graph_tiles_for(profile) else {
                continue;
            };
            for t in tiles {
                if seen.insert(t.file.clone()) {
                    let bytes = std::fs::metadata(dir.join(&t.file))
                        .map(|m| m.len())
                        .unwrap_or(0);
                    // Empirical packed car-graph density (~bytes/200 nodes).
                    let est_nodes = ((bytes / 200) as usize).max(1);
                    out.push((t.file.clone(), t.bbox, est_nodes));
                }
            }
        }
    }
    out
}

fn tiles_covering_point(
    tiles: &[(String, [f64; 4], usize)],
    lat: f64,
    lon: f64,
) -> HashSet<String> {
    tiles
        .iter()
        .filter(|(_, b, _)| crate::routing::basemap::bbox_covers_point(*b, lat, lon))
        .map(|(n, _, _)| n.clone())
        .collect()
}

fn tile_est_nodes(tiles: &[(String, [f64; 4], usize)], name: &str) -> usize {
    tiles
        .iter()
        .find(|(n, _, _)| n == name)
        .map(|(_, _, e)| *e)
        .unwrap_or(1)
}

fn nearest_coarse_index(path: &[(f64, f64)], p: (f64, f64)) -> usize {
    path.iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            let da = (a.0 - p.0).hypot(a.1 - p.1);
            let db = (b.0 - p.0).hypot(b.1 - p.1);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// Split long hops so estimated packed nodes for path-covering tiles stay
/// ≤ [`crate::routing::plan_bbox::MAX_PATH_NODES_PER_HOP`]. Path tiles are never
/// dropped; joints are nodes on the coarse path.
pub fn split_hops_by_path_tile_budget(
    hops: Vec<(f64, f64)>,
    coarse_path: &[(f64, f64)],
    pack_dirs: &[&Path],
    profile: RoutingProfile,
) -> Vec<(f64, f64)> {
    let max_nodes = crate::routing::plan_bbox::MAX_PATH_NODES_PER_HOP;
    if hops.len() < 2 || coarse_path.len() < 2 {
        return hops;
    }
    let tiles = load_profile_tile_bboxes(pack_dirs, profile);
    if tiles.is_empty() {
        return hops;
    }
    let mut out: Vec<(f64, f64)> = Vec::new();
    out.push(hops[0]);
    for w in hops.windows(2) {
        let start = w[0];
        let end = w[1];
        let mut i0 = nearest_coarse_index(coarse_path, start);
        let mut i1 = nearest_coarse_index(coarse_path, end);
        if i0 > i1 {
            std::mem::swap(&mut i0, &mut i1);
        }
        let mut active: HashSet<String> = HashSet::new();
        let mut last_emit = start;
        for &p in &coarse_path[i0..=i1] {
            let cover = tiles_covering_point(&tiles, p.0, p.1);
            if cover.is_empty() {
                continue;
            }
            let mut trial = active.clone();
            trial.extend(cover.iter().cloned());
            let trial_nodes: usize = trial.iter().map(|n| tile_est_nodes(&tiles, n)).sum();
            let min_gap = crate::routing::plan_bbox::MIN_HOP_SPLIT_GAP_M;
            let gap_ok = out.last().is_none_or(|&q| {
                haversine_m(q.0, q.1, last_emit.0, last_emit.1) >= min_gap
            });
            if trial_nodes > max_nodes && !active.is_empty() && gap_ok {
                // Emit joint at previous coarse node (exact path continuity).
                out.push(last_emit);
                active = cover;
            } else {
                // Prefer a larger hop over micro-splits when tiles are huge /
                // overlapping; densify pad still keeps peak bounded.
                active = trial;
            }
            last_emit = p;
        }
        if out
            .last()
            .is_none_or(|&q| haversine_m(q.0, q.1, end.0, end.1) > 200.0)
        {
            out.push(end);
        } else if let Some(last) = out.last_mut() {
            *last = end;
        }
    }
    out
}

/// Count unique pack tiles covering [path] samples (for diagnostics).
pub fn count_path_covering_tiles(
    pack_dirs: &[&Path],
    profile: RoutingProfile,
    path: &[(f64, f64)],
) -> usize {
    let tiles = load_profile_tile_bboxes(pack_dirs, profile);
    let mut names = HashSet::new();
    for &(lat, lon) in path {
        names.extend(tiles_covering_point(&tiles, lat, lon));
    }
    names.len()
}

/// Stage B: densify from persistent corridor skeletons.
///
/// Coarse path uses the active [`RouteOptions`] (avoid ferries/tolls/tunnels/
/// motorways). When ferries are allowed: ferry-exclusion alternatives + 2%
/// near-equal (fewest ferries, then fewer km). Hop joints = border crossings,
/// ferry terminals, user vias. No centroid densify and no even hop_deg sampling.
pub fn try_stage_b_densify_from_skeletons(
    pack_dirs: &[&Path],
    waypoints: &[(f64, f64)],
    profile: RoutingProfile,
    route_options: &RouteOptions,
) -> Option<StageBDensify> {
    if waypoints.len() < 2 {
        return None;
    }
    let skels = load_skeletons_from_dirs(pack_dirs);
    if skels.is_empty() {
        log::info!(target: "NaviPlan", "stage_b densify: no persistent skeletons");
        return None;
    }
    let border_osm = border_osm_from_skeletons(&skels);
    let mut graph = merge_skeletons_to_route_graph(&skels, profile);
    let snap_m = 35_000.0;
    let (path, edges, _) = coarse_shortest_path(&mut graph, waypoints, snap_m, route_options)?;
    let vias = &waypoints[1..waypoints.len().saturating_sub(1)];
    let free = build_coarse_route_report(
        "stage_b_free",
        &graph,
        &path,
        &edges,
        &border_osm,
        vias,
        0,
        0,
        "stage_b free",
    );

    // Candidates: free + (when ferries allowed) exclude each ferry + no ferries.
    let mut candidates: Vec<(String, CoarseRouteReport, Vec<NodeId>, Vec<usize>)> = Vec::new();
    candidates.push(("free".into(), free.clone(), path.clone(), edges.clone()));

    if !route_options.avoid_ferries {
        for leg in &free.ferries {
            clear_access_forbidden(&mut graph);
            forbid_ferry_edges_matching_leg(&mut graph, leg, 3_000.0);
            if let Some((p, e, _)) =
                coarse_shortest_path(&mut graph, waypoints, snap_m, route_options)
            {
                let r = build_coarse_route_report(
                    "stage_b_excl",
                    &graph,
                    &p,
                    &e,
                    &border_osm,
                    vias,
                    0,
                    0,
                    &format!("excl {}→{}", leg.from_terminal, leg.to_terminal),
                );
                candidates.push((
                    format!("excl_{}_{}", leg.from_terminal, leg.to_terminal),
                    r,
                    p,
                    e,
                ));
            }
        }
        clear_access_forbidden(&mut graph);
        for e in graph.edges.iter_mut().filter(|e| e.is_ferry) {
            e.access_forbidden = true;
        }
        if let Some((p, e, _)) = coarse_shortest_path(&mut graph, waypoints, snap_m, route_options)
        {
            let r = build_coarse_route_report(
                "stage_b_no_ferry",
                &graph,
                &p,
                &e,
                &border_osm,
                vias,
                0,
                0,
                "no ferries",
            );
            candidates.push(("no_ferries".into(), r, p, e));
        }
        clear_access_forbidden(&mut graph);
    }

    // Among free + per-ferry-exclusion alts (not the full no_ferries detour):
    // competitive band 15% of best time, then fewest ferries, then 2% time, then
    // km. This drops Mannheller when a 1-ferry excl alt is only a few minutes
    // slower, without letting a modest Sweden land-only detour win on ferry
    // count=0. Prefer no_ferries when it is within 2% of best time, or when the
    // ferry pick is a clear distance blow-out (>15% more km than land).
    for (name, r, _, _) in &candidates {
        log::info!(
            target: "NaviPlan",
            "stage_b candidate name={name} km={:.1} min={:.1} ferries={}",
            r.total_km,
            r.total_min,
            r.ferries.len()
        );
    }
    let best_min_all = candidates
        .iter()
        .map(|(_, r, _, _)| r.total_min)
        .fold(f64::INFINITY, f64::min);
    let ferry_alts: Vec<_> = candidates
        .iter()
        .filter(|(name, _, _, _)| name.as_str() != "no_ferries")
        .collect();
    let competitive: Vec<_> = ferry_alts
        .iter()
        .copied()
        .filter(|(_, r, _, _)| r.total_min <= best_min_all * 1.15)
        .collect();
    let pick = if competitive.is_empty() {
        candidates
            .iter()
            .min_by(|a, b| {
                a.1.total_min
                    .partial_cmp(&b.1.total_min)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })?
    } else {
        let min_ferries = competitive.iter().map(|(_, r, _, _)| r.ferries.len()).min()?;
        let fewest: Vec<_> = competitive
            .into_iter()
            .filter(|(_, r, _, _)| r.ferries.len() == min_ferries)
            .collect();
        let best_min = fewest
            .iter()
            .map(|(_, r, _, _)| r.total_min)
            .fold(f64::INFINITY, f64::min);
        let near: Vec<_> = fewest
            .into_iter()
            .filter(|(_, r, _, _)| r.total_min <= best_min * 1.02)
            .collect();
        let ferry_pick = near
            .into_iter()
            .min_by(|a, b| {
                a.1.total_km
                    .partial_cmp(&b.1.total_km)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| {
                        a.1.total_min
                            .partial_cmp(&b.1.total_min)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
            })?;
        if let Some(nf) = candidates.iter().find(|(n, _, _, _)| n == "no_ferries") {
            let near_time = nf.1.total_min <= best_min_all * 1.02;
            let ferry_blowout = ferry_pick.1.total_km > nf.1.total_km * 1.15;
            if (near_time || ferry_blowout) && nf.1.total_km < ferry_pick.1.total_km {
                nf
            } else {
                ferry_pick
            }
        } else {
            ferry_pick
        }
    };

    let start = waypoints[0];
    let end = *waypoints.last().unwrap();
    let hops_raw = hops_from_report(&pick.1, start, end);
    let coarse_path = path_latlon(&graph, &pick.2);
    let hops = split_hops_by_path_tile_budget(hops_raw, &coarse_path, pack_dirs, profile);
    let ferries: Vec<String> = pick
        .1
        .ferries
        .iter()
        .map(|f| format!("{}→{}", f.from_terminal, f.to_terminal))
        .collect();
    let note = format!(
        "stage_b pick={} km={:.1} min={:.1} hops={} skels={} near_equal_2pct \
         max_path_nodes={} ferries={}",
        pick.0,
        pick.1.total_km,
        pick.1.total_min,
        hops.len().saturating_sub(1),
        skels.len(),
        crate::routing::plan_bbox::MAX_PATH_NODES_PER_HOP,
        pick.1.ferries.len(),
    );
    log::info!(target: "NaviPlan", "{note} ferries={ferries:?}");
    Some(StageBDensify {
        hops,
        coarse_path,
        total_min: pick.1.total_min,
        total_km: pick.1.total_km,
        ferries,
        note,
    })
}

/// True when a detailed (pack) graph would never carry synthetic skeleton stitch
/// edges — those exist only in [`build_skeleton_from_pack`] under the name
/// `skeleton_carriageway_link`.
pub fn detailed_graph_has_skeleton_carriageway_link(graph: &RouteGraph) -> bool {
    graph
        .edges
        .iter()
        .any(|e| e.name.as_deref() == Some("skeleton_carriageway_link"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_str(n: usize) -> Vec<String> {
        vec![String::new(); n]
    }
    fn nan_f64(n: usize) -> Vec<f64> {
        vec![f64::NAN; n]
    }
    fn zeros_u8(n: usize) -> Vec<u8> {
        vec![0u8; n]
    }

    fn tiny_pack() -> FlatGraphPack {
        let n_e = 4;
        FlatGraphPack {
            has_delta_h: false,
            node_ids: vec![100, 101, 102, 103],
            node_lats: vec![54.5, 54.51, 54.505, 54.6],
            node_lons: vec![11.2, 11.25, 11.22, 11.5],
            edge_src: vec![0, 1, 0, 2],
            edge_tgt: vec![1, 0, 2, 3],
            edge_length_m: vec![1000.0, 1000.0, 200.0, 5000.0],
            edge_base_weight: vec![1000.0, 1000.0, 200.0, 5000.0],
            edge_delta_h_m: vec![],
            edge_start_lat: vec![54.5, 54.51, 54.5, 54.505],
            edge_start_lon: vec![11.2, 11.25, 11.2, 11.22],
            edge_end_lat: vec![54.51, 54.5, 54.505, 54.6],
            edge_end_lon: vec![11.25, 11.2, 11.22, 11.5],
            edge_highway: vec![
                "primary".into(),
                "primary".into(),
                "secondary".into(),
                "secondary".into(),
            ],
            edge_maxspeed_kmh: nan_f64(n_e),
            edge_maxspeed_practical_kmh: nan_f64(n_e),
            edge_maxspeed_advisory_kmh: nan_f64(n_e),
            edge_maxspeed_type: empty_str(n_e),
            edge_maxspeed_variable: zeros_u8(n_e),
            edge_minspeed_kmh: nan_f64(n_e),
            edge_name: empty_str(n_e),
            edge_road_ref: empty_str(n_e),
            edge_is_motorroad: zeros_u8(n_e),
            edge_is_expressway: zeros_u8(n_e),
            edge_is_oneway: zeros_u8(n_e),
            edge_lanes: zeros_u8(n_e),
            edge_maxweight_t: nan_f64(n_e),
            edge_maxaxleload_t: nan_f64(n_e),
            edge_maxbogieweight_t: nan_f64(n_e),
            edge_maxheight_m: nan_f64(n_e),
            edge_maxwidth_m: nan_f64(n_e),
            edge_maxlength_m: nan_f64(n_e),
            edge_is_toll: zeros_u8(n_e),
            edge_is_ferry: vec![0, 1, 0, 0],
            edge_is_tunnel: zeros_u8(n_e),
            edge_is_roundabout: zeros_u8(n_e),
            edge_is_boardwalk: zeros_u8(n_e),
            edge_shape_offsets: vec![0; n_e + 1],
            edge_shape_lons: vec![],
            edge_shape_lats: vec![],
            edge_motor_vehicle_conditional: empty_str(n_e),
            edge_access_conditional: empty_str(n_e),
            edge_maxspeed_conditional: empty_str(n_e),
            edge_access_forbidden: zeros_u8(n_e),
            edge_surface_quality: zeros_u8(n_e),
            node_access_blocked: zeros_u8(4),
        }
    }

    #[test]
    fn secondary_to_ferry_kept_far_secondary_dropped() {
        let pack = tiny_pack();
        let (keep, ferry_term, _) = select_skeleton_edge_indices(&pack, &HashSet::new());
        assert!(!ferry_term.is_empty());
        assert!(keep.contains(&0));
        assert!(keep.contains(&1));
        assert!(keep.contains(&2), "secondary to ferry terminal must stay");
        assert!(
            !keep.contains(&3),
            "secondary far from ferry/border must drop"
        );
    }

    #[test]
    fn unclassified_pier_stub_to_ferry_kept() {
        let mut pack = tiny_pack();
        // Replace far secondary with an unclassified pier on the ferry terminal.
        pack.edge_highway[3] = "unclassified".into();
        pack.edge_src[3] = 1; // ferry terminal
        pack.edge_tgt[3] = 3;
        let (keep, ferry_term, _) = select_skeleton_edge_indices(&pack, &HashSet::new());
        assert!(ferry_term.contains(&1));
        assert!(
            keep.contains(&3),
            "unclassified pier touching ferry terminal must stay"
        );
    }

    #[test]
    fn multi_hop_service_chain_to_primary_kept() {
        // ferry terminal --service--> mid --unclassified--> primary network
        let n_e = 4;
        let pack = FlatGraphPack {
            has_delta_h: false,
            node_ids: vec![100, 101, 102, 103],
            node_lats: vec![54.50, 54.501, 54.502, 54.51],
            node_lons: vec![11.22, 11.221, 11.222, 11.25],
            edge_src: vec![0, 1, 1, 2],
            edge_tgt: vec![1, 0, 2, 3],
            edge_length_m: vec![50.0, 50.0, 80.0, 200.0],
            edge_base_weight: vec![50.0, 50.0, 80.0, 200.0],
            edge_delta_h_m: vec![],
            edge_start_lat: vec![54.50, 54.501, 54.501, 54.502],
            edge_start_lon: vec![11.22, 11.221, 11.221, 11.222],
            edge_end_lat: vec![54.501, 54.50, 54.502, 54.51],
            edge_end_lon: vec![11.221, 11.22, 11.222, 11.25],
            edge_highway: vec![
                "primary".into(), // unused major elsewhere? use as ferry below
                "primary".into(),
                "service".into(),
                "unclassified".into(),
            ],
            edge_maxspeed_kmh: nan_f64(n_e),
            edge_maxspeed_practical_kmh: nan_f64(n_e),
            edge_maxspeed_advisory_kmh: nan_f64(n_e),
            edge_maxspeed_type: empty_str(n_e),
            edge_maxspeed_variable: zeros_u8(n_e),
            edge_minspeed_kmh: nan_f64(n_e),
            edge_name: empty_str(n_e),
            edge_road_ref: empty_str(n_e),
            edge_is_motorroad: zeros_u8(n_e),
            edge_is_expressway: zeros_u8(n_e),
            edge_is_oneway: zeros_u8(n_e),
            edge_lanes: zeros_u8(n_e),
            edge_maxweight_t: nan_f64(n_e),
            edge_maxaxleload_t: nan_f64(n_e),
            edge_maxbogieweight_t: nan_f64(n_e),
            edge_maxheight_m: nan_f64(n_e),
            edge_maxwidth_m: nan_f64(n_e),
            edge_maxlength_m: nan_f64(n_e),
            edge_is_toll: zeros_u8(n_e),
            // edge0 primary 0-1, edge1 ferry 1-0 (terminals 0,1), edge2 service 1-2, edge3 unclass 2-3
            // Wait: need primary that is NOT the ferry. Restructure:
            // nodes: 0=ferry_term, 1=mid, 2=primary_a, 3=primary_b
            // Actually rebuild below via mutation after — keep simple assert on tiny_pack growth.
            edge_is_ferry: vec![0, 1, 0, 0],
            edge_is_tunnel: zeros_u8(n_e),
            edge_is_roundabout: zeros_u8(n_e),
            edge_is_boardwalk: zeros_u8(n_e),
            edge_shape_offsets: vec![0; n_e + 1],
            edge_shape_lons: vec![],
            edge_shape_lats: vec![],
            edge_motor_vehicle_conditional: empty_str(n_e),
            edge_access_conditional: empty_str(n_e),
            edge_maxspeed_conditional: empty_str(n_e),
            edge_access_forbidden: zeros_u8(n_e),
            edge_surface_quality: zeros_u8(n_e),
            node_access_blocked: zeros_u8(4),
        };
        // Fix topology: edge0 = primary between 2-3 (major), edge1 = ferry 0-X need partner
        // Use: 0 ferry_term, 3 ferry other; 0-1 service, 1-2 unclass, 2-3 primary
        let mut pack = pack;
        pack.edge_src = vec![2, 0, 0, 1];
        pack.edge_tgt = vec![3, 3, 1, 2];
        pack.edge_highway = vec![
            "primary".into(),
            "".into(), // ferry
            "service".into(),
            "unclassified".into(),
        ];
        pack.edge_is_ferry = vec![0, 1, 0, 0];
        let (keep, ferry_term, _) = select_skeleton_edge_indices(&pack, &HashSet::new());
        assert!(ferry_term.contains(&0));
        assert!(keep.contains(&0), "primary must stay");
        assert!(keep.contains(&1), "ferry must stay");
        assert!(keep.contains(&2), "service approach hop must stay");
        assert!(keep.contains(&3), "unclassified hop to primary must stay");
    }

    #[test]
    fn secondary_to_border_kept() {
        let pack = tiny_pack();
        let mut borders = HashSet::new();
        borders.insert(103);
        let (keep, _, border_nodes) = select_skeleton_edge_indices(&pack, &borders);
        assert!(border_nodes.contains(&3));
        assert!(keep.contains(&3), "secondary touching border node must stay");
    }

    #[test]
    fn shared_osm_detects_border() {
        let a: HashSet<i64> = [1, 2, 3].into_iter().collect();
        let b: HashSet<i64> = [3, 4, 5].into_iter().collect();
        let shared = shared_osm_ids_across_regions(&[a, b]);
        assert_eq!(shared, HashSet::from([3]));
    }

    #[test]
    fn roundtrip_json_counts() {
        let pack = tiny_pack();
        let skel = build_skeleton_from_pack(&pack, "europe/test", "test", "truck", &HashSet::new());
        assert_eq!(skel.format_version, CORRIDOR_SKELETON_FORMAT_VERSION);
        assert!(skel.edge_count >= 3);
        assert!(skel.ferry_edge_count >= 1);
        assert!(skel.secondary_edge_count >= 1);
        assert_eq!(skel.edge_name.len(), skel.edge_count as usize);
        let dir = tempfile::tempdir().unwrap();
        let n = write_skeleton_file(dir.path(), &skel).unwrap();
        assert!(n > 100);
        let loaded = read_skeleton_file(&skeleton_path(dir.path(), "test")).unwrap();
        assert_eq!(loaded.node_count, skel.node_count);
        assert_eq!(loaded.edge_count, skel.edge_count);
        let g = skeleton_to_route_graph(&loaded, RoutingProfile::Car);
        assert_eq!(g.nodes.len(), loaded.node_count as usize);
        assert_eq!(g.edges.len(), loaded.edge_count as usize);
    }

    #[test]
    fn joints_are_border_ferry_via_not_even_spacing() {
        let pack = tiny_pack();
        let mut borders = HashSet::new();
        borders.insert(100);
        let skel = build_skeleton_from_pack(&pack, "europe/test", "test", "car", &borders);
        let mut g = skeleton_to_route_graph(&skel, RoutingProfile::Car);
        let path: Vec<NodeId> = skel.node_ids.iter().take(3).map(|&i| NodeId(i)).collect();
        let edges: Vec<usize> = (0..g.edges.len().min(2)).collect();
        let joints = extract_coarse_joints(&g, &path, &edges, &borders, &[(54.5, 11.2)], 5000.0);
        assert!(joints.iter().any(|j| j.joint_type == "start"));
        assert!(!joints.iter().any(|j| j.joint_type == "hop_deg"));
        let _ = &mut g;
    }

    #[test]
    fn opposite_oneway_carriageways_get_stitch_link() {
        // Two parallel oneway motorways ~20 m apart, opposite headings, same ref.
        // Without a stitch, directed travel cannot leave the southbound line.
        let n_e = 2;
        let pack = FlatGraphPack {
            has_delta_h: false,
            node_ids: vec![1, 2, 3, 4],
            // Southbound (north→south): nodes 0→1. Northbound: 2→3.
            node_lats: vec![55.4700, 55.4600, 55.4600, 55.4700],
            node_lons: vec![12.1200, 12.1200, 12.12025, 12.12025],
            edge_src: vec![0, 2],
            edge_tgt: vec![1, 3],
            edge_length_m: vec![1113.0, 1113.0],
            edge_base_weight: vec![1113.0, 1113.0],
            edge_delta_h_m: vec![],
            edge_start_lat: vec![55.4700, 55.4600],
            edge_start_lon: vec![12.1200, 12.12025],
            edge_end_lat: vec![55.4600, 55.4700],
            edge_end_lon: vec![12.1200, 12.12025],
            edge_highway: vec!["motorway".into(), "motorway".into()],
            edge_maxspeed_kmh: nan_f64(n_e),
            edge_maxspeed_practical_kmh: nan_f64(n_e),
            edge_maxspeed_advisory_kmh: nan_f64(n_e),
            edge_maxspeed_type: empty_str(n_e),
            edge_maxspeed_variable: zeros_u8(n_e),
            edge_minspeed_kmh: nan_f64(n_e),
            edge_name: empty_str(n_e),
            edge_road_ref: vec!["E 47".into(), "E 47".into()],
            edge_is_motorroad: zeros_u8(n_e),
            edge_is_expressway: zeros_u8(n_e),
            edge_is_oneway: vec![1, 1],
            edge_lanes: zeros_u8(n_e),
            edge_maxweight_t: nan_f64(n_e),
            edge_maxaxleload_t: nan_f64(n_e),
            edge_maxbogieweight_t: nan_f64(n_e),
            edge_maxheight_m: nan_f64(n_e),
            edge_maxwidth_m: nan_f64(n_e),
            edge_maxlength_m: nan_f64(n_e),
            edge_is_toll: zeros_u8(n_e),
            edge_is_ferry: zeros_u8(n_e),
            edge_is_tunnel: zeros_u8(n_e),
            edge_is_roundabout: zeros_u8(n_e),
            edge_is_boardwalk: zeros_u8(n_e),
            edge_shape_offsets: vec![0; n_e + 1],
            edge_shape_lons: vec![],
            edge_shape_lats: vec![],
            edge_motor_vehicle_conditional: empty_str(n_e),
            edge_access_conditional: empty_str(n_e),
            edge_maxspeed_conditional: empty_str(n_e),
            edge_access_forbidden: zeros_u8(n_e),
            edge_surface_quality: zeros_u8(n_e),
            node_access_blocked: zeros_u8(4),
        };
        let skel =
            build_skeleton_from_pack(&pack, "europe/test", "test-cw", "car", &HashSet::new());
        let links = skel
            .edge_name
            .iter()
            .filter(|n| n.as_str() == "skeleton_carriageway_link")
            .count();
        assert!(
            links >= 2,
            "expected bidirectional carriageway stitch, got {links}"
        );
        let g = skeleton_to_route_graph(&skel, RoutingProfile::Car);
        // From southbound mid (osm 2) should reach northbound far (osm 4) via stitch.
        let from = NodeId(2);
        let to = NodeId(4);
        let path = g.shortest_path_with_options(
            from,
            to,
            false,
            &RouteOptions {
                surface_routing_mode: Some(SurfaceRoutingMode::Offroad),
                ..RouteOptions::default()
            },
        );
        assert!(
            path.is_some(),
            "directed path must cross opposite carriageway stitch"
        );
    }

    #[test]
    fn detailed_pack_never_carries_skeleton_carriageway_link_name() {
        // Synthetic stitch edges are created only inside build_skeleton_from_pack.
        // Installed packs keep OSM names; Stage B hop graphs load packs only, so
        // skeleton_carriageway_link must never appear on a detailed RouteGraph.
        let pack = tiny_pack();
        assert!(
            pack.edge_name
                .iter()
                .all(|n| n.as_str() != "skeleton_carriageway_link"),
            "pack edge names must not include skeleton_carriageway_link"
        );
        // Opposite-carriageway fixture adds stitch only in the skeleton builder.
        let skel =
            build_skeleton_from_pack(&pack, "europe/test", "test-pack-names", "car", &HashSet::new());
        let stitch_in_skel = skel
            .edge_name
            .iter()
            .any(|n| n.as_str() == "skeleton_carriageway_link");
        let g = skeleton_to_route_graph(&skel, RoutingProfile::Car);
        // tiny_pack has no parallel carriageways → no stitch → detailed stays clean.
        assert!(
            !stitch_in_skel,
            "tiny_pack fixture must not invent carriageway stitch"
        );
        assert!(
            !detailed_graph_has_skeleton_carriageway_link(&g),
            "detailed hop graph must never expose skeleton_carriageway_link"
        );
    }

    /// Two parallel corridors A→D: short flagged edge vs longer plain primary.
    /// `land_seg_m` is each land-corridor segment length (three segments).
    fn avoid_options_fixture(
        short_hwy: &str,
        short_ferry: bool,
        short_toll: bool,
        short_tunnel: bool,
        land_seg_m: f64,
    ) -> RouteGraph {
        use geo_types::Coord;
        use osm4routing::Node;
        use std::collections::HashMap;

        let mut nodes = HashMap::new();
        let coords = [
            (1i64, 60.0, 10.0),
            (2, 60.01, 10.01),
            (3, 60.02, 10.02),
            (4, 60.03, 10.03),
            (5, 60.015, 10.0),
            (6, 60.025, 10.0),
        ];
        for (id, lat, lon) in coords {
            nodes.insert(
                NodeId(id),
                Node {
                    id: NodeId(id),
                    coord: Coord { x: lon, y: lat },
                    uses: 0,
                },
            );
        }
        let mk = |id: &str,
                  s: i64,
                  t: i64,
                  len: f64,
                  hwy: &str,
                  ferry: bool,
                  toll: bool,
                  tunnel: bool|
         -> GraphEdge {
            let sn = nodes[&NodeId(s)].coord;
            let tn = nodes[&NodeId(t)].coord;
            GraphEdge {
                id: id.into(),
                source: NodeId(s),
                target: NodeId(t),
                length_m: len,
                base_weight: len,
                cost_mult: 1.0,
                eco_weight: None,
                start_lat: sn.y,
                start_lon: sn.x,
                end_lat: tn.y,
                end_lon: tn.x,
                shape: Vec::new(),
                highway: Some(hwy.into()),
                maxspeed_kmh: Some(80.0),
                maxspeed_practical_kmh: None,
                maxspeed_advisory_kmh: None,
                maxspeed_type: None,
                maxspeed_variable: false,
                minspeed_kmh: None,
                name: None,
                road_ref: None,
                is_motorroad: false,
                is_expressway: false,
                is_oneway: false,
                lanes: None,
                maxweight_t: None,
                maxaxleload_t: None,
                maxbogieweight_t: None,
                maxheight_m: None,
                maxwidth_m: None,
                maxlength_m: None,
                is_toll: toll,
                is_ferry: ferry,
                ferry_interval_min: None,
                is_tunnel: tunnel,
                is_boardwalk_crossing: false,
                is_roundabout: false,
                motor_vehicle_conditional: None,
                access_conditional: None,
                maxspeed_conditional: None,
                access_forbidden: false,
                surface_quality: crate::routing::graph::SurfaceQuality::Good,
            }
        };
        // Short corridor: 1→2→3→4 with middle edge flagged.
        // Long corridor: 1→5→6→4 (plain primary, longer).
        let edges = vec![
            mk("s1", 1, 2, 1_000.0, "primary", false, false, false),
            mk(
                "short",
                2,
                3,
                1_000.0,
                short_hwy,
                short_ferry,
                short_toll,
                short_tunnel,
            ),
            mk("s3", 3, 4, 1_000.0, "primary", false, false, false),
            mk("l1", 1, 5, land_seg_m, "primary", false, false, false),
            mk("l2", 5, 6, land_seg_m, "primary", false, false, false),
            mk("l3", 6, 4, land_seg_m, "primary", false, false, false),
        ];
        RouteGraph::from_parts(nodes, edges, RoutingProfile::Car)
    }

    fn path_uses_edge(graph: &RouteGraph, edges: &[usize], id: &str) -> bool {
        edges.iter().any(|&i| graph.edges[i].id == id)
    }

    #[test]
    fn coarse_avoid_ferries_skips_ferry_corridor() {
        // Land must beat ferry wait when ferries are forbidden (~10+ min wait).
        let mut g = avoid_options_fixture("ferry", true, false, false, 80_000.0);
        let wps = &[(60.0, 10.0), (60.03, 10.03)];
        let free = coarse_shortest_path(&mut g, wps, 5_000.0, &RouteOptions::default())
            .expect("free path");
        assert!(
            path_uses_edge(&g, &free.1, "short"),
            "default should prefer short ferry corridor"
        );
        let avoid = RouteOptions {
            avoid_ferries: true,
            ..Default::default()
        };
        let alt = coarse_shortest_path(&mut g, wps, 5_000.0, &avoid).expect("land path");
        assert!(
            !path_uses_edge(&g, &alt.1, "short"),
            "avoid_ferries must not use ferry edge"
        );
        assert!(path_uses_edge(&g, &alt.1, "l2"));
    }

    #[test]
    fn coarse_avoid_tolls_never_use_skips_toll_corridor() {
        let mut g = avoid_options_fixture("primary", false, true, false, 4_000.0);
        let wps = &[(60.0, 10.0), (60.03, 10.03)];
        let avoid = RouteOptions {
            toll_policy: crate::routing::toll::TollPolicy::NeverUse,
            ..Default::default()
        };
        let alt = coarse_shortest_path(&mut g, wps, 5_000.0, &avoid).expect("toll-free path");
        assert!(!path_uses_edge(&g, &alt.1, "short"));
        assert!(path_uses_edge(&g, &alt.1, "l2"));
    }

    #[test]
    fn coarse_avoid_tunnels_prefers_surface_corridor() {
        // Soft ×50 on 1 km tunnel beats ~12 km surface land; free still takes tunnel.
        let mut g = avoid_options_fixture("primary", false, false, true, 4_000.0);
        let wps = &[(60.0, 10.0), (60.03, 10.03)];
        let free = coarse_shortest_path(&mut g, wps, 5_000.0, &RouteOptions::default())
            .expect("free path");
        assert!(path_uses_edge(&g, &free.1, "short"));
        let avoid = RouteOptions {
            avoid_tunnels: true,
            ..Default::default()
        };
        let alt = coarse_shortest_path(&mut g, wps, 5_000.0, &avoid).expect("surface path");
        assert!(
            !path_uses_edge(&g, &alt.1, "short"),
            "tunnel penalty must push search onto the longer surface corridor"
        );
        assert!(path_uses_edge(&g, &alt.1, "l2"));
    }

    #[test]
    fn coarse_avoid_motorways_skips_motorway_corridor() {
        let mut g = avoid_options_fixture("motorway", false, false, false, 4_000.0);
        let wps = &[(60.0, 10.0), (60.03, 10.03)];
        let avoid = RouteOptions {
            avoid_motorways: true,
            ..Default::default()
        };
        let alt = coarse_shortest_path(&mut g, wps, 5_000.0, &avoid).expect("non-motorway path");
        assert!(!path_uses_edge(&g, &alt.1, "short"));
        assert!(path_uses_edge(&g, &alt.1, "l2"));
    }

    #[test]
    fn intra_skeleton_stitch_bridges_region_component_cut() {
        // One region with two disconnected major components ~9 km apart (FU23
        // jamtland pattern). Merge must add an intra stitch so A* can cross.
        let skel = CorridorSkeletonFile {
            format_version: CORRIDOR_SKELETON_FORMAT_VERSION,
            pack_format_version: GRAPH_FORMAT_VERSION,
            region_id: "test/cut".into(),
            leaf_stem: "cut".into(),
            profile: "car".into(),
            node_count: 4,
            edge_count: 4,
            ferry_edge_count: 0,
            secondary_edge_count: 0,
            border_node_count: 0,
            ferry_terminal_count: 0,
            build_ms: 0,
            // Comp A: 1—2 at lon 13.0; Comp B: 3—4 at lon 13.12 (~6.5 km gap).
            node_ids: vec![1, 2, 3, 4],
            node_lats: vec![62.0, 62.0, 62.0, 62.0],
            node_lons: vec![13.0, 13.02, 13.12, 13.14],
            node_is_border: vec![0, 0, 0, 0],
            node_is_ferry_terminal: vec![0, 0, 0, 0],
            edge_src: vec![0, 1, 2, 3],
            edge_tgt: vec![1, 0, 3, 2],
            edge_length_m: vec![2_000.0; 4],
            edge_base_weight: vec![2_000.0; 4],
            edge_highway: vec!["primary".into(); 4],
            edge_name: vec![String::new(); 4],
            edge_road_ref: vec![String::new(); 4],
            edge_is_oneway: vec![0; 4],
            edge_is_ferry: vec![0; 4],
            edge_is_tunnel: vec![0; 4],
            edge_is_toll: vec![0; 4],
        };
        // MIN_COMP_NODES=10: pad each side with isolated-but-linked filler nodes
        // so both components clear the size floor.
        let mut skel = skel;
        for k in 0..100 {
            let id = 100 + k as i64;
            skel.node_ids.push(id);
            skel.node_lats.push(62.0);
            skel.node_lons.push(13.0 + 0.001 * (k % 10) as f64);
            skel.node_is_border.push(0);
            skel.node_is_ferry_terminal.push(0);
            // Attach fillers 100..149 to node 1 (comp A), 150..199 to node 3 (comp B).
            let attach = if k < 50 { 0u32 } else { 2u32 };
            let new_i = (skel.node_ids.len() - 1) as u32;
            skel.edge_src.push(attach);
            skel.edge_tgt.push(new_i);
            skel.edge_src.push(new_i);
            skel.edge_tgt.push(attach);
            skel.edge_length_m.extend([100.0, 100.0]);
            skel.edge_base_weight.extend([100.0, 100.0]);
            skel.edge_highway.extend(["primary".into(), "primary".into()]);
            skel.edge_name.extend([String::new(), String::new()]);
            skel.edge_road_ref.extend([String::new(), String::new()]);
            skel.edge_is_oneway.extend([0, 0]);
            skel.edge_is_ferry.extend([0, 0]);
            skel.edge_is_tunnel.extend([0, 0]);
            skel.edge_is_toll.extend([0, 0]);
        }
        skel.node_count = skel.node_ids.len() as u32;
        skel.edge_count = skel.edge_src.len() as u32;
        let g = merge_skeletons_to_route_graph(&[skel], RoutingProfile::Car);
        assert!(
            g.edges
                .iter()
                .any(|e| e.id.starts_with("skeleton_intra_stitch")),
            "must bridge the intra-region component cut"
        );
        assert!(
            g.shortest_path(NodeId(1), NodeId(4), false).is_some(),
            "path must cross the stitch"
        );
    }

    #[test]
    #[ignore = "needs .tmp-fu23-device-skels from emulator pull"]
    fn fu23_elsa_stage_b_after_stitch() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(".tmp-fu23-device-skels");
        assert!(dir.is_dir(), "missing {}", dir.display());
        let wps = &[(69.9742, 29.63342), (59.80326, 9.39866)];
        let sb = try_stage_b_densify_from_skeletons(
            &[dir.as_path()],
            wps,
            RoutingProfile::Car,
            &RouteOptions::default(),
        )
        .expect("stage b");
        eprintln!(
            "elsa stage_b km={:.1} min={:.1} note={}",
            sb.total_km, sb.total_min, sb.note
        );
        assert!(
            sb.total_km < 2500.0,
            "expected fair corridor <2500 km, got {:.1}",
            sb.total_km
        );
    }
}
