//! Phase 4 verification: MobileHome (T6 camper), HGV professional, HGV
//! non-professional on Lillehammer → Sjusjøen — native + wasmtime.

mod common;

use common::native_embedder::{
    data_dir, load_proximity, packs_present, plan_corridor, NativeCampingEmbedder, LILLEHAMMER,
    SJUSJOEN,
};
use driver_break_core::config::{Profile, SafetyConfig};
use driver_break_core::storage::Storage;
use navi_right_to_roam_camping::{
    suggest_overnight, CampingCard, CampingHost, SuggestInput, VehicleClass,
};
use navi_plugin_host::{
    CallOutcome, Capability, HostApi, PluginHost, PluginKvStatus, PluginLimits, PoiWrite, Position,
};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

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
    let pkg = crate_dir.join("pkg");
    std::fs::create_dir_all(&pkg).unwrap();
    std::fs::copy(crate_dir.join("plugin.json"), pkg.join("plugin.json")).unwrap();
    std::fs::copy(&wasm, pkg.join("plugin.wasm")).unwrap();
    std::fs::copy(&wasm, pkg.join("right_to_roam_camping.wasm")).unwrap();
    pkg
}

fn run_guest_job(stage: &Path, job: &serde_json::Value) -> (usize, usize) {
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
    assert_eq!(outcome, CallOutcome::Ok);
    let map = shared.lock().unwrap();
    let raw = map.get("rtr_suggest_result").expect("result");
    let v: serde_json::Value = serde_json::from_str(raw).unwrap();
    (
        v["vehicle_accepted"].as_u64().unwrap_or(0) as usize,
        v["on_foot_accepted"].as_u64().unwrap_or(0) as usize,
    )
}

fn dump_card(label: &str, c: &CampingCard) {
    eprintln!("=== {label} ===");
    eprintln!("accepted={} tier={:?} country={}", c.accepted, c.tier, c.country_iso);
    eprintln!("legal_basis={}", c.legal_basis);
    eprintln!("fire_text={:?}", c.fire_text);
    eprintln!("bare_rock_note={:?}", c.bare_rock_note);
    eprintln!("walk_m={:?}", c.walk_m);
    eprintln!("seed={:?}", c.seed_road_highway);
    eprintln!("sources={:?}", c.sources);
    for n in &c.notes {
        eprintln!("note: {n}");
    }
    eprintln!("disclaimer={}", c.disclaimer);
}

