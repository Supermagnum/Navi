//! Engine behaviour against a scripted host: synthetic tilted mount, calibration
//! accept / reject, persistence, reset, missing sensors, alarm, heading.

use std::collections::HashMap;

use navi_plugin_imu_engine::math::{mat_vec, rot_x, rot_y, Mat3, Vec3};
use navi_plugin_imu_engine::{
    tick, AlarmLevel, DateStamp, Host, NoticeKind, Sample, State, Status, CALIBRATION_INSTRUCTION,
    KEY_ALARM, KEY_CMD, KEY_MOUNT, SENSOR_ACCELEROMETER, SENSOR_GRAVITY, SENSOR_GYROSCOPE,
    SENSOR_LINEAR_ACCELERATION, SENSOR_MAGNETIC_FIELD,
};

const G: f64 = 9.806_65;

#[derive(Default, Clone)]
struct TestHost {
    kv: HashMap<String, String>,
    sensors: HashMap<u32, Sample>,
    clock: Option<DateStamp>,
}

impl Host for TestHost {
    fn kv_available(&self) -> bool {
        true
    }
    fn kv_get(&self, key: &str) -> Option<String> {
        self.kv.get(key).cloned().filter(|v| !v.is_empty())
    }
    fn kv_set(&mut self, key: &str, value: &str) -> Result<(), String> {
        self.kv.insert(key.into(), value.into());
        Ok(())
    }
    fn sensor(&self, kind: u32) -> Option<Sample> {
        self.sensors.get(&kind).cloned()
    }
    fn clock(&self) -> Option<DateStamp> {
        self.clock
    }
}

impl TestHost {
    /// Process restart: persistent keys survive, `live.` scratch is lost.
    fn restart(&self) -> TestHost {
        let mut h = self.clone();
        h.kv.retain(|k, _| !k.starts_with("live."));
        h.sensors.clear();
        h
    }

    fn set_up(&mut self, kind: u32, up: Vec3, t: i64) {
        self.sensors.insert(
            kind,
            Sample {
                values: up.to_vec(),
                timestamp_ms: t,
            },
        );
    }

    fn set_vec(&mut self, kind: u32, v: Vec3, t: i64) {
        self.set_up(kind, v, t);
    }
}

/// Vehicle-to-device rotation of the synthetic mount: 15 deg pitch, 10 deg roll.
fn mount_truth() -> Mat3 {
    navi_plugin_imu_engine::math::mat_mul(&rot_x(15f64.to_radians()), &rot_y(10f64.to_radians()))
}

/// Vehicle-frame up vector for a given vehicle pitch / roll (degrees).
fn up_vehicle(pitch: f64, roll: f64) -> Vec3 {
    let (p, r) = (pitch.to_radians(), roll.to_radians());
    [-r.sin() * p.cos(), p.sin(), r.cos() * p.cos()]
}

/// What the device accelerometer reads for a vehicle attitude, plus tiny
/// deterministic noise (about 0.02 deg).
fn device_up(pitch: f64, roll: f64, i: usize) -> Vec3 {
    let u = mat_vec(&mount_truth(), up_vehicle(pitch, roll));
    let n = ((i * 37 % 11) as f64 - 5.0) * 0.0007;
    [u[0] * G + n, u[1] * G - n, u[2] * G + n * 0.5]
}

fn feed_still(h: &mut TestHost, kind: u32, pitch: f64, roll: f64, start: usize, n: usize) -> State {
    let mut last = None;
    for i in start..start + n {
        let t = 10_000 + i as i64 * 100;
        h.set_up(kind, device_up(pitch, roll, i), t);
        h.set_vec(SENSOR_GYROSCOPE, [0.002, -0.001, 0.003], t);
        h.set_vec(SENSOR_LINEAR_ACCELERATION, [0.02, 0.01, -0.02], t);
        last = Some(tick(h));
    }
    last.unwrap()
}

