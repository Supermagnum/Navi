//! Memory-conscious graph build clipped to a WGS84 bbox.
//!
//! Full Ostlandet car graphs are ~300MB on disk and peak far higher in RAM; loading
//! them on a 4GB Automotive AVD kills the process (LMK) before routing starts.
//! Planning clips the same region `.pbf` to the trip bbox so we never materialize
//! the nationwide graph in-process.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use geo_types::Coord;
use osm4routing::{Node, NodeId};
use osmpbf::{Element, ElementReader};

use super::builder::{GraphEdge, RouteGraph, RoutingProfile};
use crate::routing::access;
use crate::routing::wetland::tags_map_indicate_boardwalk;

#[derive(Clone)]
struct RawWay {
    id: i64,
    nodes: Vec<i64>,
    tags: HashMap<String, String>,
}

fn in_bbox(lat: f64, lon: f64, bbox: [f64; 4]) -> bool {
    lat >= bbox[0] && lat <= bbox[2] && lon >= bbox[1] && lon <= bbox[3]
}

fn car_highway_ok(highway: &str) -> bool {
    matches!(
        highway,
        "motorway"
            | "motorway_link"
            | "motorway_junction"
            | "trunk"
            | "trunk_link"
            | "primary"
            | "primary_link"
            | "secondary"
            | "secondary_link"
            | "tertiary"
            | "tertiary_link"
            | "unclassified"
            | "residential"
            | "living_street"
            | "road"
            | "service"
            | "track"
    )
}

fn highway_ok_for_profile(highway: &str, profile: RoutingProfile) -> bool {
    match profile {
        RoutingProfile::Car | RoutingProfile::Truck => car_highway_ok(highway),
        RoutingProfile::Foot => {
            matches!(
                highway,
                "footway"
                    | "path"
                    | "steps"
                    | "pedestrian"
                    | "living_street"
                    | "residential"
                    | "service"
                    | "track"
                    | "unclassified"
                    | "tertiary"
                    | "secondary"
                    | "primary"
                    | "cycleway"
            ) || car_highway_ok(highway)
        }
        RoutingProfile::Bicycle => {
            car_highway_ok(highway) || matches!(highway, "cycleway" | "path" | "footway")
        }
    }
}

fn parse_metric(raw: &str) -> Option<f64> {
    let cleaned = raw.trim().to_lowercase().replace(['t', 'm'], "");
    cleaned.trim().parse::<f64>().ok()
}

fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6_378_100.0;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

fn oneway_forward_only(tags: &HashMap<String, String>) -> bool {
    if tags.get("junction").is_some_and(|v| v == "roundabout") {
        return true;
    }
    matches!(
        tags.get("oneway").map(String::as_str),
        Some("yes" | "true" | "1")
    )
}

