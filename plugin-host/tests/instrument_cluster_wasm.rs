//! `instrument-cluster` guest under wasmtime: nav_guidance_read +
//! vehicle_signal_publish end to end (log sink).
//!
//! Needs the `wasm32-unknown-unknown` target (pinned in `rust-toolchain.toml`).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};

use navi_plugin_host::{
    CallOutcome, Capability, HostApi, PluginHost, PluginLimits, PoiWrite, Position,
};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn built_wasm() -> &'static Path {
    static WASM: OnceLock<PathBuf> = OnceLock::new();
    WASM.get_or_init(|| {
        let root = workspace_root();
        let status = Command::new("cargo")
            .args([
                "build",
                "--release",
                "--target",
                "wasm32-unknown-unknown",
                "--manifest-path",
            ])
            .arg(root.join("plugins/instrument-cluster/Cargo.toml"))
            .status()
            .expect("spawn cargo build");
        assert!(status.success(), "instrument-cluster guest build failed");
        let target = std::env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("target"));
        let release = target.join("wasm32-unknown-unknown/release");
        [
            "navi_plugin_instrument_cluster.wasm",
            "libnavi_plugin_instrument_cluster.wasm",
        ]
        .iter()
        .map(|n| release.join(n))
        .find(|p| p.is_file())
        .unwrap_or_else(|| panic!("instrument-cluster wasm not found under {release:?}"))
    })
}

fn stage(dest: &Path) {
    let crate_dir = workspace_root().join("plugins/instrument-cluster");
    std::fs::create_dir_all(dest).unwrap();
    std::fs::copy(crate_dir.join("plugin.json"), dest.join("plugin.json")).unwrap();
    std::fs::copy(built_wasm(), dest.join("plugin.wasm")).unwrap();
}

struct Shared {
    guidance: String,
    publishes: Vec<String>,
    logs: Vec<String>,
}

#[derive(Clone)]
struct World(Arc<Mutex<Shared>>);

impl HostApi for World {
    fn position(&self) -> Option<Position> {
        None
    }
    fn poi_query(&self, _: f64, _: f64, _: f64) -> Vec<PoiWrite> {
        Vec::new()
    }
    fn poi_write(&mut self, _: PoiWrite) -> Result<(), String> {
        Ok(())
    }
    fn log(&mut self, m: &str) {
        self.0.lock().unwrap().logs.push(m.to_string());
    }
    fn nav_guidance_read(&self) -> String {
        self.0.lock().unwrap().guidance.clone()
    }
    fn vehicle_signal_publish(&mut self, payload_json: &str) -> Result<(), String> {
        self.0
            .lock()
            .unwrap()
            .publishes
            .push(payload_json.to_string());
        Ok(())
    }
}

fn granted() -> HashSet<Capability> {
    [
        Capability::Log,
        Capability::PositionRead,
        Capability::NavGuidanceRead,
        Capability::VehicleSignalPublish,
    ]
    .into_iter()
    .collect()
}

#[test]
fn instrument_cluster_publishes_cluster_v1_from_guidance() {
    let dir = tempfile::tempdir().unwrap();
    stage(dir.path());

    let world = World(Arc::new(Mutex::new(Shared {
        guidance: r#"{"schema":"navi.guidance.v1","route_active":true,"position":{"lat":60.56,"lon":11.25,"speed_kmh":72.5},"speed_limit_kmh":80,"overspeed":false,"maneuver":{"type":"turn_right","icon_key":"nav_turn_right","distance_m":320,"street_name":"Kvikneveien"},"warning":null,"trip":null,"rest":null,"eco":{"enabled":true,"profile":"Car"},"attitude":null,"route":{"polyline":""}}"#.into(),
        publishes: Vec::new(),
        logs: Vec::new(),
    })));

    let host = PluginHost::load_dir(dir.path(), &granted(), PluginLimits::default())
        .expect("load instrument-cluster");
    let outcome = host
        .call(Box::new(world.clone()))
        .expect("call plugin_main");
    assert_eq!(outcome, CallOutcome::Ok);

    let s = world.0.lock().unwrap();
    assert_eq!(s.publishes.len(), 1, "logs={:?}", s.logs);
    let pub_json = &s.publishes[0];
    assert!(
        pub_json.contains("\"schema\":\"navi.cluster.v1\"")
            || pub_json.contains("\"schema\": \"navi.cluster.v1\""),
        "{pub_json}"
    );
    assert!(pub_json.contains("nav_turn_right"), "{pub_json}");
    assert!(
        pub_json.contains("\"l_per_scandinavian_mil\":null"),
        "{pub_json}"
    );
    assert!(pub_json.contains("\"l_per_us_mile\":null"), "{pub_json}");
    assert!(
        s.logs
            .iter()
            .any(|l| l.contains("published navi.cluster.v1")),
        "logs={:?}",
        s.logs
    );
}

#[test]
fn instrument_cluster_denied_without_vehicle_signal_publish() {
    let dir = tempfile::tempdir().unwrap();
    stage(dir.path());

    let mut policy = granted();
    policy.remove(&Capability::VehicleSignalPublish);
    let err = match PluginHost::load_dir(dir.path(), &policy, PluginLimits::default()) {
        Err(e) => e,
        Ok(_) => panic!("must deny without vehicle_signal_publish"),
    };
    assert!(err.to_string().contains("vehicle_signal_publish"), "{err}");
}

#[test]
fn instrument_cluster_idle_guidance_still_publishes_null_fuel() {
    let dir = tempfile::tempdir().unwrap();
    stage(dir.path());

    let world = World(Arc::new(Mutex::new(Shared {
        guidance: HostApi::nav_guidance_read(&EmptyIdle),
        publishes: Vec::new(),
        logs: Vec::new(),
    })));

    let host = PluginHost::load_dir(dir.path(), &granted(), PluginLimits::default()).unwrap();
    host.call(Box::new(world.clone())).unwrap();

    let s = world.0.lock().unwrap();
    assert_eq!(s.publishes.len(), 1, "logs={:?}", s.logs);
    let pub_json = &s.publishes[0];
    assert!(pub_json.contains("navi.cluster.v1"), "{pub_json}");
    assert!(pub_json.contains("\"route_active\":false"), "{pub_json}");
    assert!(
        pub_json.contains("\"l_per_scandinavian_mil\":null"),
        "{pub_json}"
    );
}

struct EmptyIdle;
impl HostApi for EmptyIdle {
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
