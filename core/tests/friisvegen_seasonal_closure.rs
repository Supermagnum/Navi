//! Real-extract check: Friisvegen (way 361797686) seasonal motor closure.
//!
//! Requires Ostlandet PBF under `core/target/integration-fixtures/`.

use chrono::NaiveDate;
use driver_break_core::routing::graph::{RouteGraph, RouteOptions, RoutingProfile};
use std::path::PathBuf;

fn ostlandet_pbf() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/integration-fixtures/ostlandet-latest.osm.pbf");
    p.is_file().then_some(p)
}

#[test]
fn friisvegen_excluded_for_car_in_winter_when_fixture_present() {
    let Some(pbf) = ostlandet_pbf() else {
        eprintln!("skip: ostlandet fixture missing");
        return;
    };
    // Friisvegen (way 361797686) corridor from Ostlandet extract.
    let bbox = [61.55, 10.29, 61.68, 10.49];
    let graph =
        RouteGraph::build_from_pbf_bbox(&pbf, RoutingProfile::Car, bbox).expect("bbox car graph");
    let closed: Vec<_> = graph
        .edges
        .iter()
        .filter(|e| {
            e.motor_vehicle_conditional
                .as_deref()
                .is_some_and(|s| s.contains("Nov-Jun"))
                || e.id.contains("361797686")
        })
        .collect();
    assert!(
        !closed.is_empty(),
        "expected Friisvegen / Nov-Jun conditional edges in bbox"
    );
    let jan = NaiveDate::from_ymd_opt(2026, 1, 15)
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap();
    let opts = RouteOptions {
        departure_local: Some(jan),
        ..Default::default()
    };
    for e in &closed {
        if e.motor_vehicle_conditional
            .as_deref()
            .is_some_and(|s| s.contains("Nov-Jun"))
        {
            let idx = graph
                .edges
                .iter()
                .position(|x| x.id == e.id)
                .expect("edge idx");
            assert_eq!(
                graph.seasonal_closure_excluded_count(&[idx], &opts),
                1,
                "edge {} should be seasonally closed in January",
                e.id
            );
        }
    }
    let foot =
        RouteGraph::build_from_pbf_bbox(&pbf, RoutingProfile::Foot, bbox).expect("bbox foot graph");
    let foot_hits: Vec<_> = foot
        .edges
        .iter()
        .filter(|e| {
            e.motor_vehicle_conditional
                .as_deref()
                .is_some_and(|s| s.contains("Nov-Jun"))
        })
        .collect();
    // Foot graph may or may not retain the tag; if it does, motor_vehicle must not exclude.
    for e in foot_hits {
        let idx = foot.edges.iter().position(|x| x.id == e.id).unwrap();
        assert_eq!(
            foot.seasonal_closure_excluded_count(&[idx], &opts),
            0,
            "motor_vehicle:conditional must not exclude hiking edges"
        );
    }
}
