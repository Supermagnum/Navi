//! Attach trip waypoints to the corridor skeleton by a real detailed search.
//!
//! Stage B used to snap each waypoint to the nearest skeleton node inside 35 km.
//! That picks the wrong road when a faster attachment is further away. This
//! module snaps the coordinate to a real road within the profile waypoint
//! budget (750 m for car), searches the region's pack until it reaches the
//! skeleton, and adds each useful hit as a spur with its travel time.
//!
//! Search bounds (stop at the first that applies):
//! - [`WAYPOINT_ATTACH_MAX_EXPANSIONS`] heap pops
//! - [`WAYPOINT_ATTACH_MAX_RADIUS_M`] crow-flies from the waypoint
//! - [`WAYPOINT_ATTACH_COST_SLACK_M`] drive-equivalent metres past the first hit
//! - [`WAYPOINT_ATTACH_MAX_HITS`] skeleton nodes collected, then at most
//!   [`WAYPOINT_ATTACH_MAX_SPURS`] kept by bearing
//!
//! Each waypoint is searched once per [`TripAttachPlan`]; legs, alternatives
//! and a trip-local widen reuse that result.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use osm4routing::NodeId;

use crate::pack_server::leaf_stem_for_region_id;
use crate::routing::graph::{
    ferry_drive_equiv_m_per_s, max_waypoint_snap_m, GraphEdge, RouteGraph, RouteOptions,
    RoutingProfile, SnapRole, SurfaceQuality, TravelHit,
};
use crate::routing::indexed::{
    bbox_intersects, load_graph_pack_bbox, merge_tile_graphs, GraphTileEntry, NaviManifest,
};

/// Expansions of the attach Dijkstra (one pop from the heap).
pub const WAYPOINT_ATTACH_MAX_EXPANSIONS: u64 = 80_000;
/// Do not expand nodes further than this from the waypoint.
pub const WAYPOINT_ATTACH_MAX_RADIUS_M: f64 = 40_000.0;
/// Keep searching this far (drive-equivalent metres) past the first hit so
/// other directions can appear.
pub const WAYPOINT_ATTACH_COST_SLACK_M: f64 = 20_000.0;
/// Maximum skeleton nodes attached from one waypoint.
pub const WAYPOINT_ATTACH_MAX_SPURS: usize = 4;
/// Candidate hits collected before bearing pick (must be >= max spurs).
const WAYPOINT_ATTACH_MAX_HITS: usize = 12;
/// Two spurs must leave at least this many degrees apart.
pub const WAYPOINT_ATTACH_MIN_BEARING_DEG: f64 = 60.0;
/// Neighbouring tiles considered for a lazy expand.
const WAYPOINT_ATTACH_MAX_TILES: usize = 6;

thread_local! {
    static ATTACH_SEARCH_COUNTS: RefCell<HashMap<(i64, i64), u32>> = RefCell::new(HashMap::new());
}

fn wp_key(lat: f64, lon: f64) -> (i64, i64) {
    ((lat * 100_000.0).round() as i64, (lon * 100_000.0).round() as i64)
}

fn note_attach_search(lat: f64, lon: f64) {
    ATTACH_SEARCH_COUNTS.with(|c| {
        *c.borrow_mut().entry(wp_key(lat, lon)).or_insert(0) += 1;
    });
}

/// Forget attach-search counts (tests).
pub fn reset_attach_search_counts() {
    ATTACH_SEARCH_COUNTS.with(|c| c.borrow_mut().clear());
}

/// How many times this coordinate was searched in this thread.
pub fn attach_search_count(lat: f64, lon: f64) -> u32 {
    ATTACH_SEARCH_COUNTS.with(|c| c.borrow().get(&wp_key(lat, lon)).copied().unwrap_or(0))
}

/// Total attach searches on this thread since the last reset.
pub fn total_attach_searches() -> u32 {
    ATTACH_SEARCH_COUNTS.with(|c| c.borrow().values().copied().sum())
}

/// Why a waypoint could not be attached to the skeleton.
#[derive(Debug, Clone, PartialEq)]
pub enum WaypointAttachError {
    SnapTooFar {
        lat: f64,
        lon: f64,
        nearest_m: f64,
        max_m: f64,
    },
    NoRegion {
        lat: f64,
        lon: f64,
    },
    PackMissing {
        lat: f64,
        lon: f64,
        region_id: String,
    },
    CannotReachSkeleton {
        lat: f64,
        lon: f64,
        expansions: u64,
        radius_m: f64,
    },
}

impl std::fmt::Display for WaypointAttachError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SnapTooFar {
                lat,
                lon,
                nearest_m,
                max_m,
            } => write!(
                f,
                "waypoint {lat:.5},{lon:.5} is {nearest_m:.0} m from a road (limit {max_m:.0} m)"
            ),
            Self::NoRegion { lat, lon } => {
                write!(f, "waypoint {lat:.5},{lon:.5} is not in a catalog region")
            }
            Self::PackMissing {
                lat,
                lon,
                region_id,
            } => write!(
                f,
                "waypoint {lat:.5},{lon:.5} region {region_id} pack is not installed"
            ),
            Self::CannotReachSkeleton {
                lat,
                lon,
                expansions,
                radius_m,
            } => write!(
                f,
                "waypoint {lat:.5},{lon:.5} cannot reach the corridor skeleton \
                 within {radius_m:.0} m / {expansions} expansions"
            ),
        }
    }
}

