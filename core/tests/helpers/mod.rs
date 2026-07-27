//! Shared helpers for fixture-heavy integration tests (`#[ignore]`).
//!
//! Many helpers are only referenced from ignored tests that Clippy still
//! type-checks under `--all-targets`, so unused-item noise is expected.
#![allow(
    dead_code,
    clippy::uninlined_format_args,
    clippy::inherent_to_string,
    clippy::redundant_closure
)]

pub mod hiking;

use std::fs;
use std::path::{Path, PathBuf};

use driver_break_core::config::EcoConfig;
use driver_break_core::poi::{PoiCategory, PoiIndex, PoiRecord};
use driver_break_core::routing::elevation::ElevationService;
use driver_break_core::routing::graph::RouteGraph;
use osm4routing::NodeId;

#[derive(Debug, Clone)]
pub struct RouteMetrics {
    pub distance_m: f64,
    pub total_climb_m: f64,
    pub total_descent_m: f64,
    pub energy_j: f64,
    pub flat_weight_sum: f64,
}

#[derive(Debug, Clone)]
pub struct PoiHit {
    pub osm_id: i64,
    pub name: Option<String>,
    pub category: PoiCategory,
    pub lat: f64,
    pub lon: f64,
    pub distance_from_sample_m: f64,
    pub icon_key: String,
}

pub struct TestReport {
    title: String,
    lines: Vec<String>,
}

impl TestReport {
    pub fn new() -> Self {
        Self::with_title("Integration Report")
    }

    pub fn with_title(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            lines: Vec::new(),
        }
    }

    pub fn section(&mut self, title: &str) {
        self.lines.push(String::new());
        self.lines.push(format!("## {title}"));
    }

    pub fn line(&mut self, s: &str) {
        self.lines.push(s.to_string());
    }

    pub fn log_route_metrics(&mut self, label: &str, m: &RouteMetrics, path_cost: f64) {
        self.line(&format!("{label}:"));
        self.line(&format!("  Distance: {:.2} km", m.distance_m / 1000.0));
        self.line(&format!("  Total climb: {:.0} m", m.total_climb_m));
        self.line(&format!("  Total descent: {:.0} m", m.total_descent_m));
        self.line(&format!("  Energy (physics): {:.0} J", m.energy_j));
        self.line(&format!("  Path cost (router): {:.0}", path_cost));
    }

    pub fn to_string(&self) -> String {
        let mut out = format!("# {}\n", self.title);
        for l in &self.lines {
            out.push_str(l);
            out.push('\n');
        }
        out
    }

    pub fn write(&self, path: &Path) -> anyhow::Result<()> {
        fs::write(path, self.to_string())?;
        Ok(())
    }
}

impl Default for TestReport {
    fn default() -> Self {
        Self::new()
    }
}

pub fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6_378_100.0;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

pub fn nearest_node(graph: &RouteGraph, lat: f64, lon: f64) -> (NodeId, f64, f64) {
    use rayon::prelude::*;
    let best = graph
        .nodes
        .par_iter()
        .map(|(_, node)| {
            let d = haversine_m(lat, lon, node.coord.y, node.coord.x);
            (node.id, node.coord.y, node.coord.x, d)
        })
        .min_by(|a, b| a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal))
        .expect("empty graph");
    (best.0, best.1, best.2)
}

pub fn path_edge_indices(graph: &RouteGraph, path: &[NodeId]) -> Vec<usize> {
    let mut out = Vec::with_capacity(path.len().saturating_sub(1));
    for w in path.windows(2) {
        if let Some(idx) = graph.edge_index(w[0], w[1]) {
            out.push(idx);
        }
    }
    out
}

pub fn route_metrics(
    graph: &RouteGraph,
    edge_indices: &[usize],
    elevation: &ElevationService,
    eco: &EcoConfig,
    use_eco: bool,
) -> RouteMetrics {
    let mut distance_m = 0.0;
    let mut total_climb_m = 0.0;
    let mut total_descent_m = 0.0;
    let mut energy_j = 0.0;
    let mut flat_weight_sum = 0.0;

    for &idx in edge_indices {
        let e = &graph.edges[idx];
        distance_m += e.length_m;
        flat_weight_sum += e.base_weight;

        let h0 = elevation.get_elevation(e.start_lat, e.start_lon);
        let h1 = elevation.get_elevation(e.end_lat, e.end_lon);
        if let (Some(a), Some(b)) = (h0, h1) {
            let dh = b - a;
            if dh > 0.0 {
                total_climb_m += dh;
            } else {
                total_descent_m += -dh;
            }
            if use_eco {
                energy_j += eco.segment_energy_joules(e.length_m, dh);
            }
        } else if use_eco {
            energy_j += e.eco_weight.unwrap_or(e.base_weight);
        }
    }

    RouteMetrics {
        distance_m,
        total_climb_m,
        total_descent_m,
        energy_j,
        flat_weight_sum,
    }
}

pub fn compare_paths(a: &[NodeId], b: &[NodeId]) -> bool {
    a == b
}

pub fn sample_route_points(graph: &RouteGraph, path: &[NodeId], spacing_m: f64) -> Vec<(f64, f64)> {
    let mut pts = Vec::new();
    let mut acc = 0.0;
    for w in path.windows(2) {
        let n0 = &graph.nodes[&w[0]];
        let n1 = &graph.nodes[&w[1]];
        let seg = haversine_m(n0.coord.y, n0.coord.x, n1.coord.y, n1.coord.x);
        if seg < 1.0 {
            continue;
        }
        let mut t = if acc == 0.0 { 0.0 } else { spacing_m - acc };
        while t <= seg {
            let frac = (t / seg).clamp(0.0, 1.0);
            let lat = n0.coord.y + (n1.coord.y - n0.coord.y) * frac;
            let lon = n0.coord.x + (n1.coord.x - n0.coord.x) * frac;
            pts.push((lat, lon));
            t += spacing_m;
        }
        acc = (acc + seg) % spacing_m;
    }
    if pts.is_empty() {
        if let Some(n) = path.first().and_then(|id| graph.nodes.get(id)) {
            pts.push((n.coord.y, n.coord.x));
        }
    }
    pts
}

pub fn car_required_breaks(driving_hours: f64, max_interval_hours: f64) -> u32 {
    if driving_hours <= max_interval_hours {
        0
    } else {
        (driving_hours / max_interval_hours).floor() as u32
    }
}

/// Query POIs from two regional indices (Hedmark + Oppland extracts).
pub struct CombinedPoiIndex {
    indices: Vec<PoiIndex>,
}

impl CombinedPoiIndex {
    pub fn load(paths: &[PathBuf]) -> anyhow::Result<Self> {
        use rayon::prelude::*;
        let indices: Result<Vec<_>, _> = paths
            .par_iter()
            .map(|p| PoiIndex::load_from_pbf(p))
            .collect();
        Ok(Self { indices: indices? })
    }

    pub fn total_len(&self) -> usize {
        self.indices.iter().map(|i| i.len()).sum()
    }

    pub fn nearest(
        &self,
        category: PoiCategory,
        lat: f64,
        lon: f64,
        radius_m: f64,
    ) -> Vec<PoiRecord> {
        let mut out: Vec<PoiRecord> = Vec::new();
        for idx in &self.indices {
            for poi in idx.nearest(category, lat, lon, radius_m) {
                if out.iter().any(|p| p.osm_id == poi.osm_id) {
                    continue;
                }
                out.push(poi.clone());
            }
        }
        out
    }
}
