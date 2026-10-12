//! IMU plugin: Android sensor bridge + UniFFI session control.
//!
//! Android owns the `SensorManager`; this module owns the wasmtime guest, the
//! rate-limited [`SensorHub`] and the plugin KV. The Android host implements
//! [`SensorPlatform`] (a UniFFI foreign trait), the hub drives it with the same
//! availability / rate-limit / unregister-when-idle policy every embedder gets,
//! and the host then ticks the guest (`imu_plugin_tick`) a few times a second
//! while the app is in the foreground.
//!
//! Misbehaviour (fuel, timeout, memory, trap) disables the plugin for the
//! session (fail closed), exactly like the camping plugin.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use chrono::{Datelike, Local};
use navi_plugin_host::{
    cranelift_abi_supported, plugin_set_enabled, CallOutcome, Capability, ClockView, FilePluginKv,
    HostApi, PluginEnableStore, PluginError, PluginHost, PluginKvStatus, PluginLimits, PoiWrite,
    Position, SensorAvailabilityView, SensorBackend, SensorHub, SensorHubConfig, SensorKind,
    SensorReadView, SensorSample,
};
use navi_plugin_imu_engine::{
    AlarmConfig, CALIBRATION_INSTRUCTION, KEY_ALARM, KEY_CMD, KEY_HIST, KEY_STATE,
};

const IMU_NAME: &str = "imu";
const ENABLE_FILE: &str = "plugin_enable.json";
const KV_REL: &str = "plugin_kv/imu.json";
const PLUGINS_REL: &str = "plugins";

/// Reads faster than this get the cached sample; also the sensor sampling period.
const SENSOR_MIN_INTERVAL_MS: u64 = 100;
/// A sensor nobody read for this long is unregistered.
const SENSOR_IDLE_TIMEOUT_MS: u64 = 3_000;

/// One reading handed over by the Android `SensorManager` backend.
#[derive(uniffi::Record, Debug, Clone, PartialEq)]
pub struct ImuSensorSample {
    /// `SensorEvent.values` in SI units.
    pub values: Vec<f64>,
    /// Monotonic milliseconds (`SensorEvent.timestamp` / 1e6); only differences matter.
    pub timestamp_ms: i64,
    /// Android accuracy 0..=3, or -1 when unknown.
    pub accuracy: i32,
}

/// Platform sensor access, implemented in Kotlin on `SensorManager`.
///
/// `kind_code` is the stable `SensorKind::code()` wire code. Implementations
/// must be cheap and must not call back into this library.
#[uniffi::export(with_foreign)]
pub trait SensorPlatform: Send + Sync {
    /// Whether the device has a sensor for `kind_code`.
    fn is_sensor_available(&self, kind_code: u32) -> bool;
    /// Start delivering `kind_code` no faster than `sampling_period_us`.
    fn register_sensor(&self, kind_code: u32, sampling_period_us: u32) -> bool;
    fn unregister_sensor(&self, kind_code: u32);
    /// Most recent sample delivered since registration.
    fn latest_sensor_sample(&self, kind_code: u32) -> Option<ImuSensorSample>;
}

/// Adapts the foreign [`SensorPlatform`] to the host-side backend contract.
struct PlatformBackend {
    platform: Arc<dyn SensorPlatform>,
}

impl SensorBackend for PlatformBackend {
    fn is_available(&self, kind: SensorKind) -> bool {
        self.platform.is_sensor_available(kind.code())
    }

    fn register(&mut self, kind: SensorKind, sampling_period_us: u32) -> bool {
        self.platform
            .register_sensor(kind.code(), sampling_period_us)
    }

    fn unregister(&mut self, kind: SensorKind) {
        self.platform.unregister_sensor(kind.code());
    }

    fn latest(&self, kind: SensorKind) -> Option<SensorSample> {
        let s = self.platform.latest_sensor_sample(kind.code())?;
        Some(SensorSample {
            kind,
            values: s.values,
            timestamp_ms: s.timestamp_ms,
            accuracy: u8::try_from(s.accuracy).ok().filter(|a| *a <= 3),
        })
    }
}

