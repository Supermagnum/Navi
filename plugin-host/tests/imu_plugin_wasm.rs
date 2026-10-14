//! IMU guest under wasmtime: sensors_read + plugin_kv + clock_read end to end.
//!
//! Each `PluginHost::call` is one guest tick (new instance), so everything that
//! must survive between ticks goes through `FilePluginKv`, exactly as on device.
//! Needs the `wasm32-unknown-unknown` target (pinned in `rust-toolchain.toml`).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};

use navi_plugin_host::{
    CallOutcome, Capability, ClockView, FilePluginKv, HostApi, PluginHost, PluginKvStatus,
    PluginLimits, PoiWrite, Position, SensorAvailabilityView, SensorBackend, SensorHub,
    SensorHubConfig, SensorKind, SensorReadView, SensorSample, SensorStatus,
};
use navi_plugin_imu_engine::math::{mat_mul, mat_vec, rot_x, rot_y, Vec3};
use navi_plugin_imu_engine::{
    NoticeKind, State, Status, CALIBRATION_INSTRUCTION, KEY_CMD, KEY_MOUNT, KEY_STATE,
};

const G: f64 = 9.806_65;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

/// Build the guest once per test binary; returns the compiled `.wasm`.
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
    })
}

fn stage_imu(dest: &Path) {
    let crate_dir = workspace_root().join("plugins/imu");
    std::fs::copy(crate_dir.join("plugin.json"), dest.join("plugin.json")).unwrap();
    std::fs::copy(built_wasm(), dest.join("plugin.wasm")).unwrap();
}

struct Shared {
    sensors: HashMap<SensorKind, SensorSample>,
    available: HashSet<SensorKind>,
    kv: FilePluginKv,
    clock: Option<ClockView>,
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
    fn sensor_availability(&self) -> SensorAvailabilityView {
        let s = self.0.lock().unwrap();
        let mut available: Vec<_> = s.available.iter().copied().collect();
        available.sort();
        SensorAvailabilityView { available }
    }
    fn sensor_read(&self, kind: SensorKind) -> SensorReadView {
        let s = self.0.lock().unwrap();
        if !s.available.contains(&kind) {
            return SensorReadView::default();
        }
        match s.sensors.get(&kind) {
            Some(sample) => SensorReadView {
                status: SensorStatus::Ok,
                sample: Some(sample.clone()),
            },
            None => SensorReadView {
                status: SensorStatus::NoSample,
                sample: None,
            },
        }
    }
    fn clock_read(&self) -> Option<ClockView> {
        self.0.lock().unwrap().clock.clone()
    }
    fn plugin_kv_status(&self) -> PluginKvStatus {
        PluginKvStatus::Available
    }
    fn plugin_kv_get(&self, key: &str) -> Option<String> {
        self.0.lock().unwrap().kv.get(key)
    }
    fn plugin_kv_set(&mut self, key: &str, value: &str) -> Result<(), String> {
        self.0
            .lock()
            .unwrap()
            .kv
            .set(key, value)
            .map_err(|e| e.to_string())
    }
}

fn policy() -> HashSet<Capability> {
    Capability::all().iter().copied().collect()
}

fn mount_truth() -> [[f64; 3]; 3] {
    mat_mul(&rot_x(15f64.to_radians()), &rot_y(10f64.to_radians()))
}

fn device_up(pitch: f64, roll: f64, i: usize) -> Vec3 {
    let (p, r) = (pitch.to_radians(), roll.to_radians());
    let u = mat_vec(
        &mount_truth(),
        [-r.sin() * p.cos(), p.sin(), r.cos() * p.cos()],
    );
    let n = ((i * 37 % 11) as f64 - 5.0) * 0.0007;
    [u[0] * G + n, u[1] * G - n, u[2] * G + n * 0.5]
}

struct Rig {
    host: PluginHost,
    world: World,
    kv_path: PathBuf,
    _dir: tempfile::TempDir,
    tick_no: usize,
}

