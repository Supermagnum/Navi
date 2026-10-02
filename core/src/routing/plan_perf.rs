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
    static PACK_STAGE_MS: RefCell<PackStageMs> = const { RefCell::new(PackStageMs::zero()) };
}

#[derive(Clone, Copy, Default)]
struct PackStageMs {
    mmap: u64,
    pagein: u64,
    validate: u64,
    copy: u64,
    merge_hash: u64,
    merge_adj: u64,
    ferry: u64,
    tile_bytes: u64,
    tiles: u64,
}

impl PackStageMs {
    const fn zero() -> Self {
        Self {
            mmap: 0,
            pagein: 0,
            validate: 0,
            copy: 0,
            merge_hash: 0,
            merge_adj: 0,
            ferry: 0,
            tile_bytes: 0,
            tiles: 0,
        }
    }
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
    PACK_STAGE_MS.with(|s| *s.borrow_mut() = PackStageMs::zero());
    PEAK_RSS_KB.store(rss_kb(), Ordering::Relaxed);
    note("plan_perf", "begin");
}

/// Accumulate pack_load stage timings (Step 1 breakdown). No-op when disabled.
pub fn add_pack_stage_ms(mmap: u64, pagein: u64, validate: u64, copy: u64, tile_bytes: u64) {
    if !enabled() {
        return;
    }
    PACK_STAGE_MS.with(|s| {
        let mut st = s.borrow_mut();
        st.mmap = st.mmap.saturating_add(mmap);
        st.pagein = st.pagein.saturating_add(pagein);
        st.validate = st.validate.saturating_add(validate);
        st.copy = st.copy.saturating_add(copy);
        st.tile_bytes = st.tile_bytes.saturating_add(tile_bytes);
        st.tiles = st.tiles.saturating_add(1);
    });
}

pub fn add_merge_stage_ms(hash_ms: u64, adj_ms: u64) {
    if !enabled() {
        return;
    }
    PACK_STAGE_MS.with(|s| {
        let mut st = s.borrow_mut();
        st.merge_hash = st.merge_hash.saturating_add(hash_ms);
        st.merge_adj = st.merge_adj.saturating_add(adj_ms);
    });
}

pub fn add_ferry_stage_ms(ms: u64) {
    if !enabled() {
        return;
    }
    PACK_STAGE_MS.with(|s| {
        let mut st = s.borrow_mut();
        st.ferry = st.ferry.saturating_add(ms);
    });
}

fn flush_pack_stage_notes() {
    PACK_STAGE_MS.with(|s| {
        let st = *s.borrow();
        if st.tiles == 0 && st.merge_hash == 0 && st.ferry == 0 {
            return;
        }
        note_u64("pack_stage_tiles", st.tiles);
        note_u64("pack_stage_tile_bytes", st.tile_bytes);
        note_u64("pack_stage_mmap_ms", st.mmap);
        note_u64("pack_stage_pagein_ms", st.pagein);
        note_u64("pack_stage_validate_ms", st.validate);
        note_u64("pack_stage_copy_ms", st.copy);
        note_u64("pack_stage_merge_hash_ms", st.merge_hash);
        note_u64("pack_stage_merge_adj_ms", st.merge_adj);
        note_u64("pack_stage_ferry_ms", st.ferry);
        note(
            "pack_stage_threads",
            "single_threaded;merge_rebuilds_adjacency_per_tile",
        );
        // Short greppable line — Android logcat truncates long PLAN_PERF rows.
        log::info!(
            target: "NaviPlan",
            "PACK_STAGE_SUMMARY tiles={} tile_bytes={} mmap_ms={} pagein_ms={} validate_ms={} \
             copy_ms={} merge_hash_ms={} merge_adj_ms={} ferry_ms={} threads=1",
            st.tiles,
            st.tile_bytes,
            st.mmap,
            st.pagein,
            st.validate,
            st.copy,
            st.merge_hash,
            st.merge_adj,
            st.ferry
        );
    });
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
    flush_pack_stage_notes();
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
