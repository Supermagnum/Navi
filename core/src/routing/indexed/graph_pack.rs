//! Graph archive body (rkyv), promoted from Phase 1c PoC.

use std::collections::HashMap;

use geo_types::Coord;
use osm4routing::{Node, NodeId};
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};

use crate::routing::elevation::ElevationService;
use crate::routing::graph::{GraphEdge, RouteGraph, RoutingProfile};

/// Little-endian ASCII "NVRK".
pub const MAGIC_GRAPH: u32 = 0x4E_56_52_4B;
pub const GRAPH_FORMAT_VERSION: u32 = 1;

#[derive(Archive, RkyvSerialize, RkyvDeserialize, Debug, Clone)]
pub struct FlatGraphPack {
    pub has_delta_h: bool,
    pub node_ids: Vec<i64>,
    pub node_lats: Vec<f64>,
    pub node_lons: Vec<f64>,
    pub edge_src: Vec<u32>,
    pub edge_tgt: Vec<u32>,
    pub edge_length_m: Vec<f64>,
    pub edge_base_weight: Vec<f64>,
    /// Metres; empty when `has_delta_h` is false.
    pub edge_delta_h_m: Vec<f32>,
    pub edge_start_lat: Vec<f64>,
    pub edge_start_lon: Vec<f64>,
    pub edge_end_lat: Vec<f64>,
    pub edge_end_lon: Vec<f64>,
    pub edge_highway: Vec<String>,
    pub edge_maxspeed_kmh: Vec<f64>, // NaN = none
    pub edge_name: Vec<String>,
    pub edge_road_ref: Vec<String>,
    pub edge_is_toll: Vec<u8>,
    pub edge_is_ferry: Vec<u8>,
    pub edge_is_roundabout: Vec<u8>,
    pub edge_is_boardwalk: Vec<u8>,
}

impl FlatGraphPack {
    pub fn from_route_graph(graph: &RouteGraph, elev: Option<&ElevationService>) -> Self {
        let mut node_ids = Vec::with_capacity(graph.nodes.len());
        let mut node_lats = Vec::with_capacity(graph.nodes.len());
        let mut node_lons = Vec::with_capacity(graph.nodes.len());
        let mut id_to_idx: HashMap<i64, u32> = HashMap::with_capacity(graph.nodes.len());
        for (id, node) in &graph.nodes {
            let idx = node_ids.len() as u32;
            id_to_idx.insert(id.0, idx);
            node_ids.push(id.0);
            node_lats.push(node.coord.y);
            node_lons.push(node.coord.x);
        }

        let n = graph.edges.len();
        let mut edge_src = Vec::with_capacity(n);
        let mut edge_tgt = Vec::with_capacity(n);
        let mut edge_length_m = Vec::with_capacity(n);
        let mut edge_base_weight = Vec::with_capacity(n);
        let mut edge_delta_h_m = Vec::new();
        let mut edge_start_lat = Vec::with_capacity(n);
        let mut edge_start_lon = Vec::with_capacity(n);
        let mut edge_end_lat = Vec::with_capacity(n);
        let mut edge_end_lon = Vec::with_capacity(n);
        let mut edge_highway = Vec::with_capacity(n);
        let mut edge_maxspeed_kmh = Vec::with_capacity(n);
        let mut edge_name = Vec::with_capacity(n);
        let mut edge_road_ref = Vec::with_capacity(n);
        let mut edge_is_toll = Vec::with_capacity(n);
        let mut edge_is_ferry = Vec::with_capacity(n);
        let mut edge_is_roundabout = Vec::with_capacity(n);
        let mut edge_is_boardwalk = Vec::with_capacity(n);

        if elev.is_some() {
            edge_delta_h_m.reserve(n);
        }

        for e in &graph.edges {
            let s = *id_to_idx.get(&e.source.0).expect("src");
            let t = *id_to_idx.get(&e.target.0).expect("tgt");
            edge_src.push(s);
            edge_tgt.push(t);
            edge_length_m.push(e.length_m);
            edge_base_weight.push(e.base_weight);
            edge_start_lat.push(e.start_lat);
            edge_start_lon.push(e.start_lon);
            edge_end_lat.push(e.end_lat);
            edge_end_lon.push(e.end_lon);
            edge_highway.push(e.highway.clone().unwrap_or_default());
            edge_maxspeed_kmh.push(e.maxspeed_kmh.unwrap_or(f64::NAN));
            edge_name.push(e.name.clone().unwrap_or_default());
            edge_road_ref.push(e.road_ref.clone().unwrap_or_default());
            edge_is_toll.push(u8::from(e.is_toll));
            edge_is_ferry.push(u8::from(e.is_ferry));
            edge_is_roundabout.push(u8::from(e.is_roundabout));
            edge_is_boardwalk.push(u8::from(e.is_boardwalk_crossing));
            if let Some(elev) = elev {
                let dh = match (
                    elev.get_elevation(e.start_lat, e.start_lon),
                    elev.get_elevation(e.end_lat, e.end_lon),
                ) {
                    (Some(a), Some(b)) => (b - a) as f32,
                    _ => 0.0,
                };
                edge_delta_h_m.push(dh);
            }
        }

        Self {
            has_delta_h: elev.is_some(),
            node_ids,
            node_lats,
            node_lons,
            edge_src,
            edge_tgt,
            edge_length_m,
            edge_base_weight,
            edge_delta_h_m,
            edge_start_lat,
            edge_start_lon,
            edge_end_lat,
            edge_end_lon,
            edge_highway,
            edge_maxspeed_kmh,
            edge_name,
            edge_road_ref,
            edge_is_toll,
            edge_is_ferry,
            edge_is_roundabout,
            edge_is_boardwalk,
        }
    }

