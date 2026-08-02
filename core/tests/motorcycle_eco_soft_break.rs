//! Route-level: motorcycle eco physics ≠ car Passat; soft pause spacing honors
//! CarRestParams hours; truck HOS spacing stays independent.
//!
//! Needs Ostlandet under `core/target/integration-fixtures/ostlandet-latest.osm.pbf`.

use std::path::PathBuf;

use driver_break_core::config::{
    motorcycle_eco_config, CarRestParams, EcoConfig, Profile, RestConfig, TruckRestParams,
};
use driver_break_core::routing::elevation::{ElevationCache, ElevationService};
use driver_break_core::routing::graph::{
    load_or_build_reweighted_bbox, RouteOptions, RoutingProfile,
};
use driver_break_core::routing::{
    motor_break_interval_km, path_eco_energy_breakdown, soft_break_distances_km,
    soft_break_interval_km_fallback, truck_break_interval_km,
};

fn fixture_pbf() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/integration-fixtures/ostlandet-latest.osm.pbf")
}

fn elev_cache_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/integration-fixtures/elevation")
}

/// Lillehammer → Tretten corridor (same as avoid-motorways fixture).
const START: (f64, f64) = (61.1151, 10.4662);
const END: (f64, f64) = (61.3140, 10.3080);

#[test]
fn motorcycle_eco_defaults_differ_from_car_passat() {
    let car = EcoConfig {
        drag_coefficient: 0.28,
        frontal_area_m2: 2.2,
        mass_kg: 1500.0,
        ..EcoConfig::for_profile(Profile::Car)
    };
    let moto = EcoConfig::for_profile(Profile::Motorcycle);
    let moto2 = motorcycle_eco_config(false);
    assert!((moto.mass_kg - moto2.mass_kg).abs() < 1e-9);
    assert!((moto.mass_kg - 220.0).abs() < 1e-9);
    assert!((moto.frontal_area_m2 - 0.60).abs() < 1e-9);
    assert!(moto.mass_kg < car.mass_kg * 0.25);
    assert!(moto.frontal_area_m2 < car.frontal_area_m2 * 0.4);
    // Car Passat overlay must stay untouched by motorcycle constants.
    assert!((car.mass_kg - 1500.0).abs() < 1e-9);
    assert!((car.frontal_area_m2 - 2.2).abs() < 1e-9);
}

#[test]
#[ignore = "needs ostlandet fixture under core/target/integration-fixtures"]
fn lillehammer_tretten_motorcycle_eco_net_below_car() {
    let pbf = fixture_pbf();
    assert!(pbf.is_file(), "missing {}", pbf.display());
    let elev = ElevationService::new(ElevationCache::new(elev_cache_dir()));
    let cache =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-cache/moto-eco-lillehammer");
    let _ = std::fs::create_dir_all(&cache);

    let bbox = [
        START.0.min(END.0) - 0.35,
        START.1.min(END.1) - 0.35,
        START.0.max(END.0) + 0.35,
        START.1.max(END.1) + 0.35,
    ];

    let car_eco = EcoConfig {
        drag_coefficient: 0.28,
        frontal_area_m2: 2.2,
        mass_kg: 1500.0,
        ..EcoConfig::for_profile(Profile::Car)
    };
    let moto_eco = EcoConfig::for_profile(Profile::Motorcycle);

    let (graph_car, _) = load_or_build_reweighted_bbox(
        &pbf,
        &cache.join("car"),
        RoutingProfile::Car,
        &elev,
        &car_eco,
        bbox,
    )
    .expect("car graph");
    let (graph_moto, _) = load_or_build_reweighted_bbox(
        &pbf,
        &cache.join("moto"),
        RoutingProfile::Car, // motorcycle maps to car road graph
        &elev,
        &moto_eco,
        bbox,
    )
    .expect("moto graph");

    let opts = RouteOptions::default();
    let s = graph_car
        .nearest_routable(START.0, START.1)
        .expect("start")
        .0;
    let g = graph_car.nearest_routable(END.0, END.1).expect("end").0;
    let (path, _) = graph_car
        .shortest_path_with_options(s, g, true, &opts)
        .expect("car path");
    assert!(path.len() >= 2);

    // Same geometry for energy compare (car-graph path); physics params differ.
    let car_b = path_eco_energy_breakdown(&graph_car, &path, &elev, &car_eco);
    let moto_b = path_eco_energy_breakdown(&graph_moto, &path, &elev, &moto_eco);

    println!(
        "ROUTE_ECO_COMPARE Lillehammer→Tretten path_nodes={} car_net_j={:.0} moto_net_j={:.0} car_mass={} moto_mass={} car_A={} moto_A={}",
        path.len(),
        car_b.net_energy_j,
        moto_b.net_energy_j,
        car_eco.mass_kg,
        moto_eco.mass_kg,
        car_eco.frontal_area_m2,
        moto_eco.frontal_area_m2,
    );
    assert!(
        moto_b.net_energy_j < car_b.net_energy_j * 0.5,
        "motorcycle eco net ({:.0} J) should be well below car Passat ({:.0} J) on same corridor",
        moto_b.net_energy_j,
        car_b.net_energy_j
    );
    assert!(
        (car_eco.mass_kg - 1500.0).abs() < 1e-6,
        "car Passat mass must remain 1500 kg"
    );
}