/// One travel-time spur from the snapped waypoint to a skeleton node.
#[derive(Debug, Clone)]
pub struct ResolvedSpur {
    pub target: NodeId,
    pub target_ll: (f64, f64),
    pub length_m: f64,
    pub cost_m: f64,
}

/// Result of one waypoint search, reusable for every leg and alternative.
#[derive(Debug, Clone)]
pub struct WaypointAttachment {
    pub snap_id: NodeId,
    pub snap_ll: (f64, f64),
    pub snap_m: f64,
    pub snap_role: SnapRole,
    pub already_on_skeleton: bool,
    pub spurs: Vec<ResolvedSpur>,
    pub tiles: Vec<String>,
    pub tiles_mb: f64,
    pub load_ms: u128,
    pub snap_ms: u128,
    pub dijkstra_ms: u128,
    pub expansions: u64,
    pub hit_count: usize,
    pub hits: Vec<ResolvedSpur>,
    pub spur_ms: u128,
}

impl WaypointAttachment {
    /// Insert the snap node and its spurs onto `coarse`. Missing spur targets
    /// (trip-local graph) are skipped; a later widen reapplies the same result.
    ///
    /// A node that is already a usable skeleton snap is left alone (no extra
    /// shortcuts). A directed stub that shares an OSM id must get its spurs.
    pub fn apply(&self, coarse: &mut RouteGraph) {
        if self.already_on_skeleton
            && coarse.nodes.contains_key(&self.snap_id)
            && coarse.directed_snap_ok(self.snap_id, self.snap_role)
        {
            return;
        }
        coarse.insert_node_at(self.snap_id, self.snap_ll.0, self.snap_ll.1);
        let mut usable: Vec<&ResolvedSpur> = self
            .hits
            .iter()
            .filter(|s| {
                coarse.nodes.contains_key(&s.target) && coarse.directed_snap_ok(s.target, SnapRole::Via)
            })
            .collect();
        if usable.is_empty() {
            usable = self
                .hits
                .iter()
                .filter(|s| coarse.nodes.contains_key(&s.target))
                .collect();
        }
        if usable.is_empty() {
            usable = self
                .spurs
                .iter()
                .filter(|s| coarse.nodes.contains_key(&s.target))
                .collect();
        }
        let picked = pick_resolved_by_bearing(self.snap_ll, &usable);
        for spur in picked {
            coarse.push_directed_edge(spur_edge(
                self.snap_id,
                spur.target,
                self.snap_ll,
                spur.target_ll,
                spur.length_m,
                spur.cost_m,
            ));
            coarse.push_directed_edge(spur_edge(
                spur.target,
                self.snap_id,
                spur.target_ll,
                self.snap_ll,
                spur.length_m,
                spur.cost_m,
            ));
        }
    }
}

/// Attachments for every trip waypoint, searched once.
#[derive(Debug, Clone)]
pub struct TripAttachPlan {
    pub attachments: Vec<WaypointAttachment>,
}

impl TripAttachPlan {
    /// Search each distinct waypoint once. `skeleton_osm_ids` comes from the
    /// skeleton files, not from a loaded coarse graph.
    pub fn resolve(
        pack_dirs: &[&Path],
        waypoints: &[(f64, f64)],
        skeleton_osm_ids: &HashSet<i64>,
        profile: RoutingProfile,
        route_options: &RouteOptions,
    ) -> Result<Self, WaypointAttachError> {
        let last = waypoints
            .len()
            .checked_sub(1)
            .ok_or(WaypointAttachError::NoRegion { lat: 0.0, lon: 0.0 })?;
        let mut seen: HashMap<(i64, i64), WaypointAttachment> = HashMap::new();
        let mut attachments = Vec::with_capacity(waypoints.len());
        for (i, &wp) in waypoints.iter().enumerate() {
            let key = wp_key(wp.0, wp.1);
            if let Some(existing) = seen.get(&key) {
                attachments.push(existing.clone());
                continue;
            }
            let role = if i == 0 {
                SnapRole::Origin
            } else if i == last {
                SnapRole::Destination
            } else {
                SnapRole::Via
            };
            let att = resolve_one_lazy(
                pack_dirs,
                skeleton_osm_ids,
                wp,
                role,
                profile,
                route_options,
            )?;
            seen.insert(key, att.clone());
            attachments.push(att);
        }
        Ok(Self { attachments })
    }

    pub fn apply(&self, coarse: &mut RouteGraph) -> Vec<NodeId> {
        let mut snaps = Vec::with_capacity(self.attachments.len());
        for att in &self.attachments {
            att.apply(coarse);
            snaps.push(att.snap_id);
        }
        snaps
    }
}

