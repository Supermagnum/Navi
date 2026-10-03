use rayon::prelude::*;

use crate::config::EcoConfig;
use crate::ecu::{refine_energy_cost, LiveEnergySnapshot};
use crate::routing::elevation::ElevationService;

use super::builder::{GraphEdge, RouteGraph};

fn eco_joules_for_edge(
    edge: &GraphEdge,
    elevation: &ElevationService,
    eco: &EcoConfig,
    live: Option<&LiveEnergySnapshot>,
) -> f64 {
    let h_start = elevation.get_elevation(edge.start_lat, edge.start_lon);
    let h_end = elevation.get_elevation(edge.end_lat, edge.end_lon);
    let predicted = match (h_start, h_end) {
        (Some(a), Some(b)) => eco.segment_energy_joules(edge.length_m, b - a),
        // Keep units in joules. Falling back to `length_m` made uncovered
        // edges ~200x cheaper than DEM-covered ones and invited long detours.
        _ => eco.flat_energy_joules(edge.length_m),
    };
    refine_energy_cost(predicted, edge.length_m, live).max(0.0)
}

/// Per-plan eco overlay (parallel to `graph.edges`) — does not mutate the graph.
///
/// Prefer this on pack-hit corridors shared via [`std::sync::Arc`] so the
/// corridor LRU stays read-only.
pub fn compute_eco_weights(
    graph: &RouteGraph,
    elevation: &ElevationService,
    eco: &EcoConfig,
) -> Vec<f64> {
    compute_eco_weights_with_live(graph, elevation, eco, None)
}

pub fn compute_eco_weights_with_live(
    graph: &RouteGraph,
    elevation: &ElevationService,
    eco: &EcoConfig,
    live: Option<&LiveEnergySnapshot>,
) -> Vec<f64> {
    graph
        .edges
        .par_iter()
        .map(|edge| eco_joules_for_edge(edge, elevation, eco, live))
        .collect()
}

/// Post-processing pass: fold elevation delta into edge weight using energy model.
pub fn reweight_graph_for_eco(
    graph: &mut RouteGraph,
    elevation: &ElevationService,
    eco: &EcoConfig,
) {
    reweight_graph_for_eco_with_live(graph, elevation, eco, None);
}

pub fn reweight_graph_for_eco_with_live(
    graph: &mut RouteGraph,
    elevation: &ElevationService,
    eco: &EcoConfig,
    live: Option<&LiveEnergySnapshot>,
) {
    // Prefer calling `elevation.warm_bbox(...)` first for the route corridor so
    // parallel workers hit the read-lock fast path instead of contending on loads.
    let weights = compute_eco_weights_with_live(graph, elevation, eco, live);
    for (edge, w) in graph.edges.iter_mut().zip(weights) {
        edge.eco_weight = Some(w);
    }
}
