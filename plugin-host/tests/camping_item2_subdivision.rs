//! Item 2: Norway fylke subdivision + cloudberry gating.

mod common;

use common::native_embedder::{
    data_dir, packs_present, plan_corridor, NativeCampingEmbedder, LILLEHAMMER, SJUSJOEN,
};
use driver_break_core::{
    admin_region_at, subdivision_iso_at, subdivision_name_at, subdivision_ring_count,
};
use driver_break_core::config::Profile;
use driver_break_core::storage::Storage;
use navi_right_to_roam_camping::{
    cloudberry_decision, suggest_overnight, CloudberryDecision, SuggestInput,
};

#[test]
fn innlandet_spots_resolve_subdivision_and_omit_cloudberry() {
    assert!(
        subdivision_ring_count() > 0,
        "baked Norway fylke asset must load"
    );

    let dir = data_dir();
    if !packs_present(&dir) {
        eprintln!("SKIP corridor: missing ostlandet graph packs (subdivision unit still ran)");
    } else {
        let (graph, waypoints) =
            plan_corridor(&dir, LILLEHAMMER, SJUSJOEN).expect("plan Lillehammer→Sjusjøen");
        let storage = Storage::open_in_memory().unwrap();
        let mut emb = NativeCampingEmbedder::with_real_backends(&storage, None, Profile::Hiking);
        emb.set_route(waypoints.clone(), Some(SJUSJOEN));
        let out = suggest_overnight(
            &mut emb,
            &SuggestInput {
                graph: &graph,
                corridor_waypoints: &waypoints,
                track_walk_m: None,
                corridor_radius_m: None,
                max_suggestions: Some(12),
            },
        );

        let accepted: Vec<_> = out.list.cards.iter().filter(|c| c.accepted).take(5).collect();
        assert!(!accepted.is_empty());
        for (i, c) in accepted.iter().enumerate() {
            let ar = admin_region_at(c.lat, c.lon);
            let name = subdivision_name_at(c.lat, c.lon);
            eprintln!(
                "spot {}: lat={:.5} lon={:.5} admin_region={:?} fylke_name={:?} \
municipality=not_available (no kommune layer)",
                i + 1,
                c.lat,
                c.lon,
                ar,
                name
            );
            assert_eq!(ar.country_iso.as_deref(), Some("no"));
            let sub = ar
                .subdivision_iso
                .clone()
                .or_else(|| subdivision_iso_at(c.lat, c.lon));
            assert_eq!(sub.as_deref(), Some("no-34"), "Innlandet corridor → no-34");
            let decision = cloudberry_decision(sub.as_deref());
            eprintln!("  cloudberry_decision={decision:?}");
            assert!(matches!(
                decision,
                CloudberryDecision::OmitOutsideNorthern { .. }
            ));
            assert!(!c.notes.iter().any(|n| n.to_lowercase().contains("cloudberry")));
        }
    }

    // Northern Show path: Natural Earth Nordland label point (no nord-norge pack
    // in this workspace — Ostlandet e2e PBF is a 16KiB stub; graph tiles only).
    let nord_iso = subdivision_iso_at(66.7347, 14.7203);
    eprintln!("Nordland NE centroid subdivision={nord_iso:?}");
    assert_eq!(nord_iso.as_deref(), Some("no-18"));
    assert_eq!(
        cloudberry_decision(nord_iso.as_deref()),
        CloudberryDecision::Show
    );
    eprintln!(
        "EXPLICIT: no Nordland/Troms/Finnmark OSM pack under {}; \
cloudberry Show verified via baked NE Admin-1 subdivision at Nordland centroid only",
        dir.display()
    );
}