/// Snap `waypoint` on `detailed` (profile budget) and attach it to `coarse`
/// with one spur per useful skeleton node (travel time, different directions).
pub fn attach_waypoint_to_skeleton(
    coarse: &mut RouteGraph,
    detailed: &RouteGraph,
    waypoint: (f64, f64),
    snap_role: SnapRole,
    route_options: &RouteOptions,
) -> Result<NodeId, WaypointAttachError> {
    let ids: HashSet<i64> = coarse
        .nodes
        .keys()
        .filter(|id| coarse.directed_snap_ok(**id, SnapRole::Via))
        .map(|id| id.0)
        .collect();
    let att = resolve_one_on_graph(detailed, &ids, waypoint, snap_role, route_options, true)?;
    att.apply(coarse);
    Ok(att.snap_id)
}

/// Attach every trip waypoint; origin / vias / destination use the matching snap role.
pub fn attach_trip_waypoints(
    coarse: &mut RouteGraph,
    pack_dirs: &[&Path],
    waypoints: &[(f64, f64)],
    profile: RoutingProfile,
    route_options: &RouteOptions,
) -> Result<Vec<NodeId>, WaypointAttachError> {
    let ids: HashSet<i64> = coarse.nodes.keys().map(|id| id.0).collect();
    let plan = TripAttachPlan::resolve(pack_dirs, waypoints, &ids, profile, route_options)?;
    Ok(plan.apply(coarse))
}

fn resolve_one_on_graph(
    detailed: &RouteGraph,
    skeleton_osm_ids: &HashSet<i64>,
    waypoint: (f64, f64),
    snap_role: SnapRole,
    route_options: &RouteOptions,
    count_search: bool,
) -> Result<WaypointAttachment, WaypointAttachError> {
    let (lat, lon) = waypoint;
    if count_search {
        note_attach_search(lat, lon);
    }
    let snap_m = max_waypoint_snap_m(detailed.profile());
    let opts = RouteOptions {
        snap_role,
        ..route_options.clone()
    };
    let t_snap = std::time::Instant::now();
    let (snap_id, dist_m) = detailed
        .nearest_routable_with_options_max(lat, lon, &opts, false, snap_m)
        .map_err(|e| WaypointAttachError::SnapTooFar {
            lat,
            lon,
            nearest_m: e.nearest_m,
            max_m: e.max_m,
        })?;
    let snap_ms = t_snap.elapsed().as_millis();
    let snap_ll = detailed
        .nodes
        .get(&snap_id)
        .map(|n| (n.coord.y, n.coord.x))
        .unwrap_or(waypoint);
    let on_skel = skeleton_osm_ids.contains(&snap_id.0)
        && detailed.directed_snap_ok(snap_id, snap_role);
    if on_skel {
        return Ok(WaypointAttachment {
            snap_id,
            snap_ll,
            snap_m: dist_m,
            snap_role,
            already_on_skeleton: true,
            spurs: Vec::new(),
            tiles: Vec::new(),
            tiles_mb: 0.0,
            load_ms: 0,
            snap_ms,
            dijkstra_ms: 0,
            expansions: 0,
            hit_count: 0,
            hits: Vec::new(),
            spur_ms: 0,
        });
    }
    let targets: HashSet<NodeId> = detailed
        .nodes
        .keys()
        .copied()
        .filter(|id| skeleton_osm_ids.contains(&id.0))
        .collect();
    if targets.is_empty() {
        return Err(WaypointAttachError::CannotReachSkeleton {
            lat,
            lon,
            expansions: 0,
            radius_m: WAYPOINT_ATTACH_MAX_RADIUS_M,
        });
    }
    let t_d = std::time::Instant::now();
    let (hits, expansions, _) = detailed.reach_nodes_by_travel(
        snap_id,
        &targets,
        &opts,
        WAYPOINT_ATTACH_MAX_EXPANSIONS,
        WAYPOINT_ATTACH_MAX_RADIUS_M,
        waypoint,
        WAYPOINT_ATTACH_COST_SLACK_M,
        WAYPOINT_ATTACH_MAX_HITS,
        None,
    );
    let dijkstra_ms = t_d.elapsed().as_millis();
    let t_s = std::time::Instant::now();
    let picked = pick_spurs_by_bearing(detailed, snap_id, &hits);
    let spur_ms = t_s.elapsed().as_millis();
    if picked.is_empty() {
        return Err(WaypointAttachError::CannotReachSkeleton {
            lat,
            lon,
            expansions,
            radius_m: WAYPOINT_ATTACH_MAX_RADIUS_M,
        });
    }
    let all_hits = travel_hits_to_spurs(detailed, &hits);
    let spurs = travel_hits_to_spurs(detailed, &picked);
    Ok(WaypointAttachment {
        snap_id,
        snap_ll,
        snap_m: dist_m,
        snap_role,
        already_on_skeleton: false,
        spurs,
        tiles: Vec::new(),
        tiles_mb: 0.0,
        load_ms: 0,
        snap_ms,
        dijkstra_ms,
        expansions,
        hit_count: all_hits.len(),
        hits: all_hits,
        spur_ms,
    })
}

