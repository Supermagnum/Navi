//! DATEX corridor filter + validity windows on the Espa → Åtnbrua (Atnbrufossen) route.
//!
//! Fixture: `tests/fixtures/datex/espa-atnbru-getsituation.xml` — subset of a real
//! navi-server `/datex/GetSituation.xml` cache (GML linestrings stripped for size).

use std::collections::HashMap;

use chrono::{FixedOffset, TimeZone, Utc};
use driver_break_core::config::OVERNIGHT_BUILDING_CORRIDOR_MARGIN_M;
use driver_break_core::datex::{
    corridor_view, parse_situation_publication, planner_impacts, DatexConfig, DatexImpact,
    SituationKind, DATEX_PLUGIN_DEFAULT_ENABLED,
};
use driver_break_core::routing::graph::{
    GraphEdge, RouteGraph, RouteOptions, RoutingProfile, SurfaceQuality,
};
use geo_types::Coord;
use osm4routing::{Node, NodeId};

const FIXTURE: &str = include_str!("fixtures/datex/espa-atnbru-getsituation.xml");

/// Espa → Atnbrufossen corridor vertices (includes known DATEX display points).
fn espa_atnbru_route() -> Vec<(f64, f64)> {
    vec![
        (60.523132, 11.242463),   // Ellingrud / E6 near Espa approach
        (60.5621914, 11.2561239), // Espa (plan start)
        (60.577133, 11.273461),   // Espatunnelen
        (60.63319, 11.231814),    // Akselstua Fv. 222
        (60.883553, 10.913103),   // Langmoen E6
        (61.8512500, 10.2338420), // Atnbrufossen (plan end)
    ]
}

fn local_oslo(y: i32, m: u32, d: u32, hh: u32, mm: u32) -> chrono::DateTime<Utc> {
    let offset = FixedOffset::east_opt(2 * 3600).expect("offset");
    offset
        .with_ymd_and_hms(y, m, d, hh, mm, 0)
        .single()
        .expect("datetime")
        .with_timezone(&Utc)
}

fn fixture_block_situation() -> driver_break_core::datex::DatexSituation {
    let all = parse_situation_publication(FIXTURE).expect("parse fixture");
    all.into_iter()
        .find(|s| s.impact == DatexImpact::Block)
        .expect("fixture must classify at least one Block (stengt)")
}

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
        highway: Some("primary".into()),
        maxspeed_kmh: None,
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
        is_boardwalk_crossing: false,
        is_roundabout: false,
        motor_vehicle_conditional: None,
        access_conditional: None,
        maxspeed_conditional: None,
        access_forbidden: false,
        surface_quality: SurfaceQuality::Good,
    }
}

/// Diamond: short corridor 1→2→3, long free detour 1→4→3 (same pattern as toll tests).
fn diamond_graph() -> RouteGraph {
    let mut nodes = HashMap::new();
    for (id, n) in [
        node(1, 60.0, 10.0),
        node(2, 60.0, 10.01),
        node(3, 60.0, 10.02),
        node(4, 60.05, 10.01),
    ] {
        nodes.insert(id, n);
    }
    let ab = edge("ab", 1, 2, 60.0, 10.0, 60.0, 10.01, 100.0);
    let bc = edge("bc", 2, 3, 60.0, 10.01, 60.0, 10.02, 100.0);
    let ad = edge("ad", 1, 4, 60.0, 10.0, 60.05, 10.01, 800.0);
    let dc = edge("dc", 4, 3, 60.05, 10.01, 60.0, 10.02, 800.0);
    RouteGraph::from_parts(nodes, vec![ab, bc, ad, dc], RoutingProfile::Car)
}

/// Single corridor only (no alternate) for Penalize-must-still-reach.
fn corridor_only_graph() -> RouteGraph {
    let mut nodes = HashMap::new();
    for (id, n) in [
        node(1, 60.0, 10.0),
        node(2, 60.0, 10.01),
        node(3, 60.0, 10.02),
    ] {
        nodes.insert(id, n);
    }
    let ab = edge("ab", 1, 2, 60.0, 10.0, 60.0, 10.01, 100.0);
    let bc = edge("bc", 2, 3, 60.0, 10.01, 60.0, 10.02, 100.0);
    RouteGraph::from_parts(nodes, vec![ab, bc], RoutingProfile::Car)
}