impl RouteGraph {
    /// Build a car/truck/foot/bike graph from `path`, keeping only ways that
    /// touch `bbox` `[min_lat, min_lon, max_lat, max_lon]`.
    pub fn build_from_pbf_bbox(
        path: impl AsRef<Path>,
        profile: RoutingProfile,
        bbox: [f64; 4],
    ) -> anyhow::Result<Self> {
        let path = path.as_ref();
        crate::download::progress::set(0, Some(4), "Planning route: indexing area…");
        // Pass 1: node ids inside bbox (ids only — storing every coord OOMs on large extracts).
        let mut in_bbox_ids: HashSet<i64> = HashSet::new();
        {
            let file = std::fs::File::open(path)?;
            let reader = ElementReader::new(file);
            reader.for_each(|element| match element {
                Element::Node(n) => {
                    if in_bbox(n.lat(), n.lon(), bbox) {
                        in_bbox_ids.insert(n.id());
                    }
                }
                Element::DenseNode(n) => {
                    if in_bbox(n.lat(), n.lon(), bbox) {
                        in_bbox_ids.insert(n.id());
                    }
                }
                _ => {}
            })?;
        }

        crate::download::progress::set(1, Some(4), "Planning route: reading roads…");
        // Pass 2: highway ways that reference at least one in-bbox node.
        let mut ways: Vec<RawWay> = Vec::new();
        let mut needed: HashSet<i64> = HashSet::new();
        {
            let file = std::fs::File::open(path)?;
            let reader = ElementReader::new(file);
            reader.for_each(|element| {
                let Element::Way(way) = element else {
                    return;
                };
                let tags: HashMap<String, String> = way
                    .tags()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();
                let Some(highway) = tags.get("highway") else {
                    return;
                };
                if !highway_ok_for_profile(highway, profile) {
                    return;
                }
                let refs: Vec<i64> = way.refs().collect();
                if refs.is_empty() {
                    return;
                }
                if !refs.iter().any(|id| in_bbox_ids.contains(id)) {
                    return;
                }
                for id in &refs {
                    needed.insert(*id);
                }
                ways.push(RawWay {
                    id: way.id(),
                    nodes: refs,
                    tags,
                });
            })?;
        }
        drop(in_bbox_ids);

        crate::download::progress::set(2, Some(4), "Planning route: loading geometry…");
        // Pass 3: coords + barrier access tags for nodes referenced by kept ways.
        let mut coords: HashMap<i64, (f64, f64)> = HashMap::with_capacity(needed.len());
        let mut barrier_tags: HashMap<i64, HashMap<String, String>> = HashMap::new();
        {
            let file = std::fs::File::open(path)?;
            let reader = ElementReader::new(file);
            reader.for_each(|element| match element {
                Element::Node(n) => {
                    if needed.contains(&n.id()) {
                        coords.insert(n.id(), (n.lat(), n.lon()));
                        let tags: HashMap<String, String> = n
                            .tags()
                            .map(|(k, v)| (k.to_string(), v.to_string()))
                            .collect();
                        if tags.contains_key("barrier") {
                            barrier_tags.insert(n.id(), tags);
                        }
                    }
                }
                Element::DenseNode(n) => {
                    if needed.contains(&n.id()) {
                        coords.insert(n.id(), (n.lat(), n.lon()));
                        let tags: HashMap<String, String> = n
                            .tags()
                            .map(|(k, v)| (k.to_string(), v.to_string()))
                            .collect();
                        if tags.contains_key("barrier") {
                            barrier_tags.insert(n.id(), tags);
                        }
                    }
                }
                _ => {}
            })?;
        }
        drop(needed);

        crate::download::progress::set(3, Some(4), "Planning route: linking graph…");
        let arcs: Vec<Arc<RawWay>> = ways.into_iter().map(Arc::new).collect();
        let graph = graph_from_raw_ways(&arcs, &coords, profile, &barrier_tags)?;
        if graph.edges.is_empty() {
            anyhow::bail!("bbox graph empty for {bbox:?} from {}", path.display());
        }
        Ok(graph)
    }

