//! Append-only plan diagnostics under the app data directory.
//!
//! Logcat rotates during multi-hop plans; this file is the durable hop record.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static LOG_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn set_data_dir(data_dir: &str) {
    let dir = data_dir.trim();
    if dir.is_empty() {
        return;
    }
    let path = Path::new(dir).join("routing-plan.log");
    if let Ok(mut g) = LOG_PATH.lock() {
        *g = Some(path);
    }
}

pub fn line(msg: impl AsRef<str>) {
    let msg = msg.as_ref();
    log::info!(target: "NaviPlanFile", "{msg}");
    let path = {
        let Ok(g) = LOG_PATH.lock() else {
            return;
        };
        g.clone()
    };
    let Some(path) = path else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };
    let _ = writeln!(f, "{msg}");
    let _ = f.flush();
}