/// Android `android.hardware.Sensor` types for a wire code, most preferred first.
///
/// Single source of truth for the Kotlin backend (ambient temperature prefers
/// `TYPE_AMBIENT_TEMPERATURE` over the deprecated `TYPE_TEMPERATURE`).
#[uniffi::export]
pub fn imu_plugin_android_sensor_types(kind_code: u32) -> Vec<i32> {
    SensorKind::from_code(kind_code)
        .map(|k| k.android_sensor_types().to_vec())
        .unwrap_or_default()
}

/// Wire codes of every sensor the host may expose to guests.
#[uniffi::export]
pub fn imu_plugin_sensor_codes() -> Vec<u32> {
    SensorKind::ALL.iter().map(|k| k.code()).collect()
}

/// Calibration copy shown verbatim next to the Calibrate button.
#[uniffi::export]
pub fn imu_plugin_calibration_instruction() -> String {
    CALIBRATION_INSTRUCTION.to_string()
}

#[derive(uniffi::Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImuCallKind {
    Ok,
    FuelExhausted,
    Timeout,
    MemoryExceeded,
    Trap,
    Disabled,
    Unavailable,
    Error,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct ImuTickResult {
    pub kind: ImuCallKind,
    pub message: String,
    /// Plugin display state (`live.imu_state` JSON) when the guest ran.
    pub state_json: Option<String>,
    pub elapsed_ms: u64,
}

/// User-configurable tilt alarm (persisted by the plugin KV under `imu_alarm`).
#[derive(uniffi::Record, Debug, Clone, PartialEq)]
pub struct ImuAlarmSettings {
    pub enabled: bool,
    pub pitch_warn_deg: f64,
    pub pitch_danger_deg: f64,
    pub roll_warn_deg: f64,
    pub roll_danger_deg: f64,
    pub hysteresis_deg: f64,
}

impl From<AlarmConfig> for ImuAlarmSettings {
    fn from(c: AlarmConfig) -> Self {
        Self {
            enabled: c.enabled,
            pitch_warn_deg: c.pitch_warn_deg,
            pitch_danger_deg: c.pitch_danger_deg,
            roll_warn_deg: c.roll_warn_deg,
            roll_danger_deg: c.roll_danger_deg,
            hysteresis_deg: c.hysteresis_deg,
        }
    }
}

impl From<ImuAlarmSettings> for AlarmConfig {
    fn from(s: ImuAlarmSettings) -> Self {
        Self {
            enabled: s.enabled,
            pitch_warn_deg: s.pitch_warn_deg,
            pitch_danger_deg: s.pitch_danger_deg,
            roll_warn_deg: s.roll_warn_deg,
            roll_danger_deg: s.roll_danger_deg,
            hysteresis_deg: s.hysteresis_deg,
        }
    }
}

struct Session {
    files_dir: PathBuf,
    kv: Arc<Mutex<FilePluginKv>>,
    hub: Arc<SensorHub>,
    host: Option<Arc<PluginHost>>,
    /// Set when a sandbox limit or trap fires; cleared on re-enable.
    session_disable_reason: Option<String>,
}

impl Session {
    fn enable_path(&self) -> PathBuf {
        self.files_dir.join(ENABLE_FILE)
    }

    fn imu_dir(&self) -> PathBuf {
        self.files_dir.join(PLUGINS_REL).join(IMU_NAME)
    }

    fn is_enabled(&self) -> bool {
        PluginEnableStore::open(self.enable_path())
            .map(|s| s.is_enabled(IMU_NAME))
            .unwrap_or(false)
    }
}

static SESSION: OnceLock<Mutex<Option<Session>>> = OnceLock::new();

fn session_lock() -> &'static Mutex<Option<Session>> {
    SESSION.get_or_init(|| Mutex::new(None))
}

fn mono_ms() -> i64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as i64
}

fn imu_policy() -> HashSet<Capability> {
    [
        Capability::Log,
        Capability::SensorsRead,
        Capability::PluginKv,
        Capability::ClockRead,
    ]
    .into_iter()
    .collect()
}

/// HostApi for one guest tick: sensors via the hub, KV shared with the UI.
struct ImuHostApi {
    kv: Arc<Mutex<FilePluginKv>>,
    hub: Arc<SensorHub>,
}

impl HostApi for ImuHostApi {
    fn position(&self) -> Option<Position> {
        None
    }

    fn poi_query(&self, _lat: f64, _lon: f64, _radius_m: f64) -> Vec<PoiWrite> {
        Vec::new()
    }