fn opts_with_datex(situations: &[driver_break_core::datex::DatexSituation]) -> RouteOptions {
    RouteOptions {
        datex_impacts: planner_impacts(situations),
        ..Default::default()
    }
}

#[test]
fn datex_plugin_defaults_disabled() {
    const { assert!(!DATEX_PLUGIN_DEFAULT_ENABLED) };
    assert!(!DatexConfig::default().enabled);
}

#[test]
fn espa_atnbru_parse_and_active_at_0534() {
    let all = parse_situation_publication(FIXTURE).expect("parse fixture");
    assert!(
        all.len() >= 5,
        "expected corridor + far situations, got {}",
        all.len()
    );

    let roadworks: Vec<_> = all
        .iter()
        .filter(|s| s.kind == SituationKind::Roadworks)
        .collect();
    assert!(
        roadworks.len() >= 5,
        "expected MaintenanceWorks entries, got {}",
        roadworks.len()
    );

    // Fixed clock: 2026-09-08 05:34 Europe/Oslo (+02:00 summer).
    let now = local_oslo(2026, 9, 8, 5, 34);
    let view = corridor_view(
        &all,
        &espa_atnbru_route(),
        OVERNIGHT_BUILDING_CORRIDOR_MARGIN_M,
        now,
    );

    // Oslo-far must not appear on the Espa corridor.
    assert!(
        view.active
            .iter()
            .chain(view.inactive.iter())
            .all(|s| s.geometry.iter().all(|&(lat, _)| lat > 60.4)),
        "far Oslo situation must be corridor-filtered out"
    );

    let on_route: Vec<_> = view.active.iter().chain(view.inactive.iter()).collect();
    assert!(
        on_route.len() >= 4,
        "expected >=4 corridor road works, got {}",
        on_route.len()
    );

    // Espatunnelen: 2026-09-28T21:00 → 2026-09-29T06:00 — inactive at 05:34 on Sep 8.
    let espa_tunnel = on_route
        .iter()
        .find(|s| {
            s.location_description
                .as_deref()
                .is_some_and(|d| d.contains("Espatunnelen"))
                || s.comment
                    .as_deref()
                    .is_some_and(|c| c.contains("Espatunnelen"))
                || s.id.contains("d27c26bb")
        })
        .expect("Espatunnelen situation on corridor");
    assert!(
        !espa_tunnel.is_active_at(now),
        "Espatunnelen must be inactive at 05:34 on 2026-09-08"
    );
    assert!(
        view.inactive.iter().any(|s| s.id == espa_tunnel.id),
        "inactive list must retain Espatunnelen"
    );
    assert!(
        view.active.iter().all(|s| s.id != espa_tunnel.id),
        "active overlay must exclude Espatunnelen at 05:34"
    );

    // Akselstua / Langmoen / Ellingrud windows include 05:34 on Sep 8.
    let active_ids: Vec<&str> = view.active.iter().map(|s| s.id.as_str()).collect();
    assert!(
        !view.active.is_empty(),
        "expected at least one active roadworks at 05:34"
    );
    assert!(
        view.active
            .iter()
            .any(|s| s.kind == SituationKind::Roadworks),
        "active list should include roadworks; ids={active_ids:?}"
    );

    // Spot-check known active windows.
    let aksel = on_route
        .iter()
        .find(|s| {
            s.location_description
                .as_deref()
                .is_some_and(|d| d.contains("Akselstua"))
        })
        .expect("Akselstua on corridor");
    assert!(
        aksel.is_active_at(now),
        "Akselstua should be active at 05:34"
    );
    assert!(
        aksel.valid_from.is_some() && aksel.valid_to.is_some(),
        "Akselstua must expose validity window"
    );
    let (lat, lon) = aksel.primary_lat_lon().unwrap();
    assert!((lat - 60.63319).abs() < 1e-4 && (lon - 11.231814).abs() < 1e-4);
}