#[test]
fn phase4_norway_profiles_native_and_wasmtime() {
    let dir = data_dir();
    if !packs_present(&dir) {
        eprintln!("SKIP: missing ostlandet packs under {}", dir.display());
        return;
    }
    let stage = build_camping_guest();
    let (graph, waypoints) = plan_corridor(&dir, LILLEHAMMER, SJUSJOEN).expect("plan");
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

    let profiles = [
        ("T6_MobileHome_campervan", Profile::MobileHome, false, VehicleClass::CampervanMotorhome),
        ("HGV_professional", Profile::Truck, true, VehicleClass::Hgv),
        ("HGV_non_professional", Profile::Truck, false, VehicleClass::Hgv),
    ];

    let mut pasted_on_foot = false;

    for (label, profile, pro, expect_class) in profiles {
        let mut emb =
            NativeCampingEmbedder::with_real_backends(&storage, prox.as_ref(), profile);
        emb.is_professional_driver_under_rest_rules = pro;
        emb.set_route(waypoints.clone(), Some(SJUSJOEN));
        emb.buildings.retain(|&(blat, blon)| {
            waypoints
                .iter()
                .any(|w| (blat - w[0]).abs() < 0.008 && (blon - w[1]).abs() < 0.008)
        });
        emb.glacier_rings.clear();

        let vp = emb.vehicle_overnight_profile();
        assert_eq!(vp.class, expect_class);
        assert_eq!(vp.is_professional_driver_under_rest_rules, pro);
        let host_view = emb.vehicle_profile_read();
        assert_eq!(
            host_view.class,
            match expect_class {
                VehicleClass::CampervanMotorhome => "campervan_motorhome",
                VehicleClass::Hgv => "hgv",
                VehicleClass::Car => "car",
                VehicleClass::Unknown => "unknown",
            }
        );

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

        eprintln!("--- PROFILE {label} (native) ---");
        eprintln!(
            "vehicle_cards={} on_foot_accepted={} list_accepted={}",
            out.vehicle.cards.len(),
            out.on_foot_from_here.probes_accepted,
            out.list.probes_accepted
        );
        // No NVDB 809/39 on host → vehicle overnight never invented.
        assert!(
            out.vehicle.cards.is_empty(),
            "{label}: vehicle cards must be empty without NVDB data"
        );
        assert_eq!(
            out.list.probes_accepted, 0,
            "{label}: tent accepts must not sit in main list in motorised mode"
        );
        assert!(
            out.on_foot_from_here.probes_accepted > 0,
            "{label}: expected on-foot tent cards; log={:?}",
            out.probe_log.iter().take(20).collect::<Vec<_>>()
        );
        for c in out.on_foot_from_here.cards.iter().filter(|c| c.accepted) {
            assert!(c.notes.iter().any(|n| n.contains("On foot from here")));
            assert!(c.notes.iter().any(|n| n.contains("motorferdselloven")));
            assert!(c.walk_m.is_some());
            assert!(c.fire_text.is_some(), "on-foot NO card must carry date-gated fire text");
            assert!(c.bare_rock_note.is_some(), "on-foot NO card must carry bare-rock note");
            if !pasted_on_foot {
                dump_card("FULL Norwegian On foot from here card", c);
                pasted_on_foot = true;
            }
        }

        // Exact tent-probe set (no arbitrary take) so wasmtime matches native.
        let tent_entries: Vec<_> = out
            .probe_log
            .iter()
            .filter(|e| e.road_highway != "vehicle" && e.reason != "track_too_short_for_walk_or_pack_min_road")
            .cloned()
            .collect();
        let probes: Vec<(f64, f64)> = tent_entries.iter().map(|e| (e.lat, e.lon)).collect();
        let safety: navi_right_to_roam_camping::OvernightSafety = (&emb.safety).into();
        let countries: Vec<_> = probes
            .iter()
            .map(|&(lat, lon)| {
                (
                    lat,
                    lon,
                    emb.admin_country_iso(lat, lon)
                        .unwrap_or_else(|| "unknown".into()),
                )
            })
            .collect();
        let job = json!({
            "probes": probes.iter().map(|&(a,b)| [a,b]).collect::<Vec<_>>(),
            "max_suggestions": null,
            "buildings": emb.buildings.iter().map(|&(a,b)| [a,b]).collect::<Vec<_>>(),
            "glaciers": [],
            "safety": safety,
            "clock": emb.clock,
            "kv_ok": true,
            "countries": countries,
            "subdivisions": [],
            "travel_mode": "motorised",
            "vehicle_class": match expect_class {
                VehicleClass::CampervanMotorhome => "campervan_motorhome",
                VehicleClass::Hgv => "hgv",
                VehicleClass::Car => "car",
                VehicleClass::Unknown => "unknown",
            },
            "is_professional_driver_under_rest_rules": pro,
        });
        let (g_veh, g_foot) = run_guest_job(&stage, &job);
        let native_foot = tent_entries.iter().filter(|e| e.accepted).count();
        eprintln!("--- PROFILE {label} (wasmtime) vehicle={g_veh} on_foot={g_foot} native_foot={native_foot} ---");
        assert_eq!(g_veh, 0, "{label}: wasm vehicle must stay empty without NVDB");
        assert_eq!(g_foot, native_foot, "{label}: wasm on-foot accepts must match native");
    }

    assert!(pasted_on_foot, "must paste one full Norwegian on-foot card");
}
