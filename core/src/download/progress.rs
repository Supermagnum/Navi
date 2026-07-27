//! Shared download progress for UI polling (region PBF + PMTiles).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

static BYTES: AtomicU64 = AtomicU64::new(0);
static TOTAL: AtomicU64 = AtomicU64::new(0); // 0 => unknown
static LABEL: OnceLock<Mutex<String>> = OnceLock::new();

fn label_lock() -> &'static Mutex<String> {
    LABEL.get_or_init(|| Mutex::new(String::new()))
}

/// Update progress visible to the host UI.
pub fn set(bytes_or_units: u64, total: Option<u64>, label: &str) {
    BYTES.store(bytes_or_units, Ordering::Relaxed);
    TOTAL.store(total.unwrap_or(0), Ordering::Relaxed);
    if let Ok(mut g) = label_lock().lock() {
        *g = label.to_string();
    }
}

pub fn clear() {
    BYTES.store(0, Ordering::Relaxed);
    TOTAL.store(0, Ordering::Relaxed);
    if let Ok(mut g) = label_lock().lock() {
        g.clear();
    }
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub units_done: u64,
    pub units_total: Option<u64>,
    pub percent: Option<u32>,
    pub label: String,
}

pub fn snapshot() -> Snapshot {
    let done = BYTES.load(Ordering::Relaxed);
    let total_raw = TOTAL.load(Ordering::Relaxed);
    let total = if total_raw == 0 {
        None
    } else {
        Some(total_raw)
    };
    let percent = total.map(|t| {
        if t == 0 {
            100
        } else {
            ((done.saturating_mul(100)) / t).min(100) as u32
        }
    });
    let label = label_lock().lock().map(|g| g.clone()).unwrap_or_default();
    Snapshot {
        units_done: done,
        units_total: total,
        percent,
        label,
    }
}
