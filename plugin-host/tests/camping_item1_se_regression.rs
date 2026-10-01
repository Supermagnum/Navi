//! Item 1 follow-up: SE cards on Kongsvinger→Charlottenberg are now Tier A.
//!
//! Phase 3a deliberately flips the prior Tier D regression: Sweden has a real
//! Tier A pack, so the SE coordinate that previously declined must accept with
//! Swedish allemansrätt guidance (building distance labelled as Navi safety
//! default, not Swedish law).
//!
//! Real-pack dump (Ostlandet foot tiles, 2026-09-30):
//! - Lillehammer→Sjusjøen: **0** SE cards (all accepted country_iso=no).
//! - Separate Kongsvinger→Charlottenberg corridor produced SE cards at
//!   lat=59.889366 lon=12.192353 with admin_region_at country=se.

mod common;

use common::native_embedder::{
    data_dir, packs_present, plan_corridor, NativeCampingEmbedder, LILLEHAMMER, SJUSJOEN,
    SWEDEN_NEAR_BORDER,
};
use driver_break_core::admin_region_at;
use driver_break_core::config::Profile;
use driver_break_core::storage::Storage;
use navi_right_to_roam_camping::{suggest_overnight, SuggestInput, Tier};

/// Exact SE probe captured on the real Kongsvinger→Charlottenberg run.
const REAL_SE_CARD_LAT: f64 = 59.889366;
const REAL_SE_CARD_LON: f64 = 12.192353;

#[test]
fn lillehammer_corridor_has_zero_se_cards() {
    let dir = data_dir();
    if !packs_present(&dir) {
        eprintln!("SKIP: missing ostlandet packs");
        return;
    }
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
    let se = out
        .list
        .cards
        .iter()
        .filter(|c| c.country_iso == "se")
        .count();
    assert_eq!(se, 0, "Innlandet corridor must not emit SE cards");
    for c in out.list.cards.iter().filter(|c| c.accepted) {
        let ar = admin_region_at(c.lat, c.lon);
        assert_eq!(ar.country_iso.as_deref(), Some("no"));
    }
}

#[test]
fn real_se_card_coordinate_is_sweden() {
    let ar = admin_region_at(REAL_SE_CARD_LAT, REAL_SE_CARD_LON);
    assert_eq!(
        ar.country_iso.as_deref(),
        Some("se"),
        "regression: captured SE card coord must resolve to Sweden"
    );
}

#[test]
fn cross_border_corridor_emits_se_tier_a() {
    let dir = data_dir();
    if !packs_present(&dir) {
        eprintln!("SKIP: missing ostlandet packs");
        return;
    }
    let start_no = (60.1905, 12.0000);
    let (graph, waypoints) =
        plan_corridor(&dir, start_no, SWEDEN_NEAR_BORDER).expect("plan cross-border");
    let storage = Storage::open_in_memory().unwrap();
    let mut emb = NativeCampingEmbedder::with_real_backends(&storage, None, Profile::Hiking);
    emb.set_route(waypoints.clone(), Some(SWEDEN_NEAR_BORDER));
    let out = suggest_overnight(
        &mut emb,
        &SuggestInput {
            graph: &graph,
            corridor_waypoints: &waypoints,
            track_walk_m: None,
            corridor_radius_m: Some(1_200.0),
            max_suggestions: Some(20),
        },
    );
    let se: Vec<_> = out
        .list
        .cards
        .iter()
        .filter(|c| c.country_iso == "se")
        .collect();
    assert!(
        !se.is_empty(),
        "cross-border corridor must yield Swedish Tier A cards"
    );
    for c in &se {
        let ar = admin_region_at(c.lat, c.lon);
        eprintln!(
            "SE card dump: lat={:.6} lon={:.6} admin={:?} accepted={} tier={:?} reason={:?}",
            c.lat, c.lon, ar, c.accepted, c.tier, c.reject_reason
        );
        assert_eq!(ar.country_iso.as_deref(), Some("se"));
        assert_eq!(c.tier, Tier::A);
        assert!(
            c.accepted,
            "Phase 3a: SE must accept under Swedish Tier A pack"
        );
        assert!(c
            .notes
            .iter()
            .any(|n| n.contains("Navi safety default, not Swedish law")));
    }
}
