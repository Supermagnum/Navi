//! Durable plan artifacts for multi-hop routing.
//!
//! Canonical directory (app filesDir, also mirrored to external storage by the
//! Android UI): `{data_dir}/long-trip-ui-report/`
//!
//! Files:
//! - `routing-plan.log` — hop endpoints, stems, `hop_result=success|disconnected`,
//!   and a final `plan_summary` (km, eta, ferry legs, terminate).
//! - `route-polyline.txt` — full edge polyline (written by the app on PASS).
//! - `hops.json` — hop sidecar (written at the end of a chunked PASS).
//!
//! Host pull: `adb exec-out run-as no.navi.app cat files/long-trip-ui-report/routing-plan.log`
//! or the same folder under `getExternalFilesDir` (`Android/data/no.navi.app/files/`).

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Folder name under the app data directory (and the host-visible external twin).
pub const REPORT_DIR: &str = "long-trip-ui-report";
pub const LOG_NAME: &str = "routing-plan.log";
pub const HOPS_NAME: &str = "hops.json";
pub const POLYLINE_NAME: &str = "route-polyline.txt";

static REPORT_ROOT: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn set_data_dir(data_dir: &str) {
    let dir = data_dir.trim();
    if dir.is_empty() {
        return;
    }
    let root = Path::new(dir).join(REPORT_DIR);
    if let Ok(mut g) = REPORT_ROOT.lock() {
        *g = Some(root);
    }
}

fn report_root() -> Option<PathBuf> {
    REPORT_ROOT.lock().ok().and_then(|g| g.clone())
}

pub fn line(msg: impl AsRef<str>) {
    let msg = msg.as_ref();
    log::info!(target: "NaviPlanFile", "{msg}");
    let Some(root) = report_root() else {
        return;
    };
    let path = root.join(LOG_NAME);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };
    let _ = writeln!(f, "{msg}");
    let _ = f.flush();
}

/// Replace (not append) a named file in the canonical report directory.
pub fn write_file(name: &str, body: impl AsRef<[u8]>) {
    let Some(root) = report_root() else {
        return;
    };
    let _ = std::fs::create_dir_all(&root);
    let path = root.join(name);
    let _ = std::fs::write(path, body);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn writes_log_and_sidecar_under_long_trip_ui_report() {
        let _g = TEST_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!(
            "navi-plan-file-log-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        set_data_dir(dir.to_str().unwrap());
        line("hop_result=success km=1.0");
        write_file(HOPS_NAME, b"{\"hops\":[]}");
        let log = dir.join(REPORT_DIR).join(LOG_NAME);
        let hops = dir.join(REPORT_DIR).join(HOPS_NAME);
        let text = fs::read_to_string(&log).unwrap();
        assert!(text.contains("hop_result=success"), "{text}");
        assert_eq!(fs::read_to_string(&hops).unwrap(), "{\"hops\":[]}");
        let _ = fs::remove_dir_all(&dir);
    }
}