#[test]
#[ignore = "needs ostlandet fixture under core/target/integration-fixtures"]
fn soft_pause_spacing_follows_car_rest_hours_on_corridor_eta() {
    let pbf = fixture_pbf();
    assert!(pbf.is_file(), "missing {}", pbf.display());
    let elev = ElevationService::new(ElevationCache::new(elev_cache_dir()));
    let cache =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-cache/soft-break-lillehammer");
    let _ = std::fs::create_dir_all(&cache);

    let bbox = [
        START.0.min(END.0) - 0.35,
        START.1.min(END.1) - 0.35,
        START.0.max(END.0) + 0.35,
        START.1.max(END.1) + 0.35,
    ];
    let eco = EcoConfig::for_profile(Profile::Car);
    let (graph, _) =
        load_or_build_reweighted_bbox(&pbf, &cache, RoutingProfile::Car, &elev, &eco, bbox)
            .expect("graph");
    let s = graph.nearest_routable(START.0, START.1).expect("start").0;
    let g = graph.nearest_routable(END.0, END.1).expect("end").0;
    let (path, _) = graph
        .shortest_path_with_options(s, g, false, &RouteOptions::default())
        .expect("path");

    let mut dist_m = 0.0;
    let mut eta_min = 0.0;
    for w in path.windows(2) {
        let Some(idx) = graph.edge_index(w[0], w[1]) else {
            continue;
        };
        let e = &graph.edges[idx];
        dist_m += e.length_m;
        let spd = e.maxspeed_kmh.filter(|v| *v > 1.0).unwrap_or(50.0).max(5.0);
        eta_min += (e.length_m / 1000.0) / spd * 60.0;
    }
    let dist_km = dist_m / 1000.0;
    assert!(
        dist_km > 20.0,
        "expected multi-tens-km corridor, got {dist_km}"
    );

    let mut rest = RestConfig::default();
    // Short Lillehammer→Tretten corridor (~30 km): use a sub-hour interval so at
    // least one soft pause falls on the path (1 h would exceed route length).
    rest.car.break_interval_min_hours = 0.25;
    rest.car.break_interval_max_hours = 0.25;
    let iv = motor_break_interval_km(Profile::Car, &rest, dist_km, eta_min);
    let speed = dist_km / (eta_min / 60.0).max(1e-6);
    let expect = speed * 0.25;
    assert!(
        (iv - expect).abs() < 0.5,
        "0.25 h soft interval → {expect:.1} km at {speed:.1} km/h, got {iv:.1}"
    );
    let places = soft_break_distances_km(Profile::Car, &rest, dist_km, eta_min);
    assert!(
        !places.is_empty(),
        "0.25 h interval on {dist_km:.1} km / {eta_min:.0} min must place pauses (iv={iv:.1})"
    );
    assert!(
        (places[0] - iv).abs() < 0.1,
        "first pause at {places:?} must match interval {iv}"
    );

    // Fallback when hours invalid.
    rest.car = CarRestParams {
        break_interval_min_hours: 0.0,
        break_interval_max_hours: -1.0,
        ..CarRestParams::default()
    };
    let fb = motor_break_interval_km(Profile::MobileHome, &rest, dist_km, eta_min);
    assert!(
        (fb - soft_break_interval_km_fallback(dist_km)).abs() < 0.1,
        "invalid hours must use fallback, got {fb}"
    );

    // Truck HOS unchanged by car soft hours.
    rest.car.break_interval_min_hours = 0.25;
    rest.car.break_interval_max_hours = 0.25;
    rest.truck = TruckRestParams {
        mandatory_break_after_hours: 4.5,
        ..TruckRestParams::default()
    };
    let truck_iv = motor_break_interval_km(Profile::Truck, &rest, dist_km, eta_min);
    let truck_direct = truck_break_interval_km(&rest.truck, dist_km, eta_min);
    assert!(
        (truck_iv - truck_direct).abs() < 0.1,
        "truck must ignore car soft hours ({truck_iv} vs car soft {iv})"
    );
    assert!(
        (truck_iv - iv).abs() > 1.0,
        "truck HOS spacing must differ from 0.25 h car soft on this trip"
    );

    println!(
        "SOFT_BREAK_ROUTE dist_km={dist_km:.2} eta_min={eta_min:.1} speed_kmh={speed:.1} car_0.25h_iv={iv:.1} places={places:?} truck_iv={truck_iv:.1} fallback={fb:.1}"
    );
}
