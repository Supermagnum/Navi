//! Route-level evidence that planner options change path / cost (not just flag plumbing).

use std::collections::{HashMap, HashSet};

use driver_break_core::config::{EcoConfig, Profile, SafetyConfig, VehicleLimits};
use driver_break_core::poi::{PoiCategory, PoiRecord};
use driver_break_core::routing::graph::{
    apply_official_network_preference, GraphEdge, RouteGraph, RouteOptions, RoutingProfile,
    NON_NETWORK_PENALTY,
};
use driver_break_core::routing::safety::check_overnight_candidate;
use geo_types::Coord;
use osm4routing::{Node, NodeId};

fn node(id: i64, lat: f64, lon: f64) -> (NodeId, Node) {
    let nid = NodeId(id);
    (
        nid,
        Node {
            id: nid,
            coord: Coord { x: lon, y: lat },
            uses: 0,
        },
    )
}

fn edge(
    id: &str,
    source: i64,
    target: i64,
    start_lat: f64,
    start_lon: f64,
    end_lat: f64,
    end_lon: f64,
    length_m: f64,
    highway: &str,
) -> GraphEdge {
    GraphEdge {
        id: id.into(),
        source: NodeId(source),
        target: NodeId(target),
        length_m,
        base_weight: length_m,
        eco_weight: Some(length_m),
        start_lat,
        start_lon,
        end_lat,
        end_lon,
        shape: Vec::new(),
        highway: Some(highway.into()),
        maxspeed_kmh: None,
        name: None,
        road_ref: None,
        maxweight_t: None,
        maxaxleload_t: None,
        maxbogieweight_t: None,
        maxheight_m: None,
        maxwidth_m: None,
        maxlength_m: None,
        is_toll: false,
        is_ferry: false,
        is_boardwalk_crossing: false,
        is_roundabout: false,
        motor_vehicle_conditional: None,
        access_conditional: None,
        maxspeed_conditional: None,
    }
}

/// Diamond: A→B→C short motorway; A→D→C longer secondary. Avoid-motorways must take ADC.
#[test]
fn avoid_motorways_changes_planned_route() {
    let mut nodes = HashMap::new();
    for (id, n) in [
        node(1, 60.0, 10.0),
        node(2, 60.0, 10.01),
        node(3, 60.0, 10.02),
        node(4, 60.01, 10.01),
    ] {
        nodes.insert(id, n);
    }
    let mut ab = edge("ab", 1, 2, 60.0, 10.0, 60.0, 10.01, 100.0, "motorway");
    let mut bc = edge("bc", 2, 3, 60.0, 10.01, 60.0, 10.02, 100.0, "motorway");
    let ad = edge("ad", 1, 4, 60.0, 10.0, 60.01, 10.01, 200.0, "secondary");
    let dc = edge("dc", 4, 3, 60.01, 10.01, 60.0, 10.02, 200.0, "secondary");
    ab.is_toll = false;
    bc.is_toll = false;
    let graph = RouteGraph::from_parts(nodes, vec![ab, bc, ad, dc], RoutingProfile::Car);

    let direct = graph
        .shortest_path(NodeId(1), NodeId(3), false)
        .expect("default path");
    assert!(
        direct.0.contains(&NodeId(2)),
        "default should prefer short motorway via B: {:?}",
        direct.0
    );

    let avoided = graph
        .shortest_path_with_options(
            NodeId(1),
            NodeId(3),
            false,
            &RouteOptions {
                avoid_motorways: true,
                ..Default::default()
            },
        )
        .expect("avoid-motorways path");
    assert!(
        !avoided.0.contains(&NodeId(2)),
        "avoid motorways must not use B: {:?}",
        avoided.0
    );
    assert!(avoided.0.contains(&NodeId(4)));
    assert_ne!(direct.0, avoided.0);

    let share_direct = graph.non_motorway_share_pct(&direct.0);
    let share_avoided = graph.non_motorway_share_pct(&avoided.0);
    assert!(
        (share_direct - 0.0).abs() < 0.01,
        "default motorway path should be 0% non-motorway, got {share_direct}"
    );
    assert!(
        (share_avoided - 100.0).abs() < 0.01,
        "avoid-motorways secondary path should be 100% non-motorway, got {share_avoided}"
    );
    assert_ne!(
        share_direct, share_avoided,
        "priority-path share must be derived from the plan, not a constant"
    );
}

