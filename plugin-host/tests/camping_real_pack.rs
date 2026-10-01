//! Phase 2 real-pack verification: Lillehammer → Sjusjøen.
//!
//! Ostlandet OSM does **not** include Charlottenberg (Sweden). Earlier SE
//! Tier A results at 59.889366, 12.192353 used `admin_region_at` on fixed
//! probes, not ostlandet graph coverage. Swedish corridor UI uses the
//! europe/sweden/varmland pack.
//!
//! Verified against **real** downloaded Ostlandet packs under `target/espa-dombas-e2e`
//! when present. Fail-safe / fire-window / night-store unit coverage lives in the
//! camping crate (synthetic fixtures).
//!
//! IMPORTANT: keep Lillehammer→Sjusjøen and cross-border SE dumps in **separate**
//! test outputs — do not merge SE cards from the Kongsvinger→Charlottenberg run
//! into the Lillehammer corridor report.

mod common;

use common::native_embedder::{
    data_dir, junctions_for_route, load_proximity, packs_present, plan_corridor,
    warm_ostlandet_subdivisions, NativeCampingEmbedder, LILLEHAMMER, SJUSJOEN, SWEDEN_INLAND,
    SWEDEN_NEAR_BORDER,
};
use driver_break_core::admin_region_at;
use driver_break_core::config::{Profile, SafetyConfig};
use driver_break_core::storage::Storage;
use navi_plugin_host::{HostApi, PluginKvStatus};
use navi_right_to_roam_camping::{
    fire_guidance_norway, suggest_overnight, CampingCard, LocalDate, SuggestInput, Tier,
    CAUTIOUS_FIRE_UNKNOWN_DATE,
};

#[test]
fn native_embedder_supplies_real_not_default_unavailable() {
    let storage = Storage::open_in_memory().expect("mem db");
    let mut emb = NativeCampingEmbedder::with_real_backends(&storage, None, Profile::Hiking);
    assert!(emb.safety_config_read().is_some());
    assert!(emb.clock_read().is_some());
    assert_eq!(emb.plugin_kv_status(), PluginKvStatus::Available);
    let r = emb.admin_region_read(LILLEHAMMER.0, LILLEHAMMER.1);
    assert_eq!(r.country_iso.as_deref(), Some("no"));
    emb.plugin_kv_set("k", "v").unwrap();
    assert_eq!(emb.plugin_kv_get("k").as_deref(), Some("v"));
}

#[test]
fn fire_guidance_boundaries_and_unknown_date() {
    assert_eq!(fire_guidance_norway(None).text, CAUTIOUS_FIRE_UNKNOWN_DATE);
    let apr14 = LocalDate {
        year: 2026,
        month: 4,
        day: 14,
    };
    let apr15 = LocalDate {
        year: 2026,
        month: 4,
        day: 15,
    };
    let sep15 = LocalDate {
        year: 2026,
        month: 9,
        day: 15,
    };
    let sep16 = LocalDate {
        year: 2026,
        month: 9,
        day: 16,
    };
    assert!(!fire_guidance_norway(Some(apr14)).in_ban_window.unwrap());
    assert!(fire_guidance_norway(Some(apr15)).in_ban_window.unwrap());
    assert!(fire_guidance_norway(Some(sep15)).in_ban_window.unwrap());
    assert!(!fire_guidance_norway(Some(sep16)).in_ban_window.unwrap());
}

#[test]
fn lillehammer_sjusjoen_real_pack_report() {
    let dir = data_dir();
    if !packs_present(&dir) {
        eprintln!("SKIP real pack: missing ostlandet under {}", dir.display());
        return;
    }
    let _ = warm_ostlandet_subdivisions(&dir);

    let planned = plan_corridor(&dir, LILLEHAMMER, SJUSJOEN);
    let Some((graph, waypoints)) = planned else {
        panic!("failed to plan Lillehammer → Sjusjøen on foot pack");
    };
    assert!(
        waypoints.len() >= 2,
        "corridor must have waypoints, got {}",
        waypoints.len()
    );

    let bbox = [
        LILLEHAMMER.0.min(SJUSJOEN.0) - 0.2,
        LILLEHAMMER.1.min(SJUSJOEN.1) - 0.2,
        LILLEHAMMER.0.max(SJUSJOEN.0) + 0.2,
        LILLEHAMMER.1.max(SJUSJOEN.1) + 0.2,
    ];
    let prox = load_proximity(&dir, bbox);
    let storage = Storage::open_in_memory().expect("mem db");
    {
        let store = driver_break_core::storage::ConfigStore::new(&storage);
        let s = SafetyConfig {
            min_building_distance_m: 150.0,
            ..Default::default()
        };
        store.save_safety_config(&s).unwrap();
    }

    let mut emb =
        NativeCampingEmbedder::with_real_backends(&storage, prox.as_ref(), Profile::Hiking);
    emb.set_route(waypoints.clone(), Some(SJUSJOEN));

    let junctions = junctions_for_route(&graph, &waypoints);
    eprintln!("=== REAL PACK: Lillehammer → Sjusjøen (Innlandet only) ===");
    eprintln!("corridor waypoints: {}", waypoints.len());
    eprintln!("road∩track seeds (runtime): {}", junctions.len());
    eprintln!("overnight buildings loaded: {}", emb.buildings.len());

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

    eprintln!("seeds considered: {}", out.list.seeds_considered);
    eprintln!("probes accepted: {}", out.list.probes_accepted);
    eprintln!("probes rejected: {}", out.list.probes_rejected);
    eprintln!(
        "probe_log len={} (accept cap stops early; not all {} seeds are probed)",
        out.probe_log.len(),
        out.list.seeds_considered
    );

    let mut reason_counts: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    for e in &out.probe_log {
        *reason_counts.entry(e.reason.clone()).or_default() += 1;
    }
    eprintln!("reject/accept reasons:");
    for (r, n) in &reason_counts {
        eprintln!("  {r}: {n}");
    }

    // Regression: this corridor must not produce Sweden Tier D cards.
    let se_on_corridor: Vec<_> = out
        .list
        .cards
        .iter()
        .filter(|c| c.country_iso == "se")
        .collect();
    for c in &se_on_corridor {
        let ar = admin_region_at(c.lat, c.lon);
        eprintln!(
            "UNEXPECTED SE card on Lillehammer corridor: lat={:.6} lon={:.6} \
admin_region={:?} reason={:?} reject={:?}",
            c.lat, c.lon, ar, c.decline, c.reject_reason
        );
    }
    assert!(
        se_on_corridor.is_empty(),
        "Lillehammer→Sjusjøen must yield zero SE cards; got {}",
        se_on_corridor.len()
    );

    let samples: Vec<&CampingCard> = out
        .list
        .cards
        .iter()
        .filter(|c| c.accepted)
        .take(5)
        .collect();
    for (i, c) in samples.iter().enumerate() {
        let ar = admin_region_at(c.lat, c.lon);
        eprintln!(
            "=== accepted spot {} admin_region_at lat={:.5} lon={:.5} → {:?} ===",
            i + 1,
            c.lat,
            c.lon,
            ar
        );
        eprintln!("{}", format_card(c));
    }

    for c in out.list.cards.iter().filter(|c| c.accepted) {
        assert_eq!(c.tier, Tier::A);
        assert_eq!(c.country_iso, "no");
        assert!(c.fire_text.is_some());
        assert!(c.bare_rock_note.is_some());
        assert!(c.notes.iter().any(|n| n.contains("leave no trace")));
        assert!(c.notes.iter().any(|n| n.contains("protected")));
        assert!(!c.disclaimer.is_empty());
        assert!(c.not_checked.protected_area);
    }

    eprintln!(
        "VERIFIED (real pack): Lillehammer→Sjusjøen has zero SE cards; \
country_iso=no for all accepted."
    );
}

