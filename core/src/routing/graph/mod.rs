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

mod builder;
mod cache;
mod reweight;

pub use builder::{RouteGraph, RouteOptions, RoutingProfile};
pub use cache::{
    graph_cache_path, load_or_build_reweighted, load_reweighted_graph, save_reweighted_graph,
    GraphCacheFingerprint,
};
pub use reweight::reweight_graph_for_eco;