    /// Build graphs for all spatial tiles with **two PBF passes total** (not
    /// three × tile count). Invokes `on_tile` as each tile graph is ready so
    /// callers can write+drop without retaining every tile in RAM.
    ///
    /// Way-first (not node-first): only highway-referenced nodes are retained,
    /// so region extracts do not materialize every OSM node into a mask map
    /// (that path OOMs on ~4GB tablets). Ways are shared across tiles via
    /// [`Arc`]. `tiles` entries are `(row, col, logical_bbox)`.
    pub fn build_tiled_from_pbf(
        path: impl AsRef<Path>,
        profile: RoutingProfile,
        tiles: &[(usize, usize, [f64; 4])],
        pad_deg: f64,
        mut on_tile: impl FnMut(usize, usize, [f64; 4], Self) -> anyhow::Result<()>,
    ) -> anyhow::Result<usize> {
        let path = path.as_ref();
        if tiles.is_empty() {
            anyhow::bail!("no tiles");
        }
        if tiles.len() > 64 {
            anyhow::bail!("tile grid exceeds u64 bitmask capacity ({})", tiles.len());
        }
        let expanded: Vec<[f64; 4]> = tiles
            .iter()
            .map(|(_, _, b)| {
                [
                    b[0] - pad_deg,
                    b[1] - pad_deg,
                    b[2] + pad_deg,
                    b[3] + pad_deg,
                ]
            })
            .collect();

        // Pass 1: profile highways only (region PBFs are already clipped).
        crate::download::progress::set(0, Some(4), "Indexed maps: tiling roads…");
        let mut ways: Vec<RawWay> = Vec::new();
        let mut needed: HashSet<i64> = HashSet::new();
        {
            let file = std::fs::File::open(path)?;
            let reader = ElementReader::new(file);
            reader.for_each(|element| {
                let Element::Way(way) = element else {
                    return;
                };
                let tags: HashMap<String, String> = way
                    .tags()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();
                let Some(highway) = tags.get("highway") else {
                    return;
                };
                if !highway_ok_for_profile(highway, profile) {
                    return;
                }
                let refs: Vec<i64> = way.refs().collect();
                if refs.is_empty() {
                    return;
                }
                for id in &refs {
                    needed.insert(*id);
                }
                ways.push(RawWay {
                    id: way.id(),
                    nodes: refs,
                    tags,
                });
            })?;
        }

        // Pass 2: coords + barrier tags for highway-referenced nodes.
        crate::download::progress::set(1, Some(4), "Indexed maps: tiling geometry…");
        let mut coords: HashMap<i64, (f64, f64)> = HashMap::with_capacity(needed.len());
        let mut barrier_tags: HashMap<i64, HashMap<String, String>> = HashMap::new();
        {
            let file = std::fs::File::open(path)?;
            let reader = ElementReader::new(file);
            reader.for_each(|element| match element {
                Element::Node(n) => {
                    if needed.contains(&n.id()) {
                        coords.insert(n.id(), (n.lat(), n.lon()));
                        let tags: HashMap<String, String> = n
                            .tags()
                            .map(|(k, v)| (k.to_string(), v.to_string()))
                            .collect();
                        if tags.contains_key("barrier") {
                            barrier_tags.insert(n.id(), tags);
                        }
                    }
                }
                Element::DenseNode(n) => {
                    if needed.contains(&n.id()) {
                        coords.insert(n.id(), (n.lat(), n.lon()));
                        let tags: HashMap<String, String> = n
                            .tags()
                            .map(|(k, v)| (k.to_string(), v.to_string()))
                            .collect();
                        if tags.contains_key("barrier") {
                            barrier_tags.insert(n.id(), tags);
                        }
                    }
                }
                _ => {}
            })?;
        }
        drop(needed);

        crate::download::progress::set(2, Some(4), "Indexed maps: assigning tiles…");
        let mut tile_ways: Vec<Vec<Arc<RawWay>>> = (0..tiles.len()).map(|_| Vec::new()).collect();
        for way in ways {
            let mut mask = 0u64;
            for id in &way.nodes {
                let Some(&(lat, lon)) = coords.get(id) else {
                    continue;
                };
                for (i, bb) in expanded.iter().enumerate() {
                    if in_bbox(lat, lon, *bb) {
                        mask |= 1u64 << i;
                    }
                }
            }
            if mask == 0 {
                continue;
            }
            let raw = Arc::new(way);
            for (i, bucket) in tile_ways.iter_mut().enumerate() {
                if mask & (1u64 << i) != 0 {
                    bucket.push(Arc::clone(&raw));
                }
            }
        }

        crate::download::progress::set(3, Some(4), "Indexed maps: writing tile graphs…");
        let mut produced = 0usize;
        for (i, (row, col, logical)) in tiles.iter().enumerate() {
            let ways = std::mem::take(&mut tile_ways[i]);
            if ways.is_empty() {
                retain_coords_for_remaining_tiles(&mut coords, &tile_ways, i + 1);
                continue;
            }
            match graph_from_raw_ways(&ways, &coords, profile, &barrier_tags) {
                Ok(g) if !g.edges.is_empty() => {
                    on_tile(*row, *col, *logical, g)?;
                    produced += 1;
                }
                _ => {}
            }
            drop(ways);
            retain_coords_for_remaining_tiles(&mut coords, &tile_ways, i + 1);
        }
        if produced == 0 {
            anyhow::bail!("tiled graph empty for profile {profile:?}");
        }
        Ok(produced)
    }
}

fn retain_coords_for_remaining_tiles(
    coords: &mut HashMap<i64, (f64, f64)>,
    tile_ways: &[Vec<Arc<RawWay>>],
    from: usize,
) {
    if from >= tile_ways.len() {
        coords.clear();
        return;
    }
    let mut keep: HashSet<i64> = HashSet::new();
    for ways in &tile_ways[from..] {
        for w in ways {
            for id in &w.nodes {
                keep.insert(*id);
            }
        }
    }
    coords.retain(|id, _| keep.contains(id));
}

