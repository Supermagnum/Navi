//! Graph archive body (rkyv), promoted from Phase 1c PoC.

use std::collections::{HashMap, HashSet};

use geo_types::Coord;
use osm4routing::{Node, NodeId};
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};

use crate::routing::elevation::ElevationService;
use crate::routing::graph::{GraphEdge, RouteGraph, RoutingProfile, SurfaceQuality};

/// Little-endian ASCII "NVRK".
pub const MAGIC_GRAPH: u32 = 0x4E_56_52_4B;
/// Packed graph archive format version written by local convert / on-device PBF
/// builds. Includes [`FlatGraphPack::edge_is_tunnel`].
///
/// **Read path accepts 8 and 9** ([`graph_format_version_accepted`]): live
/// pack-server tiles stay on v8 until navi-server flips; this client ships first
/// so the server can move to v9 later without breaking installs. Anything else
/// is [`crate::routing::indexed::PackLoadError::VersionMismatch`].
///
/// Wire layout: see [`FlatGraphPack`] (v9) and [`super::graph_pack_v8::FlatGraphPackV8`] (v8).
///
/// v9: v8 + `edge_is_tunnel` (`Vec<u8>`, 0/1), immediately after `edge_is_ferry`.
///
/// v8: v7 + per-edge `surface_quality` (OSM surface/tracktype class).
/// SurfaceQuality wire bytes: `0` Good, `1` Marginal, `2` Poor, `3` Unknown
/// (Unknown appended; v8 ordinals 0–2 unchanged so old packs load without
/// reinterpretation).
pub const GRAPH_FORMAT_VERSION: u32 = 9;

/// Prior on-wire format still accepted for load / manifest / pack-server fetch.
pub const GRAPH_FORMAT_VERSION_V8: u32 = 8;

/// True when `v` is a graph pack format this client can load (8 or 9).
#[must_use]
pub fn graph_format_version_accepted(v: u32) -> bool {
    v == GRAPH_FORMAT_VERSION_V8 || v == GRAPH_FORMAT_VERSION
}

/// Corridor-band clip: keep an edge when its lat/lon envelope overlaps any clip
/// square (not only when a terminal sits inside a square).
///
/// Endpoint-only clip drops real OSM ferries and long bridges whose terminals
/// lie outside the 0.40° band while the span still crosses the hop chord
/// (Øresundsbron on a Zealand→Skåne densify hop; Fehmarn belt similarly).
#[must_use]
pub(crate) fn clip_keeps_edge(
    slat: f64,
    slon: f64,
    elat: f64,
    elon: f64,
    clips: &[[f64; 4]],
) -> bool {
    if clips.is_empty() {
        return true;
    }
    let emin_lat = slat.min(elat);
    let emax_lat = slat.max(elat);
    let emin_lon = slon.min(elon);
    let emax_lon = slon.max(elon);
    clips
        .iter()
        .any(|b| emin_lat <= b[2] && emax_lat >= b[0] && emin_lon <= b[3] && emax_lon >= b[1])
}

/// Among accepted candidates, prefer the newest (v9 over v8). Used when a region
/// could ever offer more than one compatible staged pack.
#[must_use]
pub fn preferred_graph_format_version(candidates: impl IntoIterator<Item = u32>) -> Option<u32> {
    candidates
        .into_iter()
        .filter(|v| graph_format_version_accepted(*v))
        .max()
}

/// True for densify joint pre-pass: motorway/trunk/primary (+ links) + ferries.
/// Secondary is **not** included — on Vestlandet it kept ~259k nodes (≈ full
/// car corridor) and stacked with densify hops above the tablet 933 MiB budget.
/// Pier / terminal approaches are kept separately when they touch a ferry node
/// (see [`FlatGraphPack::to_route_graph_clips`]).
pub fn densify_skeleton_edge(highway: &str, is_ferry: bool) -> bool {
    if is_ferry {
        return true;
    }
    matches!(
        highway,
        "motorway" | "motorway_link" | "trunk" | "trunk_link" | "primary" | "primary_link"
    )
}

/// Process-wide (not thread-local): tiled pack hydrate uses rayon workers, and
/// a TLS flag stayed false on those threads so "skeleton" loads kept ~259k
/// Vestlandet nodes — the tablet RSS spike above 933 MiB.
static DENSIFY_SKELETON_ONLY: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Run `f` so pack→graph materialization keeps only densify-skeleton edges.
pub fn with_densify_skeleton_only<R>(f: impl FnOnce() -> R) -> R {
    use std::sync::atomic::Ordering;
    let prev = DENSIFY_SKELETON_ONLY.swap(true, Ordering::SeqCst);
    let out = f();
    DENSIFY_SKELETON_ONLY.store(prev, Ordering::SeqCst);
    out
}