fn request(h: &mut TestHost, cmd: &str) {
    h.kv.insert(KEY_CMD.into(), format!(r#"{{"cmd":"{cmd}"}}"#));
}

fn calibrated_host(kind: u32) -> (TestHost, usize) {
    let mut h = TestHost::default();
    request(&mut h, "calibrate");
    let s = feed_still(&mut h, kind, 0.0, 0.0, 0, 14);
    assert_eq!(s.status, Status::Calibrated, "{s:?}");
    (h, 14)
}

#[test]
fn instruction_copy_is_exact() {
    assert_eq!(
        CALIBRATION_INSTRUCTION,
        "Find a level concrete surface, like the floor of your garage, then tap Calibrate. The unit will then use the current values as level."
    );
    let mut h = TestHost::default();
    h.set_up(SENSOR_GRAVITY, device_up(0.0, 0.0, 0), 1000);
    assert_eq!(tick(&mut h).instruction, CALIBRATION_INSTRUCTION);
}

#[test]
fn uncalibrated_mount_reads_tilted_then_level_after_calibrate() {
    let mut h = TestHost::default();
    let before = feed_still(&mut h, SENSOR_GRAVITY, 0.0, 0.0, 0, 3);
    assert_eq!(before.status, Status::Uncalibrated);
    assert!(
        before.tilt_deg.unwrap() > 15.0,
        "mount is tilted: {before:?}"
    );

    request(&mut h, "calibrate");
    let s = feed_still(&mut h, SENSOR_GRAVITY, 0.0, 0.0, 3, 14);
    assert_eq!(s.status, Status::Calibrated, "{s:?}");
    assert!(s.pitch_deg.unwrap().abs() < 0.05, "{s:?}");
    assert!(s.roll_deg.unwrap().abs() < 0.05, "{s:?}");
    assert_eq!(
        s.notice.as_ref().map(|n| n.kind),
        Some(NoticeKind::CalibrationSaved)
    );
    assert!(h.kv.get(KEY_CMD).is_none_or(|c| c.is_empty()));
}

#[test]
fn calibrated_unit_tracks_vehicle_pitch_and_roll() {
    let (mut h, next) = calibrated_host(SENSOR_GRAVITY);
    for (pitch, roll) in [
        (5.0, 0.0),
        (0.0, -4.0),
        (8.0, 3.0),
        (-6.0, 7.0),
        (12.0, 0.0),
    ] {
        let s = feed_still(&mut h, SENSOR_GRAVITY, pitch, roll, next, 1);
        // Yaw about vertical is unobservable from gravity; residual leakage is small.
        assert!(
            (s.pitch_deg.unwrap() - pitch).abs() < 0.6,
            "{pitch}/{roll}: {s:?}"
        );
        assert!(
            (s.roll_deg.unwrap() - roll).abs() < 0.6,
            "{pitch}/{roll}: {s:?}"
        );
    }
}

#[test]
fn accelerometer_only_device_calibrates_too() {
    let (mut h, next) = calibrated_host(SENSOR_ACCELEROMETER);
    let s = feed_still(&mut h, SENSOR_ACCELEROMETER, 5.0, 0.0, next, 1);
    assert!(!s.sensors.gravity && s.sensors.accelerometer);
    assert!((s.pitch_deg.unwrap() - 5.0).abs() < 0.6, "{s:?}");
}

#[test]
fn calibration_waits_for_enough_samples_then_times_out() {
    let mut h = TestHost::default();
    request(&mut h, "calibrate");
    let s = feed_still(&mut h, SENSOR_GRAVITY, 0.0, 0.0, 0, 3);
    assert_eq!(s.status, Status::Collecting);
    assert!(!s.calibrated);

    // Sensor stalls: same few samples never reach the window; time passes.
    h.set_up(SENSOR_GRAVITY, device_up(0.0, 0.0, 99), 10_000 + 12_000);
    let s = tick(&mut h);
    assert_ne!(s.status, Status::Collecting);
    assert_eq!(
        s.notice.map(|n| n.kind),
        Some(NoticeKind::CalibrationTimedOut)
    );
    assert!(!h.kv.contains_key(KEY_MOUNT));
}

#[test]
fn rejects_calibration_while_vehicle_is_rocking() {
    let mut h = TestHost::default();
    request(&mut h, "calibrate");
    let mut s = None;
    for i in 0..14 {
        let t = 10_000 + i as i64 * 100;
        // 3 deg pitch rocking at roughly 1 Hz.
        let pitch = 3.0 * (i as f64 * 0.6).sin();
        h.set_up(SENSOR_GRAVITY, device_up(pitch, 0.0, i), t);
        s = Some(tick(&mut h));
    }
    let s = s.unwrap();
    assert!(!s.calibrated, "{s:?}");
    assert_eq!(
        s.notice.map(|n| n.kind),
        Some(NoticeKind::CalibrationRejectedMoving)
    );
    assert!(!h.kv.contains_key(KEY_MOUNT));
}

#[test]
fn rejects_calibration_when_gyro_or_linear_acceleration_report_motion() {
    for (kind, v) in [
        (SENSOR_GYROSCOPE, [0.0, 0.0, 0.4]),
        (SENSOR_LINEAR_ACCELERATION, [1.2, 0.0, 0.0]),
    ] {
        let mut h = TestHost::default();
        request(&mut h, "calibrate");
        let mut last = None;
        for i in 0..14 {
            let t = 10_000 + i as i64 * 100;
            h.set_up(SENSOR_GRAVITY, device_up(0.0, 0.0, i), t);
            if i == 6 {
                h.set_vec(kind, v, t);
            } else if i > 6 {
                h.set_vec(kind, [0.0, 0.0, 0.0], t);
            }
            last = Some(tick(&mut h));
        }
        let s = last.unwrap();
        assert!(!s.calibrated, "kind {kind}: {s:?}");
        assert_eq!(
            s.notice.map(|n| n.kind),
            Some(NoticeKind::CalibrationRejectedMoving)
        );
    }
}

#[test]
fn rejects_implausible_gravity() {
    let mut h = TestHost::default();
    request(&mut h, "calibrate");
    let mut last = None;
    for i in 0..14 {
        let t = 10_000 + i as i64 * 100;
        h.set_up(SENSOR_GRAVITY, [0.0, 0.0, 2.0], t);
        last = Some(tick(&mut h));
    }
    let s = last.unwrap();
    assert!(!s.calibrated);
    assert_eq!(
        s.notice.map(|n| n.kind),
        Some(NoticeKind::CalibrationRejectedGravity)
    );
}

#[test]
fn calibration_persists_across_restart_with_date() {
    let (mut h, _) = calibrated_host(SENSOR_GRAVITY);
    // Re-run the calibrate flow with a clock to capture the date.
    h.clock = Some(DateStamp {
        unix_secs: 1_791_800_000,
        year: 2026,
        month: 10,
        day: 12,
    });
    request(&mut h, "calibrate");
    let s = feed_still(&mut h, SENSOR_GRAVITY, 0.0, 0.0, 14, 3);
    assert_eq!(s.calibrated_date.as_deref(), Some("2026-10-12"));
    assert_eq!(s.status_text, "Calibrated on 2026-10-12.");

    let mut restarted = h.restart();
    let s = feed_still(&mut restarted, SENSOR_GRAVITY, 4.0, 0.0, 30, 1);
    assert_eq!(s.status, Status::Calibrated);
    assert_eq!(s.calibrated_date.as_deref(), Some("2026-10-12"));
    assert_eq!(s.calibrated_unix, Some(1_791_800_000));
    assert!((s.pitch_deg.unwrap() - 4.0).abs() < 0.6, "{s:?}");
}

#[test]
fn reset_clears_mount_and_returns_to_mounted_orientation() {
    let (mut h, next) = calibrated_host(SENSOR_GRAVITY);
    assert!(h.kv.contains_key(KEY_MOUNT));
    request(&mut h, "reset");
    let s = feed_still(&mut h, SENSOR_GRAVITY, 0.0, 0.0, next, 1);
    assert_eq!(s.status, Status::Uncalibrated);
    assert!(!s.calibrated && s.calibrated_date.is_none());
    assert_eq!(s.notice.map(|n| n.kind), Some(NoticeKind::ResetDone));
    assert!(h.kv.get(KEY_MOUNT).is_none_or(|v| v.is_empty()));
    assert!(s.tilt_deg.unwrap() > 15.0);

    let mut restarted = h.restart();
    let s = feed_still(&mut restarted, SENSOR_GRAVITY, 0.0, 0.0, 40, 1);
    assert_eq!(s.status, Status::Uncalibrated);
}

#[test]
fn missing_sensor_reports_no_sensor_and_refuses_calibration() {
    let mut h = TestHost::default();
    request(&mut h, "calibrate");
    let s = tick(&mut h);
    assert_eq!(s.status, Status::NoSensor);
    assert!(s.pitch_deg.is_none() && s.roll_deg.is_none() && s.heading_deg.is_none());
    assert_eq!(s.alarm, AlarmLevel::None);
    assert_eq!(s.notice.map(|n| n.kind), Some(NoticeKind::NoSensor));
    assert!(!h.kv.contains_key(KEY_MOUNT));
}

#[test]
fn missing_gyro_and_magnetometer_degrade_gracefully() {
    let mut h = TestHost::default();
    h.set_up(SENSOR_GRAVITY, device_up(0.0, 0.0, 0), 1000);
    let s = tick(&mut h);
    assert!(s.heading_deg.is_none());
    assert!(!s.sensors.gyroscope && !s.sensors.magnetic_field);
    assert!(s.pitch_deg.is_some());
}

#[test]
fn storage_unavailable_refuses_to_calibrate_and_says_so() {
    let mut h = TestHost::default();
    feed_still(&mut h, SENSOR_GRAVITY, 0.0, 0.0, 0, 12);
    request(&mut h, "calibrate");
    // Storage reports down after the host queued the command (read-only mailbox).
    struct Flaky(TestHost);
    impl Host for Flaky {
        fn kv_available(&self) -> bool {
            false
        }
        fn kv_get(&self, key: &str) -> Option<String> {
            self.0.kv_get(key)
        }
        fn kv_set(&mut self, _: &str, _: &str) -> Result<(), String> {
            Err("down".into())
        }
        fn sensor(&self, kind: u32) -> Option<Sample> {
            self.0.sensor(kind)
        }
        fn clock(&self) -> Option<DateStamp> {
            None
        }
    }
    let mut f = Flaky(h);
    let s = tick(&mut f);
    assert!(!s.calibrated);
    assert_eq!(
        s.notice.map(|n| n.kind),
        Some(NoticeKind::StorageUnavailable)
    );
}

#[test]
fn alarm_levels_follow_user_thresholds_with_hysteresis() {
    let (mut h, mut i) = calibrated_host(SENSOR_GRAVITY);
    h.kv.insert(
        KEY_ALARM.into(),
        r#"{"enabled":true,"pitch_warn_deg":10,"pitch_danger_deg":20,"roll_warn_deg":8,"roll_danger_deg":16,"hysteresis_deg":2}"#.into(),
    );
    let mut level = |h: &mut TestHost, pitch: f64, roll: f64| {
        i += 1;
        feed_still(h, SENSOR_GRAVITY, pitch, roll, i, 1).alarm
    };
    assert_eq!(level(&mut h, 5.0, 0.0), AlarmLevel::None);
    assert_eq!(level(&mut h, 11.0, 0.0), AlarmLevel::Warning);
    assert_eq!(
        level(&mut h, 9.0, 0.0),
        AlarmLevel::Warning,
        "hysteresis holds"
    );
    assert_eq!(level(&mut h, 6.0, 0.0), AlarmLevel::None);
    assert_eq!(level(&mut h, 0.0, 17.0), AlarmLevel::Danger);
    assert_eq!(
        level(&mut h, 0.0, 15.0),
        AlarmLevel::Danger,
        "hysteresis holds"
    );
    assert_eq!(level(&mut h, 0.0, 12.0), AlarmLevel::Warning);
    assert_eq!(
        level(&mut h, -21.0, 0.0),
        AlarmLevel::Danger,
        "nose down counts"
    );
}

#[test]
fn alarm_can_be_disabled_and_bad_config_is_sanitised() {
    let (mut h, i) = calibrated_host(SENSOR_GRAVITY);
    h.kv.insert(KEY_ALARM.into(), r#"{"enabled":false}"#.into());
    let s = feed_still(&mut h, SENSOR_GRAVITY, 40.0, 0.0, i, 1);
    assert_eq!(s.alarm, AlarmLevel::None);
    assert!(!s.alarm_enabled);

    // danger below warn is raised to warn; garbage falls back to defaults
    // (alarm stays off unless enabled is set — default is opt-in).
    h.kv.insert(
        KEY_ALARM.into(),
        r#"{"enabled":true,"pitch_warn_deg":25,"pitch_danger_deg":5}"#.into(),
    );
    let s = feed_still(&mut h, SENSOR_GRAVITY, 26.0, 0.0, i + 1, 1);
    assert_eq!(s.alarm, AlarmLevel::Danger);
    h.kv.insert(KEY_ALARM.into(), "not json".into());
    let s = feed_still(&mut h, SENSOR_GRAVITY, 35.0, 0.0, i + 2, 1);
    assert_eq!(
        s.alarm,
        AlarmLevel::None,
        "corrupt config → defaults (alarm off)"
    );
    h.kv.insert(
        KEY_ALARM.into(),
        r#"{"enabled":true,"pitch_warn_deg":20,"pitch_danger_deg":30}"#.into(),
    );
    let s = feed_still(&mut h, SENSOR_GRAVITY, 35.0, 0.0, i + 3, 1);
    assert_eq!(s.alarm, AlarmLevel::Danger, "enabled + defaults 20/30");
}

#[test]
fn heading_is_tilt_compensated_in_vehicle_frame() {
    let (mut h, i) = calibrated_host(SENSOR_GRAVITY);
    // Earth field in vehicle frame facing east, level: north is -X, down is -Z.
    let mag_v = [-20.0, 0.0, -45.0];
    // Express in device frame via the synthetic mount truth (vehicle -> device).
    let mag_d = mat_vec(&mount_truth(), mag_v);
    let t = 10_000 + i as i64 * 100;
    h.set_up(SENSOR_GRAVITY, device_up(0.0, 0.0, i), t);
    h.set_vec(SENSOR_MAGNETIC_FIELD, mag_d, t);
    let s = tick(&mut h);
    let heading = s.heading_deg.expect("heading");
    assert!((heading - 90.0).abs() < 1.5, "east, got {heading}");

    // Facing north (level): north is +Y.
    let mag_d = mat_vec(&mount_truth(), [0.0, 20.0, -45.0]);
    let t = t + 100;
    h.set_up(SENSOR_GRAVITY, device_up(0.0, 0.0, i + 1), t);
    h.set_vec(SENSOR_MAGNETIC_FIELD, mag_d, t);
    let heading = tick(&mut h).heading_deg.unwrap();
    assert!(heading < 1.5 || heading > 358.5, "north, got {heading}");
}

#[test]
fn pitch_sign_conventions() {
    let (mut h, i) = calibrated_host(SENSOR_GRAVITY);
    let s = feed_still(&mut h, SENSOR_GRAVITY, 10.0, 0.0, i, 1);
    assert!(s.pitch_deg.unwrap() > 9.0, "nose up is positive");
    let s = feed_still(&mut h, SENSOR_GRAVITY, 0.0, 10.0, i + 1, 1);
    assert!(s.roll_deg.unwrap() > 9.0, "right side down is positive");
}

#[test]
fn live_scratch_keys_are_not_required_for_display() {
    let mut h = TestHost::default();
    h.set_up(SENSOR_GRAVITY, device_up(0.0, 0.0, 0), 1000);
    let s1 = tick(&mut h);
    let mut h2 = h.restart();
    h2.set_up(SENSOR_GRAVITY, device_up(0.0, 0.0, 0), 1000);
    assert_eq!(tick(&mut h2).pitch_deg, s1.pitch_deg);
}