    fn poi_write(&mut self, _poi: PoiWrite) -> Result<(), String> {
        Ok(())
    }

    fn log(&mut self, message: &str) {
        log::info!(target: "NaviImu", "{message}");
    }

    fn sensor_availability(&self) -> SensorAvailabilityView {
        self.hub.availability()
    }

    fn sensor_read(&self, kind: SensorKind) -> SensorReadView {
        self.hub.read(kind)
    }

    fn clock_read(&self) -> Option<ClockView> {
        let now = Local::now();
        Some(ClockView {
            unix_secs: now.timestamp(),
            year: now.year(),
            month: now.month(),
            day: now.day(),
            timezone: "local".into(),
        })
    }

    fn plugin_kv_status(&self) -> PluginKvStatus {
        PluginKvStatus::Available
    }

    fn plugin_kv_get(&self, key: &str) -> Option<String> {
        self.kv.lock().ok()?.get(key)
    }

    fn plugin_kv_set(&mut self, key: &str, value: &str) -> Result<(), String> {
        self.kv
            .lock()
            .map_err(|_| "plugin kv lock poisoned".to_string())?
            .set(key, value)
            .map_err(|e| e.to_string())
    }
}

fn map_plugin_error(err: PluginError) -> (ImuCallKind, String) {
    match err {
        PluginError::FuelExhausted => (
            ImuCallKind::FuelExhausted,
            "IMU plugin disabled for this session: fuel budget exceeded".into(),
        ),
        PluginError::Timeout => (
            ImuCallKind::Timeout,
            "IMU plugin disabled for this session: wall-clock timeout".into(),
        ),
        PluginError::MemoryExceeded => (
            ImuCallKind::MemoryExceeded,
            "IMU plugin disabled for this session: memory limit exceeded".into(),
        ),
        PluginError::Trap(msg) => (
            ImuCallKind::Trap,
            format!("IMU plugin disabled for this session: trap ({msg})"),
        ),
        PluginError::UnsupportedAbi(msg) => (
            ImuCallKind::Unavailable,
            format!("IMU plugin unavailable on this ABI: {msg}"),
        ),
        PluginError::CapabilityDenied(c) => (ImuCallKind::Error, format!("capability denied: {c}")),
        PluginError::Other(e) => (ImuCallKind::Error, format!("{e:#}")),
    }
}

fn map_outcome(outcome: CallOutcome) -> (ImuCallKind, String) {
    match outcome {
        CallOutcome::Ok => (ImuCallKind::Ok, "ok".into()),
        CallOutcome::FuelExhausted => map_plugin_error(PluginError::FuelExhausted),
        CallOutcome::Timeout => map_plugin_error(PluginError::Timeout),
        CallOutcome::MemoryExceeded => map_plugin_error(PluginError::MemoryExceeded),
    }
}

fn ensure_loaded(session: &mut Session) -> Result<Arc<PluginHost>, (ImuCallKind, String)> {
    if let Some(h) = &session.host {
        return Ok(Arc::clone(h));
    }
    if !cranelift_abi_supported() {
        return Err((
            ImuCallKind::Unavailable,
            format!(
                "IMU plugin unavailable on ABI {} (fail closed)",
                std::env::consts::ARCH
            ),
        ));
    }
    let dir = session.imu_dir();
    if !dir.join("plugin.json").is_file() || !dir.join("plugin.wasm").is_file() {
        return Err((
            ImuCallKind::Unavailable,
            format!("IMU plugin not installed under {}", dir.display()),
        ));
    }
    let host = PluginHost::load_dir(
        &dir,
        &imu_policy(),
        PluginLimits {
            fuel: 20_000_000,
            timeout_ms: 500,
            memory_bytes: 8 * 1024 * 1024,
        },
    )
    .map_err(map_plugin_error)?;
    let host = Arc::new(host);
    session.host = Some(Arc::clone(&host));
    Ok(host)
}

