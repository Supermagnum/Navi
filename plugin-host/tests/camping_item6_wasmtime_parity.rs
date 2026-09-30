//! WASM guest parity vs native embedder (native host tests only).
//!
//! GATE STAYS CLOSED for Android: this loads the guest through wasmtime on the
//! desktop/CI host. It does not ship the guest in the APK.

mod common;

use common::native_embedder::{
    data_dir, load_proximity, packs_present, plan_corridor, NativeCampingEmbedder, LILLEHAMMER,
    SJUSJOEN,
};
use driver_break_core::config::{Profile, SafetyConfig};
use driver_break_core::storage::Storage;
use navi_plugin_host::{
    CallOutcome, Capability, HostApi, LayerStatus, PluginHost, PluginKvStatus, PluginLimits,
    PoiWrite, Position, TravelModeView,
};
use navi_right_to_roam_camping::{
    suggest_overnight, suggest_overnight_fixed_probes, CampingHost, OvernightSafety, SuggestInput,
};
use serde_json::json;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

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

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn cargo_target_dir() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root().join("target"))
}

fn build_camping_guest() -> PathBuf {
    let crate_dir = workspace_root().join("plugins/right-to-roam-camping");
    let status = Command::new("cargo")
        .args([
            "build",
            "--release",
            "--target",
            "wasm32-unknown-unknown",
            "--manifest-path",
        ])
        .arg(crate_dir.join("Cargo.toml"))
        .status()
        .expect("spawn camping guest build");
    assert!(status.success(), "camping guest wasm build failed");
    let release = cargo_target_dir().join("wasm32-unknown-unknown/release");
    let candidates = [
        release.join("navi_plugin_right_to_roam_camping.wasm"),
        release.join("libnavi_plugin_right_to_roam_camping.wasm"),
    ];
    let wasm = candidates
        .iter()
        .find(|p| p.is_file())
        .cloned()
        .unwrap_or_else(|| panic!("missing camping wasm; tried {candidates:?}"));
    // Stage under pkg/ for PluginHost load_dir.
    let pkg = crate_dir.join("pkg");
    std::fs::create_dir_all(&pkg).unwrap();
    std::fs::copy(crate_dir.join("plugin.json"), pkg.join("plugin.json")).unwrap();
    std::fs::copy(&wasm, pkg.join("plugin.wasm")).unwrap();
    std::fs::copy(&wasm, pkg.join("right_to_roam_camping.wasm")).unwrap();
    pkg
}

fn reason_counts(log: &[navi_right_to_roam_camping::ProbeLogEntry]) -> BTreeMap<String, usize> {
    let mut c = BTreeMap::new();
    for e in log {
        *c.entry(e.reason.clone()).or_default() += 1;
    }
    c
}

fn run_guest_job(stage: &Path, job: &serde_json::Value) -> (usize, usize, BTreeMap<String, usize>) {
    let policy: HashSet<Capability> = Capability::all().iter().copied().collect();
    let host = PluginHost::load_dir(
        stage,
        &policy,
        PluginLimits {
            fuel: 500_000_000,
            timeout_ms: 60_000,
        },
    )
    .expect("load camping guest");

    let shared = std::sync::Arc::new(std::sync::Mutex::new(HashMap::from([(
        "rtr_suggest_job".to_string(),
        job.to_string(),
    )])));
    let shared_logs = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    struct SharedKv {
        kv: std::sync::Arc<std::sync::Mutex<HashMap<String, String>>>,
        logs: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    }
    impl HostApi for SharedKv {
        fn position(&self) -> Option<Position> {
            None
        }
        fn poi_query(&self, _: f64, _: f64, _: f64) -> Vec<PoiWrite> {
            Vec::new()
        }
        fn poi_write(&mut self, _: PoiWrite) -> Result<(), String> {
            Ok(())
        }
        fn log(&mut self, msg: &str) {
            self.logs.lock().unwrap().push(msg.to_string());
        }
        fn plugin_kv_status(&self) -> PluginKvStatus {
            PluginKvStatus::Available
        }
        fn plugin_kv_get(&self, key: &str) -> Option<String> {
            self.kv.lock().unwrap().get(key).cloned()
        }
        fn plugin_kv_set(&mut self, key: &str, value: &str) -> Result<(), String> {
            self.kv
                .lock()
                .unwrap()
                .insert(key.to_string(), value.to_string());
            Ok(())
        }
    }
    let api = SharedKv {
        kv: shared.clone(),
        logs: shared_logs.clone(),
    };
    let outcome = host.call(Box::new(api)).expect("guest call");
    assert_eq!(
        outcome,
        CallOutcome::Ok,
        "guest outcome={outcome:?} logs={:?}",
        shared_logs.lock().unwrap()
    );
    let map = shared.lock().unwrap();
    if !map.contains_key("rtr_suggest_result") {
        panic!(
            "guest must write rtr_suggest_result; job_len={} logs={:?} keys={:?}",
            job.to_string().len(),
            shared_logs.lock().unwrap(),
            map.keys().collect::<Vec<_>>()
        );
    }
    let raw = map.get("rtr_suggest_result").unwrap();
    let v: serde_json::Value = serde_json::from_str(raw).expect("result json");
    let accepted = v["accepted"].as_u64().unwrap() as usize;
    let rejected = v["rejected"].as_u64().unwrap() as usize;
    let mut reasons = BTreeMap::new();
    if let Some(obj) = v["reasons"].as_object() {
        for (k, val) in obj {
            reasons.insert(k.clone(), val.as_u64().unwrap_or(0) as usize);
        }
    }
    (accepted, rejected, reasons)
}

