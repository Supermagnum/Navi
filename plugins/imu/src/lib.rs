//! IMU guest: thin wasm32 shim over `navi-plugin-imu-engine`.
//!
//! Pitch / tilt / compass, vehicle-mount level calibration and the tilt-danger
//! alarm level. The guest only reads sensors and writes `plugin_kv`; the host
//! owns sensor registration (rate-limited), UI and alarm audio.

use navi_plugin_imu_engine::{DateStamp, Host, Sample};

struct SdkHost;

impl Host for SdkHost {
    fn kv_available(&self) -> bool {
        navi_plugin_sdk::host_plugin_kv_available()
    }

    fn kv_get(&self, key: &str) -> Option<String> {
        let mut buf = [0u8; 4096];
        let n = navi_plugin_sdk::host_plugin_kv_get(key, &mut buf)?;
        std::str::from_utf8(&buf[..n]).ok().map(str::to_string)
    }

    fn kv_set(&mut self, key: &str, value: &str) -> Result<(), String> {
        navi_plugin_sdk::host_plugin_kv_set(key, value).map_err(|rc| format!("kv_set {rc}"))
    }

    fn sensor(&self, kind: u32) -> Option<Sample> {
        let mut buf = [0u8; 512];
        let n = navi_plugin_sdk::host_sensor_read(kind, &mut buf);
        Sample::from_read_json(&buf[..n])
    }

    fn clock(&self) -> Option<DateStamp> {
        let mut buf = [0u8; 256];
        let n = navi_plugin_sdk::host_clock_read(&mut buf);
        serde_json::from_slice::<Option<DateStamp>>(&buf[..n])
            .ok()
            .flatten()
    }
}

#[no_mangle]
pub extern "C" fn plugin_main() {
    let state = navi_plugin_imu_engine::tick(&mut SdkHost);
    // Log only fresh one-shot notices; the display state is read from plugin_kv.
    if let Some(n) = state.notice.filter(|n| n.at_ms == state.updated_ms) {
        navi_plugin_sdk::host_log(&format!("imu: {}", n.text));
    }
}