/// Bind the plugin session to the app files dir and the Android sensor backend.
/// Safe to call more than once: the previous session's sensors are released.
#[uniffi::export]
pub fn imu_plugin_configure(files_dir: String, platform: Arc<dyn SensorPlatform>) -> String {
    crate::init_native_logging();
    let files_dir = PathBuf::from(files_dir);
    let kv = match FilePluginKv::open(files_dir.join(KV_REL)) {
        Ok(kv) => kv,
        Err(e) => return format!("FAIL: plugin kv: {e}"),
    };
    let hub = SensorHub::new(
        Box::new(PlatformBackend { platform }),
        SensorHubConfig {
            min_interval_ms: SENSOR_MIN_INTERVAL_MS,
            idle_timeout_ms: SENSOR_IDLE_TIMEOUT_MS,
        },
        Box::new(mono_ms),
    );
    let mut guard = session_lock().lock().expect("imu session lock");
    if let Some(old) = guard.as_ref() {
        old.hub.release_all();
    }
    *guard = Some(Session {
        files_dir,
        kv: Arc::new(Mutex::new(kv)),
        hub: Arc::new(hub),
        host: None,
        session_disable_reason: None,
    });
    "OK".into()
}

/// Install or replace the guest under `filesDir/plugins/imu/`. `wasm_bytes` and
/// `manifest_json` are staged from source at APK build time (assets).
#[uniffi::export]
pub fn imu_plugin_install_guest(manifest_json: String, wasm_bytes: Vec<u8>) -> String {
    let mut guard = session_lock().lock().expect("imu session lock");
    let Some(session) = guard.as_mut() else {
        return "FAIL: not configured".into();
    };
    let dir = session.imu_dir();
    if let Err(e) = fs::create_dir_all(&dir) {
        return format!("FAIL: mkdir: {e}");
    }
    if let Err(e) = fs::write(dir.join("plugin.json"), manifest_json.as_bytes()) {
        return format!("FAIL: write manifest: {e}");
    }
    if let Err(e) = fs::write(dir.join("plugin.wasm"), &wasm_bytes) {
        return format!("FAIL: write wasm: {e}");
    }
    session.host = None;
    "OK".into()
}

/// Enable / disable via [`PluginEnableStore`] (default OFF). Disabling releases
/// every sensor listener and drops the ephemeral display state; the saved level
/// calibration is kept (use reset to remove it).
#[uniffi::export]
pub fn imu_plugin_set_enabled(enabled: bool) -> String {
    let mut guard = session_lock().lock().expect("imu session lock");
    let Some(session) = guard.as_mut() else {
        return "FAIL: not configured".into();
    };
    if let Err(e) = plugin_set_enabled(&session.enable_path(), IMU_NAME, enabled) {
        return format!("FAIL: enable store: {e:#}");
    }
    session.session_disable_reason = None;
    if enabled {
        return "OK: enabled".into();
    }
    session.hub.release_all();
    session.host = None;
    if let Ok(mut kv) = session.kv.lock() {
        for key in [KEY_STATE, KEY_CMD, KEY_HIST] {
            let _ = kv.set(key, "");
        }
    }
    "OK: disabled".into()
}

#[uniffi::export]
pub fn imu_plugin_is_enabled() -> bool {
    let guard = session_lock().lock().expect("imu session lock");
    guard.as_ref().is_some_and(Session::is_enabled)
}

#[uniffi::export]
pub fn imu_plugin_session_disabled_reason() -> Option<String> {
    let guard = session_lock().lock().expect("imu session lock");
    guard
        .as_ref()
        .and_then(|s| s.session_disable_reason.clone())
}

