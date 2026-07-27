//! Path-level e-bike / EV range and e-bike climb checks against elevation + eco energy.

use osm4routing::NodeId;

use crate::config::{
    climb_capability_for, default_motor_efficiency, ev_car_range_estimate, range_estimate,
    EbikeClimbCapability, EbikeConfig, EbikeRangeEstimate, EcoConfig, EvCarConfig,
};
use crate::routing::elevation::ElevationService;
use crate::routing::graph::RouteGraph;

/// Sum mechanical energy (J) along `path` using the same eco model as reweighting.
pub fn path_mechanical_energy_j(
    graph: &RouteGraph,
    path: &[NodeId],
    elevation: &ElevationService,
    eco: &EcoConfig,
) -> f64 {
    let mut total = 0.0;
    for w in path.windows(2) {
        let Some(idx) = graph.edge_index(w[0], w[1]) else {
            continue;
        };
        let e = &graph.edges[idx];
        let delta_h = match (
            elevation.get_elevation(e.start_lat, e.start_lon),
            elevation.get_elevation(e.end_lat, e.end_lon),
        ) {
            (Some(a), Some(b)) => b - a,
            _ => 0.0,
        };
        total += eco.segment_energy_joules(e.length_m, delta_h);
    }
    total
}

#[derive(Debug, Clone)]
pub struct SteepSegment {
    pub along_m: f64,
    pub length_m: f64,
    pub grade_pct: f64,
}

/// Maximum climb grade (percent) on path edges with DEM and length ≥ 15 m.
pub fn path_max_climb_grade_pct(
    graph: &RouteGraph,
    path: &[NodeId],
    elevation: &ElevationService,
) -> f64 {
    let mut max_pct: f64 = 0.0;
    for w in path.windows(2) {
        let Some(idx) = graph.edge_index(w[0], w[1]) else {
            continue;
        };
        let e = &graph.edges[idx];
        let len = e.length_m.max(1e-3);
        if len < 15.0 {
            continue;
        }
        if let (Some(a), Some(b)) = (
            elevation.get_elevation(e.start_lat, e.start_lon),
            elevation.get_elevation(e.end_lat, e.end_lon),
        ) {
            let delta = b - a;
            if delta > 0.0 {
                max_pct = max_pct.max((delta / len) * 100.0);
            }
        }
    }
    max_pct
}

/// True when climb grade (rise/run) exceeds capability (tiny stubs ignored).
pub fn grade_exceeds_capability(delta_h_m: f64, length_m: f64, max_grade_fraction: f64) -> bool {
    let min_len = 15.0;
    let len = length_m.max(1e-3);
    delta_h_m > 0.0 && len >= min_len && (delta_h_m / len) > max_grade_fraction + 1e-9
}

/// Edges whose grade (rise/run) exceeds `max_grade_fraction`.
pub fn steep_segments_over_capability(
    graph: &RouteGraph,
    path: &[NodeId],
    elevation: &ElevationService,
    max_grade_fraction: f64,
) -> Vec<SteepSegment> {
    let mut out = Vec::new();
    let mut along = 0.0;
    for w in path.windows(2) {
        let Some(idx) = graph.edge_index(w[0], w[1]) else {
            continue;
        };
        let e = &graph.edges[idx];
        let len = e.length_m.max(1e-3);
        if let (Some(a), Some(b)) = (
            elevation.get_elevation(e.start_lat, e.start_lon),
            elevation.get_elevation(e.end_lat, e.end_lon),
        ) {
            let delta = b - a;
            if grade_exceeds_capability(delta, len, max_grade_fraction) {
                out.push(SteepSegment {
                    along_m: along,
                    length_m: len,
                    grade_pct: (delta / len) * 100.0,
                });
            }
        }
        along += e.length_m;
    }
    out
}

pub fn analyze_ebike_route(
    graph: &RouteGraph,
    path: &[NodeId],
    elevation: &ElevationService,
    eco: &EcoConfig,
    ebike: &EbikeConfig,
) -> (EbikeRangeEstimate, EbikeClimbCapability, Vec<SteepSegment>) {
    let energy = path_mechanical_energy_j(graph, path, elevation, eco);
    let range = range_estimate(
        energy,
        ebike.battery_wh_or_default(),
        default_motor_efficiency(),
    );
    let climb = climb_capability_for(ebike, eco);
    let steep = steep_segments_over_capability(graph, path, elevation, climb.max_grade_fraction);
    (range, climb, steep)
}

pub fn format_ebike_route_report(
    range: &EbikeRangeEstimate,
    climb: &EbikeClimbCapability,
    steep: &[SteepSegment],
) -> String {
    format_ebike_route_report_with_path_grade(range, climb, steep, None)
}

