//! Route-plan phase diagnostics gated by [`set_enabled`].
//!
//! When disabled (default), note helpers are no-ops beyond one atomic load.
//! Hosts mirror the Android Diagnostic logging toggle via UniFFI
//! `set_route_plan_timing_enabled`.
//!
//! Pack-stage counters and notes are process-wide (Mutex / Atomic) so bounded
//! parallel tile loads (2b) accumulate correctly across worker threads.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

static ENABLED: AtomicBool = AtomicBool::new(false);
static PEAK_RSS_KB: AtomicU64 = AtomicU64::new(0);

static NOTES: Mutex<Vec<String>> = Mutex::new(Vec::new());

static STAGE_MMAP: AtomicU64 = AtomicU64::new(0);
static STAGE_PAGEIN: AtomicU64 = AtomicU64::new(0);
static STAGE_VALIDATE: AtomicU64 = AtomicU64::new(0);
static STAGE_COPY: AtomicU64 = AtomicU64::new(0);
static STAGE_MERGE_HASH: AtomicU64 = AtomicU64::new(0);
static STAGE_MERGE_ADJ: AtomicU64 = AtomicU64::new(0);
static STAGE_FERRY: AtomicU64 = AtomicU64::new(0);
static STAGE_TILE_BYTES: AtomicU64 = AtomicU64::new(0);
static STAGE_TILES: AtomicU64 = AtomicU64::new(0);
static TILE_LOAD_PARALLEL: AtomicU64 = AtomicU64::new(1);

fn clear_pack_stages() {
    STAGE_MMAP.store(0, Ordering::Relaxed);
    STAGE_PAGEIN.store(0, Ordering::Relaxed);
    STAGE_VALIDATE.store(0, Ordering::Relaxed);
    STAGE_COPY.store(0, Ordering::Relaxed);
    STAGE_MERGE_HASH.store(0, Ordering::Relaxed);
    STAGE_MERGE_ADJ.store(0, Ordering::Relaxed);
    STAGE_FERRY.store(0, Ordering::Relaxed);
    STAGE_TILE_BYTES.store(0, Ordering::Relaxed);
    STAGE_TILES.store(0, Ordering::Relaxed);
}

/// Mirror of UniFFI `set_route_plan_timing_enabled`.
pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
    if enabled {
        sample_rss();
    } else {
        PEAK_RSS_KB.store(0, Ordering::Relaxed);
        if let Ok(mut n) = NOTES.lock() {
            n.clear();
        }
        clear_pack_stages();
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
    if let Ok(mut n) = NOTES.lock() {
        n.clear();
    }
    clear_pack_stages();
    PEAK_RSS_KB.store(rss_kb(), Ordering::Relaxed);
    note("plan_perf", "begin");
}

/// Accumulate pack_load stage timings (Step 1 breakdown). No-op when disabled.
pub fn add_pack_stage_ms(mmap: u64, pagein: u64, validate: u64, copy: u64, tile_bytes: u64) {
    if !enabled() {
        return;
    }
    STAGE_MMAP.fetch_add(mmap, Ordering::Relaxed);
    STAGE_PAGEIN.fetch_add(pagein, Ordering::Relaxed);
    STAGE_VALIDATE.fetch_add(validate, Ordering::Relaxed);
    STAGE_COPY.fetch_add(copy, Ordering::Relaxed);
    STAGE_TILE_BYTES.fetch_add(tile_bytes, Ordering::Relaxed);
    STAGE_TILES.fetch_add(1, Ordering::Relaxed);
}

pub fn add_merge_stage_ms(hash_ms: u64, adj_ms: u64) {
    if !enabled() {
        return;
    }
    STAGE_MERGE_HASH.fetch_add(hash_ms, Ordering::Relaxed);
    STAGE_MERGE_ADJ.fetch_add(adj_ms, Ordering::Relaxed);
}

pub fn add_ferry_stage_ms(ms: u64) {
    if !enabled() {
        return;
    }
    STAGE_FERRY.fetch_add(ms, Ordering::Relaxed);
}

/// Record the tile-load pool size used for this plan (for PACK_STAGE_SUMMARY).
pub fn set_tile_load_parallel(n: u64) {
    TILE_LOAD_PARALLEL.store(n.max(1), Ordering::Relaxed);
}

fn flush_pack_stage_notes() {
    let tiles = STAGE_TILES.load(Ordering::Relaxed);
    let merge_hash = STAGE_MERGE_HASH.load(Ordering::Relaxed);
    let ferry = STAGE_FERRY.load(Ordering::Relaxed);
    if tiles == 0 && merge_hash == 0 && ferry == 0 {
        return;
    }
    let tile_bytes = STAGE_TILE_BYTES.load(Ordering::Relaxed);
    let mmap = STAGE_MMAP.load(Ordering::Relaxed);
    let pagein = STAGE_PAGEIN.load(Ordering::Relaxed);
    let validate = STAGE_VALIDATE.load(Ordering::Relaxed);
    let copy = STAGE_COPY.load(Ordering::Relaxed);
    let merge_adj = STAGE_MERGE_ADJ.load(Ordering::Relaxed);
    let parallel = TILE_LOAD_PARALLEL.load(Ordering::Relaxed).max(1);
    note_u64("pack_stage_tiles", tiles);
    note_u64("pack_stage_tile_bytes", tile_bytes);
    note_u64("pack_stage_mmap_ms", mmap);
    note_u64("pack_stage_pagein_ms", pagein);
    note_u64("pack_stage_validate_ms", validate);
    note_u64("pack_stage_copy_ms", copy);
    note_u64("pack_stage_merge_hash_ms", merge_hash);
    note_u64("pack_stage_merge_adj_ms", merge_adj);
    note_u64("pack_stage_ferry_ms", ferry);
    note(
        "pack_stage_threads",
        format!("tile_load_parallel={parallel};merge=single_pass_one_adjacency"),
    );
    // Short greppable line — Android logcat truncates long PLAN_PERF rows.
    log::info!(
        target: "NaviPlan",
        "PACK_STAGE_SUMMARY tiles={} tile_bytes={} mmap_ms={} pagein_ms={} validate_ms={} \
         copy_ms={} merge_hash_ms={} merge_adj_ms={} ferry_ms={} threads={}",
        tiles, tile_bytes, mmap, pagein, validate, copy, merge_hash, merge_adj, ferry, parallel
    );
}

/// Append a `key=value` style note (only when enabled).
pub fn note(key: &str, value: impl AsRef<str>) {
    if !enabled() {
        return;
    }
    sample_rss();
    let line = format!("{key}={}", value.as_ref());
    if let Ok(mut n) = NOTES.lock() {
        n.push(line);
    }
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
    if let Ok(mut lines) = NOTES.lock() {
        if !lines.is_empty() {
            report.push_str("PLAN_PERF |");
            for line in lines.iter() {
                report.push(' ');
                report.push_str(line);
            }
            report.push('\n');
            lines.clear();
        }
    }
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
        add_pack_stage_ms(1, 2, 3, 4, 5);
        let mut report = String::new();
        let peak = drain_into(&mut report);
        set_enabled(false);
        assert!(report.contains("PLAN_PERF"));
        assert!(report.contains("stem=ostlandet-latest"));
        assert!(report.contains("tiles=3"));
        assert!(report.contains("pack_stage_copy_ms=4"));
        assert!(report.contains("peak_rss_mb="));
        assert!(peak >= 0.0);
    }
}
