//! Graph archive body (rkyv), promoted from Phase 1c PoC.

use std::collections::HashMap;

use geo_types::Coord;
use osm4routing::{Node, NodeId};
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};

use crate::routing::elevation::ElevationService;
use crate::routing::graph::{infer_surface_from_highway, GraphEdge, RouteGraph, RoutingProfile};

/// Little-endian ASCII "NVRK".
pub const MAGIC_GRAPH: u32 = 0x4E_56_52_4B;
/// v6: v5 + vehicle physical limits (maxheight/weight/width/length/axle/bogie).
pub const GRAPH_FORMAT_VERSION: u32 = 6;

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
    pub edge_is_motorroad: Vec<u8>,
    pub edge_is_expressway: Vec<u8>,
    pub edge_is_oneway: Vec<u8>,
    /// 0 = unset.
    pub edge_lanes: Vec<u8>,
    /// Tonnes; NaN = none. OSM `maxweight`.
    pub edge_maxweight_t: Vec<f64>,
    /// Tonnes; NaN = none. OSM `maxaxleload`.
    pub edge_maxaxleload_t: Vec<f64>,
    /// Tonnes; NaN = none. OSM `maxbogieweight`.
    pub edge_maxbogieweight_t: Vec<f64>,
    /// Metres; NaN = none. OSM `maxheight`.
    pub edge_maxheight_m: Vec<f64>,
    /// Metres; NaN = none. OSM `maxwidth`.
    pub edge_maxwidth_m: Vec<f64>,
    /// Metres; NaN = none. OSM `maxlength`.
    pub edge_maxlength_m: Vec<f64>,
    pub edge_is_toll: Vec<u8>,
    pub edge_is_ferry: Vec<u8>,
    pub edge_is_roundabout: Vec<u8>,
    pub edge_is_boardwalk: Vec<u8>,
    /// CSR: `edge_shape_offsets.len() == edge_src.len() + 1`.
    /// Edge `i` shape points are `edge_shape_lons[start..end]` / `…_lats` where
    /// `start = offsets[i]`, `end = offsets[i + 1]` (lon, lat; endpoints excluded).
    pub edge_shape_offsets: Vec<u32>,
    pub edge_shape_lons: Vec<f64>,
    pub edge_shape_lats: Vec<f64>,
    /// Raw OSM `motor_vehicle:conditional` (empty = none).
    pub edge_motor_vehicle_conditional: Vec<String>,
    /// Raw OSM `access:conditional` (empty = none).
    pub edge_access_conditional: Vec<String>,
    /// Raw OSM `maxspeed:conditional` (empty = none).
    pub edge_maxspeed_conditional: Vec<String>,
    /// Profile-static access forbid flag per edge (`1` = forbidden).
    pub edge_access_forbidden: Vec<u8>,
    /// Parallel to `node_ids`: `1` when the node is a profile access-blocked barrier.
    pub node_access_blocked: Vec<u8>,
}

/// Pack optional finite metric; `None` / non-finite → NaN (matches `edge_maxspeed_kmh`).
fn pack_opt_metric(v: Option<f64>) -> f64 {
    v.filter(|x| x.is_finite()).unwrap_or(f64::NAN)
}

