//! Item 1 follow-up: SE Tier D cards were **not** on Lillehammer→Sjusjøen.
//!
//! Real-pack dump (Ostlandet foot tiles, 2026-09-30):
//! - Lillehammer→Sjusjøen: **0** SE cards (all accepted country_iso=no).
//! - Separate Kongsvinger→Charlottenberg corridor produced the two SE cards:
//!   both at lat=59.889366 lon=12.192353 with admin_region_at country=se.
//!
//! Root cause of the Phase 2 report confusion: the cross-border test output was
//! narrated next to the Lillehammer results. Not an admin_region_at bug.

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
    assert_eq!(se, 0, "Innlandet corridor must not emit SE Tier D cards");
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
fn cross_border_corridor_still_emits_se_tier_d() {
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
    assert!(!se.is_empty(), "cross-border corridor must still yield SE Tier D");
    for c in &se {
        let ar = admin_region_at(c.lat, c.lon);
        eprintln!(
            "SE card dump: lat={:.6} lon={:.6} admin={:?} reason={:?}",
            c.lat, c.lon, ar, c.reject_reason
        );
        assert_eq!(ar.country_iso.as_deref(), Some("se"));
        assert_eq!(c.tier, Tier::D);
    }
}
