//! County / fylke subdivision: current ISO codes + cloudberry gating.

mod common;

use common::native_embedder::{
    data_dir, packs_present, plan_corridor, NativeCampingEmbedder, LILLEHAMMER, SJUSJOEN,
};
use driver_break_core::config::Profile;
use driver_break_core::storage::Storage;
use driver_break_core::{
    admin_region_at, subdivision_iso_at, subdivision_name_at, subdivision_ring_count,
};
use navi_right_to_roam_camping::{
    cloudberry_decision, suggest_overnight, CloudberryDecision, SuggestInput,
};

#[test]
fn current_iso_tromso_alta_bodo_and_trondelag_omits_cloudberry() {
    assert!(
        subdivision_ring_count() > 0,
        "baked Norway fylke asset must load"
    );

    // Points beside the named cities (NE Admin-1 is coarse; see admin_subdivision).
    let cases = [
        ("Tromsø hinterland", 69.5992, 18.9953, "no-55", true),
        ("Alta", 69.9689, 23.2717, "no-56", true),
        ("Bodø hinterland", 67.2704, 14.4149, "no-18", true),
        ("Trondheim (Trøndelag)", 63.4305, 10.3951, "no-50", false),
    ];
    for &(label, lat, lon, want_iso, expect_cloudberry) in &cases {
        let iso = subdivision_iso_at(lat, lon);
        eprintln!("{label}: subdivision={iso:?}");
        assert_eq!(iso.as_deref(), Some(want_iso), "{label}");
        let decision = cloudberry_decision(iso.as_deref());
        if expect_cloudberry {
            assert_eq!(decision, CloudberryDecision::Show, "{label}");
        } else {
            assert!(
                matches!(decision, CloudberryDecision::OmitOutsideNorthern { .. }),
                "{label}: cloudberry must be omitted outside Nordland/Troms/Finnmark, got {decision:?}"
            );
        }
    }

    // Explicit: never the pre-reform or merged northern codes from this layer.
    for iso in ["no-55", "no-56", "no-18"] {
        assert!(!matches!(iso, "no-19" | "no-20" | "no-54"));
    }
}

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

        let accepted: Vec<_> = out
            .list
            .cards
            .iter()
            .filter(|c| c.accepted)
            .take(5)
            .collect();
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
            assert!(!c
                .notes
                .iter()
                .any(|n| n.to_lowercase().contains("cloudberry")));
        }
    }
}