pub(crate) fn densify_skeleton_only_active() -> bool {
    DENSIFY_SKELETON_ONLY.load(std::sync::atomic::Ordering::SeqCst)
}

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
    /// NaN = none. OSM `maxspeed:practical`.
    pub edge_maxspeed_practical_kmh: Vec<f64>,
    /// NaN = none. OSM `maxspeed:advisory`.
    pub edge_maxspeed_advisory_kmh: Vec<f64>,
    /// Raw OSM `maxspeed:type` (empty = none).
    pub edge_maxspeed_type: Vec<String>,
    /// `1` when OSM `maxspeed:variable` is truthy.
    pub edge_maxspeed_variable: Vec<u8>,
    /// NaN = none. OSM `minspeed`.
    pub edge_minspeed_kmh: Vec<f64>,
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
    /// OSM `tunnel=*` with any value other than `no`. `0`/`1`.
    pub edge_is_tunnel: Vec<u8>,
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
    /// [`SurfaceQuality`] as `u8` (`0` Good, `1` Marginal, `2` Poor, `3` Unknown).
    pub edge_surface_quality: Vec<u8>,
    /// Parallel to `node_ids`: `1` when the node is a profile access-blocked barrier.
    pub node_access_blocked: Vec<u8>,
}

/// Pack optional finite metric; `None` / non-finite → NaN (matches `edge_maxspeed_kmh`).
pub(crate) fn pack_opt_metric(v: Option<f64>) -> f64 {
    v.filter(|x| x.is_finite()).unwrap_or(f64::NAN)
}