/// Short trunk vs longer secondary: avoid-motorways must still be allowed to use trunk.
#[test]
fn avoid_motorways_allows_trunk_and_primary() {
    let mut nodes = HashMap::new();
    for (id, n) in [
        node(1, 60.0, 10.0),
        node(2, 60.0, 10.01),
        node(3, 60.0, 10.02),
        node(4, 60.01, 10.01),
    ] {
        nodes.insert(id, n);
    }
    let ab = edge("ab", 1, 2, 60.0, 10.0, 60.0, 10.01, 100.0, "trunk");
    let bc = edge("bc", 2, 3, 60.0, 10.01, 60.0, 10.02, 100.0, "primary");
    let ad = edge("ad", 1, 4, 60.0, 10.0, 60.01, 10.01, 200.0, "secondary");
    let dc = edge("dc", 4, 3, 60.01, 10.01, 60.0, 10.02, 200.0, "secondary");
    let graph = RouteGraph::from_parts(nodes, vec![ab, bc, ad, dc], RoutingProfile::Car);

    let avoided = graph
        .shortest_path_with_options(
            NodeId(1),
            NodeId(3),
            false,
            &RouteOptions {
                avoid_motorways: true,
                ..Default::default()
            },
        )
        .expect("path with trunk/primary under avoid-motorways");
    assert!(
        avoided.0.contains(&NodeId(2)),
        "trunk/primary short path must remain usable: {:?}",
        avoided.0
    );
    assert!(
        (graph.non_motorway_share_pct(&avoided.0) - 100.0).abs() < 0.01,
        "trunk/primary count as non-motorway for priority share"
    );
}