/// Run one guest tick. Call a few times per second from a background thread
/// while the plugin is enabled and the app is in the foreground.
#[uniffi::export]
pub fn imu_plugin_tick() -> ImuTickResult {
    let started = Instant::now();
    let done = |kind: ImuCallKind, message: String, state_json: Option<String>| ImuTickResult {
        kind,
        message,
        state_json,
        elapsed_ms: started.elapsed().as_millis() as u64,
    };

    let (host, kv, hub) = {
        let mut guard = session_lock().lock().expect("imu session lock");
        let Some(session) = guard.as_mut() else {
            return done(ImuCallKind::Unavailable, "not configured".into(), None);
        };
        if !session.is_enabled() {
            return done(ImuCallKind::Disabled, "IMU plugin is off".into(), None);
        }
        if let Some(reason) = &session.session_disable_reason {
            return done(ImuCallKind::Disabled, reason.clone(), None);
        }
        match ensure_loaded(session) {
            Ok(host) => (host, Arc::clone(&session.kv), Arc::clone(&session.hub)),
            Err((kind, message)) => return done(kind, message, None),
        }
    };

    let api = ImuHostApi {
        kv: Arc::clone(&kv),
        hub: Arc::clone(&hub),
    };
    let result = host.call(Box::new(api));
    // Sensors the guest stopped reading (or the whole set after a failure) go
    // back to the platform instead of staying registered.
    hub.release_idle();

    let (kind, message) = match result {
        Ok(outcome) => map_outcome(outcome),
        Err(err) => map_plugin_error(err),
    };
    match kind {
        ImuCallKind::Ok => {
            let state = kv.lock().ok().and_then(|kv| kv.get(KEY_STATE));
            done(kind, message, state)
        }
        ImuCallKind::Error | ImuCallKind::Unavailable | ImuCallKind::Disabled => {
            done(kind, message, None)
        }
        _ => {
            hub.release_all();
            if let Some(session) = session_lock().lock().expect("imu session lock").as_mut() {
                session.session_disable_reason = Some(message.clone());
                session.host = None;
            }
            done(kind, message, None)
        }
    }
}

/// Unregister every sensor listener (app backgrounded, settings closed).
#[uniffi::export]
pub fn imu_plugin_release_sensors() {
    let hub = {
        let guard = session_lock().lock().expect("imu session lock");
        guard.as_ref().map(|s| Arc::clone(&s.hub))
    };
    if let Some(hub) = hub {
        hub.release_all();
    }
}

/// Sensor wire codes currently registered with the platform (diagnostics / tests).
#[uniffi::export]
pub fn imu_plugin_registered_sensor_codes() -> Vec<u32> {
    let guard = session_lock().lock().expect("imu session lock");
    guard
        .as_ref()
        .map(|s| s.hub.registered().iter().map(|k| k.code()).collect())
        .unwrap_or_default()
}

fn send_command(cmd: &str) -> String {
    let guard = session_lock().lock().expect("imu session lock");
    let Some(session) = guard.as_ref() else {
        return "FAIL: not configured".into();
    };
    if !session.is_enabled() {
        return "FAIL: plugin is off".into();
    }
    let json = format!("{{\"cmd\":\"{cmd}\"}}");
    let Ok(mut kv) = session.kv.lock() else {
        return "FAIL: kv lock".into();
    };
    match kv.set(KEY_CMD, &json) {
        Ok(()) => "OK".into(),
        Err(e) => format!("FAIL: {e}"),
    }
}

/// Ask the guest to calibrate the vehicle mount to level on its next ticks.
#[uniffi::export]
pub fn imu_plugin_request_calibrate() -> String {
    send_command("calibrate")
}

/// Ask the guest to remove the stored calibration on its next tick.
#[uniffi::export]
pub fn imu_plugin_request_reset() -> String {
    send_command("reset")
}

/// Current alarm settings (defaults when none stored; always sanitised).
#[uniffi::export]
pub fn imu_plugin_alarm_settings() -> ImuAlarmSettings {
    let guard = session_lock().lock().expect("imu session lock");
    let stored = guard
        .as_ref()
        .and_then(|s| s.kv.lock().ok().and_then(|kv| kv.get(KEY_ALARM)))
        .and_then(|raw| serde_json::from_str::<AlarmConfig>(&raw).ok())
        .unwrap_or_default();
    stored.sanitized().into()
}