fn graph_from_raw_ways(
    ways: &[Arc<RawWay>],
    coords: &HashMap<i64, (f64, f64)>,
    profile: RoutingProfile,
    barrier_tags: &HashMap<i64, HashMap<String, String>>,
) -> anyhow::Result<RouteGraph> {
    let mode = profile.access_mode();
    let mut uses: HashMap<i64, i32> = HashMap::new();
    for way in ways {
        if access::tags_forbid_mode(&way.tags, mode) {
            continue;
        }
        let n = way.nodes.len();
        for (i, id) in way.nodes.iter().enumerate() {
            let add = if i == 0 || i + 1 == n { 2 } else { 1 };
            *uses.entry(*id).or_insert(0) += add;
        }
    }

    let mut nodes: HashMap<NodeId, Node> = HashMap::new();
    for (id, (lat, lon)) in coords {
        if uses.get(id).copied().unwrap_or(0) > 1 {
            nodes.insert(
                NodeId(*id),
                Node {
                    id: NodeId(*id),
                    coord: Coord { x: *lon, y: *lat },
                    uses: uses[id] as i16,
                },
            );
        }
    }

    let mut edges: Vec<GraphEdge> = Vec::new();
    for way in ways {
        if access::tags_forbid_mode(&way.tags, mode) {
            continue;
        }
        let mut source: Option<i64> = None;
        let mut prev: Option<(i64, f64, f64)> = None;
        let mut length_m = 0.0;
        let mut shape: Vec<(f64, f64)> = Vec::new();
        let mut seg = 0usize;
        let forward_only = oneway_forward_only(&way.tags);
        let highway = way.tags.get("highway").cloned();
        let maxspeed_kmh = way
            .tags
            .get("maxspeed")
            .and_then(|v| crate::routing::eta::parse_maxspeed_kmh(v));
        let name = way.tags.get("name").cloned();
        let road_ref = way.tags.get("ref").cloned();
        let maxweight_t = way.tags.get("maxweight").and_then(|v| parse_metric(v));
        let maxaxleload_t = way.tags.get("maxaxleload").and_then(|v| parse_metric(v));
        let maxbogieweight_t = way.tags.get("maxbogieweight").and_then(|v| parse_metric(v));
        let maxheight_m = way.tags.get("maxheight").and_then(|v| parse_metric(v));
        let maxwidth_m = way.tags.get("maxwidth").and_then(|v| parse_metric(v));
        let maxlength_m = way.tags.get("maxlength").and_then(|v| parse_metric(v));
        let is_toll = way.tags.get("toll").is_some_and(|v| v == "yes");
        let is_ferry =
            way.tags.get("route").is_some_and(|v| v == "ferry") || way.tags.contains_key("ferry");
        let is_boardwalk_crossing = tags_map_indicate_boardwalk(&way.tags);
        let is_roundabout = way.tags.get("junction").is_some_and(|v| v == "roundabout");
        let motor_vehicle_conditional = way.tags.get("motor_vehicle:conditional").cloned();
        let access_conditional = way.tags.get("access:conditional").cloned();
        let maxspeed_conditional = way.tags.get("maxspeed:conditional").cloned();

        for id in &way.nodes {
            let Some(&(lat, lon)) = coords.get(id) else {
                continue;
            };
            if let Some((_, plat, plon)) = prev {
                length_m += haversine_m(plat, plon, lat, lon);
            }
            prev = Some((*id, lat, lon));

            let is_end = source.is_some() && uses.get(id).copied().unwrap_or(0) > 1;
            if source.is_none() {
                if uses.get(id).copied().unwrap_or(0) > 1 {
                    source = Some(*id);
                    length_m = 0.0;
                    shape.clear();
                }
                continue;
            }
            if !is_end {
                shape.push((lon, lat));
                continue;
            }
            let src = source.unwrap();
            let tgt = *id;
            if src == tgt || length_m <= 0.0 {
                source = Some(tgt);
                length_m = 0.0;
                shape.clear();
                continue;
            }
            let Some(sn) = nodes.get(&NodeId(src)).copied() else {
                source = Some(tgt);
                length_m = 0.0;
                shape.clear();
                continue;
            };
            let Some(tn) = nodes.get(&NodeId(tgt)).copied() else {
                source = Some(tgt);
                length_m = 0.0;
                shape.clear();
                continue;
            };
            let id_fwd = format!("{}-{}", way.id, seg);
            seg += 1;
            let shape_fwd = shape.clone();
            let mut shape_rev = shape_fwd.clone();
            shape_rev.reverse();
            edges.push(bbox_edge(
                id_fwd.clone(),
                sn.id,
                tn.id,
                sn.coord.y,
                sn.coord.x,
                tn.coord.y,
                tn.coord.x,
                length_m,
                shape_fwd,
                highway.clone(),
                maxspeed_kmh,
                name.clone(),
                road_ref.clone(),
                maxweight_t,
                maxaxleload_t,
                maxbogieweight_t,
                maxheight_m,
                maxwidth_m,
                maxlength_m,
                is_toll,
                is_ferry,
                is_boardwalk_crossing,
                is_roundabout,
                motor_vehicle_conditional.clone(),
                access_conditional.clone(),
                maxspeed_conditional.clone(),
                false,
            ));
            if !forward_only {
                edges.push(bbox_edge(
                    format!("{id_fwd}-rev"),
                    tn.id,
                    sn.id,
                    tn.coord.y,
                    tn.coord.x,
                    sn.coord.y,
                    sn.coord.x,
                    length_m,
                    shape_rev,
                    highway.clone(),
                    maxspeed_kmh,
                    name.clone(),
                    road_ref.clone(),
                    maxweight_t,
                    maxaxleload_t,
                    maxbogieweight_t,
                    maxheight_m,
                    maxwidth_m,
                    maxlength_m,
                    is_toll,
                    is_ferry,
                    is_boardwalk_crossing,
                    is_roundabout,
                    motor_vehicle_conditional.clone(),
                    access_conditional.clone(),
                    maxspeed_conditional.clone(),
                    false,
                ));
            }
            source = Some(tgt);
            length_m = 0.0;
            shape.clear();
        }
    }

    let blocked = access::blocked_barrier_nodes(barrier_tags, mode)
        .into_iter()
        .filter(|id| nodes.contains_key(id))
        .collect();
    Ok(RouteGraph::from_parts_with_blocks(
        nodes, edges, profile, blocked,
    ))
}