fn resolve_one_lazy(
    pack_dirs: &[&Path],
    skeleton_osm_ids: &HashSet<i64>,
    waypoint: (f64, f64),
    snap_role: SnapRole,
    profile: RoutingProfile,
    route_options: &RouteOptions,
) -> Result<WaypointAttachment, WaypointAttachError> {
    let (lat, lon) = waypoint;
    note_attach_search(lat, lon);
    let (region_id, tiles) = list_region_tiles(pack_dirs, waypoint, profile)?;
    if tiles.is_empty() {
        return Err(WaypointAttachError::PackMissing {
            lat,
            lon,
            region_id,
        });
    }
    let search_bbox = radius_bbox(lat, lon, WAYPOINT_ATTACH_MAX_RADIUS_M);
    let mut loaded: Vec<usize> = Vec::new();
    let cover = covering_tile_index(&tiles, lat, lon).unwrap_or(0);
    let mut load_ms = 0u128;
    let mut tiles_mb = 0.0;
    let mut tile_names: Vec<String> = Vec::new();
    let mut detailed = load_tile(&tiles[cover], profile, search_bbox, &mut load_ms, &mut tiles_mb)?;
    loaded.push(cover);
    tile_names.push(tiles[cover].name.clone());

    // Labels stay on the coarse graph. The tile snap uses Any so we do not
    // pay Kosaraju on a 30 MB tile; apply still requires a directed-ok spur.
    let opts = RouteOptions {
        snap_role: SnapRole::Any,
        ..route_options.clone()
    };
    let snap_m = max_waypoint_snap_m(profile);
    let t_snap = std::time::Instant::now();
    let (snap_id, dist_m) = detailed
        .nearest_routable_with_options_max(lat, lon, &opts, false, snap_m)
        .map_err(|e| WaypointAttachError::SnapTooFar {
            lat,
            lon,
            nearest_m: e.nearest_m,
            max_m: e.max_m,
        })?;
    let snap_ms = t_snap.elapsed().as_millis();
    let snap_ll = detailed
        .nodes
        .get(&snap_id)
        .map(|n| (n.coord.y, n.coord.x))
        .unwrap_or(waypoint);

    let mut expansions = 0u64;
    let mut dijkstra_ms = 0u128;
    let mut hits: Vec<TravelHit> = Vec::new();
    // Hint only: apply still checks the coarse graph. A directed stub can share
    // an OSM id with the skeleton (Bevensen / Dalsøren). Search unless the snap
    // is already a usable skeleton node.
    let already_on_skeleton =
        skeleton_osm_ids.contains(&snap_id.0) && detailed.directed_snap_ok(snap_id, snap_role);

    if !already_on_skeleton {
    loop {
            let targets: HashSet<NodeId> = detailed
                .nodes
                .keys()
                .copied()
                .filter(|id| skeleton_osm_ids.contains(&id.0))
                .collect();
            let clip = loaded_coverage_bbox(&tiles, &loaded);
            let t_d = std::time::Instant::now();
            let (h, exp, hit_edge) = if targets.is_empty() {
                (Vec::new(), 0u64, true)
            } else {
                detailed.reach_nodes_by_travel(
                    snap_id,
                    &targets,
                    &opts,
                    WAYPOINT_ATTACH_MAX_EXPANSIONS,
                    WAYPOINT_ATTACH_MAX_RADIUS_M,
                    waypoint,
                    WAYPOINT_ATTACH_COST_SLACK_M,
                    WAYPOINT_ATTACH_MAX_HITS,
                    Some(clip),
                )
            };
            dijkstra_ms += t_d.elapsed().as_millis();
            expansions = expansions.saturating_add(exp);
            hits = h;
            let picked_preview = pick_spurs_by_bearing(&detailed, snap_id, &hits);
            let enough = !picked_preview.is_empty() && !hit_edge;
            if enough || loaded.len() >= WAYPOINT_ATTACH_MAX_TILES {
                break;
            }
            let Some(next) = next_neighbor_tile(&tiles, &loaded, lat, lon, search_bbox) else {
                break;
            };
            match load_tile(&tiles[next], profile, search_bbox, &mut load_ms, &mut tiles_mb) {
                Ok(extra) => {
                    detailed = merge_tile_graphs(vec![detailed, extra], profile);
                    loaded.push(next);
                    tile_names.push(tiles[next].name.clone());
                }
                Err(_) => {
                    loaded.push(next);
                    break;
                }
            }
        }
    }

    let t_s = std::time::Instant::now();
    let picked = pick_spurs_by_bearing(&detailed, snap_id, &hits);
    let spur_ms = t_s.elapsed().as_millis();
    if !already_on_skeleton && picked.is_empty() {
        return Err(WaypointAttachError::CannotReachSkeleton {
            lat,
            lon,
            expansions,
            radius_m: WAYPOINT_ATTACH_MAX_RADIUS_M,
        });
    }
    let all_hits = travel_hits_to_spurs(&detailed, &hits);
    let spurs = travel_hits_to_spurs(&detailed, &picked);
    let hit_n = all_hits.len();
    drop(detailed);
    let line = format!(
        "waypoint_attach lat={lat:.5} lon={lon:.5} role={snap_role:?} \
         tiles={} tiles_n={} mb={:.1} load_ms={load_ms} snap_ms={snap_ms} \
         dijkstra_ms={dijkstra_ms} expansions={expansions} hits={} spurs={} \
         spur_ms={spur_ms} already_on_skel={already_on_skeleton} snap_m={dist_m:.0}",
        tile_names.join(","),
        tile_names.len(),
        tiles_mb,
        hit_n,
        spurs.len()
    );
    log::info!(target: "NaviPlan", "{line}");
    crate::routing::plan_file_log::line(line);
    Ok(WaypointAttachment {
        snap_id,
        snap_ll,
        snap_m: dist_m,
        snap_role,
        already_on_skeleton,
        spurs,
        tiles: tile_names,
        tiles_mb,
        load_ms,
        snap_ms,
        dijkstra_ms,
        expansions,
        hit_count: hit_n,
        hits: all_hits,
        spur_ms,
    })
}