fn build_job_from_native(
    emb: &NativeCampingEmbedder,
    probes: &[(f64, f64)],
    max_suggestions: Option<usize>,
) -> serde_json::Value {
    let safety: OvernightSafety = (&emb.safety).into();
    let countries: Vec<_> = probes
        .iter()
        .map(|&(lat, lon)| {
            (
                lat,
                lon,
                emb.admin_country_iso(lat, lon).unwrap_or_else(|| "unknown".into()),
            )
        })
        .collect();
    let subdivisions: Vec<_> = probes
        .iter()
        .filter_map(|&(lat, lon)| {
            emb.admin_subdivision_iso(lat, lon)
                .map(|iso| (lat, lon, iso))
        })
        .collect();
    json!({
        "probes": probes.iter().map(|&(la, lo)| [la, lo]).collect::<Vec<_>>(),
        "max_suggestions": max_suggestions,
        "buildings": emb.buildings.iter().map(|&(la, lo)| [la, lo]).collect::<Vec<_>>(),
        "glaciers": emb.glacier_rings,
        "safety": safety,
        "clock": emb.clock,
        "kv_ok": emb.kv_available,
        "countries": countries,
        "subdivisions": subdivisions,
    })
}

#[test]
fn empty_host_fail_safes_through_guest() {
    let stage = build_camping_guest();
    let policy: HashSet<Capability> = Capability::all().iter().copied().collect();
    let host = PluginHost::load_dir(
        &stage,
        &policy,
        PluginLimits {
            fuel: 5_000_000,
            timeout_ms: 2_000,
        },
    )
    .expect("load guest");
    let logs = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    struct LogHost {
        logs: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    }
    impl HostApi for LogHost {
        fn position(&self) -> Option<Position> {
            None
        }
        fn poi_query(&self, _: f64, _: f64, _: f64) -> Vec<PoiWrite> {
            Vec::new()
        }
        fn poi_write(&mut self, _: PoiWrite) -> Result<(), String> {
            Ok(())
        }
        fn log(&mut self, msg: &str) {
            self.logs.lock().unwrap().push(msg.to_string());
        }
    }
    let outcome = host
        .call(Box::new(LogHost {
            logs: logs.clone(),
        }))
        .expect("call");
    assert_eq!(outcome, CallOutcome::Ok);
    let joined = logs.lock().unwrap().join("\n");
    eprintln!("EmptyHost guest logs:\n{joined}");
    assert!(
        joined.contains("failsafe") && joined.contains("no rtr_suggest_job"),
        "guest must observe EmptyHost fail-safes; logs={joined}"
    );
}

#[test]
fn wasmtime_lillehammer_parity_cap12_and_uncapped() {
    let dir = data_dir();
    if !packs_present(&dir) {
        eprintln!("SKIP: missing ostlandet packs");
        return;
    }
    let stage = build_camping_guest();

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

    for &cap in &[Some(12usize), Some(200usize)] {
        let mut emb =
            NativeCampingEmbedder::with_real_backends(&storage, prox.as_ref(), Profile::Hiking);
        emb.set_route(waypoints.clone(), Some(SJUSJOEN));
        // Bound building set so the wasm job JSON fits the guest kv buffer.
        emb.buildings.retain(|&(blat, blon)| {
            waypoints.iter().any(|w| {
                (blat - w[0]).abs() < 0.008 && (blon - w[1]).abs() < 0.008
            })
        });
        emb.glacier_rings.clear();
        let native = suggest_overnight(
            &mut emb,
            &SuggestInput {
                graph: &graph,
                corridor_waypoints: &waypoints,
                track_walk_m: None,
                corridor_radius_m: None,
                max_suggestions: cap,
            },
        );
        let native_counts = reason_counts(&native.probe_log);
        // Fixed-probe / wasm paths only cover evaluate_probe — seed track-walk
        // failures stay host-side (RouteGraph).
        let eval_entries: Vec<_> = native
            .probe_log
            .iter()
            .filter(|e| e.reason != "track_too_short_for_walk_or_pack_min_road")
            .cloned()
            .collect();
        let probes: Vec<(f64, f64)> = eval_entries.iter().map(|e| (e.lat, e.lon)).collect();
        let native_eval_counts = reason_counts(&eval_entries);

        let mut emb2 = emb.clone_for_replay();
        let fixed = suggest_overnight_fixed_probes(&mut emb2, &probes, None);
        let fixed_counts = reason_counts(&fixed.probe_log);
        assert_eq!(
            fixed_counts, native_eval_counts,
            "fixed-probe native must match evaluate_probe subset for cap={cap:?}"
        );

        let job = build_job_from_native(&emb2, &probes, None);
        let job_len = job.to_string().len();
        eprintln!(
            "cap={cap:?} job_bytes={job_len} probes={} buildings={}",
            probes.len(),
            emb2.buildings.len()
        );
        assert!(
            job_len < 900_000,
            "job too large for guest 1MiB buffer: {job_len}"
        );
        let (g_acc, g_rej, g_counts) = run_guest_job(&stage, &job);
        eprintln!("cap={cap:?} native_all={native_counts:?}");
        eprintln!("cap={cap:?} native_eval={native_eval_counts:?}");
        eprintln!("cap={cap:?} guest accepted={g_acc} rejected={g_rej} reasons={g_counts:?}");
        assert_eq!(g_counts, native_eval_counts, "wasm guest reasons must match native evaluate_probe");
        assert_eq!(g_acc, native.list.probes_accepted);
        assert_eq!(
            g_rej,
            eval_entries.iter().filter(|e| !e.accepted).count()
        );
    }

    eprintln!(
        "GATE STAYS CLOSED for Android: wasmtime parity verified on native host only"
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
}
