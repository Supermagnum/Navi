use rayon::prelude::*;

use crate::config::EcoConfig;
use crate::ecu::{refine_energy_cost, LiveEnergySnapshot};
use crate::routing::elevation::ElevationService;

use super::builder::RouteGraph;

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
    graph.edges.par_iter_mut().for_each(|edge| {
        let h_start = elevation.get_elevation(edge.start_lat, edge.start_lon);
        let h_end = elevation.get_elevation(edge.end_lat, edge.end_lon);
        let delta_h = match (h_start, h_end) {
            (Some(a), Some(b)) => b - a,
            _ => {
                edge.eco_weight = None;
                return;
            }
        };
        let predicted = eco.segment_energy_joules(edge.length_m, delta_h);
        let refined = refine_energy_cost(predicted, edge.length_m, live);
        edge.eco_weight = Some(refined.max(edge.base_weight * 0.01));
    });
}
