//! IMU plugin engine: pitch / tilt / compass, level calibration of the vehicle
//! mount, and a configurable tilt-danger alarm.
//!
//! The WASM guest (`plugins/imu`) is a thin shim: it implements [`Host`] over the
//! `navi-plugin-sdk` imports and calls [`tick`] once per host invocation. All
//! state that must survive between invocations lives in `plugin_kv`:
//!
//! | Key | Lifetime | Writer | Content |
//! |---|---|---|---|
//! | `imu_mount` | persistent | guest | [`MountRecord`] (rotation + calibration date) |
//! | `imu_alarm` | persistent | host | [`AlarmConfig`] thresholds |
//! | `live.imu_cmd` | ephemeral | host/guest | `{"cmd":"calibrate"\|"reset"}` |
//! | `live.imu_state` | ephemeral | guest | [`State`] for the host UI / audio |
//! | `live.imu_hist` | ephemeral | guest | recent stationary-window samples |
//!
//! Keys starting with `live.` are never flushed to disk by the file-backed KV, so
//! a 10 Hz tick does not rewrite storage.

pub mod alarm;
pub mod calibrate;
pub mod math;
pub mod orient;

use serde::{Deserialize, Serialize};

pub use alarm::{AlarmConfig, AlarmLevel};
pub use calibrate::{CalibrationError, HistEntry};
use math::{mat_mul, transpose, Mat3, Vec3, IDENTITY};

/// Shown verbatim next to the Calibrate button. Do not reword.
pub const CALIBRATION_INSTRUCTION: &str = "Find a level concrete surface, like the floor of your garage, then tap Calibrate. The unit will then use the current values as level.";

pub const KEY_MOUNT: &str = "imu_mount";
pub const KEY_ALARM: &str = "imu_alarm";
pub const KEY_CMD: &str = "live.imu_cmd";
pub const KEY_STATE: &str = "live.imu_state";
pub const KEY_HIST: &str = "live.imu_hist";

/// `sensor_read` wire codes (must match `navi_plugin_host::SensorKind::code`).
pub const SENSOR_ACCELEROMETER: u32 = 1;
pub const SENSOR_GRAVITY: u32 = 3;
pub const SENSOR_GYROSCOPE: u32 = 4;
pub const SENSOR_LINEAR_ACCELERATION: u32 = 6;
pub const SENSOR_MAGNETIC_FIELD: u32 = 7;

/// A pending calibration request gives up after this long without a steady window.
pub const CALIBRATION_TIMEOUT_MS: i64 = 10_000;
/// How long a one-shot notice stays in the state.
pub const NOTICE_MS: i64 = 8_000;

#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub values: Vec<f64>,
    pub timestamp_ms: i64,
}

impl Sample {
    /// Parse the host `sensor_read` JSON (`{"status":"ok","sample":{..}}`).
    pub fn from_read_json(bytes: &[u8]) -> Option<Self> {
        #[derive(Deserialize)]
        struct S {
            values: Vec<f64>,
            timestamp_ms: i64,
        }
        #[derive(Deserialize)]
        struct V {
            status: String,
            sample: Option<S>,
        }
        let v: V = serde_json::from_slice(bytes).ok()?;
        if v.status != "ok" {
            return None;
        }
        let s = v.sample?;
        Some(Self {
            values: s.values,
            timestamp_ms: s.timestamp_ms,
        })
    }

