//! Persistent major-road corridor skeleton (Follow-up 13 Stage A).
//!
//! Built from installed graph packs: motorway, trunk, primary, ferries, plus
//! secondary only when needed to reach a ferry terminal or a border-crossing
//! node. Coarse search uses the same [`crate::routing::graph::RouteOptions`] as
//! the detailed profile so the corridor cannot choose a path the hop search
//! would refuse.

use crate::routing::indexed::{densify_skeleton_edge, FlatGraphPack, GRAPH_FORMAT_VERSION};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// On-disk skeleton format version (independent of pack GRAPH_FORMAT_VERSION).
pub const CORRIDOR_SKELETON_FORMAT_VERSION: u32 = 1;

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
    pub edge_src: Vec<u32>,
    pub edge_tgt: Vec<u32>,
    pub edge_length_m: Vec<f64>,
    pub edge_base_weight: Vec<f64>,
    pub edge_highway: Vec<String>,
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

/// Build skeleton membership for one flat pack tile/region.
///
/// Pass 1: motorway/trunk/primary + ferry (same as densify skeleton).
/// Pass 2: mark ferry terminals and nodes whose OSM ids are in `border_osm_ids`.
/// Pass 3: add secondary edges that touch those anchors.
pub fn select_skeleton_edge_indices(
    pack: &FlatGraphPack,
    border_osm_ids: &HashSet<i64>,
) -> (HashSet<usize>, HashSet<u32>, HashSet<u32>) {
    let n = pack.edge_src.len();
    let mut major: HashSet<usize> = HashSet::new();
    let mut ferry_terminals: HashSet<u32> = HashSet::new();
    for i in 0..n {
        let hw = pack.edge_highway[i].as_str();
        let ferry = pack.edge_is_ferry.get(i).copied().unwrap_or(0) != 0;
        if densify_skeleton_edge(hw, ferry) {
            major.insert(i);
            if ferry {
                ferry_terminals.insert(pack.edge_src[i]);
                ferry_terminals.insert(pack.edge_tgt[i]);
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
    for i in 0..n {
        if keep.contains(&i) {
            continue;
        }
        let hw = pack.edge_highway[i].as_str();
        if secondary_connects_anchor(hw, pack.edge_src[i], pack.edge_tgt[i], &anchors) {
            keep.insert(i);
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
        select_skeleton_edge_indices(pack, border_osm_ids);
    let mut used_nodes: HashMap<u32, u32> = HashMap::new();
    let mut node_ids = Vec::new();
    let mut node_lats = Vec::new();
    let mut node_lons = Vec::new();
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
        n
    };
    let mut edge_src = Vec::new();
    let mut edge_tgt = Vec::new();
    let mut edge_length_m = Vec::new();
    let mut edge_base_weight = Vec::new();
    let mut edge_highway = Vec::new();
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
        edge_is_ferry.push(ferry);
        edge_is_tunnel.push(pack.edge_is_tunnel.get(i).copied().unwrap_or(0));
        edge_is_toll.push(pack.edge_is_toll.get(i).copied().unwrap_or(0));
    }
    let build_ms = t0.elapsed().as_millis() as u64;
    CorridorSkeletonFile {
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
        edge_src,
        edge_tgt,
        edge_length_m,
        edge_base_weight,
        edge_highway,
        edge_is_ferry,
        edge_is_tunnel,
        edge_is_toll,
    }
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
        // Nodes: 0 primary, 1 ferry terminal, 2 secondary approach, 3 far secondary
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
    fn secondary_to_border_kept() {
        let pack = tiny_pack();
        let mut borders = HashSet::new();
        borders.insert(103);
        let (keep, _, border_nodes) = select_skeleton_edge_indices(&pack, &borders);
        assert!(border_nodes.contains(&3));
        assert!(keep.contains(&3), "secondary touching border node must stay");
    }

    #[test]
    fn roundtrip_json_counts() {
        let pack = tiny_pack();
        let skel = build_skeleton_from_pack(&pack, "europe/test", "test", "truck", &HashSet::new());
        assert_eq!(skel.format_version, CORRIDOR_SKELETON_FORMAT_VERSION);
        assert!(skel.edge_count >= 3);
        assert!(skel.ferry_edge_count >= 1);
        assert!(skel.secondary_edge_count >= 1);
        let dir = tempfile::tempdir().unwrap();
        let n = write_skeleton_file(dir.path(), &skel).unwrap();
        assert!(n > 100);
        let loaded = read_skeleton_file(&skeleton_path(dir.path(), "test")).unwrap();
        assert_eq!(loaded.node_count, skel.node_count);
        assert_eq!(loaded.edge_count, skel.edge_count);
    }
}