    pub fn to_route_graph(&self, profile: RoutingProfile) -> RouteGraph {
        self.to_route_graph_bbox(profile, None)
    }

    /// Materialize a [`RouteGraph`], optionally keeping only edges that touch `bbox`
    /// (`[min_lat, min_lon, max_lat, max_lon]`). Region packs are stored whole;
    /// plan-time clipping avoids OOM on large extracts (see `bbox_build.rs`).
    pub fn to_route_graph_bbox(
        &self,
        profile: RoutingProfile,
        bbox: Option<[f64; 4]>,
    ) -> RouteGraph {
        let edge_ok = |i: usize| -> bool {
            let Some(b) = bbox else {
                return true;
            };
            let slat = self.edge_start_lat[i];
            let slon = self.edge_start_lon[i];
            let elat = self.edge_end_lat[i];
            let elon = self.edge_end_lon[i];
            (slat >= b[0] && slat <= b[2] && slon >= b[1] && slon <= b[3])
                || (elat >= b[0] && elat <= b[2] && elon >= b[1] && elon <= b[3])
        };

        let mut used_nodes: HashMap<u32, ()> = HashMap::new();
        for i in 0..self.edge_src.len() {
            if edge_ok(i) {
                used_nodes.insert(self.edge_src[i], ());
                used_nodes.insert(self.edge_tgt[i], ());
            }
        }

        let mut nodes: HashMap<NodeId, Node> = HashMap::with_capacity(used_nodes.len());
        for &idx in used_nodes.keys() {
            let i = idx as usize;
            let id = NodeId(self.node_ids[i]);
            nodes.insert(
                id,
                Node {
                    id,
                    coord: Coord {
                        x: self.node_lons[i],
                        y: self.node_lats[i],
                    },
                    uses: 2,
                },
            );
        }
        let mut edges = Vec::new();
        for i in 0..self.edge_src.len() {
            if !edge_ok(i) {
                continue;
            }
            let src = NodeId(self.node_ids[self.edge_src[i] as usize]);
            let tgt = NodeId(self.node_ids[self.edge_tgt[i] as usize]);
            let hw = self.edge_highway[i].as_str();
            let name = self.edge_name[i].as_str();
            let road_ref = self.edge_road_ref[i].as_str();
            let maxspeed = self.edge_maxspeed_kmh[i];
            edges.push(GraphEdge {
                id: format!("{}-{}", src.0, tgt.0),
                source: src,
                target: tgt,
                length_m: self.edge_length_m[i],
                base_weight: self.edge_base_weight[i],
                eco_weight: Some(self.edge_base_weight[i]),
                start_lat: self.edge_start_lat[i],
                start_lon: self.edge_start_lon[i],
                end_lat: self.edge_end_lat[i],
                end_lon: self.edge_end_lon[i],
                shape: Vec::new(),
                highway: if hw.is_empty() {
                    None
                } else {
                    Some(hw.to_string())
                },
                maxspeed_kmh: if maxspeed.is_finite() {
                    Some(maxspeed)
                } else {
                    None
                },
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
                maxweight_t: None,
                maxaxleload_t: None,
                maxbogieweight_t: None,
                maxheight_m: None,
                maxwidth_m: None,
                maxlength_m: None,
                is_toll: self.edge_is_toll[i] != 0,
                is_ferry: self.edge_is_ferry[i] != 0,
                is_boardwalk_crossing: self.edge_is_boardwalk[i] != 0,
                is_roundabout: self.edge_is_roundabout[i] != 0,
            });
        }
        RouteGraph::from_parts(nodes, edges, profile)
    }
}