pub fn format_ebike_route_report_with_path_grade(
    range: &EbikeRangeEstimate,
    climb: &EbikeClimbCapability,
    steep: &[SteepSegment],
    path_max_grade_pct: Option<f64>,
) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "ebike_battery_wh={:.0}; ebike_draw_wh={:.1}; ebike_pct_of_capacity={:.1}; ebike_motor_eff={:.2}; ebike_mech_j={:.0}\n",
        range.battery_capacity_wh,
        range.battery_draw_wh,
        range.pct_of_capacity,
        range.motor_efficiency,
        range.mechanical_energy_j
    ));
    s.push_str(&format!(
        "ebike_tractive_n={:.1}; ebike_max_grade_pct={:.1}\n",
        climb.tractive_force_n, climb.max_grade_pct
    ));
    if let Some(g) = path_max_grade_pct {
        s.push_str(&format!("ebike_path_max_grade_pct={g:.2}\n"));
    }
    s.push_str(&format!(
        "Route uses an estimated {:.0}% of battery capacity (η={:.0}% motor; estimate, not measured).\n",
        range.pct_of_capacity,
        range.motor_efficiency * 100.0
    ));
    if steep.is_empty() {
        s.push_str("ebike_climb_ok=true\n");
    } else {
        s.push_str(&format!(
            "ebike_climb_ok=false; ebike_steep_segments={}\n",
            steep.len()
        ));
        let worst = steep
            .iter()
            .max_by(|a, b| a.grade_pct.partial_cmp(&b.grade_pct).unwrap())
            .unwrap();
        s.push_str(&format!(
            "WARNING: this route includes a segment steeper than your bike can climb under motor assist alone (max ~{:.0}% grade; saw ~{:.0}% over {:.0} m) — expect to dismount/push.\n",
            climb.max_grade_pct,
            worst.grade_pct,
            worst.length_m
        ));
    }
    s
}

/// EV car pack range check (no climb-capability warning).
pub fn analyze_ev_car_route(
    graph: &RouteGraph,
    path: &[NodeId],
    elevation: &ElevationService,
    eco: &EcoConfig,
    ev: &EvCarConfig,
) -> EbikeRangeEstimate {
    let energy = path_mechanical_energy_j(graph, path, elevation, eco);
    ev_car_range_estimate(energy, ev)
}

pub fn format_ev_car_route_report(range: &EbikeRangeEstimate) -> String {
    let kwh = range.battery_capacity_wh / 1000.0;
    let draw_kwh = range.battery_draw_wh / 1000.0;
    let mut s = String::new();
    s.push_str(&format!(
        "ev_battery_kwh={kwh:.1}; ev_draw_kwh={draw_kwh:.3}; ev_pct_of_capacity={:.1}; ev_motor_eff={:.2}; ev_mech_j={:.0}\n",
        range.pct_of_capacity,
        range.motor_efficiency,
        range.mechanical_energy_j
    ));
    s.push_str(&format!(
        "Route uses an estimated {:.0}% of battery capacity (η={:.0}% drivetrain; estimate, not measured).\n",
        range.pct_of_capacity,
        range.motor_efficiency * 100.0
    ));
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{climb_capability_for, ebike_eco_config, range_estimate, EbikeConfig};

    #[test]
    fn regen_reduces_battery_draw_on_undulating_route() {
        let with = ebike_eco_config(true);
        let without = ebike_eco_config(false);
        assert!(with.regen_efficiency > 0.0);
        assert_eq!(without.regen_efficiency, 0.0);
        let d = 1_000.0;
        let mech_regen = with.segment_energy_joules(d, 80.0) + with.segment_energy_joules(d, -80.0);
        let mech_no =
            without.segment_energy_joules(d, 80.0) + without.segment_energy_joules(d, -80.0);
        assert!(
            mech_regen < mech_no,
            "regen={mech_regen} should be < no-regen={mech_no}"
        );
        let pct_regen = range_estimate(mech_regen, 800.0, 0.8).pct_of_capacity;
        let pct_no = range_estimate(mech_no, 800.0, 0.8).pct_of_capacity;
        assert!(pct_regen < pct_no, "pct regen={pct_regen} vs no={pct_no}");
    }

    #[test]
    fn range_pct_changes_when_battery_capacity_edited() {
        let mech = ebike_eco_config(true).segment_energy_joules(5_000.0, 200.0);
        let at_800 = range_estimate(mech, 800.0, 0.8).pct_of_capacity;
        let at_400 = range_estimate(mech, 400.0, 0.8).pct_of_capacity;
        assert!((at_400 - 2.0 * at_800).abs() < 1e-6);
    }

    #[test]
    fn climb_warning_fires_only_above_computed_max() {
        let ebike = EbikeConfig::default();
        let eco = ebike_eco_config(true);
        let climb = climb_capability_for(&ebike, &eco);
        let mild_len = 100.0;
        let mild_dh = climb.max_grade_fraction * mild_len * 0.5;
        assert!(!grade_exceeds_capability(
            mild_dh,
            mild_len,
            climb.max_grade_fraction
        ));
        let steep_dh = 40.0;
        let steep_len = 100.0;
        assert!(
            steep_dh / steep_len > climb.max_grade_fraction,
            "fixture grade must exceed max {:.1}%",
            climb.max_grade_pct
        );
        assert!(grade_exceeds_capability(
            steep_dh,
            steep_len,
            climb.max_grade_fraction
        ));
    }

    #[test]
    fn format_report_includes_capacity_and_climb_warning() {
        let range = range_estimate(1_440_000.0, 800.0, 0.8);
        let climb = climb_capability_for(&EbikeConfig::default(), &ebike_eco_config(true));
        let steep = vec![SteepSegment {
            along_m: 1_000.0,
            length_m: 80.0,
            grade_pct: 35.0,
        }];
        let s = format_ebike_route_report(&range, &climb, &steep);
        assert!(s.contains("ebike_pct_of_capacity="));
        assert!(s.contains("Route uses an estimated"));
        assert!(s.contains("ebike_climb_ok=false"));
        assert!(s.contains("WARNING:"));
    }

    #[test]
    fn format_ev_car_report_uses_kwh() {
        let range = ev_car_range_estimate(
            21_600_000.0,
            &EvCarConfig {
                battery_capacity_kwh: Some(60.0),
            },
        );
        let s = format_ev_car_route_report(&range);
        assert!(s.contains("ev_battery_kwh=60.0"));
        assert!(s.contains("ev_pct_of_capacity="));
    }
}
