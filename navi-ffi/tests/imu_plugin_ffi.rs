//! IMU plugin through the public UniFFI surface: the staged wasm guest runs under
//! wasmtime with a fake Android `SensorPlatform`, exactly as the app drives it.
//!
//! Needs the `wasm32-unknown-unknown` target (pinned in `rust-toolchain.toml`).
//! One test function: the plugin session is process-global.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::{Duration, Instant};

use navi::{
    imu_plugin_configure, imu_plugin_install_guest, imu_plugin_is_enabled,
    imu_plugin_registered_sensor_codes, imu_plugin_release_sensors, imu_plugin_request_calibrate,
    imu_plugin_request_reset, imu_plugin_set_enabled, imu_plugin_tick, ImuCallKind,
    ImuSensorSample, SensorPlatform,
};

const GRAVITY_CODE: u32 = 3;
const GYRO_CODE: u32 = 4;
const AMBIENT_TEMPERATURE_CODE: u32 = 2;
const G: f64 = 9.806_65;
const TICK: Duration = Duration::from_millis(120);

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn built_wasm() -> PathBuf {
    let root = workspace_root();
    let status = Command::new("cargo")
        .args([
            "build",
            "--release",
            "--target",
            "wasm32-unknown-unknown",
            "--manifest-path",
        ])
        .arg(root.join("plugins/imu/Cargo.toml"))
        .status()
        .expect("spawn cargo build");
    assert!(status.success(), "imu guest build failed");
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target"));
    let release = target.join("wasm32-unknown-unknown/release");
    ["navi_plugin_imu.wasm", "libnavi_plugin_imu.wasm"]
        .iter()
        .map(|n| release.join(n))
        .find(|p| p.is_file())
        .unwrap_or_else(|| panic!("imu wasm not found under {release:?}"))
}

#[derive(Default)]
struct Recorded {
    registered: Vec<(u32, u32)>,
    unregistered: Vec<u32>,
    up: [f64; 3],
}

/// Fake `SensorManager`: gravity + gyroscope present, no ambient temperature.
struct FakePlatform {
    started: Instant,
    state: Mutex<Recorded>,
    samples: Mutex<HashMap<u32, ()>>,
}

impl FakePlatform {
    fn set_up(&self, up: [f64; 3]) {
        self.state.lock().unwrap().up = up;
    }
}

impl SensorPlatform for FakePlatform {
    fn is_sensor_available(&self, kind_code: u32) -> bool {
        kind_code == GRAVITY_CODE || kind_code == GYRO_CODE
    }

    fn register_sensor(&self, kind_code: u32, sampling_period_us: u32) -> bool {
        self.state
            .lock()
            .unwrap()
            .registered
            .push((kind_code, sampling_period_us));
        self.samples.lock().unwrap().insert(kind_code, ());
        true
    }

    fn unregister_sensor(&self, kind_code: u32) {
        self.state.lock().unwrap().unregistered.push(kind_code);
        self.samples.lock().unwrap().remove(&kind_code);
    }

    fn latest_sensor_sample(&self, kind_code: u32) -> Option<ImuSensorSample> {
        if !self.samples.lock().unwrap().contains_key(&kind_code) {
            return None;
        }
        let values = match kind_code {
            GRAVITY_CODE => self.state.lock().unwrap().up.to_vec(),
            GYRO_CODE => vec![0.0, 0.0, 0.0],
            _ => return None,
        };
        Some(ImuSensorSample {
            values,
            timestamp_ms: self.started.elapsed().as_millis() as i64,
            accuracy: 3,
        })
    }
}

fn tilted_mount() -> [f64; 3] {
    // Roughly 15 degrees of pitch and 10 degrees of roll in the device frame.
    let (p, r) = (15f64.to_radians(), 10f64.to_radians());
    [G * r.sin(), G * p.sin() * r.cos(), G * p.cos() * r.cos()]
}

