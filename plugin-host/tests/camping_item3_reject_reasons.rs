//! Item 3: rejection-reason coverage on real Østlandet data.

mod common;

use common::native_embedder::{
    data_dir, load_proximity, packs_present, plan_corridor, NativeCampingEmbedder, LILLEHAMMER,
    SJUSJOEN,
};
use driver_break_core::config::{Profile, SafetyConfig};
use driver_break_core::storage::Storage;
use navi_right_to_roam_camping::{suggest_overnight, SuggestInput};
use std::collections::BTreeMap;

/// Every reject / gate reason the Phase 2 engine can emit.
const ALL_REASONS: &[&str] = &[
    "safety_config_unavailable",
    "track_too_short_for_walk_or_pack_min_road",
    "svalbard_decline",
    "tier_d_country_*",
    "plugin_kv_unavailable",
    "clock_unavailable_for_night_store",
    "max_consecutive_nights",
    "too_close_to_building",
    "too_close_to_glacier",
];

#[test]
fn rejection_reason_coverage_on_ostlandet() {
    eprintln!("=== Implemented rejection / gate reasons ===");
    for r in ALL_REASONS {
        eprintln!("  - {r}");
    }
    eprintln!(
        "NOTE: cultivated land / protected area / water are NotCheckedLayers notes for \
Norway Phase 2 (farmland_filter=false; protected/landcover layers not ready) — not \
hard-filter reject reasons."
    );

    let dir = data_dir();
    if !packs_present(&dir) {
        eprintln!("SKIP: missing ostlandet packs");
        return;
    }

    let (graph, waypoints) =
        plan_corridor(&dir, LILLEHAMMER, SJUSJOEN).expect("plan Lillehammer→Sjusjøen");
    let bbox = [
        LILLEHAMMER.0.min(SJUSJOEN.0) - 0.25,
        LILLEHAMMER.1.min(SJUSJOEN.1) - 0.25,
        LILLEHAMMER.0.max(SJUSJOEN.0) + 0.25,
        LILLEHAMMER.1.max(SJUSJOEN.1) + 0.25,
    ];
    let prox = load_proximity(&dir, bbox);
    let storage = Storage::open_in_memory().unwrap();
    {
        let store = driver_break_core::storage::ConfigStore::new(&storage);
        store
            .save_safety_config(&SafetyConfig::default())
            .unwrap();
    }
    let mut emb =
        NativeCampingEmbedder::with_real_backends(&storage, prox.as_ref(), Profile::Hiking);
    emb.set_route(waypoints.clone(), Some(SJUSJOEN));

    // No accept cap — probe until seeds exhausted (or a high soft limit).
    let out = suggest_overnight(
        &mut emb,
        &SuggestInput {
            graph: &graph,
            corridor_waypoints: &waypoints,
            track_walk_m: None,
            corridor_radius_m: None,
            max_suggestions: Some(200),
        },
    );

    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for e in &out.probe_log {
        *counts.entry(e.reason.clone()).or_default() += 1;
    }
    eprintln!("=== Lillehammer→Sjusjøen (max_suggestions=200) ===");
    eprintln!(
        "seeds={} probe_log={} accepted={} rejected={}",
        out.list.seeds_considered,
        out.probe_log.len(),
        out.list.probes_accepted,
        out.list.probes_rejected
    );
    for (r, n) in &counts {
        eprintln!("  {r}: {n}");
    }

    // 12/12 explanation from Phase 2 report: max_suggestions=Some(12) stops after
    // the 12th accept; rejects accumulated along the way. With cap 200 we see the
    // fuller distribution.
    eprintln!(
        "12/12 SPLIT EXPLAINED: Phase 2 report used max_suggestions=Some(12). The \
engine breaks after the 12th accept, so only the first ~24 probes (12 accept + 12 \
reject) appear — not all 236 seeds. Ordering is seed rank (tertiary/unclassified \
before service) then track continuation length."
    );

    assert!(
        counts.contains_key("too_close_to_building") || counts.contains_key("accepted"),
        "expected building filter and/or accepts on this corridor"
    );

    // Glacier: try a corridor nearer Jotunheimen if tiles cover it.
    let jotun_start = (61.58, 8.10); // roughly Lom / Bøverdalen approach
    let jotun_end = (61.64, 8.30);
    if let Some((jg, jw)) = plan_corridor(&dir, jotun_start, jotun_end) {
        let jbbox = [
            jotun_start.0.min(jotun_end.0) - 0.3,
            jotun_start.1.min(jotun_end.1) - 0.3,
            jotun_start.0.max(jotun_end.0) + 0.3,
            jotun_start.1.max(jotun_end.1) + 0.3,
        ];
        let jprox = load_proximity(&dir, jbbox);
        let mut jemb =
            NativeCampingEmbedder::with_real_backends(&storage, jprox.as_ref(), Profile::Hiking);
        jemb.set_route(jw.clone(), Some(jotun_end));
        let jout = suggest_overnight(
            &mut jemb,
            &SuggestInput {
                graph: &jg,
                corridor_waypoints: &jw,
                track_walk_m: None,
                corridor_radius_m: Some(1_500.0),
                max_suggestions: Some(50),
            },
        );
        let mut jc: BTreeMap<String, usize> = BTreeMap::new();
        for e in &jout.probe_log {
            *jc.entry(e.reason.clone()).or_default() += 1;
        }
        eprintln!("=== Jotunheimen-ish corridor reasons ===");
        for (r, n) in &jc {
            eprintln!("  {r}: {n}");
        }
        if jc.contains_key("too_close_to_glacier") {
            eprintln!("VERIFIED (real pack): too_close_to_glacier fired near Jotunheimen");
        } else {
            eprintln!(
                "too_close_to_glacier did not fire on this short corridor \
(glacier rings in clip={}, probes={})",
                jemb.glacier_rings.len(),
                jout.probe_log.len()
            );
        }
    } else {
        eprintln!("Jotunheimen approach not routable on foot tiles — glacier reason unchecked on real pack");
    }

    eprintln!("=== Reasons that did NOT fire on Lillehammer→Sjusjøen ===");
    for r in ALL_REASONS {
        let fired = match *r {
            "tier_d_country_*" => counts.keys().any(|k| k.starts_with("tier_d_country_")),
            other => counts.contains_key(other),
        };
        if !fired {
            eprintln!("  never on this corridor: {r}");
        }
    }
}