#[test]
fn sweden_cross_border_se_tier_a_dump() {
    let dir = data_dir();
    if !packs_present(&dir) {
        eprintln!("SKIP real pack: missing ostlandet under {}", dir.display());
        return;
    }

    let storage = Storage::open_in_memory().expect("mem db");
    let mut emb = NativeCampingEmbedder::with_real_backends(&storage, None, Profile::Hiking);

    // Kongsvinger / Magnor approach → Charlottenberg (SE).
    let start_no = (60.1905, 12.0000);
    let Some((graph, waypoints)) = plan_corridor(&dir, start_no, SWEDEN_NEAR_BORDER) else {
        panic!("cross-border corridor not routable on foot pack");
    };
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

    let se_cards: Vec<_> = out
        .list
        .cards
        .iter()
        .filter(|c| c.country_iso == "se")
        .collect();
    eprintln!("=== SEPARATE TEST: Kongsvinger→Charlottenberg cross-border ===");
    eprintln!(
        "seeds={} accepted={} se_cards={}",
        out.list.seeds_considered,
        out.list.probes_accepted,
        se_cards.len()
    );
    assert!(
        !se_cards.is_empty(),
        "expected at least one SE Tier A card on a corridor that enters Sweden"
    );
    for (i, c) in se_cards.iter().enumerate() {
        let ar = admin_region_at(c.lat, c.lon);
        eprintln!(
            "SE card {}: lat={:.6} lon={:.6} admin_region_at={:?} tier={:?} \
accepted={} reject_reason={:?}",
            i + 1,
            c.lat,
            c.lon,
            ar,
            c.tier,
            c.accepted,
            c.reject_reason
        );
        assert_eq!(c.tier, Tier::A);
        assert!(c.accepted);
        assert_eq!(ar.country_iso.as_deref(), Some("se"));
    }
}

#[test]
fn sweden_inland_point_is_tier_a_pack() {
    let iso = admin_region_at(SWEDEN_INLAND.0, SWEDEN_INLAND.1)
        .country_iso
        .unwrap_or_default();
    eprintln!(
        "Sweden inland admin_region_at → {iso} @ {},{}",
        SWEDEN_INLAND.0, SWEDEN_INLAND.1
    );
    assert_eq!(iso, "se");
    let pack = navi_right_to_roam_camping::packs::pack_for_country(Some("se"));
    assert_eq!(pack.tier, Tier::A);
    assert!(pack
        .distance_card_label()
        .is_some_and(|l| l.contains("Navi safety default, not Swedish law")));
}

fn format_card(c: &CampingCard) -> String {
    format!(
        "lat={:.5} lon={:.5} accepted={} tier={:?} country={} subdiv={:?}\n\
         decline={:?} reason={:?}\n\
         legal={}\n\
         sources={:?}\n\
         fire={:?}\n\
         bare_rock={:?}\n\
         notes={:?}\n\
         not_checked={{protected:{}, landcover:{}}}\n\
         location_id={} walk_m={:?} road={:?}\n\
         disclaimer_prefix={}",
        c.lat,
        c.lon,
        c.accepted,
        c.tier,
        c.country_iso,
        c.subdivision_iso,
        c.decline,
        c.reject_reason,
        c.legal_basis,
        c.sources,
        c.fire_text,
        c.bare_rock_note,
        c.notes,
        c.not_checked.protected_area,
        c.not_checked.landcover,
        c.location_id,
        c.walk_m,
        c.seed_road_highway,
        c.disclaimer.chars().take(80).collect::<String>(),
    )
}