/// Persist alarm settings; returns the sanitised values actually stored.
#[uniffi::export]
pub fn imu_plugin_set_alarm_settings(settings: ImuAlarmSettings) -> ImuAlarmSettings {
    let cfg = AlarmConfig::from(settings).sanitized();
    let guard = session_lock().lock().expect("imu session lock");
    if let Some(session) = guard.as_ref() {
        if let (Ok(json), Ok(mut kv)) = (serde_json::to_string(&cfg), session.kv.lock()) {
            if let Err(e) = kv.set(KEY_ALARM, &json) {
                log::warn!(target: "NaviImu", "alarm settings not saved: {e}");
            }
        }
    }
    cfg.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct FakePlatform {
        available: Vec<u32>,
        registered: Mutex<Vec<(u32, u32)>>,
        unregistered: Mutex<Vec<u32>>,
        samples: Mutex<HashMap<u32, ImuSensorSample>>,
    }

    impl SensorPlatform for FakePlatform {
        fn is_sensor_available(&self, kind_code: u32) -> bool {
            self.available.contains(&kind_code)
        }
        fn register_sensor(&self, kind_code: u32, sampling_period_us: u32) -> bool {
            self.registered
                .lock()
                .unwrap()
                .push((kind_code, sampling_period_us));
            true
        }
        fn unregister_sensor(&self, kind_code: u32) {
            self.unregistered.lock().unwrap().push(kind_code);
        }
        fn latest_sensor_sample(&self, kind_code: u32) -> Option<ImuSensorSample> {
            self.samples.lock().unwrap().get(&kind_code).cloned()
        }
    }

    #[test]
    fn platform_backend_maps_codes_and_accuracy() {
        let fake = Arc::new(FakePlatform {
            available: vec![SensorKind::Gravity.code()],
            ..Default::default()
        });
        fake.samples.lock().unwrap().insert(
            SensorKind::Gravity.code(),
            ImuSensorSample {
                values: vec![0.0, 0.0, 9.8],
                timestamp_ms: 42,
                accuracy: -1,
            },
        );
        let mut backend = PlatformBackend {
            platform: fake.clone(),
        };
        assert!(backend.is_available(SensorKind::Gravity));
        assert!(!backend.is_available(SensorKind::Gyroscope));
        assert!(backend.register(SensorKind::Gravity, 100_000));
        let s = backend.latest(SensorKind::Gravity).unwrap();
        assert_eq!(s.kind, SensorKind::Gravity);
        assert_eq!(s.values, vec![0.0, 0.0, 9.8]);
        assert_eq!(s.accuracy, None);
        backend.unregister(SensorKind::Gravity);
        assert_eq!(
            *fake.registered.lock().unwrap(),
            vec![(SensorKind::Gravity.code(), 100_000)]
        );
        assert_eq!(
            *fake.unregistered.lock().unwrap(),
            vec![SensorKind::Gravity.code()]
        );
    }

    #[test]
    fn ambient_temperature_prefers_type_ambient_temperature() {
        let types = imu_plugin_android_sensor_types(SensorKind::AmbientTemperature.code());
        // TYPE_AMBIENT_TEMPERATURE = 13 first, deprecated TYPE_TEMPERATURE = 7 fallback.
        assert_eq!(types, vec![13, 7]);
        assert!(imu_plugin_android_sensor_types(0).is_empty());
        assert_eq!(imu_plugin_sensor_codes().len(), SensorKind::ALL.len());
    }

    #[test]
    fn calibration_instruction_is_verbatim() {
        assert_eq!(
            imu_plugin_calibration_instruction(),
            "Find a level concrete surface, like the floor of your garage, then tap Calibrate. The unit will then use the current values as level."
        );
    }

    #[test]
    fn alarm_settings_round_trip_and_sanitise() {
        let dir = tempfile::tempdir().unwrap();
        let platform: Arc<dyn SensorPlatform> = Arc::new(FakePlatform::default());
        assert_eq!(
            imu_plugin_configure(dir.path().display().to_string(), platform),
            "OK"
        );
        let defaults = imu_plugin_alarm_settings();
        assert!(defaults.enabled);

        let stored = imu_plugin_set_alarm_settings(ImuAlarmSettings {
            enabled: true,
            pitch_warn_deg: 35.0,
            pitch_danger_deg: 10.0,
            roll_warn_deg: f64::NAN,
            roll_danger_deg: 200.0,
            hysteresis_deg: 1.0,
        });
        // Danger is raised to at least warn; NaN falls back to the default; clamp to 89.
        assert_eq!(stored.pitch_danger_deg, 35.0);
        assert_eq!(stored.roll_warn_deg, AlarmConfig::default().roll_warn_deg);
        assert_eq!(stored.roll_danger_deg, 89.0);
        assert_eq!(imu_plugin_alarm_settings(), stored);

        // Persisted across a new session over the same files dir.
        let platform: Arc<dyn SensorPlatform> = Arc::new(FakePlatform::default());
        imu_plugin_configure(dir.path().display().to_string(), platform);
        assert_eq!(imu_plugin_alarm_settings(), stored);
    }
}
