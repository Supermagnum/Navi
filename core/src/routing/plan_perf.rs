//! Route-plan phase diagnostics gated by [`set_enabled`].
//!
//! When disabled (default), note helpers are no-ops beyond one atomic load.
//! Hosts mirror the Android Diagnostic logging toggle via UniFFI
//! `set_route_plan_timing_enabled`.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(false);
static PEAK_RSS_KB: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static NOTES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Mirror of UniFFI `set_route_plan_timing_enabled`.
pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
    if enabled {
        sample_rss();
    } else {
        PEAK_RSS_KB.store(0, Ordering::Relaxed);
        NOTES.with(|n| n.borrow_mut().clear());
    }
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Clear per-plan notes and reset peak RSS baseline.
pub fn begin_plan() {
    if !enabled() {
        return;
    }
    NOTES.with(|n| n.borrow_mut().clear());
    PEAK_RSS_KB.store(rss_kb(), Ordering::Relaxed);
    note("plan_perf", "begin");
}

/// Append a `key=value` style note (only when enabled).
pub fn note(key: &str, value: impl AsRef<str>) {
    if !enabled() {
        return;
    }
    sample_rss();
    let line = format!("{key}={}", value.as_ref());
    NOTES.with(|n| n.borrow_mut().push(line));
}

pub fn note_u64(key: &str, value: u64) {
    note(key, value.to_string());
}

pub fn note_f64(key: &str, value: f64) {
    note(key, format!("{value:.3}"));
}

/// Sample `/proc/self/status` VmHWM (peak) and VmRSS; keep max HWM seen.
pub fn sample_rss() {
    if !enabled() {
        return;
    }
    let hwm = rss_hwm_kb();
    let _ = PEAK_RSS_KB.fetch_max(hwm, Ordering::Relaxed);
}

pub fn peak_rss_mb() -> f64 {
    PEAK_RSS_KB.load(Ordering::Relaxed) as f64 / 1024.0
}

/// Drain notes into the plan report (one line each). Returns peak RSS MiB.
pub fn drain_into(report: &mut String) -> f64 {
    sample_rss();
    let peak = peak_rss_mb();
    if !enabled() {
        return peak;
    }
    report.push_str(&format!("peak_rss_mb={peak:.1}\n"));
    NOTES.with(|n| {
        let mut lines = n.borrow_mut();
        if !lines.is_empty() {
            report.push_str("PLAN_PERF |");
            for line in lines.iter() {
                report.push(' ');
                report.push_str(line);
            }
            report.push('\n');
            lines.clear();
        }
    });
    peak
}

fn rss_kb() -> u64 {
    read_status_kib("VmRSS:").unwrap_or(0)
}

fn rss_hwm_kb() -> u64 {
    read_status_kib("VmHWM:")
        .or_else(|| read_status_kib("VmRSS:"))
        .unwrap_or(0)
}

fn read_status_kib(key: &str) -> Option<u64> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix(key) {
            return rest.split_whitespace().next()?.parse().ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn disabled_is_noop() {
        let _g = TEST_LOCK.lock().unwrap();
        set_enabled(false);
        begin_plan();
        note("x", "y");
        let mut report = String::new();
        let _ = drain_into(&mut report);
        assert!(!report.contains("PLAN_PERF"));
    }

    #[test]
    fn enabled_collects_notes() {
        let _g = TEST_LOCK.lock().unwrap();
        set_enabled(true);
        begin_plan();
        note("stem", "ostlandet-latest");
        note_u64("tiles", 3);
        let mut report = String::new();
        let peak = drain_into(&mut report);
        set_enabled(false);
        assert!(report.contains("PLAN_PERF"));
        assert!(report.contains("stem=ostlandet-latest"));
        assert!(report.contains("tiles=3"));
        assert!(report.contains("peak_rss_mb="));
        assert!(peak >= 0.0);
    }
}
