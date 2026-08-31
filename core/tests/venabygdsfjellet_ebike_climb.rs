//! Real-corridor check: Ringebu / Venabygdsfjellet Electric Cycle climb + range,
//! plus Electric Car pack range on the same path geometry / DEM.
//!
//! Run: `cargo test -p driver-break-core --test venabygdsfjellet_ebike_climb -- --nocapture --ignored`

mod helpers;

use std::fs;
use std::path::PathBuf;

use driver_break_core::config::{
    climb_capability_for, ebike_eco_config, EbikeConfig, EcoConfig, EvCarConfig, Profile,
};
use driver_break_core::routing::elevation::{ElevationCache, ElevationService};
use driver_break_core::routing::graph::{load_or_build_reweighted_bbox, RoutingProfile};
use driver_break_core::routing::{
    analyze_ebike_route, analyze_ev_car_route, format_ebike_route_report_with_path_grade,
    format_ev_car_route_report, path_max_climb_grade_pct, path_mechanical_energy_j,
};
use helpers::TestReport;
use osm4routing::NodeId;

const START_LAT: f64 = 61.225_799_5;
const START_LON: f64 = 10.462_604_4;
const END_LAT: f64 = 61.225_276_7;
const END_LON: f64 = 10.546_839_4;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/integration-fixtures")
}

fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6_378_100.0;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

fn nearest(graph: &driver_break_core::RouteGraph, lat: f64, lon: f64) -> NodeId {
    graph
        .nodes
        .values()
        .min_by(|a, b| {
            let da = haversine_m(lat, lon, a.coord.y, a.coord.x);
            let db = haversine_m(lat, lon, b.coord.y, b.coord.x);
            da.partial_cmp(&db).unwrap()
        })
        .map(|n| n.id)
        .expect("empty graph")
}

fn pad_bbox(min_lat: f64, min_lon: f64, max_lat: f64, max_lon: f64, pad: f64) -> [f64; 4] {
    [min_lat - pad, min_lon - pad, max_lat + pad, max_lon + pad]
}