impl Rig {
    fn new(kinds: &[SensorKind]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let plugin_dir = dir.path().join("imu");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        stage_imu(&plugin_dir);
        let host = PluginHost::load_dir(&plugin_dir, &policy(), PluginLimits::default())
            .expect("load imu plugin");
        let kv_path = dir.path().join("kv/imu.json");
        let world = World(Arc::new(Mutex::new(Shared {
            sensors: HashMap::new(),
            available: kinds.iter().copied().collect(),
            kv: FilePluginKv::open(&kv_path).unwrap(),
            clock: None,
            logs: Vec::new(),
        })));
        Self {
            host,
            world,
            kv_path,
            _dir: dir,
            tick_no: 0,
        }
    }

    fn put(&self, kind: SensorKind, values: Vec3, t: i64) {
        self.world.0.lock().unwrap().sensors.insert(
            kind,
            SensorSample {
                kind,
                values: values.to_vec(),
                timestamp_ms: t,
                accuracy: Some(3),
            },
        );
    }

    fn command(&self, cmd: &str) {
        self.world
            .0
            .lock()
            .unwrap()
            .kv
            .set(KEY_CMD, &format!(r#"{{"cmd":"{cmd}"}}"#))
            .unwrap();
    }

    fn state(&self) -> State {
        let json = self
            .world
            .0
            .lock()
            .unwrap()
            .kv
            .get(KEY_STATE)
            .expect("state");
        serde_json::from_str(&json).expect("state json")
    }

    /// One still tick at vehicle pitch/roll.
    fn tick_still(&mut self, pitch: f64, roll: f64) -> State {
        let i = self.tick_no;
        self.tick_no += 1;
        let t = 50_000 + i as i64 * 100;
        self.put(SensorKind::Gravity, device_up(pitch, roll, i), t);
        self.put(SensorKind::Gyroscope, [0.001, 0.002, -0.001], t);
        self.put(SensorKind::LinearAcceleration, [0.01, 0.0, 0.02], t);
        let outcome = self
            .host
            .call(Box::new(self.world.clone()))
            .expect("guest call");
        assert_eq!(outcome, CallOutcome::Ok);
        self.state()
    }

    /// Simulate an app restart: reopen KV from disk, drop sensors.
    fn restart(&mut self) {
        let mut s = self.world.0.lock().unwrap();
        s.kv = FilePluginKv::open(&self.kv_path).unwrap();
        s.sensors.clear();
    }
}

const ALL_IMU: &[SensorKind] = &[
    SensorKind::Gravity,
    SensorKind::Gyroscope,
    SensorKind::LinearAcceleration,
    SensorKind::MagneticField,
];

#[test]
fn manifest_declares_exactly_the_sensor_capabilities() {
    let manifest =
        std::fs::read_to_string(workspace_root().join("plugins/imu/plugin.json")).unwrap();
    for cap in ["sensors_read", "plugin_kv", "clock_read", "log"] {
        assert!(manifest.contains(cap), "{cap}");
    }
    assert!(!manifest.contains("position_read"));
}

#[test]
fn synthetic_tilted_mount_levels_after_calibrate_in_wasm_and_persists() {
    let mut rig = Rig::new(ALL_IMU);
    rig.world.0.lock().unwrap().clock = Some(ClockView {
        unix_secs: 1_791_800_000,
        year: 2026,
        month: 10,
        day: 12,
        timezone: "local".into(),
    });

    let s = rig.tick_still(0.0, 0.0);
    assert_eq!(s.status, Status::Uncalibrated);
    assert!(s.tilt_deg.unwrap() > 15.0, "{s:?}");
    assert_eq!(s.instruction, CALIBRATION_INSTRUCTION);

    rig.command("calibrate");
    let mut s = rig.tick_still(0.0, 0.0);
    assert_eq!(s.status, Status::Collecting);
    for _ in 0..12 {
        s = rig.tick_still(0.0, 0.0);
    }
    assert_eq!(s.status, Status::Calibrated, "{s:?}");
    assert!(s.pitch_deg.unwrap().abs() < 0.05 && s.roll_deg.unwrap().abs() < 0.05);
    assert_eq!(s.calibrated_date.as_deref(), Some("2026-10-12"));
    assert!(rig
        .world
        .0
        .lock()
        .unwrap()
        .logs
        .iter()
        .any(|l| l.contains("Calibration saved")));

    // Persistence: mount survives a process restart; scratch state does not.
    let on_disk = std::fs::read_to_string(&rig.kv_path).unwrap();
    assert!(on_disk.contains(KEY_MOUNT), "{on_disk}");
    assert!(
        !on_disk.contains("live."),
        "ephemeral keys leaked: {on_disk}"
    );
    rig.restart();
    let s = rig.tick_still(6.0, -3.0);
    assert_eq!(s.status, Status::Calibrated);
    assert_eq!(s.calibrated_date.as_deref(), Some("2026-10-12"));
    assert!((s.pitch_deg.unwrap() - 6.0).abs() < 0.6, "{s:?}");
    assert!((s.roll_deg.unwrap() + 3.0).abs() < 0.6, "{s:?}");
}

#[test]
fn calibration_rejected_while_moving_in_wasm() {
    let mut rig = Rig::new(ALL_IMU);
    rig.command("calibrate");
    let mut s = rig.tick_still(0.0, 0.0);
    for i in 0..13 {
        s = rig.tick_still(3.0 * (i as f64 * 0.6).sin(), 0.0);
    }
    assert!(!s.calibrated, "{s:?}");
    assert_eq!(
        s.notice.as_ref().map(|n| n.kind),
        Some(NoticeKind::CalibrationRejectedMoving)
    );
    let on_disk = std::fs::read_to_string(&rig.kv_path).unwrap_or_default();
    assert!(!on_disk.contains(KEY_MOUNT));
}

#[test]
fn reset_clears_persisted_calibration_in_wasm() {
    let mut rig = Rig::new(ALL_IMU);
    rig.command("calibrate");
    let mut s = rig.tick_still(0.0, 0.0);
    for _ in 0..12 {
        s = rig.tick_still(0.0, 0.0);
    }
    assert_eq!(s.status, Status::Calibrated);
    rig.command("reset");
    let s = rig.tick_still(0.0, 0.0);
    assert_eq!(s.status, Status::Uncalibrated);
    assert_eq!(
        s.notice.as_ref().map(|n| n.kind),
        Some(NoticeKind::ResetDone)
    );
    assert!(!std::fs::read_to_string(&rig.kv_path)
        .unwrap()
        .contains(KEY_MOUNT));
    rig.restart();
    assert_eq!(rig.tick_still(0.0, 0.0).status, Status::Uncalibrated);
}

#[test]
fn missing_sensors_degrade_without_trapping_in_wasm() {
    // No inertial sensor at all.
    let rig = Rig::new(&[SensorKind::Light]);
    rig.command("calibrate");
    let outcome = rig.host.call(Box::new(rig.world.clone())).unwrap();
    assert_eq!(outcome, CallOutcome::Ok);
    let s = rig.state();
    assert_eq!(s.status, Status::NoSensor);
    assert_eq!(s.notice.map(|n| n.kind), Some(NoticeKind::NoSensor));
    assert!(s.pitch_deg.is_none());

    // Gravity only: display works, heading absent.
    let rig = Rig::new(&[SensorKind::Gravity]);
    let i = rig.tick_no;
    rig.put(SensorKind::Gravity, device_up(5.0, 0.0, i), 1000);
    rig.host.call(Box::new(rig.world.clone())).unwrap();
    let s = rig.state();
    assert!(s.pitch_deg.is_some() && s.heading_deg.is_none());
}

#[test]
fn guest_without_sensors_capability_cannot_link_sensor_imports() {
    // The imu guest imports sensor_read; a host policy without sensors_read
    // must refuse the manifest before instantiation (capability gate).
    let dir = tempfile::tempdir().unwrap();
    stage_imu(dir.path());
    let mut policy = policy();
    policy.remove(&Capability::SensorsRead);
    let err = match PluginHost::load_dir(dir.path(), &policy, PluginLimits::default()) {
        Ok(_) => panic!("expected capability denial"),
        Err(e) => e,
    };
    assert!(err.to_string().contains("sensors_read"), "{err}");
}

/// Backend double: only gravity exists; records register / unregister.
struct GravityOnlyBackend {
    registered: Arc<Mutex<Vec<SensorKind>>>,
}

impl SensorBackend for GravityOnlyBackend {
    fn is_available(&self, kind: SensorKind) -> bool {
        kind == SensorKind::Gravity
    }
    fn register(&mut self, kind: SensorKind, _: u32) -> bool {
        self.registered.lock().unwrap().push(kind);
        true
    }
    fn unregister(&mut self, kind: SensorKind) {
        self.registered.lock().unwrap().retain(|k| *k != kind);
    }
    fn latest(&self, kind: SensorKind) -> Option<SensorSample> {
        Some(SensorSample {
            kind,
            values: device_up(0.0, 0.0, 0).to_vec(),
            timestamp_ms: 1_000,
            accuracy: Some(3),
        })
    }
}

struct HubHost {
    hub: Arc<SensorHub>,
    kv: Arc<Mutex<HashMap<String, String>>>,
}

impl HostApi for HubHost {
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
    fn sensor_availability(&self) -> SensorAvailabilityView {
        self.hub.availability()
    }
    fn sensor_read(&self, kind: SensorKind) -> SensorReadView {
        self.hub.read(kind)
    }
    fn plugin_kv_status(&self) -> PluginKvStatus {
        PluginKvStatus::Available
    }
    fn plugin_kv_get(&self, key: &str) -> Option<String> {
        self.kv.lock().unwrap().get(key).cloned()
    }
    fn plugin_kv_set(&mut self, key: &str, value: &str) -> Result<(), String> {
        self.kv.lock().unwrap().insert(key.into(), value.into());
        Ok(())
    }
}

#[test]
fn sensor_hub_registers_only_what_the_guest_reads_and_releases_when_idle() {
    use std::sync::atomic::{AtomicI64, Ordering};

    let dir = tempfile::tempdir().unwrap();
    stage_imu(dir.path());
    let plugin = PluginHost::load_dir(dir.path(), &policy(), PluginLimits::default()).unwrap();

    let registered = Arc::new(Mutex::new(Vec::new()));
    let now = Arc::new(AtomicI64::new(0));
    let clock = Arc::clone(&now);
    let hub = Arc::new(SensorHub::new(
        Box::new(GravityOnlyBackend {
            registered: Arc::clone(&registered),
        }),
        SensorHubConfig::default(),
        Box::new(move || clock.load(Ordering::SeqCst)),
    ));
    let kv = Arc::new(Mutex::new(HashMap::new()));

    let outcome = plugin
        .call(Box::new(HubHost {
            hub: Arc::clone(&hub),
            kv: Arc::clone(&kv),
        }))
        .unwrap();
    assert_eq!(outcome, CallOutcome::Ok);
    // Gyro / linear accel / magnetometer / accelerometer are absent: never registered.
    assert_eq!(*registered.lock().unwrap(), vec![SensorKind::Gravity]);
    let state: State = serde_json::from_str(&kv.lock().unwrap()[KEY_STATE]).unwrap();
    assert_eq!(state.status, Status::Uncalibrated);
    assert!(state.sensors.gravity && !state.sensors.gyroscope);

    // Guest stops ticking (plugin disabled / screen off): listener is released.
    now.store(5_000, Ordering::SeqCst);
    assert_eq!(hub.release_idle(), vec![SensorKind::Gravity]);
    assert!(registered.lock().unwrap().is_empty());
}