    fn vec3(&self) -> Option<Vec3> {
        match self.values.as_slice() {
            [x, y, z, ..] if [x, y, z].iter().all(|v| v.is_finite()) => Some([*x, *y, *z]),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DateStamp {
    pub unix_secs: i64,
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

/// Host services the engine needs (implemented over the SDK in the guest).
pub trait Host {
    fn kv_available(&self) -> bool;
    fn kv_get(&self, key: &str) -> Option<String>;
    fn kv_set(&mut self, key: &str, value: &str) -> Result<(), String>;
    /// Latest sample for a `SENSOR_*` code; `None` when unavailable / no sample.
    fn sensor(&self, kind: u32) -> Option<Sample>;
    /// Device-local date when the host provides a clock.
    fn clock(&self) -> Option<DateStamp>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Neither gravity nor accelerometer is available.
    NoSensor,
    Uncalibrated,
    Calibrated,
    /// A calibrate request is waiting for a steady window.
    Collecting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoticeKind {
    CalibrationSaved,
    CalibrationRejectedMoving,
    CalibrationRejectedGravity,
    CalibrationTimedOut,
    StorageUnavailable,
    NoSensor,
    ResetDone,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Notice {
    pub kind: NoticeKind,
    pub text: String,
    pub at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SensorsPresent {
    pub gravity: bool,
    pub accelerometer: bool,
    pub gyroscope: bool,
    pub linear_acceleration: bool,
    pub magnetic_field: bool,
}

/// Output for the host UI and audio layer, rewritten every tick.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub status: Status,
    pub status_text: String,
    pub notice: Option<Notice>,
    /// Calibration UI copy; show verbatim.
    pub instruction: String,
    pub calibrated: bool,
    /// Local calibration date, `YYYY-MM-DD`, when the clock was available.
    pub calibrated_date: Option<String>,
    pub calibrated_unix: Option<i64>,
    pub pitch_deg: Option<f64>,
    pub roll_deg: Option<f64>,
    pub tilt_deg: Option<f64>,
    /// Magnetic heading of the vehicle forward axis (needs a magnetometer).
    pub heading_deg: Option<f64>,
    pub alarm: AlarmLevel,
    pub alarm_enabled: bool,
    pub sensors: SensorsPresent,
    pub updated_ms: i64,
}

/// Persisted mount rotation (device frame to vehicle frame).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MountRecord {
    pub r: Mat3,
    pub unix_secs: Option<i64>,
    pub date: Option<String>,
}

impl MountRecord {
    fn valid(&self) -> bool {
        if !math::is_finite_mat(&self.r) {
            return false;
        }
        let rrt = mat_mul(&self.r, &transpose(&self.r));
        (0..3).all(|i| (0..3).all(|j| (rrt[i][j] - IDENTITY[i][j]).abs() < 1e-3))
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Command {
    cmd: String,
    #[serde(default)]
    since: Option<i64>,
}

fn format_date(d: &DateStamp) -> String {
    format!("{:04}-{:02}-{:02}", d.year, d.month, d.day)
}

fn notice(kind: NoticeKind, now_ms: i64) -> Notice {
    let text = match kind {
        NoticeKind::CalibrationSaved => "Calibration saved.",
        NoticeKind::CalibrationRejectedMoving => {
            "Calibration rejected: the unit is moving. Park, keep the vehicle still and tap Calibrate again."
        }
        NoticeKind::CalibrationRejectedGravity => {
            "Calibration rejected: the gravity reading is not plausible. Keep the unit still and tap Calibrate again."
        }
        NoticeKind::CalibrationTimedOut => {
            "Calibration timed out before enough steady samples were collected."
        }
        NoticeKind::StorageUnavailable => "Calibration not saved: plugin storage is unavailable.",
        NoticeKind::NoSensor => "Calibration impossible: no accelerometer or gravity sensor.",
        NoticeKind::ResetDone => "Calibration reset. No mount rotation is applied.",
    };
    Notice {
        kind,
        text: text.to_string(),
        at_ms: now_ms,
    }
}

fn load<T: for<'de> Deserialize<'de>>(host: &dyn Host, key: &str) -> Option<T> {
    serde_json::from_str(&host.kv_get(key)?).ok()
}

fn store<T: Serialize>(host: &mut dyn Host, key: &str, value: &T) -> bool {
    match serde_json::to_string(value) {
        Ok(json) => host.kv_set(key, &json).is_ok(),
        Err(_) => false,
    }
}

fn clear(host: &mut dyn Host, key: &str) {
    let _ = host.kv_set(key, "");
}

/// Run one host invocation: read sensors, process any pending command, compute
/// the display state and persist it.
pub fn tick(host: &mut dyn Host) -> State {
    let kv_ok = host.kv_available();
    let gravity = host.sensor(SENSOR_GRAVITY).filter(|s| s.vec3().is_some());
    let accel = host
        .sensor(SENSOR_ACCELEROMETER)
        .filter(|s| s.vec3().is_some());
    let gyro = host.sensor(SENSOR_GYROSCOPE);
    let linear = host.sensor(SENSOR_LINEAR_ACCELERATION);
    let mag = host
        .sensor(SENSOR_MAGNETIC_FIELD)
        .filter(|s| s.vec3().is_some());
    let sensors = SensorsPresent {
        gravity: gravity.is_some(),
        accelerometer: accel.is_some(),
        gyroscope: gyro.is_some(),
        linear_acceleration: linear.is_some(),
        magnetic_field: mag.is_some(),
    };
    // Gravity sensor is already low-passed by the platform; prefer it.
    let up_sample = gravity.as_ref().or(accel.as_ref());
    let now_ms = up_sample.map(|s| s.timestamp_ms).unwrap_or(0);
    let up = up_sample.and_then(Sample::vec3);

    let alarm_cfg: AlarmConfig = load::<AlarmConfig>(host, KEY_ALARM)
        .unwrap_or_default()
        .sanitized();
    let prev: Option<State> = load(host, KEY_STATE);
    let cmd: Option<Command> = load(host, KEY_CMD);
    let mut mount: Option<MountRecord> = if kv_ok {
        load::<MountRecord>(host, KEY_MOUNT).filter(MountRecord::valid)
    } else {
        None
    };
    let mut note = prev
        .as_ref()
        .and_then(|p| p.notice.clone())
        .filter(|n| now_ms >= n.at_ms && now_ms - n.at_ms < NOTICE_MS);
    let mut collecting = false;

    match cmd.as_ref().map(|c| c.cmd.as_str()) {
        Some("reset") => {
            if kv_ok {
                clear(host, KEY_MOUNT);
                mount = None;
                note = Some(notice(NoticeKind::ResetDone, now_ms));
            } else {
                note = Some(notice(NoticeKind::StorageUnavailable, now_ms));
            }
            clear(host, KEY_CMD);
        }
        Some("calibrate") => {}
        Some(_) => clear(host, KEY_CMD),
        None => {}
    }

    // Maintain the stationary window.
    let mut hist: Vec<HistEntry> = load(host, KEY_HIST).unwrap_or_default();
    if let (Some(u), Some(s)) = (up, up_sample) {
        let fresh = hist.last().is_none_or(|l| s.timestamp_ms != l.t);
        if hist.last().is_some_and(|l| s.timestamp_ms < l.t) {
            hist.clear();
        }
        if fresh {
            hist.push(HistEntry {
                t: s.timestamp_ms,
                u,
                w: gyro.as_ref().and_then(Sample::vec3).map(math::norm),
                l: linear.as_ref().and_then(Sample::vec3).map(math::norm),
            });
            if hist.len() > calibrate::HIST_LEN {
                let excess = hist.len() - calibrate::HIST_LEN;
                hist.drain(..excess);
            }
            if kv_ok {
                store(host, KEY_HIST, &hist);
            }
        }
    }

    if let Some(c) = cmd.as_ref().filter(|c| c.cmd == "calibrate") {
        if up.is_none() {
            note = Some(notice(NoticeKind::NoSensor, now_ms));
            clear(host, KEY_CMD);
        } else if !kv_ok {
            note = Some(notice(NoticeKind::StorageUnavailable, now_ms));
            clear(host, KEY_CMD);
        } else {
            match calibrate::calibrate(&hist) {
                Ok(r) => {
                    let date = host.clock();
                    let record = MountRecord {
                        r,
                        unix_secs: date.map(|d| d.unix_secs),
                        date: date.as_ref().map(format_date),
                    };
                    if store(host, KEY_MOUNT, &record) {
                        mount = Some(record);
                        note = Some(notice(NoticeKind::CalibrationSaved, now_ms));
                    } else {
                        note = Some(notice(NoticeKind::StorageUnavailable, now_ms));
                    }
                    clear(host, KEY_CMD);
                }
                Err(CalibrationError::NotEnoughSamples) => {
                    let since = c.since.unwrap_or(now_ms);
                    if now_ms - since > CALIBRATION_TIMEOUT_MS {
                        note = Some(notice(NoticeKind::CalibrationTimedOut, now_ms));
                        clear(host, KEY_CMD);
                    } else {
                        collecting = true;
                        if c.since.is_none() {
                            store(
                                host,
                                KEY_CMD,
                                &Command {
                                    cmd: "calibrate".into(),
                                    since: Some(since),
                                },
                            );
                        }
                    }
                }
                Err(CalibrationError::Moving) => {
                    note = Some(notice(NoticeKind::CalibrationRejectedMoving, now_ms));
                    clear(host, KEY_CMD);
                }
                Err(CalibrationError::ImplausibleGravity) => {
                    note = Some(notice(NoticeKind::CalibrationRejectedGravity, now_ms));
                    clear(host, KEY_CMD);
                }
            }
        }
    }

    let r = mount.as_ref().map(|m| m.r).unwrap_or(IDENTITY);
    let att = up.and_then(|u| orient::attitude(&r, u));
    let heading = match (up, mag.as_ref().and_then(Sample::vec3)) {
        (Some(u), Some(m)) => orient::heading_deg(&r, u, m),
        _ => None,
    };
    let alarm = match att {
        Some(a) => alarm_cfg.evaluate(
            prev.as_ref().map(|p| p.alarm).unwrap_or_default(),
            a.pitch_deg,
            a.roll_deg,
        ),
        None => AlarmLevel::None,
    };

    let status = if up.is_none() {
        Status::NoSensor
    } else if collecting {
        Status::Collecting
    } else if mount.is_some() {
        Status::Calibrated
    } else {
        Status::Uncalibrated
    };
    let status_text = match status {
        Status::NoSensor => "No accelerometer or gravity sensor available on this device.".into(),
        Status::Collecting => "Hold the unit still while samples are collected.".into(),
        Status::Calibrated => match mount.as_ref().and_then(|m| m.date.as_deref()) {
            Some(d) => format!("Calibrated on {d}."),
            None => "Calibrated.".into(),
        },
        Status::Uncalibrated => "Not calibrated. Using the unit orientation as mounted.".into(),
    };

    let state = State {
        status,
        status_text,
        notice: note,
        instruction: CALIBRATION_INSTRUCTION.to_string(),
        calibrated: mount.is_some(),
        calibrated_date: mount.as_ref().and_then(|m| m.date.clone()),
        calibrated_unix: mount.as_ref().and_then(|m| m.unix_secs),
        pitch_deg: att.map(|a| a.pitch_deg),
        roll_deg: att.map(|a| a.roll_deg),
        tilt_deg: att.map(|a| a.tilt_deg),
        heading_deg: heading,
        alarm,
        alarm_enabled: alarm_cfg.enabled,
        sensors,
        updated_ms: now_ms,
    };
    if kv_ok {
        store(host, KEY_STATE, &state);
    }
    state
}