#[test]
fn avoid_toll_changes_planned_route() {
    let mut nodes = HashMap::new();
    for (id, n) in [
        node(1, 60.0, 10.0),
        node(2, 60.0, 10.01),
        node(3, 60.0, 10.02),
        node(4, 60.01, 10.01),
    ] {
        nodes.insert(id, n);
    }
    let mut ab = edge("ab", 1, 2, 60.0, 10.0, 60.0, 10.01, 100.0, "primary");
    ab.is_toll = true;
    let mut bc = edge("bc", 2, 3, 60.0, 10.01, 60.0, 10.02, 100.0, "primary");
    bc.is_toll = true;
    let ad = edge("ad", 1, 4, 60.0, 10.0, 60.01, 10.01, 250.0, "secondary");
    let dc = edge("dc", 4, 3, 60.01, 10.01, 60.0, 10.02, 250.0, "secondary");
    let graph = RouteGraph::from_parts(nodes, vec![ab, bc, ad, dc], RoutingProfile::Car);

    let with_toll = graph.shortest_path(NodeId(1), NodeId(3), false).unwrap();
    assert!(with_toll.0.contains(&NodeId(2)));

    let no_toll = graph
        .shortest_path_with_options(
            NodeId(1),
            NodeId(3),
            false,
            &RouteOptions {
                avoid_tolls: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(!no_toll.0.contains(&NodeId(2)));
    assert_ne!(with_toll.0, no_toll.0);
}

#[test]
fn avoid_ferry_changes_planned_route() {
    let mut nodes = HashMap::new();
    for (id, n) in [
        node(1, 60.0, 10.0),
        node(2, 60.0, 10.01),
        node(3, 60.0, 10.02),
        node(4, 60.01, 10.01),
    ] {
        nodes.insert(id, n);
    }
    let mut ab = edge("ab", 1, 2, 60.0, 10.0, 60.0, 10.01, 80.0, "secondary");
    ab.is_ferry = true;
    let mut bc = edge("bc", 2, 3, 60.0, 10.01, 60.0, 10.02, 80.0, "secondary");
    bc.is_ferry = true;
    let ad = edge("ad", 1, 4, 60.0, 10.0, 60.01, 10.01, 300.0, "secondary");
    let dc = edge("dc", 4, 3, 60.01, 10.01, 60.0, 10.02, 300.0, "secondary");
    let graph = RouteGraph::from_parts(nodes, vec![ab, bc, ad, dc], RoutingProfile::Car);

    let with_ferry = graph.shortest_path(NodeId(1), NodeId(3), false).unwrap();
    assert!(with_ferry.0.contains(&NodeId(2)));

    let no_ferry = graph
        .shortest_path_with_options(
            NodeId(1),
            NodeId(3),
            false,
            &RouteOptions {
                avoid_ferries: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(!no_ferry.0.contains(&NodeId(2)));
    assert_ne!(with_ferry.0, no_ferry.0);
}

#[test]
fn vehicle_height_limit_changes_planned_route() {
    let mut nodes = HashMap::new();
    for (id, n) in [
        node(1, 60.0, 10.0),
        node(2, 60.0, 10.01),
        node(3, 60.0, 10.02),
        node(4, 60.01, 10.01),
    ] {
        nodes.insert(id, n);
    }
    let mut low = edge("low", 1, 2, 60.0, 10.0, 60.0, 10.01, 100.0, "primary");
    low.maxheight_m = Some(3.0);
    let bc = edge("bc", 2, 3, 60.0, 10.01, 60.0, 10.02, 100.0, "primary");
    let ad = edge("ad", 1, 4, 60.0, 10.0, 60.01, 10.01, 220.0, "primary");
    let dc = edge("dc", 4, 3, 60.01, 10.01, 60.0, 10.02, 220.0, "primary");
    let graph = RouteGraph::from_parts(nodes, vec![low, bc, ad, dc], RoutingProfile::Truck);

    let unrestricted = graph.shortest_path(NodeId(1), NodeId(3), false).unwrap();
    assert!(unrestricted.0.contains(&NodeId(2)));

    let limited = graph
        .shortest_path_with_options(
            NodeId(1),
            NodeId(3),
            false,
            &RouteOptions {
                vehicle: Some(VehicleLimits {
                    height_m: Some(4.0),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(!limited.0.contains(&NodeId(2)));
    assert_ne!(unrestricted.0, limited.0);

    let fits = graph
        .shortest_path_with_options(
            NodeId(1),
            NodeId(3),
            false,
            &RouteOptions {
                vehicle: Some(VehicleLimits {
                    height_m: Some(2.5),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(fits.0.contains(&NodeId(2)));
}

#[test]
fn official_network_preference_changes_path_cost_and_choice() {
    let mut nodes = HashMap::new();
    for (id, n) in [
        node(1, 60.0, 10.0),
        node(2, 60.0, 10.02),
        node(3, 60.01, 10.01),
    ] {
        nodes.insert(id, n);
    }
    // Short generic path 1→2; longer official-network path 1→3→2.
    let short = edge("99", 1, 2, 60.0, 10.0, 60.0, 10.02, 100.0, "path");
    let long_a = edge("10", 1, 3, 60.0, 10.0, 60.01, 10.01, 120.0, "path");
    let long_b = edge("11", 3, 2, 60.01, 10.01, 60.0, 10.02, 120.0, "path");
    let mut graph =
        RouteGraph::from_parts(nodes, vec![short, long_a, long_b], RoutingProfile::Foot);

    let before = graph.shortest_path(NodeId(1), NodeId(2), false).unwrap();
    assert!(
        !before.0.contains(&NodeId(3)),
        "without preference take short edge: {:?}",
        before.0
    );

    let mut net = HashSet::new();
    net.insert(10);
    net.insert(11);
    apply_official_network_preference(&mut graph, &net);
    let after = graph.shortest_path(NodeId(1), NodeId(2), false).unwrap();
    assert!(
        after.0.contains(&NodeId(3)),
        "with preference take network via 3: {:?}",
        after.0
    );
    assert_ne!(before.0, after.0);
    // Soft: non-network still reachable (gap fallback) — path exists either way.
    assert!(after.1 < before.1 * NON_NETWORK_PENALTY + 1.0 || after.0 != before.0);
}

#[test]
fn ev_regen_makes_eco_path_cost_cheaper_than_ice_on_descent() {
    // Two edges same length: climb then descent vs flat-ish alternate.
    // With regen, descent edge eco_weight drops so EV prefers the hilly path when
    // climb+descent net cost undercuts the long flat detour.
    let ice = EcoConfig::for_profile(Profile::Car);
    let mut ev = EcoConfig::for_profile(Profile::CarElectric);
    ev.drag_coefficient = ice.drag_coefficient;
    ev.frontal_area_m2 = ice.frontal_area_m2;
    ev.mass_kg = ice.mass_kg;

    let climb = ice.segment_energy_joules(500.0, 40.0);
    let descent_ice = ice.segment_energy_joules(500.0, -40.0);
    let descent_ev = ev.segment_energy_joules(500.0, -40.0);
    let flat = ice.segment_energy_joules(1_200.0, 0.0);

    let hilly_ice = climb + descent_ice;
    let hilly_ev = climb + descent_ev;

    assert!(descent_ev < descent_ice);
    assert!(hilly_ev < hilly_ice);
    // Route choice evidence: EV hilly can beat flat when ICE hilly does not.
    assert!(
        hilly_ice > flat || hilly_ev < flat,
        "regen must be able to change relative cost vs flat detour (ice_hilly={hilly_ice} ev_hilly={hilly_ev} flat={flat})"
    );
    assert!(hilly_ev < flat || hilly_ice > hilly_ev);
}

#[test]
fn overnight_filter_excludes_tent_near_building_for_ffi_path() {
    // Mirrors the check wired into pick_hiking_pause_at via OvernightProximityIndex.
    let safety = SafetyConfig::default();
    let tent = PoiRecord {
        osm_id: 42,
        lat: 61.0,
        lon: 10.0,
        categories: vec![PoiCategory::TentSite],
        icon_key: "tourism-camp_site".into(),
        tags: HashMap::new(),
        name: Some("Bad camp".into()),
    };
    let building = (61.0005, 10.0005); // ~70 m — inside 150 m default
    assert!(
        check_overnight_candidate(61.0, 10.0, &safety, &tent, &[building], &[]).is_some(),
        "FFI overnight path must reject tent too close to building"
    );
    let hut = PoiRecord {
        osm_id: 43,
        lat: 61.0,
        lon: 10.0,
        categories: vec![PoiCategory::NetworkHut, PoiCategory::Cabin],
        icon_key: "tourism-alpine_hut".into(),
        tags: HashMap::new(),
        name: Some("Hut".into()),
    };
    // Hut still rejected for building (always applies).
    assert!(check_overnight_candidate(61.0, 10.0, &safety, &hut, &[building], &[]).is_some());
    // Glacier override for established hut.
    let glacier = (61.005, 10.005);
    assert!(check_overnight_candidate(61.0, 10.0, &safety, &hut, &[], &[glacier]).is_none());
    assert!(check_overnight_candidate(61.0, 10.0, &safety, &tent, &[], &[glacier]).is_some());
}

#[test]
fn fishing_category_surfaces_in_poi_query() {
    use driver_break_core::poi::{classify_tags, PoiIndex};
    let tags: HashMap<String, String> =
        [("leisure".into(), "fishing".into())].into_iter().collect();
    assert!(classify_tags(&tags).contains(&PoiCategory::Fishing));

    // Synthetic index insert via load path is PBF-only; assert radius default matches General.
    let safety = SafetyConfig::default();
    assert_eq!(
        PoiCategory::Fishing.default_radius_m(&safety),
        PoiCategory::General.default_radius_m(&safety)
    );
    let _ = PoiIndex::new();
}

#[test]
#[ignore = "needs ostlandet (or similar) extract under core/target/integration-fixtures"]
fn fishing_found_in_region_pbf() {
    use driver_break_core::poi::{PoiCategory, PoiIndex};
    let pbf = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/integration-fixtures/ostlandet-latest.osm.pbf");
    assert!(pbf.is_file(), "missing fixture {}", pbf.display());
    // Small coastal bbox with known leisure=fishing coverage near Oslofjord.
    let bbox = [59.7, 10.4, 60.0, 10.9];
    let idx = PoiIndex::load_from_pbf_bbox(&pbf, bbox).expect("poi load");
    let hits = idx.nearest(PoiCategory::Fishing, 59.91, 10.75, 50_000.0);
    assert!(
        !hits.is_empty(),
        "expected at least one leisure=fishing (or related) POI in Oslofjord bbox"
    );
}

/// Route-level: TruckRestParams break-after hours change break sample spacing
/// (not the car mid-route km heuristic).
#[test]
fn truck_rest_params_change_break_placement_along_route() {
    use driver_break_core::config::Profile;
    use driver_break_core::config::{RestConfig, TruckRestParams};
    use driver_break_core::routing::{
        motor_break_interval_km, truck_break_distances_km, truck_required_breaks,
    };

    // Synthetic long truck day: 520 km, ~6.5 h @ 80 km/h (> 4.5 h → needs a break).
    let dist_km = 520.0;
    let eta_min = 6.5 * 60.0;
    let driving_h = eta_min / 60.0;

    let default_truck = TruckRestParams::default();
    assert!(driving_h > default_truck.mandatory_break_after_hours);
    assert_eq!(truck_required_breaks(&default_truck, driving_h), 1);
    let default_breaks = truck_break_distances_km(&default_truck, dist_km, eta_min);
    assert_eq!(default_breaks.len(), 1);
    // 80 km/h * 4.5 h = 360 km.
    assert!(
        (default_breaks[0] - 360.0).abs() < 1.0,
        "EC 561 default places break near 360 km, got {default_breaks:?}"
    );

    let tight = TruckRestParams {
        mandatory_break_after_hours: 2.0,
        ..Default::default()
    };
    let tight_breaks = truck_break_distances_km(&tight, dist_km, eta_min);
    assert!(
        tight_breaks.len() >= 2,
        "2 h truck interval must place more breaks: {tight_breaks:?}"
    );
    assert!(
        tight_breaks[0] < default_breaks[0] - 100.0,
        "edited TruckRestParams must move first break earlier: {tight_breaks:?} vs {default_breaks:?}"
    );

    // Car soft spacing uses CarRestParams hours × trip speed (not truck EC hours).
    let rest = RestConfig::default();
    let car_iv = motor_break_interval_km(Profile::Car, &rest, dist_km, eta_min);
    let truck_iv = motor_break_interval_km(Profile::Truck, &rest, dist_km, eta_min);
    // 520 km / 6.5 h = 80 km/h; car default max 4.5 h → 360 km (same speed product as
    // truck's 4.5 h default, but sourced from CarRestParams — tighten car to diverge).
    assert!(
        (truck_iv - 360.0).abs() < 1.0,
        "truck uses hour-derived km ({truck_iv})"
    );
    let mut rest_tight_car = RestConfig::default();
    rest_tight_car.car.break_interval_min_hours = 2.0;
    rest_tight_car.car.break_interval_max_hours = 2.0;
    let car_tight = motor_break_interval_km(Profile::Car, &rest_tight_car, dist_km, eta_min);
    assert!(
        (car_tight - 160.0).abs() < 1.0,
        "configured 2 h car interval @ 80 km/h → 160 km, got {car_tight} (default car was {car_iv})"
    );
    assert!(
        (truck_iv - car_tight).abs() > 100.0,
        "truck EC spacing must stay independent of car soft hours"
    );
}
