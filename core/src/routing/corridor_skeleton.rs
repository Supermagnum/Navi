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
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// On-disk skeleton format version (independent of pack GRAPH_FORMAT_VERSION).
/// v2 adds road ref/name, oneway, and per-node border/ferry flags.
/// v3: binary sidecar (bincode) + border-crossing rim (no full AABB rim keep).
pub const CORRIDOR_SKELETON_FORMAT_VERSION: u32 = 3;

/// Filename written next to region packs: `{stem}.navi-corridor-skeleton.bin`.
pub fn skeleton_filename(leaf_stem: &str) -> String {
    format!("{leaf_stem}.navi-corridor-skeleton.bin")
}

/// Legacy JSON filename (read-only migration; idle rebuild writes binary).
pub fn skeleton_filename_json(leaf_stem: &str) -> String {
    format!("{leaf_stem}.navi-corridor-skeleton.json")
}

pub fn skeleton_path(dir: &Path, leaf_stem: &str) -> PathBuf {
    dir.join(skeleton_filename(leaf_stem))
}

pub fn skeleton_path_json(dir: &Path, leaf_stem: &str) -> PathBuf {
    dir.join(skeleton_filename_json(leaf_stem))
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

/// Road classes a border crossing may use below the major skeleton. Pass 5 of
/// [`select_skeleton_edge_indices_for_region`] walks these from a border node
/// to the major skeleton.
pub fn is_border_approach_highway(highway: &str) -> bool {
    matches!(
        highway,
        "secondary"
            | "secondary_link"
            | "tertiary"
            | "tertiary_link"
            | "unclassified"
            | "residential"
    )
}

/// Overlap of two `[min_lat, min_lon, max_lat, max_lon]` bboxes, expanded by
/// `eps_deg`. Used so neighbour packs contribute approach-class nodes only
/// along the shared cut, not their whole interior.
pub fn border_band_bbox(a: &[f64; 4], b: &[f64; 4], eps_deg: f64) -> [f64; 4] {
    [
        a[0].max(b[0]) - eps_deg,
        a[1].max(b[1]) - eps_deg,
        a[2].min(b[2]) + eps_deg,
        a[3].min(b[3]) + eps_deg,
    ]
}

/// OSM node ids that can be a land-border crossing or a shared ferry terminal:
/// nodes incident to major, ferry or border-approach edges. Regions join where
/// these sets intersect; a crossing carried only by a secondary or minor road
/// is not on a major edge on either side.
///
/// `within` is `[min_lat, min_lon, max_lat, max_lon]`, same as [`crate::routing::basemap::region_bbox`].
pub fn border_candidate_osm_ids(pack: &FlatGraphPack, within: Option<&[f64; 4]>) -> HashSet<i64> {
    let mut out = HashSet::new();
    let inside = |n: usize| {
        within.is_none_or(|b| {
            let (lat, lon) = (pack.node_lats[n], pack.node_lons[n]);
            lat >= b[0] && lon >= b[1] && lat <= b[2] && lon <= b[3]
        })
    };
    for i in 0..pack.edge_src.len() {
        let hw = pack.edge_highway[i].as_str();
        let ferry = pack.edge_is_ferry.get(i).copied().unwrap_or(0) != 0;
        if !densify_skeleton_edge(hw, ferry) && !is_border_approach_highway(hw) {
            continue;
        }
        for n in [pack.edge_src[i] as usize, pack.edge_tgt[i] as usize] {
            if inside(n) {
                out.insert(pack.node_ids[n]);
            }
        }
    }
    out
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

/// Build skeleton membership for one flat pack tile/region.
///
/// Pass 1: motorway/trunk/primary + ferry (same as densify skeleton).
/// Pass 2: mark ferry terminals and nodes whose OSM ids are in `border_osm_ids`.
/// Pass 3: grow land approaches from each ferry terminal through any non-ferry
///         highway until a major-skeleton node is reached (hop-capped).
/// Pass 4: secondary edges that touch ferry or border anchors.
/// Pass 5: minor roads only on the short path from a shared border node to the
///         major skeleton, so a real border crossing reaches it on both sides.
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
        if pier_connects_ferry_terminal(ferry, pack.edge_src[i], pack.edge_tgt[i], &ferry_terminals)
        {
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
    // Pass 5: from each border node that is not already on the major skeleton,
    // keep the shortest approach-class path (by metres) to the nearest major
    // node. A hop budget of 12 dropped rural Geofabrik cuts whose shared node
    // sits tens of kilometres from the trunk — the two spines never joined.
    // Still one path per border node, not every minor road in the AABB.
    let _ = region_id;
    let mut minor_adj: HashMap<u32, Vec<(u32, usize)>> = HashMap::new();
    for i in 0..n {
        if pack.edge_is_ferry.get(i).copied().unwrap_or(0) != 0 {
            continue;
        }
        if !is_border_approach_highway(pack.edge_highway[i].as_str()) {
            continue;
        }
        let s = pack.edge_src[i];
        let t = pack.edge_tgt[i];
        minor_adj.entry(s).or_default().push((t, i));
        minor_adj.entry(t).or_default().push((s, i));
    }
    const BORDER_APPROACH_MAX_M: f64 = 50_000.0;
    for &bn in &border_nodes {
        if major_nodes.contains(&bn) {
            continue;
        }
        let mut dist: HashMap<u32, i64> = HashMap::from([(bn, 0)]);
        let mut parent: HashMap<u32, (u32, usize)> = HashMap::new();
        let mut heap: BinaryHeap<(Reverse<i64>, u32)> = BinaryHeap::from([(Reverse(0), bn)]);
        let mut reached_major: Option<u32> = None;
        while let Some((Reverse(du), u)) = heap.pop() {
            if dist.get(&u).copied().unwrap_or(i64::MAX) < du {
                continue;
            }
            if u != bn && major_nodes.contains(&u) {
                reached_major = Some(u);
                break;
            }
            let Some(neigh) = minor_adj.get(&u) else {
                continue;
            };
            for &(v, ei) in neigh {
                let w = pack
                    .edge_length_m
                    .get(ei)
                    .copied()
                    .unwrap_or(0.0)
                    .max(1.0)
                    .round() as i64;
                let alt = du.saturating_add(w);
                if alt as f64 > BORDER_APPROACH_MAX_M {
                    continue;
                }
                if dist.get(&v).copied().unwrap_or(i64::MAX) <= alt {
                    continue;
                }
                dist.insert(v, alt);
                parent.insert(v, (u, ei));
                heap.push((Reverse(alt), v));
            }
        }
        if let Some(mut cur) = reached_major {
            while cur != bn {
                let Some(&(prev, ei)) = parent.get(&cur) else {
                    break;
                };
                keep.insert(ei);
                // Pack stores a bidirectional road as two directed edges.
                // The walk is undirected; keeping only the traversed index
                // left the opposite carriageway out and the cut one-way.
                if let Some(neigh) = minor_adj.get(&cur) {
                    for &(v, rev) in neigh {
                        if v == prev {
                            keep.insert(rev);
                        }
                    }
                }
                if let Some(neigh) = minor_adj.get(&prev) {
                    for &(v, rev) in neigh {
                        if v == cur {
                            keep.insert(rev);
                        }
                    }
                }
                cur = prev;
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
        edge_name.push(pack.edge_name.get(i).cloned().unwrap_or_default());
        edge_road_ref.push(pack.edge_road_ref.get(i).cloned().unwrap_or_default());
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
            pack.edge_start_lat
                .push(part.node_lats[part.edge_src[e] as usize]);
            pack.edge_start_lon
                .push(part.node_lons[part.edge_src[e] as usize]);
            pack.edge_end_lat
                .push(part.node_lats[part.edge_tgt[e] as usize]);
            pack.edge_end_lon
                .push(part.node_lons[part.edge_tgt[e] as usize]);
            pack.edge_highway.push(part.edge_highway[e].clone());
            pack.edge_name
                .push(part.edge_name.get(e).cloned().unwrap_or_default());
            pack.edge_road_ref
                .push(part.edge_road_ref.get(e).cloned().unwrap_or_default());
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
        if border_osm.contains(oid) && i < skel.node_is_border.len() {
            skel.node_is_border[i] = 1;
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
    let payload = bincode::serialize(skel).map_err(std::io::Error::other)?;
    {
        let f = File::create(&path)?;
        let mut w = BufWriter::new(f);
        w.write_all(&payload)?;
        w.flush()?;
    }
    // Drop legacy JSON so idle rebuilds do not leave a stale twin.
    let json = skeleton_path_json(dir, &skel.leaf_stem);
    let _ = std::fs::remove_file(json);
    Ok(std::fs::metadata(path)?.len())
}

pub fn read_skeleton_file(path: &Path) -> std::io::Result<CorridorSkeletonFile> {
    let bytes = std::fs::read(path)?;
    // Binary (v3+) first; fall back to JSON for pre-FU25 sidecars still on disk.
    if let Ok(skel) = bincode::deserialize::<CorridorSkeletonFile>(&bytes) {
        return Ok(skel);
    }
    serde_json::from_slice(&bytes).map_err(std::io::Error::other)
}

/// Resolve on-disk skeleton path: prefer `.bin`, else legacy `.json`.
pub fn resolve_skeleton_path(dir: &Path, leaf_stem: &str) -> Option<PathBuf> {
    let bin = skeleton_path(dir, leaf_stem);
    if bin.is_file() {
        return Some(bin);
    }
    let json = skeleton_path_json(dir, leaf_stem);
    if json.is_file() {
        return Some(json);
    }
    None
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
    let mut b = CoarseGraphBuilder::new(true);
    for s in skels {
        b.add(s);
    }
    b.finish(profile).graph
}

/// Node arrays of one region skeleton, kept after its edges are merged (border
/// marks and gap stitching need them; edge arrays and strings do not stay).
pub struct SkeletonRegionNodes {
    pub region_id: String,
    pub leaf_stem: String,
    pub node_ids: Vec<i64>,
    pub node_lats: Vec<f64>,
    pub node_lons: Vec<f64>,
    pub node_is_border: Vec<u8>,
}

impl From<&CorridorSkeletonFile> for SkeletonRegionNodes {
    fn from(s: &CorridorSkeletonFile) -> Self {
        Self {
            region_id: s.region_id.clone(),
            leaf_stem: s.leaf_stem.clone(),
            node_ids: s.node_ids.clone(),
            node_lats: s.node_lats.clone(),
            node_lons: s.node_lons.clone(),
            node_is_border: s.node_is_border.clone(),
        }
    }
}

/// Road names and refs of the merged skeleton edges, kept beside the graph and
/// put on an edge only when it lies on a reported path.
#[derive(Default)]
pub struct CoarseEdgeNames {
    strings: Vec<String>,
    edge: Vec<(u32, u32)>,
}

impl CoarseEdgeNames {
    /// Give the edges in `edge_indices` their name and ref.
    pub fn apply(&self, graph: &mut RouteGraph, edge_indices: &[usize]) {
        for &i in edge_indices {
            let Some(&(name, road_ref)) = self.edge.get(i) else {
                continue;
            };
            let e = &mut graph.edges[i];
            if name != 0 && e.name.is_none() {
                e.name = Some(self.strings[name as usize].clone());
            }
            if road_ref != 0 && e.road_ref.is_none() {
                e.road_ref = Some(self.strings[road_ref as usize].clone());
            }
        }
    }
}

/// Merged coarse graph with the per-region node arrays and edge names.
pub struct CoarseGraph {
    pub graph: RouteGraph,
    pub regions: Vec<SkeletonRegionNodes>,
    pub names: CoarseEdgeNames,
}

struct CoarseEdge {
    source: i64,
    target: i64,
    start_lat: f64,
    start_lon: f64,
    end_lat: f64,
    end_lon: f64,
    length_m: f64,
    base_weight: f64,
    highway: u32,
    name: u32,
    road_ref: u32,
    is_oneway: bool,
    is_ferry: bool,
    is_tunnel: bool,
    is_toll: bool,
}

/// Room for synthetic stitch edges so pushing them never regrows the edge array.
const STITCH_EDGE_HEADROOM: usize = 1024;

/// Builds the merged coarse graph one region skeleton at a time.
///
/// Same nodes, edges and edge order as converting every skeleton to its own
/// graph and merging them with [`crate::routing::indexed::merge_tile_graphs`]:
/// regions in load order, edges in file order, the first copy of a repeated
/// edge kept, construction/proposed highways dropped, the last region's copy
/// of a shared node kept. Edges carry no id or shape (a skeleton edge is a
/// straight chord); names and refs stay in [`CoarseEdgeNames`] unless
/// `keep_names` is set.
struct CoarseGraphBuilder {
    keep_names: bool,
    nodes: HashMap<NodeId, Node>,
    edges: Vec<CoarseEdge>,
    seen: HashSet<(i64, i64, u64, u64, u64, u64, u64)>,
    strings: Vec<String>,
    string_ix: HashMap<String, u32>,
    regions: Vec<SkeletonRegionNodes>,
}

impl CoarseGraphBuilder {
    fn new(keep_names: bool) -> Self {
        Self {
            keep_names,
            nodes: HashMap::new(),
            edges: Vec::new(),
            seen: HashSet::new(),
            strings: vec![String::new()],
            string_ix: HashMap::new(),
            regions: Vec::new(),
        }
    }

    fn intern(&mut self, s: &str) -> u32 {
        if s.is_empty() {
            return 0;
        }
        if let Some(&i) = self.string_ix.get(s) {
            return i;
        }
        let i = self.strings.len() as u32;
        self.strings.push(s.to_string());
        self.string_ix.insert(s.to_string(), i);
        i
    }

    fn add(&mut self, s: &CorridorSkeletonFile) {
        self.nodes.reserve(s.node_ids.len());
        for i in 0..s.node_ids.len() {
            let id = NodeId(s.node_ids[i]);
            self.nodes.insert(
                id,
                Node {
                    id,
                    coord: Coord {
                        x: s.node_lons[i],
                        y: s.node_lats[i],
                    },
                    uses: 2,
                },
            );
        }
        for i in 0..s.edge_src.len() {
            let hw = s.edge_highway.get(i).map(|s| s.as_str()).unwrap_or("");
            if crate::routing::graph::is_construction_or_proposed_highway(
                (!hw.is_empty()).then_some(hw),
            ) {
                continue;
            }
            let si = s.edge_src[i] as usize;
            let ti = s.edge_tgt[i] as usize;
            let e = CoarseEdge {
                source: s.node_ids[si],
                target: s.node_ids[ti],
                start_lat: s.node_lats[si],
                start_lon: s.node_lons[si],
                end_lat: s.node_lats[ti],
                end_lon: s.node_lons[ti],
                length_m: s.edge_length_m[i],
                base_weight: s.edge_base_weight[i],
                highway: 0,
                name: 0,
                road_ref: 0,
                is_oneway: s.edge_is_oneway.get(i).copied().unwrap_or(0) != 0,
                is_ferry: s.edge_is_ferry.get(i).copied().unwrap_or(0) != 0,
                is_tunnel: s.edge_is_tunnel.get(i).copied().unwrap_or(0) != 0,
                is_toll: s.edge_is_toll.get(i).copied().unwrap_or(0) != 0,
            };
            let key = (
                e.source,
                e.target,
                e.length_m.to_bits(),
                e.start_lat.to_bits(),
                e.start_lon.to_bits(),
                e.end_lat.to_bits(),
                e.end_lon.to_bits(),
            );
            if !self.seen.insert(key) {
                continue;
            }
            let highway = self.intern(hw);
            let name = self.intern(s.edge_name.get(i).map(|s| s.as_str()).unwrap_or(""));
            let road_ref = self.intern(s.edge_road_ref.get(i).map(|s| s.as_str()).unwrap_or(""));
            self.edges.push(CoarseEdge {
                highway,
                name,
                road_ref,
                ..e
            });
        }
        self.regions.push(SkeletonRegionNodes::from(s));
    }

    fn finish(self, profile: RoutingProfile) -> CoarseGraph {
        let Self {
            keep_names,
            nodes,
            edges: compact,
            seen,
            strings,
            string_ix,
            regions,
        } = self;
        drop(seen);
        drop(string_ix);
        let text = |i: u32| (i != 0).then(|| strings[i as usize].clone());
        let mut edges = Vec::with_capacity(compact.len() + STITCH_EDGE_HEADROOM);
        let mut names = Vec::with_capacity(if keep_names { 0 } else { compact.len() });
        for e in &compact {
            let (name, road_ref) = if keep_names {
                (text(e.name), text(e.road_ref))
            } else {
                names.push((e.name, e.road_ref));
                (None, None)
            };
            edges.push(GraphEdge {
                id: String::new(),
                source: NodeId(e.source),
                target: NodeId(e.target),
                length_m: e.length_m,
                base_weight: e.base_weight,
                cost_mult: 1.0,
                eco_weight: Some(e.base_weight),
                start_lat: e.start_lat,
                start_lon: e.start_lon,
                end_lat: e.end_lat,
                end_lon: e.end_lon,
                shape: Vec::new(),
                highway: text(e.highway),
                maxspeed_kmh: None,
                maxspeed_practical_kmh: None,
                maxspeed_advisory_kmh: None,
                maxspeed_type: None,
                maxspeed_variable: false,
                minspeed_kmh: None,
                name,
                road_ref,
                is_motorroad: false,
                is_expressway: false,
                is_oneway: e.is_oneway,
                lanes: None,
                maxweight_t: None,
                maxaxleload_t: None,
                maxbogieweight_t: None,
                maxheight_m: None,
                maxwidth_m: None,
                maxlength_m: None,
                is_toll: e.is_toll,
                is_ferry: e.is_ferry,
                ferry_interval_min: None,
                is_tunnel: e.is_tunnel,
                is_boardwalk_crossing: false,
                is_roundabout: false,
                motor_vehicle_conditional: None,
                access_conditional: None,
                maxspeed_conditional: None,
                access_forbidden: false,
                surface_quality: SurfaceQuality::Good,
            });
        }
        drop(compact);
        let mut graph = RouteGraph::from_parts(nodes, edges, profile);
        let n_intra =
            stitch_intra_skeleton_component_gaps(&mut graph, &regions, INTRA_SKELETON_STITCH_MAX_M);
        let n_adj =
            stitch_adjacent_skeleton_gaps(&mut graph, &regions, ADJACENT_SKELETON_STITCH_MAX_M);
        if n_intra + n_adj > 0 {
            log::info!(
                target: "NaviPlan",
                "skeleton_adjacent_stitch intra={n_intra} adjacent={n_adj} \
                 adj_max_m={ADJACENT_SKELETON_STITCH_MAX_M} intra_max_m={INTRA_SKELETON_STITCH_MAX_M}"
            );
        }
        CoarseGraph {
            graph,
            regions,
            names: CoarseEdgeNames {
                strings: if keep_names { Vec::new() } else { strings },
                edge: names,
            },
        }
    }
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
    skels: &[SkeletonRegionNodes],
    max_m: f64,
) -> u32 {
    let mut added = 0u32;
    // Ignore tiny islands; Elsa jamtland cut is 3342↔177 nodes.
    const MIN_COMP_NODES: usize = 50;
    for skel in skels {
        let skel_set: HashSet<i64> = skel.node_ids.iter().copied().collect();
        let mut parent: HashMap<i64, i64> = skel.node_ids.iter().map(|&oid| (oid, oid)).collect();
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
    skels: &[SkeletonRegionNodes],
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
        let max_lat = s
            .node_lats
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let min_lon = s.node_lons.iter().copied().fold(f64::INFINITY, f64::min);
        let max_lon = s
            .node_lons
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
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
    border_osm_from_node_arrays(
        skels
            .iter()
            .map(|s| (&s.node_ids[..], &s.node_is_border[..])),
    )
}

/// [`border_osm_from_skeletons`] on the node arrays kept after a merge.
pub fn border_osm_from_regions(regions: &[SkeletonRegionNodes]) -> HashSet<i64> {
    border_osm_from_node_arrays(
        regions
            .iter()
            .map(|r| (&r.node_ids[..], &r.node_is_border[..])),
    )
}

fn border_osm_from_node_arrays<'a>(
    regions: impl Iterator<Item = (&'a [i64], &'a [u8])> + Clone,
) -> HashSet<i64> {
    let mut out = HashSet::new();
    for (ids, border) in regions.clone() {
        for (i, &oid) in ids.iter().enumerate() {
            if border.get(i).copied().unwrap_or(0) != 0 {
                out.insert(oid);
            }
        }
    }
    // Also treat OSM ids present in ≥2 skeletons as borders even if unmarked.
    let sets: Vec<HashSet<i64>> = regions
        .map(|(ids, _)| ids.iter().copied().collect())
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
        (true, true) => e.highway.clone().unwrap_or_else(|| {
            if e.is_ferry {
                "ferry".into()
            } else {
                "?".into()
            }
        }),
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
        ("Mannheller", 61.1435, 7.3239, 4000.0),
        ("Fodnes", 61.0863, 7.3742, 4000.0),
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
    let raw = e
        .name
        .as_deref()
        .or(e.road_ref.as_deref())
        .unwrap_or("")
        .trim();
    for sep in [" – ", " - ", " — ", "–"] {
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

/// Metres the path must continue in the new country before a border node is a
/// hop joint. A shorter visit that returns the way it came is a hair, not a crossing.
const NET_CROSSING_STAY_M: f64 = 8_000.0;

/// Extract joints: user vias + region border crossings + ferry terminals on path.
/// No evenly spaced hop_deg samples.
///
/// A border crossing is emitted only when the path really passes from one
/// country to another (or boards a ferry): the node is in `border_osm`, the
/// country ISO (or ferry leg) changes, and the path does not visit that node
/// and return the same way. A short enter-and-return through the same crossing
/// is not a joint.
pub fn extract_coarse_joints(
    graph: &RouteGraph,
    path: &[NodeId],
    edge_indices: &[usize],
    border_osm: &HashSet<i64>,
    vias: &[(f64, f64)],
    via_snap_m: f64,
) -> Vec<CoarseJoint> {
    let (path, edge_indices) = collapse_path_retraces(path, edge_indices);
    let path = path.as_slice();
    let edge_indices = edge_indices.as_slice();
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
        // Border: shared OSM node where the path really changes country.
        if border_osm.contains(&arrive.0) {
            let iso_a = country_iso_for_edge(e);
            let iso_b = edge_indices
                .get(ei + 1)
                .and_then(|&j| graph.edges.get(j))
                .map(country_iso_for_edge)
                .unwrap_or_else(|| iso_a.clone());
            if (iso_a != iso_b || e.is_ferry)
                && !border_visit_returns_same_way(graph, edge_indices, ei, arrive, &iso_a)
            {
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

/// Drop A-B-A (and longer return-to-the-same-node) hairs so a border node the
/// path only touches on the way out and back is not a hop joint.
/// Drop a geometric out-and-back on a lat/lon path: if the walk returns
/// within 200 m of an earlier vertex after at least 3 km, keep the first
/// visit and discard the excursion. Used for hop-corridor samples so a
/// skeleton spur is not loaded as the detailed search band.
fn collapse_coord_retraces(path: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = Vec::with_capacity(path.len());
    for &p in path {
        if let Some(j) = coord_return_index(&out, p) {
            out.truncate(j + 1);
            continue;
        }
        out.push(p);
    }
    out
}

fn coord_return_index(out: &[(f64, f64)], p: (f64, f64)) -> Option<usize> {
    if out.len() < 2 {
        return None;
    }
    let last = *out.last().unwrap();
    let mut walked = haversine_m(last.0, last.1, p.0, p.1);
    for j in (0..out.len()).rev() {
        if j + 1 < out.len() {
            walked += haversine_m(out[j].0, out[j].1, out[j + 1].0, out[j + 1].1);
        }
        if walked < 3_000.0 {
            continue;
        }
        if haversine_m(out[j].0, out[j].1, p.0, p.1) <= 200.0 {
            return Some(j);
        }
        if walked > 40_000.0 {
            break;
        }
    }
    None
}

fn collapse_node_retraces(path: &[NodeId]) -> Vec<NodeId> {
    let mut nodes: Vec<NodeId> = Vec::with_capacity(path.len());
    let mut pos: HashMap<i64, usize> = HashMap::new();
    for &nid in path {
        if let Some(&j) = pos.get(&nid.0) {
            for dropped in nodes.drain(j + 1..) {
                pos.remove(&dropped.0);
            }
            continue;
        }
        pos.insert(nid.0, nodes.len());
        nodes.push(nid);
    }
    nodes
}

fn collapse_path_retraces(path: &[NodeId], edge_indices: &[usize]) -> (Vec<NodeId>, Vec<usize>) {
    let nodes = collapse_node_retraces(path);
    if nodes.len() < 2 {
        return (nodes, Vec::new());
    }
    let mut ix: HashMap<(i64, i64), usize> = HashMap::new();
    for (i, &eidx) in edge_indices.iter().enumerate() {
        if i + 1 >= path.len() {
            break;
        }
        ix.insert((path[i].0, path[i + 1].0), eidx);
    }
    let mut edges = Vec::with_capacity(nodes.len().saturating_sub(1));
    for w in nodes.windows(2) {
        if let Some(&e) = ix.get(&(w[0].0, w[1].0)) {
            edges.push(e);
        }
    }
    (nodes, edges)
}

/// True when the path touches `crossing` and comes back the same way, or
/// enters the neighbouring country and returns to `old_iso` before staying
/// [`NET_CROSSING_STAY_M`].
fn border_visit_returns_same_way(
    graph: &RouteGraph,
    edge_indices: &[usize],
    crossing_ei: usize,
    crossing: NodeId,
    old_iso: &str,
) -> bool {
    let Some(arrive_e) = graph.edges.get(edge_indices[crossing_ei]) else {
        return false;
    };
    if let Some(&next_i) = edge_indices.get(crossing_ei + 1) {
        if let Some(next) = graph.edges.get(next_i) {
            if next.source == arrive_e.target && next.target == arrive_e.source {
                return true;
            }
        }
    }
    let mut walked = 0.0_f64;
    for &idx in edge_indices.iter().skip(crossing_ei + 1) {
        let Some(ne) = graph.edges.get(idx) else {
            continue;
        };
        if ne.target == crossing {
            return true;
        }
        if country_iso_for_edge(ne) == old_iso {
            return true;
        }
        walked += ne.length_m;
        if walked >= NET_CROSSING_STAY_M {
            return false;
        }
    }
    false
}

fn country_iso_for_edge(e: &GraphEdge) -> String {
    // Test graphs may pin ISO on the edge id so unit tests do not wait on the
    // Natural Earth index build (`iso_at` can block for minutes when cold).
    if let Some(rest) = e.id.strip_prefix("iso:") {
        let code = rest.split('-').next().unwrap_or(rest);
        if code.len() == 2 {
            return code.to_ascii_uppercase();
        }
    }
    let lat = (e.start_lat + e.end_lat) * 0.5;
    let lon = (e.start_lon + e.end_lon) * 0.5;
    country_iso_at(lat, lon)
        .unwrap_or("XX")
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
        let ent = by_iso
            .entry(iso)
            .or_insert_with(|| (0.0, 0.0, 0.0, Vec::new()));
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

/// True when `path` visits each via in order (nearest node within `snap_m`).
pub fn path_visits_vias_in_order(
    graph: &RouteGraph,
    path: &[NodeId],
    vias: &[(f64, f64)],
    snap_m: f64,
) -> bool {
    if vias.is_empty() {
        return true;
    }
    if path.is_empty() {
        return false;
    }
    let mut from = 0usize;
    for &(vlat, vlon) in vias {
        let hit = path.iter().enumerate().skip(from).find_map(|(i, nid)| {
            let n = graph.nodes.get(nid)?;
            let d = haversine_m(vlat, vlon, n.coord.y, n.coord.x);
            (d <= snap_m).then_some(i)
        });
        match hit {
            Some(i) => from = i.saturating_add(1),
            None => return false,
        }
    }
    true
}

/// Directed travel-time coarse path through optional vias.
///
/// Applies the caller's avoid flags (ferries / tolls / tunnels / motorways) so
/// the Stage B corridor matches detailed hop planning. Surface-transition state
/// stays off: on a major-road skeleton it bloated expansions and steered free
/// A* off Fehmarn onto HH.
///
/// Each waypoint is snapped once ([`snap_coarse_waypoints`]); each consecutive
/// pair is its own leg, and legs join at the shared via snap node.
pub fn coarse_shortest_path(
    graph: &mut RouteGraph,
    waypoints: &[(f64, f64)],
    snap_m: f64,
    route_options: &RouteOptions,
) -> Option<(Vec<NodeId>, Vec<usize>, f64)> {
    if waypoints.len() < 2 {
        return None;
    }
    let snaps = snap_coarse_waypoints(graph, waypoints, snap_m, route_options)?;
    let opts = coarse_route_options(route_options);
    let mut full_path: Vec<NodeId> = Vec::new();
    let mut full_edges: Vec<usize> = Vec::new();
    let mut total_cost = 0.0;
    for w in snaps.windows(2) {
        let (path, edges, cost) = graph.shortest_path_with_options(w[0], w[1], false, &opts)?;
        append_leg_path(&mut full_path, &mut full_edges, path, edges);
        total_cost += cost;
    }
    Some((full_path, full_edges, total_cost))
}

fn coarse_route_options(route_options: &RouteOptions) -> RouteOptions {
    RouteOptions {
        surface_routing_mode: Some(SurfaceRoutingMode::Offroad),
        avoid_motorways: route_options.avoid_motorways,
        toll_policy: route_options.toll_policy,
        avoid_ferries: route_options.avoid_ferries,
        avoid_tunnels: route_options.avoid_tunnels,
        vehicle: route_options.vehicle.clone(),
        departure_local: route_options.departure_local,
        allowed_countries: route_options.allowed_countries.clone(),
        ..RouteOptions::default()
    }
}

/// Snap every waypoint once on the coarse graph: origin must reach the main
/// network, destination must be reachable from it, vias must do both.
pub fn snap_coarse_waypoints(
    graph: &mut RouteGraph,
    waypoints: &[(f64, f64)],
    snap_m: f64,
    route_options: &RouteOptions,
) -> Option<Vec<NodeId>> {
    use crate::routing::graph::SnapRole;
    graph.ensure_directed_snap_labels();
    let opts = coarse_route_options(route_options);
    let last = waypoints.len().checked_sub(1)?;
    waypoints
        .iter()
        .enumerate()
        .map(|(i, &(lat, lon))| {
            let snap_role = if i == 0 {
                SnapRole::Origin
            } else if i == last {
                SnapRole::Destination
            } else {
                SnapRole::Via
            };
            let o = RouteOptions {
                snap_role,
                ..opts.clone()
            };
            graph
                .nearest_routable_with_options_max(lat, lon, &o, false, snap_m)
                .ok()
                .map(|(id, _)| id)
        })
        .collect()
}

/// Append one leg to a trip path; the leg must start at the trip's last node.
fn append_leg_path(
    full_path: &mut Vec<NodeId>,
    full_edges: &mut Vec<usize>,
    path: Vec<NodeId>,
    edges: Vec<usize>,
) {
    if full_path.is_empty() {
        *full_path = path;
    } else {
        full_path.extend(path.into_iter().skip(1));
    }
    full_edges.extend(edges);
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
            let multi = rs.contains('|')
                || rt.contains('|')
                || (rs != rt && !rs.is_empty() && !rt.is_empty());
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
    /// Ordered hop waypoints: origin, joints (border/ferry), exact user vias,
    /// destination.
    pub hops: Vec<(f64, f64)>,
    /// Full coarse path node lat/lon (for corridor tiles + pad).
    pub coarse_path: Vec<(f64, f64)>,
    pub total_min: f64,
    pub total_km: f64,
    pub ferries: Vec<String>,
    pub note: String,
    /// Per-leg alternatives with eligibility and pick (one entry per leg).
    pub legs: Vec<Vec<StageBLegCandidate>>,
    /// `stage_b_timing ...` line: phase wall times, graph size, VmHWM.
    pub timing: String,
    /// Uninstalled outlines named by the geometric checks (band + major ends).
    pub missing_advisory: String,
}

impl StageBDensify {
    /// One report line per leg alternative.
    pub fn alternatives_report(&self) -> String {
        let mut out = format!("{}\n", self.timing);
        for (li, leg) in self.legs.iter().enumerate() {
            for c in leg {
                out.push_str(&format!(
                    "stage_b_leg={} alt={} km={:.1} min={:.1} ferries={} eligible={} pick={} \
                     search_ms={}\n",
                    li + 1,
                    c.name,
                    c.km,
                    c.min,
                    c.ferries,
                    c.eligible,
                    c.picked,
                    c.search_ms
                ));
            }
        }
        if !self.missing_advisory.is_empty() {
            out.push_str(&self.missing_advisory);
        }
        out
    }
}

/// One coarse alternative for one leg (between two consecutive waypoints).
#[derive(Debug, Clone)]
pub struct StageBLegCandidate {
    pub name: String,
    pub km: f64,
    pub min: f64,
    pub ferries: usize,
    pub eligible: bool,
    pub picked: bool,
    pub search_ms: u128,
}

/// Approved near-equal rule. Input per alternative: `(total_min, ferries, km)`.
///
/// Eligible iff total time is within 2 % of the fastest and it has no more
/// ferries than the fastest (several equally fastest: the fewest of theirs).
/// Among eligible, fewer km wins; equal km falls back to time.
/// Returns the eligibility flags and the picked index.
pub fn near_equal_pick(alts: &[(f64, usize, f64)]) -> Option<(Vec<bool>, usize)> {
    let best_min = alts.iter().map(|a| a.0).fold(f64::INFINITY, f64::min);
    if !best_min.is_finite() {
        return None;
    }
    let fastest_ferries = alts
        .iter()
        .filter(|a| a.0 <= best_min + 1e-9)
        .map(|a| a.1)
        .min()?;
    let eligible: Vec<bool> = alts
        .iter()
        .map(|a| a.0 <= best_min * 1.02 && a.1 <= fastest_ferries)
        .collect();
    let pick = (0..alts.len()).filter(|&i| eligible[i]).min_by(|&i, &j| {
        alts[i]
            .2
            .total_cmp(&alts[j].2)
            .then_with(|| alts[i].0.total_cmp(&alts[j].0))
    })?;
    Some((eligible, pick))
}

/// One coarse alternative path for a leg.
struct LegAlternative {
    name: String,
    report: CoarseRouteReport,
    path: Vec<NodeId>,
    search_ms: u128,
}

/// Alternatives for one leg `from` to `to`: free, each ferry of the free path
/// excluded, and no ferries (the last two only when ferries are allowed).
/// Every alternative starts at `from` and ends at `to`.
///
/// A search whose result is already known is not run: with no ferry on the free
/// path there is nothing to exclude and no-ferries is the free path; an
/// exclusion that forbids no edge is the free path; and once an exclusion
/// returns a path without ferries, that path is also the no-ferries optimum.
fn leg_alternatives(
    graph: &mut RouteGraph,
    names: &CoarseEdgeNames,
    border_osm: &HashSet<i64>,
    from: NodeId,
    to: NodeId,
    route_options: &RouteOptions,
) -> Vec<LegAlternative> {
    let opts = coarse_route_options(route_options);
    let mut out: Vec<LegAlternative> = Vec::new();
    let push = |graph: &mut RouteGraph, name: String, note: &str, out: &mut Vec<LegAlternative>| {
        let t = std::time::Instant::now();
        let Some((path, edges, _)) = graph.shortest_path_with_options(from, to, false, &opts)
        else {
            return;
        };
        let search_ms = t.elapsed().as_millis();
        if path.first() != Some(&from) || path.last() != Some(&to) {
            return;
        }
        names.apply(graph, &edges);
        let report =
            build_coarse_route_report(&name, graph, &path, &edges, border_osm, &[], 0, 0, note);
        out.push(LegAlternative {
            name,
            report,
            path,
            search_ms,
        });
    };
    push(graph, "free".into(), "stage_b free", &mut out);
    if route_options.avoid_ferries || out.is_empty() || out[0].report.ferries.is_empty() {
        return out;
    }
    let free_ferries = out[0].report.ferries.clone();
    for leg in &free_ferries {
        clear_access_forbidden(graph);
        if forbid_ferry_edges_matching_leg(graph, leg, 3_000.0) == 0 {
            continue;
        }
        let name = format!("excl_{}_{}", leg.from_terminal, leg.to_terminal);
        let note = format!("excl {}→{}", leg.from_terminal, leg.to_terminal);
        push(graph, name, &note, &mut out);
    }
    clear_access_forbidden(graph);
    if out.iter().any(|a| a.report.ferries.is_empty()) {
        return out;
    }
    for e in graph.edges.iter_mut().filter(|e| e.is_ferry) {
        e.access_forbidden = true;
    }
    push(graph, "no_ferries".into(), "no ferries", &mut out);
    clear_access_forbidden(graph);
    out
}

/// Stage B on a merged skeleton graph: waypoints are already attached, pick
/// each leg from its own alternatives, join legs at the via snap nodes.
struct StageBPlan {
    snaps: Vec<NodeId>,
    legs: Vec<Vec<StageBLegCandidate>>,
    picks: Vec<LegAlternative>,
    snap_ms: u128,
}

fn stage_b_plan_on_graph(
    graph: &mut RouteGraph,
    names: &CoarseEdgeNames,
    border_osm: &HashSet<i64>,
    snaps: Vec<NodeId>,
    snap_ms: u128,
    route_options: &RouteOptions,
) -> Option<StageBPlan> {
    if snaps.len() < 2 {
        return None;
    }
    let mut legs = Vec::with_capacity(snaps.len() - 1);
    let mut picks = Vec::with_capacity(snaps.len() - 1);
    for (li, w) in snaps.windows(2).enumerate() {
        let mut alts = leg_alternatives(graph, names, border_osm, w[0], w[1], route_options);
        let summary: Vec<(f64, usize, f64)> = alts
            .iter()
            .map(|a| {
                (
                    a.report.total_min,
                    a.report.ferries.len(),
                    a.report.total_km,
                )
            })
            .collect();
        let Some((eligible, pick)) = near_equal_pick(&summary) else {
            log::warn!(target: "NaviPlan", "stage_b leg={} no alternative", li + 1);
            return None;
        };
        let table: Vec<StageBLegCandidate> = alts
            .iter()
            .enumerate()
            .map(|(i, a)| StageBLegCandidate {
                name: a.name.clone(),
                km: a.report.total_km,
                min: a.report.total_min,
                ferries: a.report.ferries.len(),
                eligible: eligible[i],
                picked: i == pick,
                search_ms: a.search_ms,
            })
            .collect();
        for c in &table {
            log::info!(
                target: "NaviPlan",
                "stage_b leg={} alt={} km={:.1} min={:.1} ferries={} eligible={} pick={}",
                li + 1,
                c.name,
                c.km,
                c.min,
                c.ferries,
                c.eligible,
                c.picked
            );
        }
        legs.push(table);
        picks.push(alts.swap_remove(pick));
    }
    Some(StageBPlan {
        snaps,
        legs,
        picks,
        snap_ms,
    })
}

/// Merge the fresh skeletons of `dirs` into one coarse graph, reading and
/// dropping one region file at a time; when `only_stems` is set, every other
/// region is skipped. `None` when no skeleton is fresh. Adds file read time
/// to `load_ms`.
///
/// A skeleton is used only when its meta matches the current build, format,
/// pack and neighbour-pack fingerprint ([`skeleton_fresh`]), the same test the
/// app's idle builder uses, so a skeleton from another build is never read.
///
/// [`skeleton_fresh`]: crate::routing::indexed::skeleton_fresh
fn load_coarse_graph(
    dirs: &[&Path],
    only_stems: Option<&HashSet<String>>,
    profile: RoutingProfile,
    load_ms: &mut u128,
) -> Option<CoarseGraph> {
    let mut b = CoarseGraphBuilder::new(false);
    for_each_fresh_skeleton(dirs, only_stems, profile, load_ms, |s| b.add(&s));
    if b.regions.is_empty() {
        return None;
    }
    Some(b.finish(profile))
}

fn for_each_fresh_skeleton(
    dirs: &[&Path],
    only_stems: Option<&HashSet<String>>,
    profile: RoutingProfile,
    load_ms: &mut u128,
    mut f: impl FnMut(CorridorSkeletonFile),
) {
    let mut seen = HashSet::new();
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut stems: Vec<String> = rd
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                name.strip_suffix(".navi-corridor-skeleton.bin")
                    .or_else(|| name.strip_suffix(".navi-corridor-skeleton.json"))
                    .map(str::to_string)
            })
            .collect();
        stems.sort();
        stems.dedup();
        for stem in stems {
            if only_stems.is_some_and(|want| !want.contains(&stem)) || seen.contains(&stem) {
                continue;
            }
            let t = Instant::now();
            if !crate::routing::indexed::skeleton_fresh(dir, &stem, profile) {
                log::warn!(target: "NaviPlan", "stage_b skip stale skeleton stem={stem}");
                continue;
            }
            let Some(p) = resolve_skeleton_path(dir, &stem) else {
                continue;
            };
            let Ok(s) = read_skeleton_file(&p) else {
                continue;
            };
            *load_ms += t.elapsed().as_millis();
            seen.insert(stem);
            f(s);
        }
    }
}

/// OSM ids of every fresh skeleton (or only `only_stems`). Used as attach
/// targets without loading the coarse graph.
pub fn collect_fresh_skeleton_osm_ids(
    dirs: &[&Path],
    only_stems: Option<&HashSet<String>>,
    profile: RoutingProfile,
) -> HashSet<i64> {
    let mut ids = HashSet::new();
    let mut dummy = 0u128;
    for_each_fresh_skeleton(dirs, only_stems, profile, &mut dummy, |s| {
        ids.extend(s.node_ids.iter().copied());
    });
    ids
}

/// Stems for Stage B: direct-corridor regions plus one-hop adjacency neighbours.
/// Returns `None` when the corridor cannot be resolved (caller loads all).
fn trip_local_skeleton_stems(waypoints: &[(f64, f64)]) -> Option<HashSet<String>> {
    let corridor = crate::long_trip::direct_corridor_regions_for_trip(waypoints, None).ok()?;
    if corridor.is_empty() {
        return None;
    }
    let mut region_ids: HashSet<String> = HashSet::new();
    for id in &corridor {
        region_ids.insert(id.clone());
        for n in crate::long_trip::adjacent_region_ids(id) {
            region_ids.insert(n.to_string());
        }
    }
    Some(
        region_ids
            .into_iter()
            .map(|id| crate::pack_server::leaf_stem_for_region_id(&id))
            .collect(),
    )
}

/// Forbid the ferry edges of `leg` (either direction); returns how many.
fn forbid_ferry_edges_matching_leg(
    graph: &mut RouteGraph,
    leg: &CoarseFerryLeg,
    match_m: f64,
) -> usize {
    let mut n = 0;
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
            n += 1;
        }
    }
    n
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

fn hops_from_report(
    report: &CoarseRouteReport,
    start: (f64, f64),
    end: (f64, f64),
) -> Vec<(f64, f64)> {
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
    nearest_coarse_path_node(path, p).0
}

/// Nearest coarse-path vertex to `p` and the haversine distance in metres.
pub fn nearest_coarse_path_node(path: &[(f64, f64)], p: (f64, f64)) -> (usize, f64) {
    let mut best = (0usize, f64::MAX);
    for (i, &q) in path.iter().enumerate() {
        let d = haversine_m(p.0, p.1, q.0, q.1);
        if d < best.1 {
            best = (i, d);
        }
    }
    best
}

/// Every intermediate hop end must be a coarse-path node (within `match_m`).
/// User start/end of the chain are left as given (planner snaps those later).
pub fn ensure_hops_on_coarse_path(
    hops: &[(f64, f64)],
    coarse_path: &[(f64, f64)],
    match_m: f64,
) -> Result<Vec<(f64, f64)>, String> {
    if hops.len() < 2 || coarse_path.len() < 2 {
        return Ok(hops.to_vec());
    }
    let mut out = Vec::with_capacity(hops.len());
    for (i, &h) in hops.iter().enumerate() {
        let user_od = i == 0 || i + 1 == hops.len();
        if user_od {
            out.push(h);
            continue;
        }
        let (idx, d) = nearest_coarse_path_node(coarse_path, h);
        if d > match_m {
            return Err(format!(
                "hop joint {:.5},{:.5} is {d:.1} m from the coarse path (limit {match_m} m)",
                h.0, h.1
            ));
        }
        out.push(coarse_path[idx]);
    }
    Ok(out)
}

/// Graph node at `p` within `match_m`, if that exact neighbourhood is loaded.
fn graph_node_near_point(
    graph: &RouteGraph,
    p: (f64, f64),
    match_m: f64,
) -> Option<osm4routing::NodeId> {
    let opts = crate::routing::graph::RouteOptions::default();
    graph
        .nearest_routable_with_options_max(p.0, p.1, &opts, false, match_m)
        .ok()
        .map(|(id, _)| id)
}

/// Last coarse-path vertex after `start_id` that sits in the start node's
/// weak component. Walks the path and checks components; does not search for
/// a nearest off-path node.
pub fn last_reachable_coarse_path_node(
    graph: &RouteGraph,
    start_id: osm4routing::NodeId,
    coarse_path: &[(f64, f64)],
    intended_end: (f64, f64),
    match_m: f64,
) -> Option<(osm4routing::NodeId, (f64, f64))> {
    if coarse_path.len() < 2 {
        return None;
    }
    let start_ll = graph.node_lat_lon(start_id)?;
    let i0 = nearest_coarse_index(coarse_path, start_ll);
    let i1 = nearest_coarse_index(coarse_path, intended_end);
    if i1 <= i0 {
        return None;
    }
    let mut last = None;
    for &p in &coarse_path[i0 + 1..=i1] {
        let Some(nid) = graph_node_near_point(graph, p, match_m) else {
            continue;
        };
        if graph.same_weak_component(start_id, nid) {
            last = Some((nid, p));
        }
    }
    last
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
            let gap_ok = out
                .last()
                .is_none_or(|&q| haversine_m(q.0, q.1, last_emit.0, last_emit.1) >= min_gap);
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
/// The trip is planned leg by leg between consecutive waypoints. Every waypoint
/// is attached to the skeleton by a detailed search (real road snap + travel-
/// time spurs); each leg has its own alternatives (free, each ferry excluded,
/// no ferries; the last two only when ferries are allowed) and its own
/// near-equal pick ([`near_equal_pick`]). Legs join at the via snap node, and
/// hop chains run to the exact via coordinates, so no step sees a trip-wide
/// route that skips a via. Hop joints = border crossings, ferry terminals,
/// user vias.
pub fn try_stage_b_densify_from_skeletons(
    pack_dirs: &[&Path],
    waypoints: &[(f64, f64)],
    profile: RoutingProfile,
    route_options: &RouteOptions,
) -> Result<StageBDensify, String> {
    if waypoints.len() < 2 {
        return Err("stage_b needs at least two waypoints".into());
    }
    let t0 = std::time::Instant::now();
    let mut load_ms = 0u128;
    let mut graph_ms = 0u128;
    let mut plan_ms = 0u128;
    let mut widened = false;
    // Trip-local first: corridor + neighbours. Widen to every installed skeleton
    // only when the coarse search finds no route on that subset.
    let local_stems = trip_local_skeleton_stems(waypoints);
    let t = std::time::Instant::now();
    let load_before = load_ms;
    let coarse = match &local_stems {
        Some(stems) => {
            let c = load_coarse_graph(pack_dirs, Some(stems), profile, &mut load_ms);
            log::info!(
                target: "NaviPlan",
                "stage_b trip_local stems={} loaded={}",
                stems.len(),
                c.as_ref().map_or(0, |c| c.regions.len())
            );
            c
        }
        None => load_coarse_graph(pack_dirs, None, profile, &mut load_ms),
    };
    graph_ms += t
        .elapsed()
        .as_millis()
        .saturating_sub(load_ms - load_before);
    let Some(mut coarse) = coarse else {
        log::info!(target: "NaviPlan", "stage_b densify: no fresh persistent skeletons");
        return Err("no fresh persistent skeletons".into());
    };
    let mut border_osm = border_osm_from_regions(&coarse.regions);
    coarse.graph.ensure_directed_snap_labels();
    // Targets are directed-ok skeleton nodes of the loaded graph (same set the
    // coarse search can use). Search each waypoint once; a widen reapplies.
    let skeleton_ids: HashSet<i64> = coarse
        .graph
        .nodes
        .keys()
        .filter(|id| coarse.graph.directed_snap_ok(**id, crate::routing::graph::SnapRole::Via))
        .map(|id| id.0)
        .collect();
    let t_att = std::time::Instant::now();
    let attach_plan = match crate::routing::waypoint_attach::TripAttachPlan::resolve(
        pack_dirs,
        waypoints,
        &skeleton_ids,
        profile,
        route_options,
    ) {
        Ok(p) => p,
        Err(e) => {
            log::error!(target: "NaviPlan", "stage_b waypoint attach: {e}");
            return Err(e.to_string());
        }
    };
    let snap_ms = t_att.elapsed().as_millis();
    let snaps = attach_plan.apply(&mut coarse.graph);
    let t_plan = std::time::Instant::now();
    let mut plan = stage_b_plan_on_graph(
        &mut coarse.graph,
        &coarse.names,
        &border_osm,
        snaps,
        snap_ms,
        route_options,
    );
    plan_ms += t_plan.elapsed().as_millis();
    if plan.is_none() && local_stems.is_some() {
        log::info!(
            target: "NaviPlan",
            "stage_b trip_local miss; widening to all installed skeletons"
        );
        widened = true;
        drop(coarse);
        let t = std::time::Instant::now();
        let load_before = load_ms;
        coarse = load_coarse_graph(pack_dirs, None, profile, &mut load_ms)
            .ok_or_else(|| "no fresh persistent skeletons".to_string())?;
        graph_ms += t
            .elapsed()
            .as_millis()
            .saturating_sub(load_ms - load_before);
        border_osm = border_osm_from_regions(&coarse.regions);
        coarse.graph.ensure_directed_snap_labels();
        let snaps = attach_plan.apply(&mut coarse.graph);
        let t_plan = std::time::Instant::now();
        plan = stage_b_plan_on_graph(
            &mut coarse.graph,
            &coarse.names,
            &border_osm,
            snaps,
            snap_ms,
            route_options,
        );
        plan_ms += t_plan.elapsed().as_millis();
    }
    let plan = match plan {
        Some(p) => p,
        None => {
            return Err("stage_b no coarse path between attached waypoints".into());
        }
    };
    let snap_ms = plan.snap_ms;
    let t = std::time::Instant::now();
    let skels = coarse.regions.len();
    let mut out = assemble_stage_b(plan, &coarse.graph, waypoints, pack_dirs, profile, skels);
    let assemble_ms = t.elapsed().as_millis();
    let installed: Vec<String> = coarse
        .regions
        .iter()
        .map(|s| {
            if s.region_id.is_empty() {
                s.leaf_stem.clone()
            } else {
                s.region_id.clone()
            }
        })
        .collect();
    let mut skel_files = Vec::new();
    for r in &coarse.regions {
        for d in pack_dirs {
            if let Some(p) = resolve_skeleton_path(d, &r.leaf_stem) {
                if let Ok(s) = read_skeleton_file(&p) {
                    skel_files.push(s);
                    break;
                }
            }
        }
    }
    let major_hints =
        crate::long_trip::missing_major_continuations(&skel_files, &out.coarse_path, &installed);
    let band_missing =
        crate::long_trip::geometric_missing_regions_for_trip(waypoints, &installed, None)
            .unwrap_or_default();
    out.missing_advisory =
        crate::long_trip::missing_region_advisory_lines(&major_hints, &band_missing);
    let (graph_nodes, graph_edges) = (coarse.graph.nodes.len(), coarse.graph.edges.len());
    let hwm_mb = vm_hwm_mb();
    let rss_before_mb = vm_rss_mb();
    drop(coarse);
    drop(border_osm);
    release_free_heap();
    let rss_after_mb = vm_rss_mb();
    out.timing = format!(
        "stage_b_timing skels={skels} widened={widened} graph_nodes={graph_nodes} \
         graph_edges={graph_edges} load_ms={load_ms} graph_ms={graph_ms} plan_ms={plan_ms} \
         snap_ms={snap_ms} assemble_ms={assemble_ms} total_ms={} vm_hwm_mb={hwm_mb}\n\
         stage_b_release rss_before_mb={rss_before_mb} rss_after_mb={rss_after_mb}",
        t0.elapsed().as_millis(),
    );
    log::info!(target: "NaviPlan", "{}", out.timing);
    Ok(out)
}

/// Return freed heap pages to the system so the first detailed hop does not
/// start on top of the corridor stage's footprint.
fn release_free_heap() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    unsafe {
        libc::malloc_trim(0);
    }
    #[cfg(target_os = "android")]
    {
        extern "C" {
            fn mallopt(param: libc::c_int, value: libc::c_int) -> libc::c_int;
        }
        // Bionic M_PURGE_ALL (API 34+), else M_PURGE.
        const M_PURGE_ALL: libc::c_int = -104;
        const M_PURGE: libc::c_int = -101;
        unsafe {
            if mallopt(M_PURGE_ALL, 0) == 0 {
                mallopt(M_PURGE, 0);
            }
        }
    }
}

fn proc_status_mb(key: &str) -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix(key))
                .and_then(|v| v.split_whitespace().next()?.parse::<u64>().ok())
        })
        .map(|kb| kb / 1024)
        .unwrap_or(0)
}

/// Process resident set (VmRSS) in MiB; 0 where `/proc` is unavailable.
pub fn vm_rss_mb() -> u64 {
    proc_status_mb("VmRSS:")
}

/// Process peak resident set (VmHWM) in MiB; 0 where `/proc` is unavailable.
pub fn vm_hwm_mb() -> u64 {
    proc_status_mb("VmHWM:")
}

/// Join the per-leg picks into one hop chain. Each leg's hops run from its exact
/// start waypoint to its exact end waypoint; a via is the end of one leg and the
/// start of the next.
fn assemble_stage_b(
    plan: StageBPlan,
    graph: &RouteGraph,
    waypoints: &[(f64, f64)],
    pack_dirs: &[&Path],
    profile: RoutingProfile,
    skel_count: usize,
) -> StageBDensify {
    let mut hops: Vec<(f64, f64)> = Vec::new();
    let mut coarse_path: Vec<(f64, f64)> = Vec::new();
    let mut total_min = 0.0;
    let mut total_km = 0.0;
    let mut ferries: Vec<String> = Vec::new();
    let mut picks_note: Vec<String> = Vec::new();
    for (li, pick) in plan.picks.iter().enumerate() {
        let leg_start = waypoints[li];
        let leg_end = waypoints[li + 1];
        let path = collapse_node_retraces(&pick.path);
        let leg_coarse = collapse_coord_retraces(&path_latlon(graph, &path));
        let raw_hops = split_hops_by_path_tile_budget(
            hops_from_report(&pick.report, leg_start, leg_end),
            &leg_coarse,
            pack_dirs,
            profile,
        );
        let match_m = crate::routing::plan_bbox::HOP_END_MATCH_M;
        let leg_hops = match ensure_hops_on_coarse_path(&raw_hops, &leg_coarse, match_m) {
            Ok(h) => h,
            Err(e) => {
                log::error!(target: "NaviPlan", "hop_chain {e}");
                // Never keep an off-path leftover as a hop end: pin to the path.
                raw_hops
                    .iter()
                    .enumerate()
                    .map(|(i, &h)| {
                        if i == 0 || i + 1 == raw_hops.len() {
                            h
                        } else {
                            let (idx, _) = nearest_coarse_path_node(&leg_coarse, h);
                            leg_coarse[idx]
                        }
                    })
                    .collect()
            }
        };
        let skip = usize::from(!hops.is_empty());
        hops.extend(leg_hops.into_iter().skip(skip));
        let skip = usize::from(!coarse_path.is_empty());
        coarse_path.extend(leg_coarse.into_iter().skip(skip));
        total_min += pick.report.total_min;
        total_km += pick.report.total_km;
        ferries.extend(
            pick.report
                .ferries
                .iter()
                .map(|f| format!("{}→{}", f.from_terminal, f.to_terminal)),
        );
        picks_note.push(format!("leg{}={}", li + 1, pick.name));
        debug_assert_eq!(pick.path.first(), Some(&plan.snaps[li]));
        debug_assert_eq!(pick.path.last(), Some(&plan.snaps[li + 1]));
    }
    let note = format!(
        "stage_b pick={} km={:.1} min={:.1} hops={} skels={} legs={} near_equal_2pct \
         max_path_nodes={} ferries={}",
        picks_note.join(","),
        total_km,
        total_min,
        hops.len().saturating_sub(1),
        skel_count,
        plan.picks.len(),
        crate::routing::plan_bbox::MAX_PATH_NODES_PER_HOP,
        ferries.len(),
    );
    log::info!(target: "NaviPlan", "{note} ferries={ferries:?}");
    StageBDensify {
        hops,
        coarse_path,
        total_min,
        total_km,
        ferries,
        note,
        legs: plan.legs,
        timing: String::new(),
        missing_advisory: String::new(),
    }
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
        assert!(
            keep.contains(&3),
            "secondary touching border node must stay"
        );
    }

    #[test]
    fn shared_osm_detects_border() {
        let a: HashSet<i64> = [1, 2, 3].into_iter().collect();
        let b: HashSet<i64> = [3, 4, 5].into_iter().collect();
        let shared = shared_osm_ids_across_regions(&[a, b]);
        assert_eq!(shared, HashSet::from([3]));
    }

    fn approach_pack(node_ids: [i64; 3], lats: [f64; 3], lons: [f64; 3]) -> FlatGraphPack {
        let n_e = 2;
        FlatGraphPack {
            has_delta_h: false,
            node_ids: node_ids.to_vec(),
            node_lats: lats.to_vec(),
            node_lons: lons.to_vec(),
            edge_src: vec![0, 1],
            edge_tgt: vec![1, 2],
            edge_length_m: vec![1_000.0, 400.0],
            edge_base_weight: vec![1_000.0, 400.0],
            edge_delta_h_m: vec![],
            edge_start_lat: vec![lats[0], lats[1]],
            edge_start_lon: vec![lons[0], lons[1]],
            edge_end_lat: vec![lats[1], lats[2]],
            edge_end_lon: vec![lons[1], lons[2]],
            edge_highway: vec!["trunk".into(), "secondary".into()],
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
            node_access_blocked: zeros_u8(3),
        }
    }

    #[test]
    fn shared_approach_node_joins_when_major_ids_miss() {
        // Two packs meet on a secondary node the major skeleton never shares.
        let north = approach_pack([10, 11, 99], [62.2, 62.1, 62.0], [13.4, 13.4, 13.4]);
        let south = approach_pack([20, 21, 99], [61.8, 61.9, 62.0], [13.4, 13.4, 13.4]);
        let major_shared: HashSet<i64> = major_node_osm_ids(&north)
            .intersection(&major_node_osm_ids(&south))
            .copied()
            .collect();
        assert!(
            major_shared.is_empty(),
            "the cut is not on a major edge: {major_shared:?}"
        );
        let cand_n = border_candidate_osm_ids(&north, None);
        let cand_s = border_candidate_osm_ids(&south, None);
        let shared: HashSet<i64> = cand_n.intersection(&cand_s).copied().collect();
        assert_eq!(shared, HashSet::from([99]));
        let band = border_band_bbox(&[61.9, 13.3, 62.1, 13.5], &[61.9, 13.3, 62.1, 13.5], 0.0);
        assert!(border_candidate_osm_ids(&north, Some(&band)).contains(&99));
        assert!(!border_candidate_osm_ids(&north, Some(&[63.0, 13.3, 64.0, 13.5])).contains(&99));

        let n = build_skeleton_from_pack(&north, "europe/n", "n", "car", &shared);
        let s = build_skeleton_from_pack(&south, "europe/s", "s", "car", &shared);
        assert!(n.node_ids.contains(&99));
        assert!(s.node_ids.contains(&99));
        let g = merge_skeletons_to_route_graph(&[n, s], RoutingProfile::Car);
        let a = g.weak_component_id(NodeId(10));
        let b = g.weak_component_id(NodeId(20));
        assert_eq!(a, b, "secondary-only cut must join the two major spines");
    }

    #[test]
    fn long_secondary_chain_to_major_is_kept() {
        // 20 x 800 m of secondary from a border node to a trunk (~16 km).
        // A 12-hop walk would stop short and leave the cut unjoined.
        let hops = 20usize;
        let n = hops + 2;
        let mut node_ids = vec![0i64; n];
        let mut lats = vec![0.0; n];
        let mut lons = vec![0.0; n];
        for i in 0..n {
            node_ids[i] = 1_000 + i as i64;
            lats[i] = 62.0 - i as f64 * 0.007;
            lons[i] = 13.4;
        }
        node_ids[0] = 99;
        let n_e = hops + 1;
        let mut src = Vec::new();
        let mut tgt = Vec::new();
        let mut hw = Vec::new();
        let mut len = Vec::new();
        src.push(hops as u32);
        tgt.push((hops + 1) as u32);
        hw.push("trunk".into());
        len.push(800.0);
        for i in 0..hops {
            src.push(i as u32);
            tgt.push((i + 1) as u32);
            hw.push("secondary".into());
            len.push(800.0);
        }
        let pack = FlatGraphPack {
            has_delta_h: false,
            node_ids,
            node_lats: lats.clone(),
            node_lons: lons.clone(),
            edge_src: src,
            edge_tgt: tgt,
            edge_length_m: len.clone(),
            edge_base_weight: len,
            edge_delta_h_m: vec![],
            edge_start_lat: vec![0.0; n_e],
            edge_start_lon: vec![0.0; n_e],
            edge_end_lat: vec![0.0; n_e],
            edge_end_lon: vec![0.0; n_e],
            edge_highway: hw,
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
            node_access_blocked: zeros_u8(n),
        };
        let borders = HashSet::from([99]);
        let (keep, _, border_nodes) = select_skeleton_edge_indices(&pack, &borders);
        assert!(border_nodes.contains(&0));
        assert!(keep.contains(&0), "trunk must stay");
        assert_eq!(
            keep.len(),
            n_e,
            "every secondary hop on the path to the trunk must stay, kept={keep:?}"
        );
    }

    #[test]
    fn border_approach_keeps_both_directed_edges() {
        let n_e = 3;
        let pack = FlatGraphPack {
            has_delta_h: false,
            node_ids: vec![99, 11, 10],
            node_lats: vec![62.0, 62.01, 62.02],
            node_lons: vec![13.4, 13.4, 13.4],
            edge_src: vec![2, 0, 1],
            edge_tgt: vec![1, 1, 0],
            edge_length_m: vec![800.0, 400.0, 400.0],
            edge_base_weight: vec![800.0, 400.0, 400.0],
            edge_delta_h_m: vec![],
            edge_start_lat: vec![62.02, 62.0, 62.01],
            edge_start_lon: vec![13.4, 13.4, 13.4],
            edge_end_lat: vec![62.01, 62.01, 62.0],
            edge_end_lon: vec![13.4, 13.4, 13.4],
            edge_highway: vec!["trunk".into(), "secondary".into(), "secondary".into()],
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
            node_access_blocked: zeros_u8(3),
        };
        let (keep, _, _) = select_skeleton_edge_indices(&pack, &HashSet::from([99]));
        assert!(keep.contains(&0), "trunk");
        assert!(keep.contains(&1), "approach forward");
        assert!(keep.contains(&2), "approach reverse");
    }

    #[test]
    fn roundtrip_binary_counts() {
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
        let path = skeleton_path(dir.path(), "test");
        assert!(path.extension().and_then(|e| e.to_str()) == Some("bin"));
        let loaded = read_skeleton_file(&path).unwrap();
        assert_eq!(loaded.node_count, skel.node_count);
        assert_eq!(loaded.edge_count, skel.edge_count);
        let g = skeleton_to_route_graph(&loaded, RoutingProfile::Car);
        assert_eq!(g.nodes.len(), loaded.node_count as usize);
        assert_eq!(g.edges.len(), loaded.edge_count as usize);
    }

    #[test]
    fn stage_b_alternative_must_visit_every_via_in_order() {
        // Synthetic: origin → via → dest on a straight primary; a shortcut
        // edge skips the via. path_visits_vias_in_order must reject the skip.
        use geo_types::Coord;
        use osm4routing::Node;
        use std::collections::HashMap;

        let mut nodes = HashMap::new();
        for (id, lat, lon) in [
            (1i64, 60.0, 10.0),
            (2, 60.5, 10.0), // via
            (3, 61.0, 10.0),
            (4, 60.5, 10.5), // off-via diversion
        ] {
            nodes.insert(
                NodeId(id),
                Node {
                    id: NodeId(id),
                    coord: Coord { x: lon, y: lat },
                    uses: 0,
                },
            );
        }
        let mk = |id: &str, s: i64, t: i64, len: f64| -> GraphEdge {
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
                highway: Some("primary".into()),
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
                surface_quality: crate::routing::graph::SurfaceQuality::Good,
            }
        };
        let edges = vec![
            mk("a", 1, 2, 50_000.0),
            mk("b", 2, 3, 50_000.0),
            mk("skip", 1, 4, 40_000.0),
            mk("skip2", 4, 3, 40_000.0),
        ];
        let g = RouteGraph::from_parts(nodes, edges, RoutingProfile::Car);
        let via = (60.5, 10.0);
        let through_via = vec![NodeId(1), NodeId(2), NodeId(3)];
        let misses_via = vec![NodeId(1), NodeId(4), NodeId(3)];
        assert!(
            path_visits_vias_in_order(&g, &through_via, &[via], 2_500.0),
            "path through the via node must pass"
        );
        assert!(
            !path_visits_vias_in_order(&g, &misses_via, &[via], 2_500.0),
            "Stage B alternative that skips a user via must fail"
        );
    }

    /// Origin 1, via 2, destination 3 on a primary; 1-4-3 is a shorter trip that
    /// skips the via; a ferry 1-2 gives leg 1 a ferry alternative.
    fn via_trip_graph() -> RouteGraph {
        use geo_types::Coord;
        use osm4routing::Node;

        let mut nodes = HashMap::new();
        for (id, lat, lon) in [
            (1i64, 60.0, 10.0),
            (2, 60.5, 10.0),
            (3, 61.0, 10.0),
            (4, 60.5, 10.5),
        ] {
            nodes.insert(
                NodeId(id),
                Node {
                    id: NodeId(id),
                    coord: Coord { x: lon, y: lat },
                    uses: 0,
                },
            );
        }
        let mk = |id: &str, s: i64, t: i64, len: f64, ferry: bool| -> GraphEdge {
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
                highway: Some("primary".into()),
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
                is_toll: false,
                is_ferry: ferry,
                ferry_interval_min: None,
                is_tunnel: false,
                is_boardwalk_crossing: false,
                is_roundabout: false,
                motor_vehicle_conditional: None,
                access_conditional: None,
                maxspeed_conditional: None,
                access_forbidden: false,
                surface_quality: crate::routing::graph::SurfaceQuality::Good,
            }
        };
        let mut edges = Vec::new();
        for (id, s, t, len, ferry) in [
            ("a", 1, 2, 50_000.0, false),
            ("b", 2, 3, 50_000.0, false),
            ("skip", 1, 4, 40_000.0, false),
            ("skip2", 4, 3, 40_000.0, false),
            ("ferry", 1, 2, 45_000.0, true),
        ] {
            edges.push(mk(id, s, t, len, ferry));
            edges.push(mk(&format!("{id}_rev"), t, s, len, ferry));
        }
        RouteGraph::from_parts(nodes, edges, RoutingProfile::Car)
    }

    const VIA_TRIP: [(f64, f64); 3] = [(60.0, 10.0), (60.5003, 10.0004), (61.0, 10.0)];

    #[test]
    fn near_equal_pick_follows_the_approved_rule() {
        // (total_min, ferries, km): fastest has 1 ferry; within 2 % and no more
        // ferries are eligible; fewer km wins among them.
        let alts = [
            (100.0, 1, 1500.0),
            (101.5, 1, 1450.0),
            (101.0, 2, 1300.0),
            (103.0, 0, 1200.0),
        ];
        let (eligible, pick) = near_equal_pick(&alts).expect("pick");
        assert_eq!(eligible, vec![true, true, false, false]);
        assert_eq!(pick, 1);
        let (_, pick) = near_equal_pick(&[(100.0, 0, 900.0), (102.5, 0, 800.0)]).expect("pick");
        assert_eq!(pick, 0, "nothing within 2 %: the fastest stays");
        assert!(near_equal_pick(&[]).is_none());
    }

    #[test]
    fn stage_b_every_leg_alternative_runs_between_its_waypoints() {
        let mut g = via_trip_graph();
        let opts = RouteOptions::default();
        let snaps = snap_coarse_waypoints(&mut g, &VIA_TRIP, 5_000.0, &opts).expect("snaps");
        assert_eq!(snaps, vec![NodeId(1), NodeId(2), NodeId(3)]);
        let border = HashSet::new();
        let mut ferry_legs = 0;
        for w in snaps.windows(2) {
            let alts = leg_alternatives(
                &mut g,
                &CoarseEdgeNames::default(),
                &border,
                w[0],
                w[1],
                &opts,
            );
            assert!(!alts.is_empty(), "every leg has a free alternative");
            if alts[0].report.ferries.is_empty() {
                assert_eq!(
                    alts.len(),
                    1,
                    "no ferry on the free path: nothing else to search"
                );
            } else {
                ferry_legs += 1;
                assert!(
                    alts.len() >= 2,
                    "a ferry leg also has a ferry-free alternative"
                );
            }
            for (i, a) in alts.iter().enumerate() {
                assert!(
                    alts[..i]
                        .iter()
                        .all(|b| b.path != a.path
                            || b.report.ferries.len() != a.report.ferries.len()),
                    "{} repeats an earlier alternative",
                    a.name
                );
            }
            for a in &alts {
                assert_eq!(
                    a.path.first(),
                    Some(&w[0]),
                    "{} must start at the leg start",
                    a.name
                );
                assert_eq!(
                    a.path.last(),
                    Some(&w[1]),
                    "{} must end at the leg end",
                    a.name
                );
                assert!(
                    !a.path.contains(&NodeId(4)),
                    "{} takes the trip shortcut that skips the via",
                    a.name
                );
            }
        }
        assert_eq!(ferry_legs, 1, "the via trip graph has one ferry leg");
    }

    #[test]
    fn stage_b_delivered_route_passes_every_via() {
        let mut g = via_trip_graph();
        let opts = coarse_route_options(&RouteOptions::default());
        let (direct, _, _) = g
            .shortest_path_with_options(NodeId(1), NodeId(3), false, &opts)
            .expect("direct");
        assert!(
            direct.contains(&NodeId(4)),
            "fixture: the whole-trip shortest path skips the via"
        );
        let snaps = snap_coarse_waypoints(&mut g, &VIA_TRIP, 5_000.0, &RouteOptions::default())
            .expect("snaps");
        let plan = stage_b_plan_on_graph(
            &mut g,
            &CoarseEdgeNames::default(),
            &HashSet::new(),
            snaps,
            0,
            &RouteOptions::default(),
        )
        .expect("plan");
        let sb = assemble_stage_b(plan, &g, &VIA_TRIP, &[], RoutingProfile::Car, 1);
        assert_eq!(sb.hops.first(), Some(&VIA_TRIP[0]));
        assert_eq!(sb.hops.last(), Some(&VIA_TRIP[2]));
        let via_hop = sb
            .hops
            .iter()
            .position(|&h| h == VIA_TRIP[1])
            .expect("the exact via coordinate must be a hop end");
        assert!(via_hop > 0 && via_hop + 1 < sb.hops.len());
        assert!(
            sb.coarse_path.contains(&(60.5, 10.0)),
            "coarse path joins at the via node"
        );
        assert!(
            !sb.coarse_path.contains(&(60.5, 10.5)),
            "coarse path skips the via"
        );
        assert_eq!(sb.legs.len(), 2);
        for leg in &sb.legs {
            assert_eq!(leg.iter().filter(|c| c.picked).count(), 1);
        }
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

    fn linear_joint_graph(coords: &[(i64, f64, f64)]) -> RouteGraph {
        use geo_types::Coord;
        use osm4routing::Node;
        use std::collections::HashMap;
        let mut nodes = HashMap::new();
        for &(id, lat, lon) in coords {
            nodes.insert(
                NodeId(id),
                Node {
                    id: NodeId(id),
                    coord: Coord { x: lon, y: lat },
                    uses: 2,
                },
            );
        }
        let mut edges = Vec::new();
        let mut push = |s: i64, slat: f64, slon: f64, t: i64, tlat: f64, tlon: f64, iso: &str| {
            let len = haversine_m(slat, slon, tlat, tlon);
            edges.push(GraphEdge {
                id: format!("iso:{iso}-{s}-{t}"),
                source: NodeId(s),
                target: NodeId(t),
                length_m: len,
                base_weight: len,
                cost_mult: 1.0,
                eco_weight: None,
                start_lat: slat,
                start_lon: slon,
                end_lat: tlat,
                end_lon: tlon,
                shape: Vec::new(),
                highway: Some("primary".into()),
                maxspeed_kmh: None,
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
        };
        for w in coords.windows(2) {
            let (s, slat, slon) = w[0];
            let (t, tlat, tlon) = w[1];
            let iso_fwd = if slon.max(tlon) > 12.0 { "SE" } else { "NO" };
            let iso_rev = if slon.min(tlon) > 12.0 { "SE" } else { "NO" };
            push(s, slat, slon, t, tlat, tlon, iso_fwd);
            push(t, tlat, tlon, s, slat, slon, iso_rev);
        }
        RouteGraph::from_parts(nodes, edges, RoutingProfile::Car)
    }

    fn edge_ix(g: &RouteGraph, s: i64, t: i64) -> usize {
        g.edges
            .iter()
            .position(|e| e.source == NodeId(s) && e.target == NodeId(t))
            .expect("edge")
    }

    #[test]
    fn border_touch_and_return_is_not_a_joint() {
        // Oslo (NO) to a border node, into Sweden, and back through the same node.
        let g = linear_joint_graph(&[(1, 59.91, 10.75), (2, 59.91, 11.98), (3, 59.91, 12.05)]);
        let path = vec![NodeId(1), NodeId(2), NodeId(3), NodeId(2), NodeId(1)];
        let edges = vec![
            edge_ix(&g, 1, 2),
            edge_ix(&g, 2, 3),
            edge_ix(&g, 3, 2),
            edge_ix(&g, 2, 1),
        ];
        let mut borders = HashSet::new();
        borders.insert(2);
        let joints = extract_coarse_joints(&g, &path, &edges, &borders, &[], 500.0);
        assert!(
            !joints.iter().any(|j| j.joint_type == "border_crossing"),
            "return through the same border node must not be a hop joint: {joints:?}"
        );
    }

    #[test]
    fn coord_out_and_back_is_collapsed() {
        let path = vec![
            (60.0, 10.0),
            (60.0, 10.04),
            (60.0, 10.08),
            (60.0, 10.04),
            (60.0, 10.12),
        ];
        let got = collapse_coord_retraces(&path);
        assert!(
            got.len() < path.len(),
            "expected the east-and-back spur dropped: {got:?}"
        );
        assert!((got[0].1 - 10.0).abs() < 1e-6);
        assert!(got.last().unwrap().1 > 10.10, "{got:?}");
    }

    #[test]
    fn path_that_returns_to_a_node_is_collapsed() {
        let p = vec![NodeId(1), NodeId(2), NodeId(3), NodeId(2), NodeId(4)];
        assert_eq!(
            collapse_node_retraces(&p),
            vec![NodeId(1), NodeId(2), NodeId(4)]
        );
    }

    #[test]
    fn net_country_crossing_is_a_joint() {
        let g = linear_joint_graph(&[
            (1, 59.91, 10.75),
            (2, 59.91, 11.05),
            (3, 59.33, 18.07),
            (4, 59.20, 18.20),
        ]);
        let path: Vec<NodeId> = vec![NodeId(1), NodeId(2), NodeId(3), NodeId(4)];
        let edges = vec![edge_ix(&g, 1, 2), edge_ix(&g, 2, 3), edge_ix(&g, 3, 4)];
        let mut borders = HashSet::new();
        borders.insert(2);
        let joints = extract_coarse_joints(&g, &path, &edges, &borders, &[], 500.0);
        assert!(
            joints
                .iter()
                .any(|j| j.joint_type == "border_crossing" && j.osm_id == 2),
            "a crossing the path stays on must remain a joint: {joints:?}"
        );
    }

    #[test]
    fn hop_end_far_from_coarse_path_is_rejected() {
        let path = vec![(60.0, 10.0), (60.1, 10.0), (60.2, 10.0)];
        let hops = vec![(60.0, 10.0), (60.15, 10.5), (60.2, 10.0)];
        let err =
            ensure_hops_on_coarse_path(&hops, &path, crate::routing::plan_bbox::HOP_END_MATCH_M)
                .expect_err("joint 0.5° off the path must fail");
        assert!(
            err.contains("from the coarse path"),
            "expected a hop-end tolerance failure, got {err}"
        );
        let on_path = vec![(60.0, 10.0), (60.1, 10.0), (60.2, 10.0)];
        assert!(ensure_hops_on_coarse_path(
            &on_path,
            &path,
            crate::routing::plan_bbox::HOP_END_MATCH_M
        )
        .is_ok());
    }

    #[test]
    fn last_reachable_coarse_node_skips_disconnected_joint() {
        // 1-2-3 connected; 4-5 a separate component. Intended end is 5.
        let g = linear_joint_graph(&[(1, 60.00, 10.00), (2, 60.01, 10.00), (3, 60.02, 10.00)]);
        let mut nodes = g.nodes.clone();
        let mut edges = g.edges.clone();
        for &(id, lat, lon) in &[(4i64, 60.03, 10.00), (5, 60.04, 10.00)] {
            nodes.insert(
                NodeId(id),
                osm4routing::Node {
                    id: NodeId(id),
                    coord: geo_types::Coord { x: lon, y: lat },
                    uses: 2,
                },
            );
        }
        let len = haversine_m(60.03, 10.00, 60.04, 10.00);
        edges.push(GraphEdge {
            id: "iso:NO-4-5".into(),
            source: NodeId(4),
            target: NodeId(5),
            length_m: len,
            base_weight: len,
            cost_mult: 1.0,
            eco_weight: None,
            start_lat: 60.03,
            start_lon: 10.00,
            end_lat: 60.04,
            end_lon: 10.00,
            shape: Vec::new(),
            highway: Some("primary".into()),
            maxspeed_kmh: None,
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
        edges.push(GraphEdge {
            id: "iso:NO-5-4".into(),
            source: NodeId(5),
            target: NodeId(4),
            length_m: len,
            base_weight: len,
            cost_mult: 1.0,
            eco_weight: None,
            start_lat: 60.04,
            start_lon: 10.00,
            end_lat: 60.03,
            end_lon: 10.00,
            shape: Vec::new(),
            highway: Some("primary".into()),
            maxspeed_kmh: None,
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
        let g = RouteGraph::from_parts(nodes, edges, RoutingProfile::Car);
        assert!(!g.same_weak_component(NodeId(1), NodeId(5)));
        let path = vec![
            (60.00, 10.00),
            (60.01, 10.00),
            (60.02, 10.00),
            (60.03, 10.00),
            (60.04, 10.00),
        ];
        let got = last_reachable_coarse_path_node(
            &g,
            NodeId(1),
            &path,
            (60.04, 10.00),
            crate::routing::plan_bbox::HOP_END_MATCH_M,
        )
        .expect("node 3 is still in the start component");
        assert_eq!(got.0, NodeId(3));
        assert!((got.1 .0 - 60.02).abs() < 1e-6);
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
        let skel = build_skeleton_from_pack(
            &pack,
            "europe/test",
            "test-pack-names",
            "car",
            &HashSet::new(),
        );
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

    fn merge_fixture(
        stem: &str,
        ids: Vec<i64>,
        lons: Vec<f64>,
        edges: &[(u32, u32, &str, &str, &str, u8)],
    ) -> CorridorSkeletonFile {
        let n = ids.len();
        let length: Vec<f64> = edges
            .iter()
            .map(|e| 1_000.0 + ids[e.0 as usize] as f64)
            .collect();
        let weight: Vec<f64> = edges
            .iter()
            .map(|e| 900.0 + ids[e.1 as usize] as f64)
            .collect();
        CorridorSkeletonFile {
            format_version: CORRIDOR_SKELETON_FORMAT_VERSION,
            pack_format_version: GRAPH_FORMAT_VERSION,
            region_id: format!("test/{stem}"),
            leaf_stem: stem.into(),
            profile: "car".into(),
            node_count: n as u32,
            edge_count: edges.len() as u32,
            ferry_edge_count: 0,
            secondary_edge_count: 0,
            border_node_count: 0,
            ferry_terminal_count: 0,
            build_ms: 0,
            node_ids: ids,
            node_lats: vec![60.0; n],
            node_lons: lons,
            node_is_border: vec![0; n],
            node_is_ferry_terminal: vec![0; n],
            edge_src: edges.iter().map(|e| e.0).collect(),
            edge_tgt: edges.iter().map(|e| e.1).collect(),
            edge_length_m: length,
            edge_base_weight: weight,
            edge_highway: edges.iter().map(|e| e.2.to_string()).collect(),
            edge_name: edges.iter().map(|e| e.3.to_string()).collect(),
            edge_road_ref: edges.iter().map(|e| e.4.to_string()).collect(),
            edge_is_oneway: vec![1; edges.len()],
            edge_is_ferry: edges.iter().map(|e| e.5).collect(),
            edge_is_tunnel: vec![0; edges.len()],
            edge_is_toll: vec![0; edges.len()],
        }
    }

    #[test]
    fn streaming_merge_matches_per_region_merge() {
        // Two regions share node 2 and the edge 1-2; region a also carries a
        // construction edge, which must not be routable.
        let a = merge_fixture(
            "a",
            vec![1, 2, 3],
            vec![10.0, 10.1, 10.2],
            &[
                (0, 1, "primary", "Storgata", "E6", 0),
                (1, 2, "construction", "", "", 0),
                (1, 0, "primary", "Storgata", "", 0),
                (2, 1, "", "Fjord - Kai", "", 1),
            ],
        );
        let b = merge_fixture(
            "b",
            vec![2, 1, 4],
            vec![10.1, 10.0, 10.3],
            &[
                (1, 0, "primary", "Storgata", "E6", 0),
                (0, 2, "trunk", "", "E39", 0),
            ],
        );
        let skels = [a, b];
        let old = crate::routing::indexed::merge_tile_graphs(
            skels
                .iter()
                .map(|s| skeleton_to_route_graph(s, RoutingProfile::Car))
                .collect(),
            RoutingProfile::Car,
        );
        let mut builder = CoarseGraphBuilder::new(false);
        for s in &skels {
            builder.add(s);
        }
        let mut new = builder.finish(RoutingProfile::Car);
        let all: Vec<usize> = (0..new.graph.edges.len()).collect();
        new.names.apply(&mut new.graph, &all);
        let key = |e: &GraphEdge| {
            (
                e.source,
                e.target,
                e.length_m.to_bits(),
                e.base_weight.to_bits(),
                e.start_lon.to_bits(),
                e.end_lon.to_bits(),
                e.highway.clone(),
                e.name.clone(),
                e.road_ref.clone(),
                (e.is_oneway, e.is_ferry, e.is_tunnel, e.is_toll),
            )
        };
        let old_edges: Vec<_> = old.edges.iter().map(key).collect();
        let new_edges: Vec<_> = new.graph.edges.iter().map(key).collect();
        assert_eq!(
            old_edges.len(),
            4,
            "construction edge and repeated 1-2 dropped"
        );
        assert_eq!(new_edges, old_edges);
        let mut old_nodes: Vec<_> = old
            .nodes
            .values()
            .map(|n| (n.id, n.coord.x.to_bits()))
            .collect();
        let mut new_nodes: Vec<_> = new
            .graph
            .nodes
            .values()
            .map(|n| (n.id, n.coord.x.to_bits()))
            .collect();
        old_nodes.sort();
        new_nodes.sort();
        assert_eq!(new_nodes, old_nodes);
        assert_eq!(new.regions.len(), 2);
        assert!(new
            .graph
            .edges
            .iter()
            .all(|e| e.id.is_empty() && e.shape.is_empty()));
    }

    #[test]
    fn coarse_names_stay_off_the_graph_until_applied() {
        let a = merge_fixture(
            "a",
            vec![1, 2],
            vec![10.0, 10.1],
            &[(0, 1, "primary", "Storgata", "E6", 0)],
        );
        let mut builder = CoarseGraphBuilder::new(false);
        builder.add(&a);
        let mut c = builder.finish(RoutingProfile::Car);
        assert_eq!(c.graph.edges[0].name, None);
        assert_eq!(c.graph.edges[0].road_ref, None);
        c.names.apply(&mut c.graph, &[0]);
        assert_eq!(c.graph.edges[0].name.as_deref(), Some("Storgata"));
        assert_eq!(c.graph.edges[0].road_ref.as_deref(), Some("E6"));
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
            skel.edge_highway
                .extend(["primary".into(), "primary".into()]);
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
