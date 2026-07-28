use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use navi_plugin_host::{
    CallOutcome, Capability, HostApi, PluginHost, PluginLimits, PoiWrite, Position,
};

struct MockApi {
    logs: Arc<Mutex<Vec<String>>>,
    position: Option<Position>,
    pois: Vec<PoiWrite>,
}

impl HostApi for MockApi {
    fn position(&self) -> Option<Position> {
        self.position
    }

    fn poi_query(&self, _lat: f64, _lon: f64, _radius_m: f64) -> Vec<PoiWrite> {
        self.pois.clone()
    }

    fn poi_write(&mut self, poi: PoiWrite) -> Result<(), String> {
        self.pois.push(poi);
        Ok(())
    }

    fn log(&mut self, message: &str) {
        self.logs.lock().unwrap().push(message.to_string());
    }
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

fn build_plugin(crate_dir: &Path) -> PathBuf {
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
        .expect("spawn cargo build for plugin");
    assert!(status.success(), "plugin build failed for {crate_dir:?}");

    let name = crate_dir
        .file_name()
        .unwrap()
        .to_string_lossy()
        .replace('-', "_");
    // crate navi-plugin-log-hello -> libnavi_plugin_log_hello.wasm
    let pkg = format!(
        "navi_plugin_{}",
        crate_dir
            .file_name()
            .unwrap()
            .to_string_lossy()
            .replace('-', "_")
    );
    let release = cargo_target_dir().join("wasm32-unknown-unknown/release");
    let wasm = release.join(format!("lib{pkg}.wasm"));
    // Also try without lib prefix / with hyphenated names cargo emits
    let candidates = [
        wasm.clone(),
        release.join(format!("{pkg}.wasm")),
        release.join(format!("libnavi_plugin_{name}.wasm")),
    ];
    for c in &candidates {
        if c.is_file() {
            return c.clone();
        }
    }
    panic!("compiled wasm not found for {crate_dir:?}; tried {candidates:?}");
}

fn stage_plugin(plugin_dir_name: &str) -> PathBuf {
    let root = workspace_root();
    let src = root.join("plugins").join(plugin_dir_name);
    let wasm_src = build_plugin(&src);
    // Unique per call: isolation tests run in parallel and must not race on a
    // shared plugin.json (empty partial write → "EOF while parsing a value").
    let unique = format!(
        "{}-{}-{}",
        plugin_dir_name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    );
    let stage = root.join("target").join("plugin-fixtures").join(unique);
    std::fs::create_dir_all(&stage).unwrap();
    std::fs::copy(src.join("plugin.json"), stage.join("plugin.json")).unwrap();
    std::fs::copy(&wasm_src, stage.join("plugin.wasm")).unwrap();
    stage
}

fn full_policy() -> HashSet<Capability> {
    [
        Capability::Log,
        Capability::PositionRead,
        Capability::PoiQuery,
        Capability::PoiWrite,
    ]
    .into_iter()
    .collect()
}

#[test]
fn load_call_log_hello_end_to_end() {
    let stage = stage_plugin("log-hello");
    let host = PluginHost::load_dir(&stage, &full_policy(), PluginLimits::default()).unwrap();
    let logs = Arc::new(Mutex::new(Vec::new()));
    let api = MockApi {
        logs: Arc::clone(&logs),
        position: Some(Position {
            lat: 61.0,
            lon: 10.0,
        }),
        pois: Vec::new(),
    };
    let outcome = host.call(Box::new(api)).unwrap();
    assert_eq!(outcome, CallOutcome::Ok);
    let lines = logs.lock().unwrap();
    assert!(
        lines.iter().any(|l| l.contains("log_hello")),
        "expected log line, got {lines:?}"
    );
}

#[test]
fn busy_loop_is_killed_by_fuel_or_timeout_without_blocking_host() {
    let stage = stage_plugin("busy-loop");
    let host = PluginHost::load_dir(
        &stage,
        &full_policy(),
        PluginLimits {
            fuel: 50_000,
            timeout_ms: 60,
        },
    )
    .unwrap();

    let host_thread_ok = Arc::new(Mutex::new(false));
    let flag = Arc::clone(&host_thread_ok);
    let heartbeat = thread::spawn(move || {
        let start = Instant::now();
        let mut n = 0u64;
        while start.elapsed() < Duration::from_millis(400) {
            n = n.wrapping_add(1);
            thread::sleep(Duration::from_millis(5));
        }
        assert!(n > 10, "host heartbeat must keep progressing");
        *flag.lock().unwrap() = true;
    });

    let logs = Arc::new(Mutex::new(Vec::new()));
    let api = MockApi {
        logs,
        position: None,
        pois: Vec::new(),
    };
    let started = Instant::now();
    let outcome = host.call(Box::new(api)).unwrap();
    let elapsed = started.elapsed();

    assert!(
        matches!(outcome, CallOutcome::FuelExhausted | CallOutcome::Timeout),
        "busy loop must be terminated, got {outcome:?}"
    );
    assert!(
        elapsed < Duration::from_secs(2),
        "kill must be prompt, took {elapsed:?}"
    );

    heartbeat.join().unwrap();
    assert!(
        *host_thread_ok.lock().unwrap(),
        "routing/UI stand-in thread must remain responsive during plugin kill"
    );
}

#[test]
fn manifest_capability_checked_before_load() {
    let stage = stage_plugin("log-hello");
    let mut policy = HashSet::new();
    policy.insert(Capability::PositionRead); // log not granted
    let err = match PluginHost::load_dir(&stage, &policy, PluginLimits::default()) {
        Err(e) => e,
        Ok(_) => panic!("expected capability denial, load succeeded"),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("capability") || msg.contains("log"),
        "expected capability denial, got {msg}"
    );
}