#[test]
fn espa_atnbru_fixture_classifies_impacts() {
    let all = parse_situation_publication(FIXTURE).expect("parse fixture");

    let stengt = all
        .iter()
        .find(|s| {
            s.comment
                .as_deref()
                .is_some_and(|c| c.to_ascii_lowercase().contains("stengt"))
        })
        .expect("stengt comment in fixture");
    assert_eq!(stengt.impact, DatexImpact::Block);
    assert_eq!(stengt.lanes_restricted, Some(2));

    let ellingrud = all
        .iter()
        .find(|s| {
            s.location_description
                .as_deref()
                .is_some_and(|d| d.contains("Ellingrud"))
        })
        .expect("Ellingrud");
    assert_eq!(ellingrud.impact, DatexImpact::Penalize);
    assert_eq!(ellingrud.lanes_restricted, Some(1));

    let espa = all
        .iter()
        .find(|s| s.id.contains("d27c26bb"))
        .expect("Espatunnelen");
    assert_eq!(espa.impact, DatexImpact::Ignore);
    assert_eq!(espa.lanes_restricted, Some(0));
}

#[test]
fn planner_impacts_at_0534_contain_only_active_entries() {
    let all = parse_situation_publication(FIXTURE).expect("parse fixture");
    let now = local_oslo(2026, 9, 8, 5, 34);
    let view = corridor_view(
        &all,
        &espa_atnbru_route(),
        OVERNIGHT_BUILDING_CORRIDOR_MARGIN_M,
        now,
    );

    // Hard rule: planner-facing input is built from active only.
    let impacts = planner_impacts(&view.active);
    for c in &impacts {
        assert!(
            view.active.iter().any(|s| s.id == c.situation_id),
            "planner constraint {} not in active list",
            c.situation_id
        );
        assert!(
            view.inactive.iter().all(|s| s.id != c.situation_id),
            "inactive situation {} must not reach planner",
            c.situation_id
        );
    }

    // Espatunnelen is inactive at 05:34 — must not appear in planner constraints.
    assert!(
        impacts.iter().all(|c| !c.situation_id.contains("d27c26bb")),
        "inactive Espatunnelen leaked into planner_impacts"
    );

    // Sanity: every active situation used for planner must report is_active_at.
    for s in &view.active {
        assert!(
            s.is_active_at(now),
            "active slice contains inactive {}",
            s.id
        );
    }
    for s in &view.inactive {
        assert!(
            !s.is_active_at(now),
            "inactive slice contains active {}",
            s.id
        );
    }
}

#[test]
fn espa_atnbru_active_set_grows_when_clock_enters_night_works() {
    let all = parse_situation_publication(FIXTURE).expect("parse fixture");
    let route = espa_atnbru_route();
    let margin = OVERNIGHT_BUILDING_CORRIDOR_MARGIN_M;

    let at_0534 = corridor_view(&all, &route, margin, local_oslo(2026, 9, 8, 5, 34));
    // Night window for Espatunnelen.
    let at_night = corridor_view(&all, &route, margin, local_oslo(2026, 9, 28, 22, 0));

    let espa_at_0534 = at_0534.active.iter().any(|s| {
        s.id.contains("d27c26bb")
            || s.location_description
                .as_deref()
                .is_some_and(|d| d.contains("Espatunnelen"))
    });
    let espa_at_night = at_night.active.iter().any(|s| {
        s.id.contains("d27c26bb")
            || s.location_description
                .as_deref()
                .is_some_and(|d| d.contains("Espatunnelen"))
    });

    assert!(
        !espa_at_0534,
        "Espatunnelen must not be active at Sep 8 05:34"
    );
    assert!(espa_at_night, "Espatunnelen must be active at Sep 28 22:00");

    // Ellingrud ends 2026-09-17 — active at Sep 8, inactive at Sep 28.
    let ell_0534 = at_0534.active.iter().any(|s| {
        s.location_description
            .as_deref()
            .is_some_and(|d| d.contains("Ellingrud"))
    });
    let ell_night = at_night.active.iter().any(|s| {
        s.location_description
            .as_deref()
            .is_some_and(|d| d.contains("Ellingrud"))
    });
    assert!(ell_0534, "Ellingrud should be active at Sep 8 05:34");
    assert!(!ell_night, "Ellingrud should be inactive at Sep 28 22:00");

    assert!(
        at_night.active.len() != at_0534.active.len()
            || espa_at_night != espa_at_0534
            || ell_night != ell_0534,
        "time filter must change membership both ways (not always-exclude)"
    );
}