struct PackTile {
    name: String,
    path: PathBuf,
    bbox: [f64; 4],
}

fn list_region_tiles(
    pack_dirs: &[&Path],
    waypoint: (f64, f64),
    profile: RoutingProfile,
) -> Result<(String, Vec<PackTile>), WaypointAttachError> {
    let (lat, lon) = waypoint;
    let region_id = crate::long_trip::region_containing(lat, lon, None)
        .map(|s| s.to_string())
        .ok_or(WaypointAttachError::NoRegion { lat, lon })?;
    let stem = leaf_stem_for_region_id(&region_id);
    let home = pack_dirs
        .iter()
        .copied()
        .find(|d| d.join(format!("{stem}.navi-manifest.json")).is_file())
        .ok_or_else(|| WaypointAttachError::PackMissing {
            lat,
            lon,
            region_id: region_id.clone(),
        })?;
    let text =
        std::fs::read_to_string(home.join(format!("{stem}.navi-manifest.json"))).map_err(|_| {
            WaypointAttachError::PackMissing {
                lat,
                lon,
                region_id: region_id.clone(),
            }
        })?;
    let man: NaviManifest =
        serde_json::from_str(&text).map_err(|_| WaypointAttachError::PackMissing {
            lat,
            lon,
            region_id: region_id.clone(),
        })?;
    let mut tiles = Vec::new();
    if let Some(entries) = man.graph_tiles_for(profile) {
        for t in entries {
            tiles.push(pack_tile(home, t));
        }
    }
    if tiles.is_empty() {
        if let Some(p) = man.graph_path(home, profile) {
            let bbox = man
                .graph_tiles_for(profile)
                .and_then(|ts| ts.first().map(|t| t.bbox))
                .unwrap_or([-90.0, -180.0, 90.0, 180.0]);
            tiles.push(PackTile {
                name: p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("graph")
                    .to_string(),
                path: p,
                bbox,
            });
        }
    }
    if tiles.is_empty() {
        return Err(WaypointAttachError::PackMissing { lat, lon, region_id });
    }
    Ok((region_id, tiles))
}

fn pack_tile(home: &Path, t: &GraphTileEntry) -> PackTile {
    PackTile {
        name: t.file.clone(),
        path: home.join(&t.file),
        bbox: t.bbox,
    }
}

fn covering_tile_index(tiles: &[PackTile], lat: f64, lon: f64) -> Option<usize> {
    tiles.iter().position(|t| {
        crate::routing::basemap::bbox_covers_point(t.bbox, lat, lon)
    })
}

fn loaded_coverage_bbox(tiles: &[PackTile], loaded: &[usize]) -> [f64; 4] {
    let mut out = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for &i in loaded {
        let b = tiles[i].bbox;
        out[0] = out[0].min(b[0]);
        out[1] = out[1].min(b[1]);
        out[2] = out[2].max(b[2]);
        out[3] = out[3].max(b[3]);
    }
    out
}

fn next_neighbor_tile(
    tiles: &[PackTile],
    loaded: &[usize],
    lat: f64,
    lon: f64,
    search_bbox: [f64; 4],
) -> Option<usize> {
    let loaded_set: HashSet<usize> = loaded.iter().copied().collect();
    let pad = 0.02;
    let mut best: Option<(f64, usize)> = None;
    for (i, t) in tiles.iter().enumerate() {
        if loaded_set.contains(&i) || !bbox_intersects(t.bbox, search_bbox) {
            continue;
        }
        let touches = loaded.iter().any(|&j| {
            let mut b = tiles[j].bbox;
            b[0] -= pad;
            b[1] -= pad;
            b[2] += pad;
            b[3] += pad;
            bbox_intersects(b, t.bbox)
        });
        if !touches {
            continue;
        }
        let clat = (t.bbox[0] + t.bbox[2]) * 0.5;
        let clon = (t.bbox[1] + t.bbox[3]) * 0.5;
        let d = (clat - lat).hypot(clon - lon);
        if best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, i));
        }
    }
    best.map(|(_, i)| i)
}

