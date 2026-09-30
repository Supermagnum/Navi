//! Item 6: wasmtime parity vs native embedder — gate stays closed.
//!
//! There is no right-to-roam camping `.wasm` guest yet (engine is native-only in
//! Phase 2). This test records the native Lillehammer→Sjusjøen reference and
//! re-checks HostApi fail-safe defaults. It does **not** open the wasmtime gate.

mod common;

use common::native_embedder::{
    data_dir, load_proximity, packs_present, plan_corridor, NativeCampingEmbedder, LILLEHAMMER,
    SJUSJOEN,
};
use driver_break_core::config::{Profile, SafetyConfig};
use driver_break_core::storage::Storage;
use navi_plugin_host::{HostApi, LayerStatus, PluginKvStatus, PoiWrite, Position, TravelModeView};
use navi_right_to_roam_camping::{suggest_overnight, SuggestInput};
use std::collections::BTreeMap;
use std::path::PathBuf;

struct EmptyHost;
impl HostApi for EmptyHost {
    fn position(&self) -> Option<Position> {
        None
    }
    fn poi_query(&self, _: f64, _: f64, _: f64) -> Vec<PoiWrite> {
        Vec::new()
    }
    fn poi_write(&mut self, _: PoiWrite) -> Result<(), String> {
        Ok(())
    }
    fn log(&mut self, _: &str) {}
}

#[test]
fn wasmtime_gate_stays_closed_no_camping_guest() {
    let guest = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../plugins/right-to-roam-camping/pkg/right_to_roam_camping.wasm");
    assert!(
        !guest.is_file(),
        "unexpected camping wasm at {} — gate must stay closed until parity lands",
        guest.display()
    );
    eprintln!(
        "GATE CLOSED: no camping wasm guest at {}; cannot diff Lillehammer scenario \
through wasmtime host yet",
        guest.display()
    );
}

#[test]
fn fail_safe_defaults_match_audit_table() {
    let h = EmptyHost;
    assert!(h.safety_config_read().is_none());
    assert!(h.clock_read().is_none());
    assert_eq!(h.plugin_kv_status(), PluginKvStatus::Unavailable);
    assert!(h.admin_region_read(61.1, 10.5).country_iso.is_none());
    assert_eq!(h.protected_area_query(61.1, 10.5).status, LayerStatus::Unknown);
    assert_eq!(h.landcover_query(61.1, 10.5).status, LayerStatus::Unknown);
    assert_eq!(h.land_tenure_query(61.1, 10.5).manager_type, "unknown");
    assert_eq!(h.travel_mode_read(), TravelModeView::Unknown);
    assert!(h.route_read().waypoints.is_empty());
    eprintln!("fail-safe HostApi defaults verified (EmptyHost; same as wasmtime undeclared caps)");
}

#[test]
fn native_lillehammer_reference_snapshot() {
    let dir = data_dir();
    if !packs_present(&dir) {
        eprintln!("SKIP: missing ostlandet packs");
        return;
    }
    let (graph, waypoints) =
        plan_corridor(&dir, LILLEHAMMER, SJUSJOEN).expect("plan");
    let bbox = [
        LILLEHAMMER.0.min(SJUSJOEN.0) - 0.2,
        LILLEHAMMER.1.min(SJUSJOEN.1) - 0.2,
        LILLEHAMMER.0.max(SJUSJOEN.0) + 0.2,
        LILLEHAMMER.1.max(SJUSJOEN.1) + 0.2,
    ];
    let prox = load_proximity(&dir, bbox);
    let storage = Storage::open_in_memory().unwrap();
    {
        let store = driver_break_core::storage::ConfigStore::new(&storage);
        store.save_safety_config(&SafetyConfig::default()).unwrap();
    }
    let mut emb =
        NativeCampingEmbedder::with_real_backends(&storage, prox.as_ref(), Profile::Hiking);
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
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for e in &out.probe_log {
        *counts.entry(e.reason.clone()).or_default() += 1;
    }
    eprintln!("NATIVE REFERENCE Lillehammer→Sjusjøen (cap=12): {counts:?}");
    eprintln!(
        "accepted_tiers={:?}",
        out.list
            .cards
            .iter()
            .filter(|c| c.accepted)
            .map(|c| (c.tier, c.country_iso.as_str()))
            .collect::<Vec<_>>()
    );
    // Stable expectations for future wasmtime diff.
    assert_eq!(counts.get("accepted").copied().unwrap_or(0), 12);
    assert!(counts.get("too_close_to_building").copied().unwrap_or(0) > 0);
    assert!(
        out.list.cards.iter().filter(|c| c.accepted).all(|c| c.country_iso == "no")
    );
    eprintln!(
        "PARITY STATUS: wasmtime side not runnable (no guest). Native reference recorded. \
GATE CLOSED."
    );
}