/// Unpack NaN-sentinel metric vector entry.
pub(crate) fn unpack_opt_metric(vals: &[f64], i: usize) -> Option<f64> {
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
        let mut edge_maxspeed_practical_kmh = Vec::with_capacity(n);
        let mut edge_maxspeed_advisory_kmh = Vec::with_capacity(n);
        let mut edge_maxspeed_type = Vec::with_capacity(n);
        let mut edge_maxspeed_variable = Vec::with_capacity(n);
        let mut edge_minspeed_kmh = Vec::with_capacity(n);
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
        let mut edge_is_tunnel = Vec::with_capacity(n);
        let mut edge_is_roundabout = Vec::with_capacity(n);
        let mut edge_is_boardwalk = Vec::with_capacity(n);
        let mut edge_shape_offsets = Vec::with_capacity(n + 1);
        let mut edge_shape_lons: Vec<f64> = Vec::new();
        let mut edge_shape_lats: Vec<f64> = Vec::new();
        let mut edge_motor_vehicle_conditional = Vec::with_capacity(n);
        let mut edge_access_conditional = Vec::with_capacity(n);
        let mut edge_maxspeed_conditional = Vec::with_capacity(n);
        let mut edge_access_forbidden = Vec::with_capacity(n);
        let mut edge_surface_quality = Vec::with_capacity(n);
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
            edge_maxspeed_practical_kmh.push(pack_opt_metric(e.maxspeed_practical_kmh));
            edge_maxspeed_advisory_kmh.push(pack_opt_metric(e.maxspeed_advisory_kmh));
            edge_maxspeed_type.push(e.maxspeed_type.clone().unwrap_or_default());
            edge_maxspeed_variable.push(u8::from(e.maxspeed_variable));
            edge_minspeed_kmh.push(pack_opt_metric(e.minspeed_kmh));
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
            edge_is_tunnel.push(u8::from(e.is_tunnel));
            edge_is_roundabout.push(u8::from(e.is_roundabout));
            edge_is_boardwalk.push(u8::from(e.is_boardwalk_crossing));
            edge_motor_vehicle_conditional
                .push(e.motor_vehicle_conditional.clone().unwrap_or_default());
            edge_access_conditional.push(e.access_conditional.clone().unwrap_or_default());
            edge_maxspeed_conditional.push(e.maxspeed_conditional.clone().unwrap_or_default());
            edge_access_forbidden.push(u8::from(e.access_forbidden));
            edge_surface_quality.push(e.surface_quality.as_u8());
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
            edge_maxspeed_practical_kmh,
            edge_maxspeed_advisory_kmh,
            edge_maxspeed_type,
            edge_maxspeed_variable,
            edge_minspeed_kmh,
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
            edge_is_tunnel,
            edge_is_roundabout,
            edge_is_boardwalk,
            edge_shape_offsets,
            edge_shape_lons,
            edge_shape_lats,
            edge_motor_vehicle_conditional,
            edge_access_conditional,
            edge_maxspeed_conditional,
            edge_access_forbidden,
            edge_surface_quality,
            node_access_blocked,
        }
    }

    pub fn to_route_graph(&self, profile: RoutingProfile) -> RouteGraph {
        self.to_route_graph_clips(profile, None)
    }

    /// Materialize a [`RouteGraph`], optionally keeping only edges that touch `bbox`
    /// (`[min_lat, min_lon, max_lat, max_lon]`). Region packs are stored whole;
    /// plan-time clipping avoids OOM on large extracts (see `bbox_build.rs`).
    pub fn to_route_graph_bbox(
        &self,
        profile: RoutingProfile,
        bbox: Option<[f64; 4]>,
    ) -> RouteGraph {
        match bbox {
            Some(b) => self.to_route_graph_clips(profile, Some(std::slice::from_ref(&b))),
            None => self.to_route_graph_clips(profile, None),
        }
    }

    /// Like [`Self::to_route_graph_bbox`], keeping edges whose lat/lon envelope
    /// overlaps **any** clip box (corridor band along the OD polyline).
    pub fn to_route_graph_clips(
        &self,
        profile: RoutingProfile,
        clips: Option<&[[f64; 4]]>,
    ) -> RouteGraph {
        self.to_route_graph_clips_with_ferry_intervals(profile, clips, None)
    }

    /// Like [`Self::to_route_graph_clips`], applying optional per-edge ferry
    /// departure intervals (ferry overlay sidecar only; region packs pass `None`).
    pub fn to_route_graph_clips_with_ferry_intervals(
        &self,
        profile: RoutingProfile,
        clips: Option<&[[f64; 4]]>,
        ferry_intervals: Option<&[f64]>,
    ) -> RouteGraph {
        let skeleton = densify_skeleton_only_active();
        let in_clips = |i: usize| -> bool {
            let Some(clips) = clips else {
                return true;
            };
            clip_keeps_edge(
                self.edge_start_lat[i],
                self.edge_start_lon[i],
                self.edge_end_lat[i],
                self.edge_end_lon[i],
                clips,
            )
        };
        // Ferry terminals often hang off secondary/service stubs. Keep every
        // clipped edge that touches a ferry endpoint without importing the
        // whole secondary network (~259k nodes on Vestlandet).
        let ferry_nodes: HashSet<u32> = if skeleton {
            let mut s = HashSet::new();
            for i in 0..self.edge_src.len() {
                if self.edge_is_ferry.get(i).copied().unwrap_or(0) == 0 || !in_clips(i) {
                    continue;
                }
                s.insert(self.edge_src[i]);
                s.insert(self.edge_tgt[i]);
            }
            s
        } else {
            HashSet::new()
        };
        let edge_ok = |i: usize| -> bool {
            if skeleton {
                let hw = self.edge_highway[i].as_str();
                let ferry = self.edge_is_ferry.get(i).copied().unwrap_or(0) != 0;
                let major = densify_skeleton_edge(hw, ferry);
                let pier = !ferry
                    && (ferry_nodes.contains(&self.edge_src[i])
                        || ferry_nodes.contains(&self.edge_tgt[i]));
                if !major && !pier {
                    return false;
                }
            }
            in_clips(i)
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
                cost_mult: 1.0,
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
                maxspeed_practical_kmh: unpack_opt_metric(&self.edge_maxspeed_practical_kmh, i),
                maxspeed_advisory_kmh: unpack_opt_metric(&self.edge_maxspeed_advisory_kmh, i),
                maxspeed_type: {
                    let s = self
                        .edge_maxspeed_type
                        .get(i)
                        .map(String::as_str)
                        .unwrap_or("");
                    if s.is_empty() {
                        None
                    } else {
                        Some(s.to_string())
                    }
                },
                maxspeed_variable: self.edge_maxspeed_variable.get(i).copied().unwrap_or(0) != 0,
                minspeed_kmh: unpack_opt_metric(&self.edge_minspeed_kmh, i),
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
                ferry_interval_min: ferry_intervals
                    .and_then(|v| unpack_opt_metric(v, i))
                    .filter(|_| self.edge_is_ferry[i] != 0),
                is_tunnel: self.edge_is_tunnel[i] != 0,
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
                surface_quality: SurfaceQuality::from_u8(
                    self.edge_surface_quality
                        .get(i)
                        .copied()
                        .unwrap_or_else(|| {
                            // Pre-v8 packs should not reach here (format version gate).
                            crate::routing::graph::infer_surface_from_highway(Some(hw)).as_u8()
                        }),
                ),
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
        RouteGraph::from_parts_with_blocks(nodes, edges, profile, blocked, false)
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

fn archived_str_opt(s: &str) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

#[inline]
fn arch_f64(v: impl Into<f64>) -> f64 {
    v.into()
}

#[inline]
fn arch_u32(v: impl Into<u32>) -> u32 {
    v.into()
}

#[inline]
fn arch_i64(v: impl Into<i64>) -> i64 {
    v.into()
}

#[inline]
fn arch_u8(v: impl Into<u8>) -> u8 {
    v.into()
}

fn archived_opt_metric_at(
    vals: &rkyv::vec::ArchivedVec<rkyv::rend::f64_le>,
    i: usize,
) -> Option<f64> {
    if i >= vals.len() {
        return None;
    }
    let v = arch_f64(vals[i]);
    if v.is_finite() {
        Some(v)
    } else {
        None
    }
}

impl ArchivedFlatGraphPack {
    /// Materialize a [`RouteGraph`] from the mmap'd archive **without** owning a
    /// full [`FlatGraphPack`]. Plan-time bbox clip then only allocates strings /
    /// shapes for kept edges — critical when merging several 100MB+ tiles on
    /// Automotive devices (full `rkyv::deserialize` peaks at pack size + graph).
    pub fn to_route_graph_bbox(
        &self,
        profile: RoutingProfile,
        bbox: Option<[f64; 4]>,
    ) -> RouteGraph {
        match bbox {
            Some(b) => self.to_route_graph_clips(profile, Some(std::slice::from_ref(&b))),
            None => self.to_route_graph_clips(profile, None),
        }
    }

    /// Like [`Self::to_route_graph_bbox`], keeping edges whose lat/lon envelope
    /// overlaps **any** clip box (corridor band along the OD).
    pub fn to_route_graph_clips(
        &self,
        profile: RoutingProfile,
        clips: Option<&[[f64; 4]]>,
    ) -> RouteGraph {
        self.to_route_graph_clips_with_ferry_intervals(profile, clips, None)
    }

    /// Like [`Self::to_route_graph_clips`], applying optional per-edge ferry
    /// departure intervals (ferry overlay sidecar only).
    pub fn to_route_graph_clips_with_ferry_intervals(
        &self,
        profile: RoutingProfile,
        clips: Option<&[[f64; 4]]>,
        ferry_intervals: Option<&[f64]>,
    ) -> RouteGraph {
        let n_edges = self.edge_src.len();
        let skeleton = densify_skeleton_only_active();
        let in_clips = |i: usize| -> bool {
            let Some(clips) = clips else {
                return true;
            };
            clip_keeps_edge(
                arch_f64(self.edge_start_lat[i]),
                arch_f64(self.edge_start_lon[i]),
                arch_f64(self.edge_end_lat[i]),
                arch_f64(self.edge_end_lon[i]),
                clips,
            )
        };
        let ferry_nodes: HashSet<u32> = if skeleton {
            let mut s = HashSet::new();
            for i in 0..n_edges {
                if self.edge_is_ferry.get(i).copied().map(arch_u8).unwrap_or(0) == 0 || !in_clips(i)
                {
                    continue;
                }
                s.insert(arch_u32(self.edge_src[i]));
                s.insert(arch_u32(self.edge_tgt[i]));
            }
            s
        } else {
            HashSet::new()
        };
        let edge_ok = |i: usize| -> bool {
            if skeleton {
                let hw = self.edge_highway[i].as_str();
                let ferry = self.edge_is_ferry.get(i).copied().map(arch_u8).unwrap_or(0) != 0;
                let src = arch_u32(self.edge_src[i]);
                let tgt = arch_u32(self.edge_tgt[i]);
                let major = densify_skeleton_edge(hw, ferry);
                let pier = !ferry && (ferry_nodes.contains(&src) || ferry_nodes.contains(&tgt));
                if !major && !pier {
                    return false;
                }
            }
            in_clips(i)
        };

        let mut used_nodes: HashMap<u32, ()> = HashMap::new();
        for i in 0..n_edges {
            if edge_ok(i) {
                used_nodes.insert(arch_u32(self.edge_src[i]), ());
                used_nodes.insert(arch_u32(self.edge_tgt[i]), ());
            }
        }

        let mut nodes: HashMap<NodeId, Node> = HashMap::with_capacity(used_nodes.len());
        for &idx in used_nodes.keys() {
            let i = idx as usize;
            let id = NodeId(arch_i64(self.node_ids[i]));
            nodes.insert(
                id,
                Node {
                    id,
                    coord: Coord {
                        x: arch_f64(self.node_lons[i]),
                        y: arch_f64(self.node_lats[i]),
                    },
                    uses: 2,
                },
            );
        }

        let mut edges = Vec::new();
        for i in 0..n_edges {
            if !edge_ok(i) {
                continue;
            }
            let src = NodeId(arch_i64(self.node_ids[arch_u32(self.edge_src[i]) as usize]));
            let tgt = NodeId(arch_i64(self.node_ids[arch_u32(self.edge_tgt[i]) as usize]));
            let hw = self.edge_highway[i].as_str();
            let name = self.edge_name[i].as_str();
            let road_ref = self.edge_road_ref[i].as_str();
            let maxspeed = arch_f64(self.edge_maxspeed_kmh[i]);
            let shape = self.shape_for_edge(i);
            edges.push(GraphEdge {
                id: format!("{}-{}-{}", src.0, tgt.0, i),
                source: src,
                target: tgt,
                length_m: arch_f64(self.edge_length_m[i]),
                base_weight: arch_f64(self.edge_base_weight[i]),
                cost_mult: 1.0,
                eco_weight: Some(arch_f64(self.edge_base_weight[i])),
                start_lat: arch_f64(self.edge_start_lat[i]),
                start_lon: arch_f64(self.edge_start_lon[i]),
                end_lat: arch_f64(self.edge_end_lat[i]),
                end_lon: arch_f64(self.edge_end_lon[i]),
                shape,
                highway: archived_str_opt(hw),
                maxspeed_kmh: if maxspeed.is_finite() {
                    Some(maxspeed)
                } else {
                    None
                },
                maxspeed_practical_kmh: archived_opt_metric_at(
                    &self.edge_maxspeed_practical_kmh,
                    i,
                ),
                maxspeed_advisory_kmh: archived_opt_metric_at(&self.edge_maxspeed_advisory_kmh, i),
                maxspeed_type: archived_str_opt(
                    self.edge_maxspeed_type
                        .get(i)
                        .map(|s| s.as_str())
                        .unwrap_or(""),
                ),
                maxspeed_variable: self
                    .edge_maxspeed_variable
                    .get(i)
                    .copied()
                    .map(arch_u8)
                    .unwrap_or(0)
                    != 0,
                minspeed_kmh: archived_opt_metric_at(&self.edge_minspeed_kmh, i),
                name: archived_str_opt(name),
                road_ref: archived_str_opt(road_ref),
                is_motorroad: self
                    .edge_is_motorroad
                    .get(i)
                    .copied()
                    .map(arch_u8)
                    .unwrap_or(0)
                    != 0,
                is_expressway: self
                    .edge_is_expressway
                    .get(i)
                    .copied()
                    .map(arch_u8)
                    .unwrap_or(0)
                    != 0,
                is_oneway: self
                    .edge_is_oneway
                    .get(i)
                    .copied()
                    .map(arch_u8)
                    .unwrap_or(0)
                    != 0,
                lanes: {
                    let n = self.edge_lanes.get(i).copied().map(arch_u8).unwrap_or(0);
                    if n == 0 {
                        None
                    } else {
                        Some(n)
                    }
                },
                maxweight_t: archived_opt_metric_at(&self.edge_maxweight_t, i),
                maxaxleload_t: archived_opt_metric_at(&self.edge_maxaxleload_t, i),
                maxbogieweight_t: archived_opt_metric_at(&self.edge_maxbogieweight_t, i),
                maxheight_m: archived_opt_metric_at(&self.edge_maxheight_m, i),
                maxwidth_m: archived_opt_metric_at(&self.edge_maxwidth_m, i),
                maxlength_m: archived_opt_metric_at(&self.edge_maxlength_m, i),
                is_toll: arch_u8(self.edge_is_toll[i]) != 0,
                is_ferry: arch_u8(self.edge_is_ferry[i]) != 0,
                ferry_interval_min: ferry_intervals
                    .and_then(|v| unpack_opt_metric(v, i))
                    .filter(|_| arch_u8(self.edge_is_ferry[i]) != 0),
                is_tunnel: arch_u8(self.edge_is_tunnel[i]) != 0,
                is_boardwalk_crossing: arch_u8(self.edge_is_boardwalk[i]) != 0,
                is_roundabout: arch_u8(self.edge_is_roundabout[i]) != 0,
                motor_vehicle_conditional: archived_str_opt(
                    self.edge_motor_vehicle_conditional
                        .get(i)
                        .map(|s| s.as_str())
                        .unwrap_or(""),
                ),
                access_conditional: archived_str_opt(
                    self.edge_access_conditional
                        .get(i)
                        .map(|s| s.as_str())
                        .unwrap_or(""),
                ),
                maxspeed_conditional: archived_str_opt(
                    self.edge_maxspeed_conditional
                        .get(i)
                        .map(|s| s.as_str())
                        .unwrap_or(""),
                ),
                access_forbidden: self
                    .edge_access_forbidden
                    .get(i)
                    .copied()
                    .map(arch_u8)
                    .unwrap_or(0)
                    != 0,
                surface_quality: SurfaceQuality::from_u8(
                    self.edge_surface_quality
                        .get(i)
                        .copied()
                        .map(arch_u8)
                        .unwrap_or_else(|| {
                            if hw == "track" {
                                SurfaceQuality::Poor.as_u8()
                            } else {
                                SurfaceQuality::Good.as_u8()
                            }
                        }),
                ),
            });
        }

        let mut blocked = std::collections::HashSet::new();
        for (i, flag) in self.node_access_blocked.iter().enumerate() {
            if arch_u8(*flag) != 0 && used_nodes.contains_key(&(i as u32)) {
                blocked.insert(NodeId(arch_i64(self.node_ids[i])));
            }
        }
        RouteGraph::from_parts_with_blocks(nodes, edges, profile, blocked, false)
    }

    fn shape_for_edge(&self, i: usize) -> Vec<(f64, f64)> {
        if self.edge_shape_offsets.len() < 2 || i + 1 >= self.edge_shape_offsets.len() {
            return Vec::new();
        }
        let start = arch_u32(self.edge_shape_offsets[i]) as usize;
        let end = arch_u32(self.edge_shape_offsets[i + 1]) as usize;
        if end > self.edge_shape_lons.len()
            || end > self.edge_shape_lats.len()
            || start > end
            || self.edge_shape_lons.len() != self.edge_shape_lats.len()
        {
            return Vec::new();
        }
        (start..end)
            .map(|j| {
                (
                    arch_f64(self.edge_shape_lons[j]),
                    arch_f64(self.edge_shape_lats[j]),
                )
            })
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
            cost_mult: 1.0,
            eco_weight: Some(1_000.0),
            start_lat: 60.0,
            start_lon: 10.0,
            end_lat: 60.1,
            end_lon: 10.2,
            shape: vec![(10.05, 60.04), (10.12, 60.07), (10.18, 60.09)],
            highway: Some("secondary".into()),
            maxspeed_kmh: Some(80.0),
            maxspeed_practical_kmh: None,
            maxspeed_advisory_kmh: None,
            maxspeed_type: None,
            maxspeed_variable: false,
            minspeed_kmh: None,
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
            ferry_interval_min: None,
            is_tunnel: false,
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
    fn pack_roundtrip_preserves_surface_quality() {
        let mut graph = tiny_curved_graph();
        graph.edges[0].surface_quality = SurfaceQuality::Marginal;
        let pack = FlatGraphPack::from_route_graph(&graph, None);
        assert_eq!(
            pack.edge_surface_quality,
            vec![SurfaceQuality::Marginal.as_u8()]
        );
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&pack).expect("serialize");
        let archived =
            rkyv::access::<ArchivedFlatGraphPack, rkyv::rancor::Error>(&bytes[..]).expect("access");
        let restored: FlatGraphPack =
            rkyv::deserialize::<FlatGraphPack, rkyv::rancor::Error>(archived).expect("deserialize");
        let back = restored.to_route_graph(RoutingProfile::Car);
        assert_eq!(back.edges[0].surface_quality, SurfaceQuality::Marginal);
    }

    #[test]
    fn archived_bbox_matches_owned_materialize() {
        let graph = tiny_curved_graph();
        let pack = FlatGraphPack::from_route_graph(&graph, None);
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&pack).expect("serialize");
        let archived =
            rkyv::access::<ArchivedFlatGraphPack, rkyv::rancor::Error>(&bytes[..]).expect("access");
        let bbox = Some([59.9, 9.9, 60.2, 10.3]);
        let from_owned = pack.to_route_graph_bbox(RoutingProfile::Car, bbox);
        let from_arch = archived.to_route_graph_bbox(RoutingProfile::Car, bbox);
        assert_eq!(from_owned.edges.len(), from_arch.edges.len());
        assert_eq!(from_owned.nodes.len(), from_arch.nodes.len());
        assert_eq!(from_owned.edges[0].highway, from_arch.edges[0].highway);
        assert_eq!(from_owned.edges[0].shape, from_arch.edges[0].shape);
        // Outside bbox → empty graph either path.
        let miss = Some([0.0, 0.0, 1.0, 1.0]);
        assert!(pack
            .to_route_graph_bbox(RoutingProfile::Car, miss)
            .edges
            .is_empty());
        assert!(archived
            .to_route_graph_bbox(RoutingProfile::Car, miss)
            .edges
            .is_empty());
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
            cost_mult: 1.0,
            eco_weight: Some(length_m),
            start_lat,
            start_lon,
            end_lat,
            end_lon,
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
            maxheight_m,
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

    /// Write format is v9; load still accepts v8 ([`graph_format_version_accepted`]).
    #[test]
    fn graph_format_version_write_is_v9_and_accepts_v8() {
        assert_eq!(GRAPH_FORMAT_VERSION, 9);
        assert_eq!(GRAPH_FORMAT_VERSION_V8, 8);
        assert!(graph_format_version_accepted(8));
        assert!(graph_format_version_accepted(9));
        assert!(!graph_format_version_accepted(7));
        assert!(!graph_format_version_accepted(10));
        assert_eq!(preferred_graph_format_version([8, 9, 7]), Some(9));
        assert_eq!(preferred_graph_format_version([8, 8]), Some(8));
        assert_eq!(preferred_graph_format_version([7, 10]), None);
    }

    #[test]
    fn clip_keeps_oresund_span_when_terminals_miss_band_square() {
        // Kastrup / Lernacken-class terminals sit outside a water-only clip
        // square that still overlaps the bridge envelope.
        let slat = 55.628_f64;
        let slon = 12.647;
        let elat = 55.573;
        let elon = 12.891;
        let water = [[55.58, 12.75, 55.62, 12.80]];
        let in_box = |lat: f64, lon: f64, b: [f64; 4]| {
            lat >= b[0] && lat <= b[2] && lon >= b[1] && lon <= b[3]
        };
        assert!(!in_box(slat, slon, water[0]) && !in_box(elat, elon, water[0]));
        assert!(clip_keeps_edge(slat, slon, elat, elon, &water));
        let miss = [[56.2, 13.5, 56.3, 13.6]];
        assert!(!clip_keeps_edge(slat, slon, elat, elon, &miss));
    }

    fn tunnel_pair_graph() -> RouteGraph {
        let n1 = NodeId(1);
        let n2 = NodeId(2);
        let n3 = NodeId(3);
        let mut nodes = HashMap::new();
        for (id, lat, lon) in [(n1, 60.0, 10.0), (n2, 60.0, 10.01), (n3, 60.0, 10.02)] {
            nodes.insert(
                id,
                Node {
                    id,
                    coord: Coord { x: lon, y: lat },
                    uses: 2,
                },
            );
        }
        let mut tunnel = diamond_edge("tunnel", n1, n2, 60.0, 10.0, 60.0, 10.01, 100.0, None);
        tunnel.is_tunnel = true;
        let surface = diamond_edge("surface", n2, n3, 60.0, 10.01, 60.0, 10.02, 100.0, None);
        RouteGraph::from_parts(nodes, vec![tunnel, surface], RoutingProfile::Car)
    }

    #[test]
    fn pack_materialize_preserves_edge_is_tunnel_owned_and_archived() {
        let graph = tunnel_pair_graph();
        let pack = FlatGraphPack::from_route_graph(&graph, None);
        assert_eq!(pack.edge_is_tunnel, vec![1, 0]);

        let from_owned = pack.to_route_graph(RoutingProfile::Car);
        assert_eq!(from_owned.edges.len(), 2);
        let owned_tunnel = from_owned
            .edges
            .iter()
            .find(|e| e.source == NodeId(1) && e.target == NodeId(2))
            .expect("tunnel edge");
        let owned_surface = from_owned
            .edges
            .iter()
            .find(|e| e.source == NodeId(2) && e.target == NodeId(3))
            .expect("surface edge");
        assert!(owned_tunnel.is_tunnel);
        assert!(!owned_surface.is_tunnel);

        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&pack).expect("serialize");
        let archived =
            rkyv::access::<ArchivedFlatGraphPack, rkyv::rancor::Error>(&bytes[..]).expect("access");
        let from_arch = archived.to_route_graph_bbox(RoutingProfile::Car, None);
        assert_eq!(from_arch.edges.len(), 2);
        let arch_tunnel = from_arch
            .edges
            .iter()
            .find(|e| e.source == NodeId(1) && e.target == NodeId(2))
            .expect("archived tunnel edge");
        let arch_surface = from_arch
            .edges
            .iter()
            .find(|e| e.source == NodeId(2) && e.target == NodeId(3))
            .expect("archived surface edge");
        assert!(arch_tunnel.is_tunnel);
        assert!(!arch_surface.is_tunnel);
    }

    #[test]
    fn load_v8_pack_materializes_is_tunnel_false() {
        use crate::routing::indexed::graph_pack_v8::FlatGraphPackV8;
        use crate::routing::indexed::header::Preamble;
        use crate::routing::indexed::io::write_archive_atomic;
        use crate::routing::indexed::load::load_graph_pack;

        let graph = tunnel_pair_graph();
        let pack_v8 = FlatGraphPackV8::from_route_graph(&graph, None);
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&pack_v8).expect("serialize v8");
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = dir.path().join("v8.navi-graph-car.rkyv");
        write_archive_atomic(
            &path,
            Preamble::new(MAGIC_GRAPH, GRAPH_FORMAT_VERSION_V8),
            &bytes,
        )
        .expect("write v8 pack");
        let loaded = load_graph_pack(&path, RoutingProfile::Car).expect("load v8");
        assert_eq!(loaded.edges.len(), 2);
        assert!(
            loaded.edges.iter().all(|e| !e.is_tunnel),
            "v8 packs have no tunnel wire field; all is_tunnel must be false"
        );
    }

    #[test]
    fn load_v9_pack_preserves_tunnel_bits() {
        use crate::routing::indexed::header::Preamble;
        use crate::routing::indexed::io::write_archive_atomic;
        use crate::routing::indexed::load::load_graph_pack;

        let graph = tunnel_pair_graph();
        let pack = FlatGraphPack::from_route_graph(&graph, None);
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&pack).expect("serialize v9");
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = dir.path().join("v9.navi-graph-car.rkyv");
        write_archive_atomic(
            &path,
            Preamble::new(MAGIC_GRAPH, GRAPH_FORMAT_VERSION),
            &bytes,
        )
        .expect("write v9 pack");
        let loaded = load_graph_pack(&path, RoutingProfile::Car).expect("load v9");
        let tunnel = loaded
            .edges
            .iter()
            .find(|e| e.source == NodeId(1) && e.target == NodeId(2))
            .expect("tunnel edge");
        let surface = loaded
            .edges
            .iter()
            .find(|e| e.source == NodeId(2) && e.target == NodeId(3))
            .expect("surface edge");
        assert!(tunnel.is_tunnel);
        assert!(!surface.is_tunnel);
    }

    #[test]
    fn load_rejects_v7_and_v10_graph_preamble() {
        use crate::routing::indexed::header::Preamble;
        use crate::routing::indexed::io::write_archive_atomic;
        use crate::routing::indexed::load::{load_graph_pack, PackLoadError};

        let dir = tempfile::tempdir().expect("tmpdir");
        for ver in [7u32, 10] {
            let path = dir.path().join(format!("stale-v{ver}.navi-graph-car.rkyv"));
            write_archive_atomic(&path, Preamble::new(MAGIC_GRAPH, ver), b"body-ignored")
                .expect("write preamble pack");
            let err = match load_graph_pack(&path, RoutingProfile::Car) {
                Err(e) => e,
                Ok(_) => panic!("v{ver} preamble must fail load, got Ok"),
            };
            assert!(
                matches!(err, PackLoadError::VersionMismatch),
                "v{ver} preamble must be VersionMismatch, got {err}"
            );
        }
    }

    fn sample_manifest(
        graph_format_version: u32,
    ) -> crate::routing::indexed::manifest::NaviManifest {
        use crate::routing::indexed::manifest::NaviManifest;
        use crate::routing::indexed::poi_barrier_pack::POI_BARRIER_FORMAT_VERSION;
        use crate::routing::indexed::wetland_pack::WETLAND_FORMAT_VERSION;

        NaviManifest {
            schema: NaviManifest::SCHEMA,
            stem: "tiny".into(),
            pbf_filename: "tiny.osm.pbf".into(),
            pbf_size_bytes: 0,
            pbf_modified_unix_secs: 0,
            graph_files: Default::default(),
            graph_tiles: Default::default(),
            graph_format_version,
            poi_barrier_file: "tiny.navi-poi-barrier.rkyv".into(),
            poi_barrier_format_version: POI_BARRIER_FORMAT_VERSION,
            wetland_file: None,
            wetland_tiles: Vec::new(),
            wetland_format_version: WETLAND_FORMAT_VERSION,
            has_delta_h: false,
            elev_dir: None,
        }
    }

    #[test]
    fn manifest_accepts_v8_and_v9_rejects_neighbors() {
        use crate::routing::indexed::manifest::PackStatus;

        let dir = tempfile::tempdir().expect("tmpdir");
        // Version gate passes for 8/9; missing pack files → Missing (not VersionMismatch).
        assert_eq!(
            sample_manifest(8).status_pack_files(dir.path()),
            PackStatus::Missing
        );
        assert_eq!(
            sample_manifest(9).status_pack_files(dir.path()),
            PackStatus::Missing
        );
        assert_eq!(
            sample_manifest(7).status_pack_files(dir.path()),
            PackStatus::VersionMismatch
        );
        assert_eq!(
            sample_manifest(10).status_pack_files(dir.path()),
            PackStatus::VersionMismatch
        );
    }

    #[test]
    fn mixed_v8_v9_manifests_coexist_in_status_paths() {
        use crate::routing::indexed::manifest::PackStatus;

        let dir = tempfile::tempdir().expect("tmpdir");
        let v8 = sample_manifest(8);
        let v9 = sample_manifest(9);
        // Both pass the format gate independently (Missing only because files absent).
        assert_ne!(
            v8.status_pack_files(dir.path()),
            PackStatus::VersionMismatch
        );
        assert_ne!(
            v9.status_pack_files(dir.path()),
            PackStatus::VersionMismatch
        );
        assert_eq!(
            preferred_graph_format_version([v8.graph_format_version, v9.graph_format_version]),
            Some(9)
        );
    }
}