fn load_tile(
    tile: &PackTile,
    profile: RoutingProfile,
    search_bbox: [f64; 4],
    load_ms: &mut u128,
    tiles_mb: &mut f64,
) -> Result<RouteGraph, WaypointAttachError> {
    let t0 = std::time::Instant::now();
    let g = load_graph_pack_bbox(&tile.path, profile, Some(search_bbox)).map_err(|_| {
        WaypointAttachError::PackMissing {
            lat: 0.0,
            lon: 0.0,
            region_id: tile.name.clone(),
        }
    })?;
    *load_ms += t0.elapsed().as_millis();
    if let Ok(meta) = std::fs::metadata(&tile.path) {
        *tiles_mb += meta.len() as f64 / (1024.0 * 1024.0);
    }
    Ok(g)
}

fn radius_bbox(lat: f64, lon: f64, radius_m: f64) -> [f64; 4] {
    let dlat = radius_m / 111_320.0;
    let cos = lat.to_radians().cos().abs().max(0.2);
    let dlon = radius_m / (111_320.0 * cos);
    [lat - dlat, lon - dlon, lat + dlat, lon + dlon]
}

fn pick_spurs_by_bearing(
    detailed: &RouteGraph,
    snap_id: NodeId,
    hits: &[TravelHit],
) -> Vec<TravelHit> {
    let origin = match detailed.nodes.get(&snap_id) {
        Some(n) => (n.coord.y, n.coord.x),
        None => {
            return hits
                .iter()
                .take(WAYPOINT_ATTACH_MAX_SPURS)
                .cloned()
                .collect()
        }
    };
    let mut ordered = hits.to_vec();
    ordered.sort_by(|a, b| a.cost_m.total_cmp(&b.cost_m));
    let mut picked: Vec<TravelHit> = Vec::new();
    let mut bearings: Vec<f64> = Vec::new();
    for h in ordered {
        let Some(n) = detailed.nodes.get(&h.node) else {
            continue;
        };
        let b = bearing_deg(origin, (n.coord.y, n.coord.x));
        let ok = bearings
            .iter()
            .all(|&pb| angle_diff_deg(pb, b) >= WAYPOINT_ATTACH_MIN_BEARING_DEG);
        if picked.is_empty() || ok {
            picked.push(h);
            bearings.push(b);
            if picked.len() >= WAYPOINT_ATTACH_MAX_SPURS {
                break;
            }
        }
    }
    picked
}

fn travel_hits_to_spurs(detailed: &RouteGraph, hits: &[TravelHit]) -> Vec<ResolvedSpur> {
    hits.iter()
        .filter_map(|hit| {
            let n = detailed.nodes.get(&hit.node)?;
            Some(ResolvedSpur {
                target: hit.node,
                target_ll: (n.coord.y, n.coord.x),
                length_m: hit.length_m,
                cost_m: hit.cost_m,
            })
        })
        .collect()
}

fn pick_resolved_by_bearing(origin: (f64, f64), hits: &[&ResolvedSpur]) -> Vec<ResolvedSpur> {
    let mut ordered = hits.to_vec();
    ordered.sort_by(|a, b| a.cost_m.total_cmp(&b.cost_m));
    let mut picked: Vec<ResolvedSpur> = Vec::new();
    let mut bearings: Vec<f64> = Vec::new();
    for h in ordered {
        let b = bearing_deg(origin, h.target_ll);
        let ok = bearings
            .iter()
            .all(|&pb| angle_diff_deg(pb, b) >= WAYPOINT_ATTACH_MIN_BEARING_DEG);
        if picked.is_empty() || ok {
            picked.push((*h).clone());
            bearings.push(b);
            if picked.len() >= WAYPOINT_ATTACH_MAX_SPURS {
                break;
            }
        }
    }
    picked
}

fn bearing_deg(from: (f64, f64), to: (f64, f64)) -> f64 {
    let y = (to.1 - from.1).to_radians() * to.0.to_radians().cos();
    let x = (to.0 - from.0).to_radians();
    y.atan2(x).to_degrees().rem_euclid(360.0)
}

fn angle_diff_deg(a: f64, b: f64) -> f64 {
    let d = (a - b).abs() % 360.0;
    if d > 180.0 {
        360.0 - d
    } else {
        d
    }
}