/// Unpack NaN-sentinel metric vector entry.
fn unpack_opt_metric(vals: &[f64], i: usize) -> Option<f64> {
    vals.get(i).copied().filter(|v| v.is_finite())
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
        let mut edge_is_motorroad = Vec::with_capacity(n);
        let mut edge_is_expressway = Vec::with_capacity(n);
        let mut edge_is_oneway = Vec::with_capacity(n);
        let mut edge_lanes = Vec::with_capacity(n);
        let mut edge_maxweight_t = Vec::with_capacity(n);
        let mut edge_maxaxleload_t = Vec::with_capacity(n);
        let mut edge_maxbogieweight_t = Vec::with_capacity(n);
        let mut edge_maxheight_m = Vec::with_capacity(n);
        let mut edge_maxwidth_m = Vec::with_capacity(n);
        let mut edge_maxlength_m = Vec::with_capacity(n);
        let mut edge_is_toll = Vec::with_capacity(n);
        let mut edge_is_ferry = Vec::with_capacity(n);
        let mut edge_is_roundabout = Vec::with_capacity(n);
        let mut edge_is_boardwalk = Vec::with_capacity(n);
        let mut edge_shape_offsets = Vec::with_capacity(n + 1);
        let mut edge_shape_lons: Vec<f64> = Vec::new();
        let mut edge_shape_lats: Vec<f64> = Vec::new();
        let mut edge_motor_vehicle_conditional = Vec::with_capacity(n);
        let mut edge_access_conditional = Vec::with_capacity(n);
        let mut edge_maxspeed_conditional = Vec::with_capacity(n);
        let mut edge_access_forbidden = Vec::with_capacity(n);
        edge_shape_offsets.push(0);

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
            edge_is_motorroad.push(u8::from(e.is_motorroad));
            edge_is_expressway.push(u8::from(e.is_expressway));
            edge_is_oneway.push(u8::from(e.is_oneway));
            edge_lanes.push(e.lanes.unwrap_or(0));
            edge_maxweight_t.push(pack_opt_metric(e.maxweight_t));
            edge_maxaxleload_t.push(pack_opt_metric(e.maxaxleload_t));
            edge_maxbogieweight_t.push(pack_opt_metric(e.maxbogieweight_t));
            edge_maxheight_m.push(pack_opt_metric(e.maxheight_m));
            edge_maxwidth_m.push(pack_opt_metric(e.maxwidth_m));
            edge_maxlength_m.push(pack_opt_metric(e.maxlength_m));
            edge_is_toll.push(u8::from(e.is_toll));
            edge_is_ferry.push(u8::from(e.is_ferry));
            edge_is_roundabout.push(u8::from(e.is_roundabout));
            edge_is_boardwalk.push(u8::from(e.is_boardwalk_crossing));
            edge_motor_vehicle_conditional
                .push(e.motor_vehicle_conditional.clone().unwrap_or_default());
            edge_access_conditional.push(e.access_conditional.clone().unwrap_or_default());
            edge_maxspeed_conditional.push(e.maxspeed_conditional.clone().unwrap_or_default());
            edge_access_forbidden.push(u8::from(e.access_forbidden));
            for &(lon, lat) in &e.shape {
                edge_shape_lons.push(lon);
                edge_shape_lats.push(lat);
            }
            edge_shape_offsets.push(edge_shape_lons.len() as u32);
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

        let node_access_blocked: Vec<u8> = node_ids
            .iter()
            .map(|id| u8::from(graph.access_blocked_nodes.contains(&NodeId(*id))))
            .collect();

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
            edge_is_motorroad,
            edge_is_expressway,
            edge_is_oneway,
            edge_lanes,
            edge_maxweight_t,
            edge_maxaxleload_t,
            edge_maxbogieweight_t,
            edge_maxheight_m,
            edge_maxwidth_m,
            edge_maxlength_m,
            edge_is_toll,
            edge_is_ferry,
            edge_is_roundabout,
            edge_is_boardwalk,
            edge_shape_offsets,
            edge_shape_lons,
            edge_shape_lats,
            edge_motor_vehicle_conditional,
            edge_access_conditional,
            edge_maxspeed_conditional,
            edge_access_forbidden,
            node_access_blocked,
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
            let shape = self.shape_for_edge(i);
            edges.push(GraphEdge {
                // Pack edge index keeps parallel edges distinct (tile merge and
                // adjacency both need unique ids; way id is not stored in v6 packs).
                id: format!("{}-{}-{}", src.0, tgt.0, i),
                source: src,
                target: tgt,
                length_m: self.edge_length_m[i],
                base_weight: self.edge_base_weight[i],
                eco_weight: Some(self.edge_base_weight[i]),
                start_lat: self.edge_start_lat[i],
                start_lon: self.edge_start_lon[i],
                end_lat: self.edge_end_lat[i],
                end_lon: self.edge_end_lon[i],
                shape,
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
                is_motorroad: self.edge_is_motorroad.get(i).copied().unwrap_or(0) != 0,
                is_expressway: self.edge_is_expressway.get(i).copied().unwrap_or(0) != 0,
                is_oneway: self.edge_is_oneway.get(i).copied().unwrap_or(0) != 0,
                lanes: {
                    let n = self.edge_lanes.get(i).copied().unwrap_or(0);
                    if n == 0 {
                        None
                    } else {
                        Some(n)
                    }
                },
                maxweight_t: unpack_opt_metric(&self.edge_maxweight_t, i),
                maxaxleload_t: unpack_opt_metric(&self.edge_maxaxleload_t, i),
                maxbogieweight_t: unpack_opt_metric(&self.edge_maxbogieweight_t, i),
                maxheight_m: unpack_opt_metric(&self.edge_maxheight_m, i),
                maxwidth_m: unpack_opt_metric(&self.edge_maxwidth_m, i),
                maxlength_m: unpack_opt_metric(&self.edge_maxlength_m, i),
                is_toll: self.edge_is_toll[i] != 0,
                is_ferry: self.edge_is_ferry[i] != 0,
                is_boardwalk_crossing: self.edge_is_boardwalk[i] != 0,
                is_roundabout: self.edge_is_roundabout[i] != 0,
                motor_vehicle_conditional: {
                    let s = self
                        .edge_motor_vehicle_conditional
                        .get(i)
                        .map(String::as_str)
                        .unwrap_or("");
                    if s.is_empty() {
                        None
                    } else {
                        Some(s.to_string())
                    }
                },
                access_conditional: {
                    let s = self
                        .edge_access_conditional
                        .get(i)
                        .map(String::as_str)
                        .unwrap_or("");
                    if s.is_empty() {
                        None
                    } else {
                        Some(s.to_string())
                    }
                },
                maxspeed_conditional: {
                    let s = self
                        .edge_maxspeed_conditional
                        .get(i)
                        .map(String::as_str)
                        .unwrap_or("");
                    if s.is_empty() {
                        None
                    } else {
                        Some(s.to_string())
                    }
                },
                access_forbidden: self.edge_access_forbidden.get(i).copied().unwrap_or(0) != 0,
                surface_quality: infer_surface_from_highway(if hw.is_empty() {
                    None
                } else {
                    Some(hw)
                }),
            });
        }
        let mut blocked = std::collections::HashSet::new();
        for (i, flag) in self.node_access_blocked.iter().enumerate() {
            if *flag != 0 && used_nodes.contains_key(&(i as u32)) {
                blocked.insert(NodeId(self.node_ids[i]));
            }
        }
        // When packing older in-memory graphs without parallel flags, length may be 0.
        if self.node_access_blocked.is_empty() {
            // nothing
        }
        RouteGraph::from_parts_with_blocks(nodes, edges, profile, blocked)
    }

    fn shape_for_edge(&self, i: usize) -> Vec<(f64, f64)> {
        if self.edge_shape_offsets.len() < 2 || i + 1 >= self.edge_shape_offsets.len() {
            return Vec::new();
        }
        let start = self.edge_shape_offsets[i] as usize;
        let end = self.edge_shape_offsets[i + 1] as usize;
        if end > self.edge_shape_lons.len()
            || end > self.edge_shape_lats.len()
            || start > end
            || self.edge_shape_lons.len() != self.edge_shape_lats.len()
        {
            return Vec::new();
        }
        (start..end)
            .map(|j| (self.edge_shape_lons[j], self.edge_shape_lats[j]))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::graph::{GraphEdge, RouteGraph, RoutingProfile, SurfaceQuality};
    use geo_types::Coord;
    use osm4routing::{Node, NodeId};
    use std::collections::HashMap;

    fn tiny_curved_graph() -> RouteGraph {
        let n1 = NodeId(1);
        let n2 = NodeId(2);
        let mut nodes = HashMap::new();
        nodes.insert(
            n1,
            Node {
                id: n1,
                coord: Coord { x: 10.0, y: 60.0 },
                uses: 2,
            },
        );
        nodes.insert(
            n2,
            Node {
                id: n2,
                coord: Coord { x: 10.2, y: 60.1 },
                uses: 2,
            },
        );
        let edges = vec![GraphEdge {
            id: "1-2".into(),
            source: n1,
            target: n2,
            length_m: 1_000.0,
            base_weight: 1_000.0,
            eco_weight: Some(1_000.0),
            start_lat: 60.0,
            start_lon: 10.0,
            end_lat: 60.1,
            end_lon: 10.2,
            shape: vec![(10.05, 60.04), (10.12, 60.07), (10.18, 60.09)],
            highway: Some("secondary".into()),
            maxspeed_kmh: Some(80.0),
            name: Some("Curvy".into()),
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
            is_boardwalk_crossing: false,
            is_roundabout: false,
            motor_vehicle_conditional: None,
            access_conditional: None,
            maxspeed_conditional: None,
            access_forbidden: false,
            surface_quality: SurfaceQuality::Good,
        }];
        RouteGraph::from_parts(nodes, edges, RoutingProfile::Car)
    }

    #[test]
    fn pack_roundtrip_preserves_edge_shape() {
        let graph = tiny_curved_graph();
        let pack = FlatGraphPack::from_route_graph(&graph, None);
        assert_eq!(pack.edge_shape_offsets, vec![0, 3]);
        assert_eq!(pack.edge_shape_lons.len(), 3);
        let back = pack.to_route_graph(RoutingProfile::Car);
        assert_eq!(back.edges.len(), 1);
        assert_eq!(
            back.edges[0].shape,
            vec![(10.05, 60.04), (10.12, 60.07), (10.18, 60.09)]
        );
        let poly = back.path_overlay_polyline(&[NodeId(1), NodeId(2)]);
        // Endpoints + 3 shape points => denser than a pure chord (2 verts).
        assert!(poly.split(';').count() >= 5, "poly={poly}");
    }

    #[test]
    fn pack_roundtrip_preserves_motorway_grade_tags() {
        let mut graph = tiny_curved_graph();
        graph.edges[0].is_motorroad = true;
        graph.edges[0].is_expressway = true;
        graph.edges[0].is_oneway = true;
        graph.edges[0].lanes = Some(3);
        let pack = FlatGraphPack::from_route_graph(&graph, None);
        assert_eq!(pack.edge_is_motorroad, vec![1]);
        assert_eq!(pack.edge_is_expressway, vec![1]);
        assert_eq!(pack.edge_is_oneway, vec![1]);
        assert_eq!(pack.edge_lanes, vec![3]);
        let back = pack.to_route_graph(RoutingProfile::Car);
        assert!(back.edges[0].is_motorroad);
        assert!(back.edges[0].is_expressway);
        assert!(back.edges[0].is_oneway);
        assert_eq!(back.edges[0].lanes, Some(3));
    }

    fn diamond_edge(
        id: &str,
        source: NodeId,
        target: NodeId,
        start_lat: f64,
        start_lon: f64,
        end_lat: f64,
        end_lon: f64,
        length_m: f64,
        maxheight_m: Option<f64>,
    ) -> GraphEdge {
        GraphEdge {
            id: id.into(),
            source,
            target,
            length_m,
            base_weight: length_m,
            eco_weight: Some(length_m),
            start_lat,
            start_lon,
            end_lat,
            end_lon,
            shape: Vec::new(),
            highway: Some("primary".into()),
            maxspeed_kmh: None,
            name: None,
            road_ref: None,
            is_motorroad: false,
            is_expressway: false,
            is_oneway: false,
            lanes: None,
            maxweight_t: None,
            maxaxleload_t: None,
            maxbogieweight_t: None,
            maxheight_m,
            maxwidth_m: None,
            maxlength_m: None,
            is_toll: false,
            is_ferry: false,
            is_boardwalk_crossing: false,
            is_roundabout: false,
            motor_vehicle_conditional: None,
            access_conditional: None,
            maxspeed_conditional: None,
            access_forbidden: false,
            surface_quality: SurfaceQuality::Good,
        }
    }

    #[test]
    fn pack_roundtrip_preserves_vehicle_physical_limits() {
        let mut graph = tiny_curved_graph();
        graph.edges[0].maxheight_m = Some(2.4);
        graph.edges[0].maxweight_t = Some(7.5);
        graph.edges[0].maxwidth_m = Some(2.55);
        graph.edges[0].maxlength_m = Some(12.0);
        graph.edges[0].maxaxleload_t = Some(10.0);
        graph.edges[0].maxbogieweight_t = Some(18.0);
        let pack = FlatGraphPack::from_route_graph(&graph, None);
        assert_eq!(pack.edge_maxheight_m[0], 2.4);
        assert_eq!(pack.edge_maxweight_t[0], 7.5);
        assert_eq!(pack.edge_maxwidth_m[0], 2.55);
        assert_eq!(pack.edge_maxlength_m[0], 12.0);
        assert_eq!(pack.edge_maxaxleload_t[0], 10.0);
        assert_eq!(pack.edge_maxbogieweight_t[0], 18.0);
        assert!(pack.edge_maxspeed_kmh[0].is_finite()); // unrelated field still set

        // Full rkyv serialize → deserialize (on-disk body), not just from/to_route_graph.
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&pack).expect("serialize pack");
        let archived =
            rkyv::access::<ArchivedFlatGraphPack, rkyv::rancor::Error>(&bytes[..]).expect("access");
        let restored: FlatGraphPack =
            rkyv::deserialize::<FlatGraphPack, rkyv::rancor::Error>(archived).expect("deserialize");
        let back = restored.to_route_graph(RoutingProfile::Car);
        assert_eq!(back.edges[0].maxheight_m, Some(2.4));
        assert_eq!(back.edges[0].maxweight_t, Some(7.5));
        assert_eq!(back.edges[0].maxwidth_m, Some(2.55));
        assert_eq!(back.edges[0].maxlength_m, Some(12.0));
        assert_eq!(back.edges[0].maxaxleload_t, Some(10.0));
        assert_eq!(back.edges[0].maxbogieweight_t, Some(18.0));
    }

    /// Height-restricted short edge must be rejected after FlatGraphPack round-trip
    /// when the vehicle is taller than the posted limit (production pack path).
    #[test]
    fn pack_roundtrip_height_limit_changes_planned_route() {
        use crate::config::VehicleLimits;
        use crate::routing::graph::RouteOptions;

        let n1 = NodeId(1);
        let n2 = NodeId(2);
        let n3 = NodeId(3);
        let n4 = NodeId(4);
        let mut nodes = HashMap::new();
        for (id, lat, lon) in [
            (n1, 60.0, 10.0),
            (n2, 60.0, 10.01),
            (n3, 60.0, 10.02),
            (n4, 60.01, 10.01),
        ] {
            nodes.insert(
                id,
                Node {
                    id,
                    coord: Coord { x: lon, y: lat },
                    uses: 2,
                },
            );
        }
        let graph = RouteGraph::from_parts(
            nodes,
            vec![
                diamond_edge("low", n1, n2, 60.0, 10.0, 60.0, 10.01, 100.0, Some(2.4)),
                diamond_edge("bc", n2, n3, 60.0, 10.01, 60.0, 10.02, 100.0, None),
                diamond_edge("ad", n1, n4, 60.0, 10.0, 60.01, 10.01, 220.0, None),
                diamond_edge("dc", n4, n3, 60.01, 10.01, 60.0, 10.02, 220.0, None),
            ],
            RoutingProfile::Truck,
        );

        let pack = FlatGraphPack::from_route_graph(&graph, None);
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&pack).expect("serialize");
        let archived =
            rkyv::access::<ArchivedFlatGraphPack, rkyv::rancor::Error>(&bytes[..]).expect("access");
        let restored: FlatGraphPack =
            rkyv::deserialize::<FlatGraphPack, rkyv::rancor::Error>(archived).expect("deserialize");
        let back = restored.to_route_graph(RoutingProfile::Truck);
        let low = back
            .edges
            .iter()
            .find(|e| e.source == n1 && e.target == n2)
            .expect("low bridge edge");
        assert_eq!(
            low.maxheight_m,
            Some(2.4),
            "maxheight must survive pack round-trip"
        );

        let unrestricted = back.shortest_path(n1, n3, false).expect("unrestricted");
        assert!(
            unrestricted.0.contains(&n2),
            "without height limit, short path via n2: {:?}",
            unrestricted.0
        );

        let limited = back
            .shortest_path_with_options(
                n1,
                n3,
                false,
                &RouteOptions {
                    vehicle: Some(VehicleLimits {
                        height_m: Some(2.8),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )
            .expect("height-limited path");
        assert!(
            !limited.0.contains(&n2),
            "2.8m vehicle must avoid maxheight=2.4 edge via n2: {:?}",
            limited.0
        );
        assert!(limited.0.contains(&n4));
        assert_ne!(unrestricted.0, limited.0);
    }
}
