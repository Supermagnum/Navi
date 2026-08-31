//! Road network graph construction and elevation reweighting.
//!
//! ## Graph engine choice
//!
//! [`routx`] was evaluated for on-device OSM routing. This pass uses
//! [`osm4routing`] (the Rust rewrite of osm4routing2) plus [`pathfinding`] because:
//!
//! - We need a post-build edge reweighting pass for eco-mode energy costs.
//! - `osm4routing` exposes full edge geometry and tags for custom profile filtering.
//! - Edge weights are mutable in our adjacency structure after the initial graph build.
//!
//! `routx` remains a valid alternative if its profile/tag API grows; swap the builder
//! in [`builder`] without changing elevation or rest logic.

mod bbox_build;
mod bike_suitability;
mod builder;
mod cache;
mod network_pref;
mod reweight;
mod road_near;
mod surface_quality;

pub use bbox_build::TiledBuildTimings;
pub use bike_suitability::{
    apply_bike_suitability, apply_bike_suitability_from_pbf, load_way_terrain_tags,
    tags_unsuitable_for, way_id_from_edge_id, BikeCapability,
};
pub use builder::{
    append_seasonal_closure_report, edge_is_motorway_grade, format_route_avoidance_report,
    highway_is_motorway, max_waypoint_snap_m, GraphEdge, RouteGraph, RouteOptions, RoutingProfile,
    SnapTooFar, WetlandApplyStats,
};
pub use cache::{
    graph_cache_path, load_or_build_reweighted, load_or_build_reweighted_bbox,
    load_reweighted_graph, save_reweighted_graph, GraphCacheFingerprint,
};
pub use network_pref::{
    apply_official_network_preference, apply_slow_road_preference, difficulty_notes_for_path,
    is_official_route_relation, is_pilgrim_route_relation, load_named_route_entries,
    load_official_network_way_ids, load_pilgrim_route_way_ids, load_way_difficulty_tags,
    slow_road_edge_multiplier, NamedRouteEntry, OfficialNetworkKind, HIGH_SPEED_ROAD_KMH,
    HIGH_SPEED_ROAD_PENALTY, NON_NETWORK_PENALTY,
};
pub use reweight::reweight_graph_for_eco;
pub use road_near::{
    edge_distance_m, nearest_road_hit, nearest_road_label, NearestRoadHit, RoadLabelSticky,
    RoadNodeIndex,
};
pub use surface_quality::{
    apply_surface_preference, apply_surface_quality_from_pbf, best_incident_surface,
    classify_surface_tags, edge_surface_multiplier, infer_surface_from_highway,
    surface_transition_cost_m, worst_incident_surface, SurfaceQuality, SurfaceRoutingMode,
    SNAP_VIRTUAL_APPROACH_SURFACE, SURFACE_MARGINAL_EDGE_PENALTY, SURFACE_POOR_EDGE_PENALTY,
    SURFACE_TRANSITION_MAX_CLASS_DROP, SURFACE_TRANSITION_PENALTY_M,
};
