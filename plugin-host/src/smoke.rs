//! Isolation smoke checks shared by host `tests/isolation.rs` and the
//! `android_isolation_smoke` binary (aarch64-linux-android execution).
//!
//! Guests must already be staged as directories containing `plugin.json` +
//! `plugin.wasm` — the Android smoke path cannot spawn `cargo` on-device.

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

use crate::{CallOutcome, Capability, HostApi, PluginHost, PluginLimits, PoiWrite, Position};

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

fn full_policy() -> HashSet<Capability> {
    [
        Capability::Log,
        Capability::PositionRead,
        Capability::PoiQuery,
        Capability::PoiWrite,
        Capability::WeatherRead,
    ]
    .into_iter()
    .collect()
}

/// Run the three isolation checks against pre-staged plugin fixture dirs.
///
/// `log_hello_dir` and `busy_loop_dir` must each contain `plugin.json` and
/// `plugin.wasm` (same layout as `tests/isolation.rs` staging).
pub fn run_isolation_smoke(log_hello_dir: &Path, busy_loop_dir: &Path) -> Result<()> {
    check_capability_deny(log_hello_dir)?;
    check_log_hello(log_hello_dir)?;
    check_busy_loop(busy_loop_dir)?;
    Ok(())
}

pub fn check_capability_deny(log_hello_dir: &Path) -> Result<()> {
    let mut policy = HashSet::new();
    policy.insert(Capability::PositionRead); // log not granted
    let err = match PluginHost::load_dir(log_hello_dir, &policy, PluginLimits::default()) {
        Err(e) => e,
        Ok(_) => bail!("expected capability denial, load succeeded"),
    };
    let msg = err.to_string();
    if !(msg.contains("capability") || msg.contains("log")) {
        bail!("expected capability denial, got {msg}");
    }
    Ok(())
}

pub fn check_log_hello(log_hello_dir: &Path) -> Result<()> {
    let host = PluginHost::load_dir(log_hello_dir, &full_policy(), PluginLimits::default())
        .context("load log-hello")?;
    let logs = Arc::new(Mutex::new(Vec::new()));
    let api = MockApi {
        logs: Arc::clone(&logs),
        position: Some(Position {
            lat: 61.0,
            lon: 10.0,
        }),
        pois: Vec::new(),
    };
    let outcome = host.call(Box::new(api)).context("call log-hello")?;
    if outcome != CallOutcome::Ok {
        bail!("log-hello expected Ok, got {outcome:?}");
    }
    let lines = logs.lock().unwrap();
    if !lines.iter().any(|l| l.contains("log_hello")) {
        bail!("expected log line containing log_hello, got {lines:?}");
    }
    Ok(())
}

pub fn check_busy_loop(busy_loop_dir: &Path) -> Result<()> {
    let host = PluginHost::load_dir(
        busy_loop_dir,
        &full_policy(),
        PluginLimits {
            fuel: 50_000,
            timeout_ms: 60,
        },
    )
    .context("load busy-loop")?;

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
    let outcome = host.call(Box::new(api)).context("call busy-loop")?;
    let elapsed = started.elapsed();

    if !matches!(outcome, CallOutcome::FuelExhausted | CallOutcome::Timeout) {
        bail!("busy loop must be terminated, got {outcome:?}");
    }
    if elapsed >= Duration::from_secs(2) {
        bail!("kill must be prompt, took {elapsed:?}");
    }

    heartbeat
        .join()
        .map_err(|_| anyhow::anyhow!("host heartbeat thread panicked"))?;
    if !*host_thread_ok.lock().unwrap() {
        bail!("routing/UI stand-in thread must remain responsive during plugin kill");
    }
    Ok(())
}