#[allow(clippy::too_many_arguments)]
fn bbox_edge(
    id: String,
    source: NodeId,
    target: NodeId,
    start_lat: f64,
    start_lon: f64,
    end_lat: f64,
    end_lon: f64,
    length_m: f64,
    shape: Vec<(f64, f64)>,
    highway: Option<String>,
    maxspeed_kmh: Option<f64>,
    name: Option<String>,
    road_ref: Option<String>,
    maxweight_t: Option<f64>,
    maxaxleload_t: Option<f64>,
    maxbogieweight_t: Option<f64>,
    maxheight_m: Option<f64>,
    maxwidth_m: Option<f64>,
    maxlength_m: Option<f64>,
    is_toll: bool,
    is_ferry: bool,
    is_boardwalk_crossing: bool,
    is_roundabout: bool,
    motor_vehicle_conditional: Option<String>,
    access_conditional: Option<String>,
    maxspeed_conditional: Option<String>,
    access_forbidden: bool,
) -> GraphEdge {
    GraphEdge {
        id,
        source,
        target,
        length_m,
        base_weight: length_m,
        eco_weight: None,
        start_lat,
        start_lon,
        end_lat,
        end_lon,
        shape,
        highway,
        maxspeed_kmh,
        name,
        road_ref,
        maxweight_t,
        maxaxleload_t,
        maxbogieweight_t,
        maxheight_m,
        maxwidth_m,
        maxlength_m,
        is_toll,
        is_ferry,
        is_boardwalk_crossing,
        is_roundabout,
        motor_vehicle_conditional,
        access_conditional,
        maxspeed_conditional,
        access_forbidden,
    }
}

#[cfg(test)]
mod bbox_tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn bbox_build_gps_atnbrua_from_ostlandet() {
        let pbf = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/integration-fixtures/ostlandet-latest.osm.pbf");
        if !pbf.is_file() {
            eprintln!("skip: missing {pbf:?}");
            return;
        }
        let start_lat: f64 = 60.750_920;
        let start_lon: f64 = 10.960_358;
        let end_lat: f64 = 61.851_250;
        let end_lon: f64 = 10.233_842;
        let pad = 0.35_f64;
        let bbox = [
            start_lat.min(end_lat) - pad,
            start_lon.min(end_lon) - pad,
            start_lat.max(end_lat) + pad,
            start_lon.max(end_lon) + pad,
        ];
        let g =
            RouteGraph::build_from_pbf_bbox(&pbf, RoutingProfile::Car, bbox).expect("bbox build");
        assert!(g.nodes.len() > 1000, "nodes={}", g.nodes.len());
        assert!(g.edges.len() > 1000, "edges={}", g.edges.len());
        eprintln!("bbox graph nodes={} edges={}", g.nodes.len(), g.edges.len());
    }
}
