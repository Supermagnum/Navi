//! Prove `too_close_to_glacier` on real Ostlandet glacier rings (Gjende / Jotunheimen).

mod common;

use common::native_embedder::{data_dir, packs_present, plan_corridor, NativeCampingEmbedder};
use driver_break_core::config::{Profile, SafetyConfig};
use driver_break_core::routing::safety::{
    check_overnight_candidate, min_distance_to_glacier_rings_m, DangerBarrierIndex,
    OvernightRejectReason,
};
use driver_break_core::storage::Storage;
use navi_right_to_roam_camping::{suggest_overnight, suggest_overnight_fixed_probes, SuggestInput};
use std::path::PathBuf;

/// Real Ostlandet PBF (espa e2e dir ships a 16 KiB stub; fixture has full extract).
fn real_ostlandet_pbf() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../core/target/integration-fixtures/ostlandet-latest.osm.pbf")
}

#[test]
fn too_close_to_glacier_fires_on_real_gjende_rings() {
    let pbf = real_ostlandet_pbf();
    if !pbf.is_file() {
        eprintln!(
            "SKIP: missing real PBF at {} — cannot prove glacier on pack data",
            pbf.display()
        );
        return;
    }

    // Same Gjende tongue bbox as core glacier_overnight_edge (way 380644665 area).
    let bbox = [61.48, 8.35, 61.56, 8.46];
    let barriers = DangerBarrierIndex::load_from_pbf_bbox(&pbf, bbox).expect("barriers");
    assert!(
        barriers.glacier_ring_count() >= 1,
        "expected glacier rings near Gjende in real PBF"
    );
    let rings = barriers.glacier_rings().to_vec();
    eprintln!(
        "REAL DATA: loaded {} glacier rings near Gjende from {}",
        rings.len(),
        pbf.display()
    );

    // Probe ~200 m south of the tongue edge — known exclude in core edge test.
    let probe = (61.5149_f64, 8.4060_f64);
    let d = min_distance_to_glacier_rings_m(probe.0, probe.1, &rings).unwrap_or(f64::INFINITY);
    eprintln!("probe beside glacier: edge_dist_m={d:.1}");
    assert!(
        d < 1_000.0,
        "probe must be inside SafetyConfig glacier buffer"
    );

    let storage = Storage::open_in_memory().unwrap();
    {
        let store = driver_break_core::storage::ConfigStore::new(&storage);
        store.save_safety_config(&SafetyConfig::default()).unwrap();
    }
    let mut emb = NativeCampingEmbedder::with_real_backends(&storage, None, Profile::Hiking);
    emb.glacier_rings = rings;
    emb.buildings.clear();

    let out = suggest_overnight_fixed_probes(&mut emb, &[probe], Some(1));
    let reasons: Vec<_> = out.probe_log.iter().map(|e| e.reason.as_str()).collect();
    eprintln!("camping engine probe_log reasons={reasons:?}");
    assert!(
        out.probe_log
            .iter()
            .any(|e| e.reason == "too_close_to_glacier"),
        "expected too_close_to_glacier on real Gjende rings, got {reasons:?}"
    );
    eprintln!("VERIFIED (real pack rings): too_close_to_glacier fired at Gjende probe");

    // Corridor search on foot tiles (may not walk close enough to the tongue).
    let dir = data_dir();
    if packs_present(&dir) {
        let start = (61.50, 8.38);
        let end = (61.53, 8.42);
        if let Some((graph, waypoints)) = plan_corridor(&dir, start, end) {
            let mut cemb =
                NativeCampingEmbedder::with_real_backends(&storage, None, Profile::Hiking);
            cemb.glacier_rings = emb.glacier_rings.clone();
            cemb.buildings.clear();
            cemb.set_route(waypoints.clone(), Some(end));
            let cout = suggest_overnight(
                &mut cemb,
                &SuggestInput {
                    graph: &graph,
                    corridor_waypoints: &waypoints,
                    track_walk_m: None,
                    corridor_radius_m: Some(1_500.0),
                    max_suggestions: Some(50),
                },
            );
            let fired = cout
                .probe_log
                .iter()
                .any(|e| e.reason == "too_close_to_glacier");
            eprintln!(
                "Gjende corridor track probes: n={} too_close_to_glacier={}",
                cout.probe_log.len(),
                fired
            );
            if !fired {
                eprintln!(
                    "NOTE: no road∩track probe landed inside the 1 km glacier buffer on this \
corridor; fixed real-ring probe above remains the pack proof."
                );
            }
        } else {
            eprintln!("Gjende corridor not routable on foot tiles");
        }
    }
}

#[test]
fn too_close_to_glacier_synthetic_fixture_labelled() {
    // SYNTHETIC: unit-shaped ring so CI without the multi-hundred-MB fixture still
    // exercises the camping reject reason string.
    let ring: Vec<[f64; 2]> = vec![
        [8.40, 61.52],
        [8.41, 61.52],
        [8.41, 61.53],
        [8.40, 61.53],
        [8.40, 61.52],
    ];
    let safety = SafetyConfig::default();
    let inside = check_overnight_candidate(
        61.525,
        8.405,
        &safety,
        &driver_break_core::poi::PoiRecord {
            osm_id: 0,
            lat: 61.525,
            lon: 8.405,
            categories: vec![],
            icon_key: String::new(),
            tags: Default::default(),
            name: None,
        },
        &[],
        std::slice::from_ref(&ring),
    );
    assert_eq!(inside, Some(OvernightRejectReason::TooCloseToGlacier));

    let storage = Storage::open_in_memory().unwrap();
    {
        let store = driver_break_core::storage::ConfigStore::new(&storage);
        store.save_safety_config(&safety).unwrap();
    }
    let mut emb = NativeCampingEmbedder::with_real_backends(&storage, None, Profile::Hiking);
    emb.glacier_rings = vec![ring];
    emb.buildings.clear();
    let out = suggest_overnight_fixed_probes(&mut emb, &[(61.525, 8.405)], Some(1));
    assert!(
        out.probe_log
            .iter()
            .any(|e| e.reason == "too_close_to_glacier"),
        "SYNTHETIC fixture must still emit too_close_to_glacier"
    );
    eprintln!("SYNTHETIC fixture: too_close_to_glacier ok (labelled; not pack geometry)");
}