#[test]
fn block_active_avoids_short_edge_but_still_finds_route() {
    // Fixture Block is in Oslo; place its classified impact on the diamond short
    // corridor so A* must take the long free detour (same idea as toll widen /
    // never-use: keep an alternate in-graph so the plan succeeds).
    let mut block = fixture_block_situation();
    assert_eq!(block.impact, DatexImpact::Block);
    // Midpoint of edge 1→2.
    block.geometry = vec![(60.0, 10.005)];

    let active_at = local_oslo(2026, 9, 8, 1, 0);
    assert!(
        block.is_active_at(active_at),
        "fixture Block window includes Sep 8 01:00"
    );

    let graph = diamond_graph();
    let baseline = graph
        .shortest_path_with_options(NodeId(1), NodeId(3), false, &RouteOptions::default())
        .expect("baseline path");
    assert!(
        baseline.0.contains(&NodeId(2)),
        "without DATEX, short corridor wins: {:?}",
        baseline.0
    );

    let with_block = graph
        .shortest_path_with_options(
            NodeId(1),
            NodeId(3),
            false,
            &opts_with_datex(std::slice::from_ref(&block)),
        )
        .expect("Block must leave a free detour (do not clip the only alternate)");
    assert!(
        !with_block.0.contains(&NodeId(2)),
        "active Block must avoid short edge via node 2: {:?}",
        with_block.0
    );
    assert!(
        with_block.0.contains(&NodeId(4)),
        "detour via node 4 expected: {:?}",
        with_block.0
    );
}

#[test]
fn block_inactive_at_0534_next_day_has_zero_route_effect() {
    // Fixture Block ends 2026-09-08T06:00+02 — still active at Sep 8 05:34.
    // Same wall-clock next day is inactive; planner_impacts on an empty active
    // slice must leave the short corridor preferred.
    let mut block = fixture_block_situation();
    block.geometry = vec![(60.0, 10.005)];

    let inactive_at = local_oslo(2026, 9, 9, 5, 34);
    assert!(
        !block.is_active_at(inactive_at),
        "Block must be inactive at Sep 9 05:34"
    );

    let graph = diamond_graph();
    let active_only: Vec<_> = std::slice::from_ref(&block)
        .iter()
        .filter(|s| s.is_active_at(inactive_at))
        .cloned()
        .collect();
    assert!(
        active_only.is_empty(),
        "active-only filter must drop inactive Block"
    );
    let opts = opts_with_datex(&active_only);
    assert!(
        opts.datex_impacts.is_empty(),
        "planner_impacts must be empty when no actives"
    );

    let path = graph
        .shortest_path_with_options(NodeId(1), NodeId(3), false, &opts)
        .expect("path");
    assert!(
        path.0.contains(&NodeId(2)),
        "inactive Block must not change chosen route: {:?}",
        path.0
    );
}

#[test]
fn penalize_keeps_edge_usable_when_no_detour() {
    let all = parse_situation_publication(FIXTURE).expect("parse");
    let mut penalize = all
        .into_iter()
        .find(|s| s.impact == DatexImpact::Penalize)
        .expect("Ellingrud Penalize in fixture");
    penalize.geometry = vec![(60.0, 10.005)];
    assert!(
        penalize.is_active_at(local_oslo(2026, 9, 8, 5, 34)),
        "Ellingrud Penalize should be active at Sep 8 05:34"
    );

    let graph = corridor_only_graph();
    let path = graph
        .shortest_path_with_options(
            NodeId(1),
            NodeId(3),
            false,
            &opts_with_datex(std::slice::from_ref(&penalize)),
        )
        .expect("Penalize must keep the only corridor searchable");
    assert!(
        path.0.contains(&NodeId(2)),
        "no-detour Penalize must still use the corridor: {:?}",
        path.0
    );
}