fn tick_json() -> serde_json::Value {
    sleep(TICK);
    let r = imu_plugin_tick();
    assert_eq!(r.kind, ImuCallKind::Ok, "tick failed: {}", r.message);
    serde_json::from_str(&r.state_json.expect("state json")).expect("state parses")
}

fn stage(wasm: &Path) {
    let manifest = std::fs::read_to_string(workspace_root().join("plugins/imu/plugin.json"))
        .expect("manifest");
    let bytes = std::fs::read(wasm).expect("wasm bytes");
    assert_eq!(imu_plugin_install_guest(manifest, bytes), "OK");
}

#[test]
fn imu_plugin_end_to_end_through_ffi_surface() {
    let wasm = built_wasm();
    let dir = tempfile::tempdir().unwrap();
    let platform = Arc::new(FakePlatform {
        started: Instant::now(),
        state: Mutex::new(Recorded::default()),
        samples: Mutex::new(HashMap::new()),
    });
    platform.set_up(tilted_mount());

    assert_eq!(
        imu_plugin_configure(dir.path().display().to_string(), platform.clone()),
        "OK"
    );
    stage(&wasm);
    assert!(dir.path().join("plugins/imu/plugin.wasm").is_file());

    // Default OFF: no guest run, no sensor registered.
    assert!(!imu_plugin_is_enabled());
    assert_eq!(imu_plugin_tick().kind, ImuCallKind::Disabled);
    assert!(platform.state.lock().unwrap().registered.is_empty());
    assert!(imu_plugin_request_calibrate().starts_with("FAIL"));

    assert!(imu_plugin_set_enabled(true).starts_with("OK"));
    let state = tick_json();
    assert_eq!(state["status"], "uncalibrated");
    assert_eq!(state["sensors"]["gravity"], true);
    assert_eq!(state["sensors"]["magnetic_field"], false);
    assert!(state["tilt_deg"].as_f64().unwrap() > 5.0);
    assert_eq!(
        state["instruction"],
        "Find a level concrete surface, like the floor of your garage, then tap Calibrate. The unit will then use the current values as level."
    );
    // Only sensors the guest reads are registered, at the hub sampling period;
    // an absent ambient-temperature sensor is never registered.
    let regs = platform.state.lock().unwrap().registered.clone();
    assert!(regs.contains(&(GRAVITY_CODE, 100_000)));
    assert!(regs.iter().all(|(c, _)| *c != AMBIENT_TEMPERATURE_CODE));
    assert!(imu_plugin_registered_sensor_codes().contains(&GRAVITY_CODE));

    // Calibrate to level (needs about one second of steady samples).
    assert_eq!(imu_plugin_request_calibrate(), "OK");
    let mut calibrated = None;
    for _ in 0..30 {
        let s = tick_json();
        if s["calibrated"] == true {
            calibrated = Some(s);
            break;
        }
    }
    let calibrated = calibrated.expect("calibration completed");
    assert_eq!(calibrated["status"], "calibrated");
    assert!(calibrated["tilt_deg"].as_f64().unwrap() < 0.5);
    assert!(calibrated["calibrated_date"].is_string());

    // Tilt still updates when the device is raised from the calibrated level.
    platform.set_up([0.0, 0.7 * G, 0.7 * G]);
    let tilted = tick_json();
    assert!(tilted["tilt_deg"].as_f64().unwrap() > 5.0);

    // Reset removes the stored calibration.
    assert_eq!(imu_plugin_request_reset(), "OK");
    let reset = tick_json();
    assert_eq!(reset["calibrated"], false);
    assert_eq!(reset["status"], "uncalibrated");

    // Unregister when unused.
    imu_plugin_release_sensors();
    assert!(imu_plugin_registered_sensor_codes().is_empty());
    assert!(platform
        .state
        .lock()
        .unwrap()
        .unregistered
        .contains(&GRAVITY_CODE));

    // Disabling stops the guest and releases sensors.
    assert!(imu_plugin_set_enabled(false).starts_with("OK"));
    assert_eq!(imu_plugin_tick().kind, ImuCallKind::Disabled);
    assert!(imu_plugin_registered_sensor_codes().is_empty());
}
