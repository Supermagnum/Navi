//! Glacier overnight edge-distance probes near Gjende (way 380644665).
//!
//! Confirms distance-to-edge (not centroid) exclusion and documents probe results
//! against the PBF-backed [`DangerBarrierIndex`].

use driver_break_core::config::SafetyConfig;
use driver_break_core::poi::{PoiCategory, PoiRecord};
use driver_break_core::routing::safety::{
    check_overnight_candidate, min_distance_to_glacier_rings_m, DangerBarrierIndex,
    OvernightRejectReason,
};
use std::collections::HashMap;
use std::path::PathBuf;

fn fixture_pbf() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/integration-fixtures/ostlandet-latest.osm.pbf")
}

fn tent_at(lat: f64, lon: f64) -> PoiRecord {
    PoiRecord {
        osm_id: 1,
        lat,
        lon,
        categories: vec![PoiCategory::TentSite],
        icon_key: "tourism-camp_site".into(),
        tags: HashMap::new(),
        name: Some("Tent".into()),
    }
}

#[test]
#[ignore = "needs ostlandet fixture under core/target/integration-fixtures"]
fn gjende_glacier_edge_distance_probes() {
    let pbf = fixture_pbf();
    assert!(pbf.is_file(), "missing {}", pbf.display());

    // Same tight bbox as the divergence investigation.
    let bbox = [61.48, 8.35, 61.56, 8.46];
    let idx = DangerBarrierIndex::load_from_pbf_bbox(&pbf, bbox).expect("barriers");
    assert!(
        idx.glacier_ring_count() >= 1,
        "expected glacier rings near Gjende"
    );
    let rings = idx.glacier_rings();
    let safety = SafetyConfig::default();

    // Way 380644665 centroid (from Overpass / PBF).
    let probes: &[(&str, f64, f64)] = &[
        ("inside_approx", 61.5210, 8.4060),
        ("south_edge_out_200m", 61.5149, 8.4060),
        ("south_edge_out_800m", 61.5096, 8.4060),
        ("south_edge_out_1200m", 61.5060, 8.4060),
        ("far_3km_south", 61.4940, 8.4060),
    ];

    eprintln!("GJENDE_GLACIER_EDGE_PROBES rings={}", rings.len());
    for &(name, lat, lon) in probes {
        let d = min_distance_to_glacier_rings_m(lat, lon, rings).unwrap_or(f64::INFINITY);
        let tent = tent_at(lat, lon);
        let reason = check_overnight_candidate(lat, lon, &safety, &tent, &[], rings);
        let exclude = reason == Some(OvernightRejectReason::TooCloseToGlacier);
        eprintln!("  {name}: edge_dist_m={d:.1} overnight_exclude={exclude}");
        match name {
            "inside_approx" | "south_edge_out_200m" => {
                assert!(exclude, "{name} should exclude (edge < 1 km), d={d:.1}");
            }
            "far_3km_south" => {
                // May still exclude if another glacier is nearer than 1 km; only
                // assert when the nearest edge is clearly beyond the threshold.
                if d >= 1_000.0 {
                    assert!(!exclude, "{name} should allow when edge>={d:.1}");
                }
            }
            _ => {}
        }
    }
}