fn spur_edge(
    source: NodeId,
    target: NodeId,
    start: (f64, f64),
    end: (f64, f64),
    length_m: f64,
    cost_m: f64,
) -> GraphEdge {
    let length_m = length_m.max(1.0);
    let cost_m = cost_m.max(1.0);
    let time_s = cost_m / ferry_drive_equiv_m_per_s();
    let speed_kmh = if time_s > 0.0 {
        (length_m / 1000.0) / (time_s / 3600.0)
    } else {
        50.0
    };
    GraphEdge {
        id: format!("waypoint_spur:{}:{}", source.0, target.0),
        source,
        target,
        length_m,
        base_weight: cost_m,
        cost_mult: 1.0,
        eco_weight: None,
        start_lat: start.0,
        start_lon: start.1,
        end_lat: end.0,
        end_lon: end.1,
        shape: Vec::new(),
        highway: Some("unclassified".into()),
        maxspeed_kmh: Some(speed_kmh.clamp(5.0, 130.0)),
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::graph::{RouteGraph, RoutingProfile};
    use geo_types::Coord;
    use osm4routing::Node;
    use std::collections::HashMap;

    fn node(id: i64, lat: f64, lon: f64) -> (NodeId, Node) {
        let id = NodeId(id);
        (
            id,
            Node {
                id,
                coord: Coord { x: lon, y: lat },
                uses: 0,
            },
        )
    }

    fn edge(
        id: &str,
        s: i64,
        t: i64,
        nodes: &HashMap<NodeId, Node>,
        len: f64,
        kmh: f64,
    ) -> GraphEdge {
        let sn = nodes[&NodeId(s)].coord;
        let tn = nodes[&NodeId(t)].coord;
        let mut e = spur_edge(
            NodeId(s),
            NodeId(t),
            (sn.y, sn.x),
            (tn.y, tn.x),
            len,
            crate::routing::graph::road_base_weight_m(len, kmh),
        );
        e.id = id.into();
        e.highway = Some("primary".into());
        e.maxspeed_kmh = Some(kmh);
        e
    }

    fn graph(nodes: HashMap<NodeId, Node>, edges: Vec<GraphEdge>) -> RouteGraph {
        RouteGraph::from_parts(nodes, edges, RoutingProfile::Car)
    }

    /// Closest skeleton node by crow-flies is a slow dead-end; the faster
    /// attachment is further north.
    #[test]
    fn attach_prefers_travel_time_over_nearest_distance() {
        let mut dnodes = HashMap::new();
        // Waypoint on a minor road at 60.00, 10.00.
        dnodes.extend([
            node(10, 60.00, 10.00),
            node(11, 60.00, 10.02), // 1.1 km east: slow crawl to near skeleton
            node(1, 60.00, 10.03),  // skeleton A, nearest by distance (~1.7 km)
            node(12, 60.04, 10.00), // 4.4 km north, fast
            node(2, 60.05, 10.00),  // skeleton B, further (~5.6 km)
        ]);
        let mut dedges = Vec::new();
        for (id, s, t, len, kmh) in [
            ("slow", 10i64, 11, 1_200.0, 10.0),
            ("slow2", 11, 1, 600.0, 10.0),
            ("fast", 10, 12, 4_400.0, 80.0),
            ("fast2", 12, 2, 1_200.0, 80.0),
        ] {
            dedges.push(edge(id, s, t, &dnodes, len, kmh));
            dedges.push(edge(&format!("{id}_r"), t, s, &dnodes, len, kmh));
        }
        let detailed = graph(dnodes, dedges);

        let mut cnodes = HashMap::new();
        cnodes.extend([node(1, 60.00, 10.03), node(2, 60.05, 10.00)]);
        // Skeleton A--B is a long way around, so the coarse search must use a spur.
        let cedges = vec![
            edge("skel", 1, 2, &cnodes, 80_000.0, 50.0),
            edge("skel_r", 2, 1, &cnodes, 80_000.0, 50.0),
        ];
        let mut coarse = graph(cnodes, cedges);

        let id = attach_waypoint_to_skeleton(
            &mut coarse,
            &detailed,
            (60.00, 10.00),
            SnapRole::Origin,
            &RouteOptions::default(),
        )
        .expect("attach");
        assert_eq!(id, NodeId(10));
        let to_a = coarse.shortest_path(NodeId(10), NodeId(1), false);
        let to_b = coarse.shortest_path(NodeId(10), NodeId(2), false);
        let cost_a = to_a.expect("path A").2;
        let cost_b = to_b.expect("path B").2;
        assert!(
            cost_b < cost_a,
            "travel-time spur to B must beat the slow near node A: B={cost_b:.0} A={cost_a:.0}"
        );
    }

    /// Minor road between two mains: both skeleton nodes get a spur.
    #[test]
    fn attach_minor_road_between_two_mains() {
        let mut dnodes = HashMap::new();
        dnodes.extend([
            node(1, 60.00, 9.90),   // west main (skeleton)
            node(10, 60.00, 10.00), // waypoint on minor
            node(2, 60.00, 10.10),  // east main (skeleton)
        ]);
        let mut dedges = Vec::new();
        for (id, s, t, len) in [("w", 1i64, 10, 5_500.0), ("e", 10, 2, 5_500.0)] {
            dedges.push(edge(id, s, t, &dnodes, len, 50.0));
            dedges.push(edge(&format!("{id}_r"), t, s, &dnodes, len, 50.0));
        }
        let detailed = graph(dnodes, dedges);

        let mut cnodes = HashMap::new();
        cnodes.extend([
            node(1, 60.00, 9.90),
            node(2, 60.00, 10.10),
            node(3, 60.20, 10.00),
        ]);
        // Mains only meet far north; the minor is the local connection.
        let cedges = vec![
            edge("n1", 1, 3, &cnodes, 30_000.0, 80.0),
            edge("n1r", 3, 1, &cnodes, 30_000.0, 80.0),
            edge("n2", 2, 3, &cnodes, 30_000.0, 80.0),
            edge("n2r", 3, 2, &cnodes, 30_000.0, 80.0),
        ];
        let mut coarse = graph(cnodes, cedges);
        attach_waypoint_to_skeleton(
            &mut coarse,
            &detailed,
            (60.00, 10.00),
            SnapRole::Via,
            &RouteOptions::default(),
        )
        .expect("attach");
        assert!(coarse.shortest_path(NodeId(10), NodeId(1), false).is_some());
        assert!(coarse.shortest_path(NodeId(10), NodeId(2), false).is_some());
        let via_1 = coarse
            .shortest_path(NodeId(10), NodeId(1), false)
            .unwrap()
            .2;
        let via_north = coarse
            .shortest_path(NodeId(10), NodeId(3), false)
            .unwrap()
            .2;
        assert!(
            via_1 * 2.0 < via_north,
            "direct west spur must beat going north: via_1={via_1:.0} north={via_north:.0}"
        );
    }

    #[test]
    fn attach_errors_when_skeleton_is_unreachable() {
        let mut dnodes = HashMap::new();
        dnodes.extend([node(10, 60.00, 10.00), node(11, 60.00, 10.01)]);
        let mut dedges = Vec::new();
        dedges.push(edge("iso", 10, 11, &dnodes, 800.0, 40.0));
        dedges.push(edge("iso_r", 11, 10, &dnodes, 800.0, 40.0));
        let detailed = graph(dnodes, dedges);

        let mut cnodes = HashMap::new();
        cnodes.extend([node(1, 61.00, 11.00), node(2, 61.10, 11.00)]);
        let cedges = vec![
            edge("s", 1, 2, &cnodes, 12_000.0, 80.0),
            edge("sr", 2, 1, &cnodes, 12_000.0, 80.0),
        ];
        let mut coarse = graph(cnodes, cedges);
        let err = attach_waypoint_to_skeleton(
            &mut coarse,
            &detailed,
            (60.00, 10.00),
            SnapRole::Destination,
            &RouteOptions::default(),
        )
        .expect_err("isolated road must not attach");
        match err {
            WaypointAttachError::CannotReachSkeleton { .. } => {}
            other => panic!("expected CannotReachSkeleton, got {other}"),
        }
    }

    /// A plan searches each waypoint once; applying the result again (widen,
    /// another alternative) must not search again.
    #[test]
    fn each_waypoint_is_attached_once_per_plan() {
        reset_attach_search_counts();
        let mut dnodes = HashMap::new();
        dnodes.extend([
            node(1, 60.00, 9.90),
            node(10, 60.00, 10.00),
            node(2, 60.00, 10.10),
        ]);
        let mut dedges = Vec::new();
        for (id, s, t, len) in [("w", 1i64, 10, 5_500.0), ("e", 10, 2, 5_500.0)] {
            dedges.push(edge(id, s, t, &dnodes, len, 50.0));
            dedges.push(edge(&format!("{id}_r"), t, s, &dnodes, len, 50.0));
        }
        let detailed = graph(dnodes, dedges);
        let mut cnodes = HashMap::new();
        cnodes.extend([
            node(1, 60.00, 9.90),
            node(2, 60.00, 10.10),
        ]);
        let cedges = vec![
            edge("s", 1, 2, &cnodes, 20_000.0, 80.0),
            edge("sr", 2, 1, &cnodes, 20_000.0, 80.0),
        ];
        let mut coarse = graph(cnodes, cedges);
        let ids: HashSet<i64> = coarse.nodes.keys().map(|id| id.0).collect();
        let wps = [(60.00, 10.00), (60.00, 10.10)];
        let mut atts = Vec::new();
        for (i, wp) in wps.iter().enumerate() {
            let role = if i == 0 {
                SnapRole::Origin
            } else {
                SnapRole::Destination
            };
            atts.push(
                resolve_one_on_graph(&detailed, &ids, *wp, role, &RouteOptions::default(), true)
                    .expect("attach"),
            );
        }
        let plan = TripAttachPlan { attachments: atts };
        assert_eq!(attach_search_count(60.00, 10.00), 1);
        assert_eq!(attach_search_count(60.00, 10.10), 1);
        assert_eq!(total_attach_searches(), 2);
        let snaps = plan.apply(&mut coarse);
        assert_eq!(snaps.len(), 2);
        let mut coarse2 = graph(
            coarse
                .nodes
                .iter()
                .filter(|(id, _)| id.0 == 1 || id.0 == 2)
                .map(|(id, n)| (*id, n.clone()))
                .collect(),
            vec![
                edge("s", 1, 2, &{
                    let mut m = HashMap::new();
                    m.extend([node(1, 60.00, 9.90), node(2, 60.00, 10.10)]);
                    m
                }, 20_000.0, 80.0),
            ],
        );
        let _ = plan.apply(&mut coarse2);
        assert_eq!(
            attach_search_count(60.00, 10.00),
            1,
            "widen/apply must not search a waypoint again"
        );
        assert_eq!(total_attach_searches(), 2);
        if attach_search_count(60.00, 10.00) > 1 {
            panic!("waypoint attached more than once in a plan");
        }
    }
}