#[test]
#[ignore = "needs ostlandet/oppland/espa PBF + DEM under integration-fixtures"]
fn venabygdsfjellet_ebike_climb_and_ev_car_range() {
    let root = fixture_dir();
    let pbf = [
        root.join("oppland-latest.osm.pbf"),
        root.join("ostlandet-latest.osm.pbf"),
        root.join("espa-atnbrufossen-corridor.osm.pbf"),
    ]
    .into_iter()
    .find(|p| p.is_file())
    .expect("oppland, ostlandet, or espa corridor PBF");
    let elev = root.join("elevation");
    let cache = root.join("graph-cache-venabygdsfjellet");
    let _ = fs::create_dir_all(&elev);
    let _ = fs::create_dir_all(&cache);

    let elevation = ElevationService::new(ElevationCache::new(&elev));
    assert!(
        elevation.get_elevation(START_LAT, START_LON).is_some()
            || elevation.get_elevation(END_LAT, END_LON).is_some(),
        "DEM missing for Ringebu corridor — expected N61E010 under elevation/"
    );

    let bbox = pad_bbox(
        START_LAT.min(END_LAT),
        START_LON.min(END_LON),
        START_LAT.max(END_LAT),
        START_LON.max(END_LON),
        0.08,
    );

    let mut report = TestReport::with_title("Venabygdsfjellet e-bike climb / EV car range");
    report.line(&format!("pbf={}", pbf.display()));
    report.line(&format!(
        "start={START_LAT},{START_LON}; end={END_LAT},{END_LON}"
    ));

    // --- Electric Cycle path (bicycle graph) ---
    let eco_bike = ebike_eco_config(true);
    let (graph_bike, hit) = load_or_build_reweighted_bbox(
        &pbf,
        &cache,
        &cache,
        RoutingProfile::Bicycle,
        &elevation,
        &eco_bike,
        bbox,
    )
    .expect("bicycle graph");
    report.line(&format!(
        "bicycle_cache_hit={hit}; nodes={}; edges={}",
        graph_bike.nodes.len(),
        graph_bike.edges.len()
    ));

    let s = nearest(&graph_bike, START_LAT, START_LON);
    let g = nearest(&graph_bike, END_LAT, END_LON);
    let (path, _, _cost) = graph_bike
        .shortest_path(s, g, true)
        .expect("ebike route Venabygdsfjellet");
    assert!(path.len() >= 2, "path too short");

    let mut distance_m = 0.0;
    for w in path.windows(2) {
        if let Some(idx) = graph_bike.edge_index(w[0], w[1]) {
            distance_m += graph_bike.edges[idx].length_m;
        }
    }
    let dist_km = distance_m / 1000.0;
    report.section("Electric Cycle — default 800 Wh / 85 Nm / 27.5\"");
    report.line(&format!(
        "distance_km={dist_km:.3}; path_nodes={}",
        path.len()
    ));

    let path_max = path_max_climb_grade_pct(&graph_bike, &path, &elevation);
    report.line(&format!("path_max_climb_grade_pct={path_max:.2}"));

    let dem_hits = path
        .windows(2)
        .filter(|w| {
            graph_bike.edge_index(w[0], w[1]).is_some_and(|i| {
                let e = &graph_bike.edges[i];
                elevation.get_elevation(e.start_lat, e.start_lon).is_some()
                    && elevation.get_elevation(e.end_lat, e.end_lon).is_some()
            })
        })
        .count();
    report.line(&format!(
        "dem_covered_edges={dem_hits}/{}",
        path.len().saturating_sub(1)
    ));
    assert!(
        dem_hits > 0,
        "no DEM samples on path — climb/range not using real elevation"
    );

    let default_ebike = EbikeConfig::default();
    let (range, climb, steep) =
        analyze_ebike_route(&graph_bike, &path, &elevation, &eco_bike, &default_ebike);
    let default_report =
        format_ebike_route_report_with_path_grade(&range, &climb, &steep, Some(path_max));
    report.line(&format!(
        "default_max_capability_grade_pct={:.1}; tractive_n={:.1}",
        climb.max_grade_pct, climb.tractive_force_n
    ));
    report.line(&format!(
        "default_battery_pct={:.1}; draw_wh={:.1}; steep_segments={}",
        range.pct_of_capacity,
        range.battery_draw_wh,
        steep.len()
    ));
    report.line("--- plan report (default) ---");
    for line in default_report.lines() {
        report.line(line);
    }

    // Reduced torque: warning threshold must shift (more steep segments, or newly fail).
    let weak = EbikeConfig {
        battery_capacity_wh: Some(800.0),
        motor_torque_nm: Some(40.0),
        wheel_diameter_in: Some(27.5),
    };
    let (range_w, climb_w, steep_w) =
        analyze_ebike_route(&graph_bike, &path, &elevation, &eco_bike, &weak);
    report.section("Electric Cycle — reduced torque 40 Nm");
    report.line(&format!(
        "weak_max_capability_grade_pct={:.1}; tractive_n={:.1}; steep_segments={}",
        climb_w.max_grade_pct,
        climb_w.tractive_force_n,
        steep_w.len()
    ));
    assert!(
        climb_w.max_grade_pct < climb.max_grade_pct,
        "40 Nm must lower max climbable grade vs 85 Nm"
    );
    assert!(
        steep_w.len() >= steep.len(),
        "weaker motor must not flag fewer steep segments (got {} vs {})",
        steep_w.len(),
        steep.len()
    );
    // If path max grade exceeds weak capability, warning must fire.
    if path_max > climb_w.max_grade_pct {
        assert!(
            !steep_w.is_empty(),
            "path max grade {:.2}% exceeds weak max {:.1}% but no steep segments flagged",
            path_max,
            climb_w.max_grade_pct
        );
    }
    // Default: do not spuriously warn when path stays under capability.
    if path_max <= climb.max_grade_pct + 1e-6 {
        assert!(
            steep.is_empty(),
            "spurious climb warning under default capability (path_max={path_max:.2}%, max={:.1}%)",
            climb.max_grade_pct
        );
    }

    // Regen credit on this real elevation profile.
    let eco_no_regen = ebike_eco_config(false);
    let j_regen = path_mechanical_energy_j(&graph_bike, &path, &elevation, &eco_bike);
    let j_no = path_mechanical_energy_j(&graph_bike, &path, &elevation, &eco_no_regen);
    report.section("Regen on real DEM profile");
    report.line(&format!(
        "mech_j_regen={j_regen:.0}; mech_j_no_regen={j_no:.0}"
    ));
    assert!(
        j_regen <= j_no + 1.0,
        "regen must not increase mechanical energy (regen={j_regen} no={j_no})"
    );
    // Meaningful descent credit only when there is descent PE to recover.
    let mut descent_m = 0.0;
    for w in path.windows(2) {
        if let Some(idx) = graph_bike.edge_index(w[0], w[1]) {
            let e = &graph_bike.edges[idx];
            if let (Some(a), Some(b)) = (
                elevation.get_elevation(e.start_lat, e.start_lon),
                elevation.get_elevation(e.end_lat, e.end_lon),
            ) {
                if b < a {
                    descent_m += a - b;
                }
            }
        }
    }
    report.line(&format!("total_descent_m={descent_m:.1}"));
    if descent_m > 20.0 {
        assert!(
            j_regen < j_no,
            "with {descent_m:.0} m descent, regen energy must be strictly lower"
        );
        let pct_r = analyze_ebike_route(&graph_bike, &path, &elevation, &eco_bike, &default_ebike)
            .0
            .pct_of_capacity;
        let pct_n = analyze_ebike_route(
            &graph_bike,
            &path,
            &elevation,
            &eco_no_regen,
            &default_ebike,
        )
        .0
        .pct_of_capacity;
        report.line(&format!(
            "battery_pct_regen={pct_r:.1}; battery_pct_no_regen={pct_n:.1}"
        ));
        assert!(pct_r < pct_n);
    }

    report.line(&format!(
        "weak_battery_pct={:.1} (capacity unchanged; energy same)",
        range_w.pct_of_capacity
    ));
    assert!(
        (range_w.pct_of_capacity - range.pct_of_capacity).abs() < 1e-6,
        "torque must not change battery % for same path energy"
    );

    // --- Electric Car range on car graph over same endpoints ---
    report.section("Electric Car — 60 kWh default pack range");
    let eco_car = EcoConfig::for_profile(Profile::CarElectric);
    let mut eco_car = eco_car;
    eco_car.drag_coefficient = 0.28;
    eco_car.frontal_area_m2 = 2.2;
    eco_car.mass_kg = 1500.0;
    let (graph_car, hit_car) = load_or_build_reweighted_bbox(
        &pbf,
        &cache,
        &cache,
        RoutingProfile::Car,
        &elevation,
        &eco_car,
        bbox,
    )
    .expect("car graph");
    report.line(&format!(
        "car_cache_hit={hit_car}; nodes={}; edges={}",
        graph_car.nodes.len(),
        graph_car.edges.len()
    ));
    let sc = nearest(&graph_car, START_LAT, START_LON);
    let gc = nearest(&graph_car, END_LAT, END_LON);
    let (path_car, _, _) = graph_car
        .shortest_path(sc, gc, true)
        .expect("car route Venabygdsfjellet");
    let ev = EvCarConfig::default();
    let ev_range = analyze_ev_car_route(&graph_car, &path_car, &elevation, &eco_car, &ev);
    let ev_report = format_ev_car_route_report(&ev_range);
    report.line(&format!(
        "ev_battery_pct={:.1}; draw_kwh={:.3}",
        ev_range.pct_of_capacity,
        ev_range.battery_draw_wh / 1000.0
    ));
    for line in ev_report.lines() {
        report.line(line);
    }
    assert!(ev_range.pct_of_capacity > 0.0);
    assert!(
        ev_range.pct_of_capacity < 50.0,
        "short mountain hop should be << 50% of 60 kWh"
    );

    let out = root.join("venabygdsfjellet_ebike_climb_report.md");
    report.write(&out).expect("write report");
    eprintln!("{}", report.to_string());
    eprintln!("wrote {}", out.display());

    // Keep climb_capability_for referenced for clarity in report path.
    let _ = climb_capability_for(&weak, &eco_bike);
}
