//! Wall-clock phase timing for download / index pipelines.
//!
//! Logs use target `PHASE_TIMING` so device runs can be grepped without noise:
//! `adb logcat -s PHASE_TIMING:I NaviNative:I` (Rust lines also appear under
//! the `NaviNative` tag with this module path).
//!
//! Every [`start`] also emits a lightweight `HOST` snapshot (RAM / thermal /
//! process I/O / free space) on the same target so a field slowdown has
//! diagnostic context without a separate debug build.
//!
//! # Disk-contention diagnostics (platform limits)
//!
//! **System-wide** block I/O counters (`/proc/diskstats`, `/sys/block/*/stat`)
//! are **not readable** by the Android app UID on locked-down devices
//! (confirmed on Samsung SM-P613). Android public APIs do not fill that gap:
//! [`StatFs`](https://developer.android.com/reference/android/os/StatFs) and
//! `StorageStatsManager` expose capacity / per-app usage only — not device I/O
//! rates or other processes' flash traffic. Returning `disk_*_sectors=0` with
//! `disk_stats=unavailable` is therefore a **permanent platform limitation**,
//! not an incomplete implementation.
//!
//! **Proxy:** each phase end logs `/proc/self/io` deltas and MB/s since the
//! matching [`start`]. For I/O-heavy phases, if observed throughput falls far
//! below the SM-P613 baseline floor, we set `suspected_disk_contention=1`.
//! That flag cannot name the contending writer; it only marks that *our*
//! storage throughput collapsed relative to a healthy run.

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::{LazyLock, Mutex};
use std::time::Instant;

struct IoAnchor {
    proc_r: u64,
    proc_w: u64,
}

static IO_ANCHORS: LazyLock<Mutex<HashMap<String, IoAnchor>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Start a named phase; logs `START` plus a [`HOST`](host_snapshot_line) line.
pub fn start(phase: &str) -> Instant {
    log::info!(target: "PHASE_TIMING", "START phase={phase}");
    log_host_snapshot(phase);
    let (proc_r, proc_w) = read_proc_self_io();
    if let Ok(mut map) = IO_ANCHORS.lock() {
        map.insert(phase.to_string(), IoAnchor { proc_r, proc_w });
    }
    Instant::now()
}

/// End a phase started with [`start`]; logs `END` with elapsed ms plus I/O proxy.
pub fn end(phase: &str, t0: Instant) {
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    log::info!(
        target: "PHASE_TIMING",
        "END phase={phase} elapsed_ms={ms:.1}"
    );
    log_phase_io_proxy(phase, ms);
}

/// End a phase with an extra key=value detail suffix (already formatted).
pub fn end_detail(phase: &str, t0: Instant, detail: &str) {
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    log::info!(
        target: "PHASE_TIMING",
        "END phase={phase} elapsed_ms={ms:.1} {detail}"
    );
    log_phase_io_proxy(phase, ms);
}

/// Time a closure as a named phase.
pub fn timed<T>(phase: &str, f: impl FnOnce() -> T) -> T {
    let t0 = start(phase);
    let out = f();
    end(phase, t0);
    out
}

fn log_host_snapshot(phase: &str) {
    let line = host_snapshot_line(phase);
    log::info!(target: "PHASE_TIMING", "{line}");
}

/// One-line host health for logcat / session logs. Best-effort; never panics.
pub fn host_snapshot_line(phase: &str) -> String {
    let (mem_avail_kb, mem_total_kb, mem_free_kb) = read_meminfo_kb();
    let thermal = read_thermal_sample();
    let (disk_rd, disk_wr, disk_stats) = read_diskstats_labeled();
    let (proc_r_bytes, proc_w_bytes) = read_proc_self_io();
    let avail_bytes = read_avail_bytes();
    format!(
        "HOST phase={phase} mem_avail_kb={mem_avail_kb} mem_free_kb={mem_free_kb} \
         mem_total_kb={mem_total_kb} thermal_mC={thermal} \
         disk_rd_sectors={disk_rd} disk_wr_sectors={disk_wr} disk_stats={disk_stats} \
         proc_read_bytes={proc_r_bytes} proc_write_bytes={proc_w_bytes} \
         avail_bytes={avail_bytes}"
    )
}

fn log_phase_io_proxy(phase: &str, elapsed_ms: f64) {
    let anchor = IO_ANCHORS.lock().ok().and_then(|mut map| map.remove(phase));
    let Some(anchor) = anchor else {
        return;
    };
    let (proc_r, proc_w) = read_proc_self_io();
    let read_delta = proc_r.saturating_sub(anchor.proc_r);
    let write_delta = proc_w.saturating_sub(anchor.proc_w);
    let secs = (elapsed_ms / 1000.0).max(0.001);
    let read_mb_s = (read_delta as f64) / (1024.0 * 1024.0) / secs;
    let write_mb_s = (write_delta as f64) / (1024.0 * 1024.0) / secs;
    let suspected = suspected_disk_contention(
        phase,
        elapsed_ms,
        read_delta,
        write_delta,
        read_mb_s,
        write_mb_s,
    );
    log::info!(
        target: "PHASE_TIMING",
        "IO_PROXY phase={phase} elapsed_ms={elapsed_ms:.1} \
         proc_read_delta_b={read_delta} proc_write_delta_b={write_delta} \
         proc_read_mb_s={read_mb_s:.3} proc_write_mb_s={write_mb_s:.3} \
         suspected_disk_contention={suspected}"
    );
}

/// SM-P613 healthy-run floors (MB/s). Flag when a long enough sample falls
/// below ~¼ of the measured baseline for that phase's dominant direction.
fn suspected_disk_contention(
    phase: &str,
    elapsed_ms: f64,
    read_delta: u64,
    write_delta: u64,
    read_mb_s: f64,
    write_mb_s: f64,
) -> u8 {
    // Need a meaningful sample: ≥15 s and ≥1 MiB of process I/O in the
    // direction we care about (avoids flagging CPU-bound tails).
    const MIN_MS: f64 = 15_000.0;
    const MIN_BYTES: u64 = 1024 * 1024;
    if elapsed_ms < MIN_MS {
        return 0;
    }

    // Floors from SM-P613 Østlandet pack-server / place-index baselines
    // (~5.8–6.2 min place index, ~7.4 min PMTiles). ~4× below healthy ⇒ flag.
    let (need_write, floor_mb_s) = match phase {
        "place_index.sqlite_insert_rows" | "place_index.sqlite_write" => (true, 0.25),
        "pmtiles.extract.write_archive" => (true, 1.0),
        "geofabrik_pbf.download" | "pack_fetch.download_files" | "pack_fetch.file" => (true, 1.0),
        "place_index.admin"
        | "place_index.admin.relations"
        | "place_index.admin.ways"
        | "place_index.admin.nodes"
        | "place_index.ways"
        | "place_index.nodes"
        | "place_index.named_routes"
        | "pmtiles.extract.fetch_coalesced" => (false, 0.5),
        _ => return 0,
    };

    if need_write {
        if write_delta < MIN_BYTES {
            return 0;
        }
        u8::from(write_mb_s < floor_mb_s)
    } else {
        if read_delta < MIN_BYTES {
            return 0;
        }
        u8::from(read_mb_s < floor_mb_s)
    }
}

fn read_meminfo_kb() -> (i64, i64, i64) {
    let Ok(file) = fs::File::open("/proc/meminfo") else {
        return (-1, -1, -1);
    };
    let mut avail = -1_i64;
    let mut total = -1_i64;
    let mut free = -1_i64;
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        if let Some(v) = parse_meminfo_kib(&line, "MemAvailable:") {
            avail = v;
        } else if let Some(v) = parse_meminfo_kib(&line, "MemTotal:") {
            total = v;
        } else if let Some(v) = parse_meminfo_kib(&line, "MemFree:") {
            free = v;
        }
        if avail >= 0 && total >= 0 && free >= 0 {
            break;
        }
    }
    (avail, total, free)
}

fn parse_meminfo_kib(line: &str, key: &str) -> Option<i64> {
    let rest = line.strip_prefix(key)?.trim();
    let num = rest.split_whitespace().next()?;
    num.parse().ok()
}

/// Up to four thermal zones as `type:temp_mC` joined by `,`.
fn read_thermal_sample() -> String {
    let mut preferred = Vec::new();
    let mut other = Vec::new();
    for idx in 0..16 {
        let base = format!("/sys/class/thermal/thermal_zone{idx}");
        let Ok(ty) = fs::read_to_string(format!("{base}/type")) else {
            continue;
        };
        let Ok(temp_s) = fs::read_to_string(format!("{base}/temp")) else {
            continue;
        };
        let ty = ty.trim();
        let temp_s = temp_s.trim();
        if ty.is_empty() || temp_s.is_empty() {
            continue;
        }
        let entry = format!("{ty}:{temp_s}");
        let prefer = ty.contains("cpu")
            || ty.contains("skin")
            || ty.contains("battery")
            || ty.contains("AP")
            || ty.contains("LITTLE")
            || ty.contains("BIG");
        if prefer {
            preferred.push(entry);
        } else {
            other.push(entry);
        }
    }
    preferred.extend(other);
    preferred.truncate(4);
    if preferred.is_empty() {
        "none".into()
    } else {
        preferred.join(",")
    }
}

/// `(rd, wr, source)` where source is `proc`, `sysfs`, or `unavailable`.
///
/// `unavailable` is expected on app-UID Android (no root). Do not treat zeros
/// alone as “idle disk” — check `disk_stats=` on the HOST line.
fn read_diskstats_labeled() -> (u64, u64, &'static str) {
    let from_proc = read_diskstats_proc();
    if from_proc.0 > 0 || from_proc.1 > 0 {
        return (from_proc.0, from_proc.1, "proc");
    }
    let from_sys = read_diskstats_sysfs();
    if from_sys.0 > 0 || from_sys.1 > 0 {
        return (from_sys.0, from_sys.1, "sysfs");
    }
    (0, 0, "unavailable")
}

fn read_diskstats_proc() -> (u64, u64) {
    let Ok(file) = fs::File::open("/proc/diskstats") else {
        return (0, 0);
    };
    let mut rd = 0_u64;
    let mut wr = 0_u64;
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let mut parts = line.split_whitespace();
        let Some(_maj) = parts.next() else { continue };
        let Some(_min) = parts.next() else { continue };
        let Some(name) = parts.next() else { continue };
        if !keep_block_device(name) {
            continue;
        }
        let Some(rd_sec) = parts.nth(2).and_then(|s| s.parse::<u64>().ok()) else {
            continue;
        };
        let Some(wr_sec) = parts.nth(3).and_then(|s| s.parse::<u64>().ok()) else {
            continue;
        };
        rd = rd.saturating_add(rd_sec);
        wr = wr.saturating_add(wr_sec);
    }
    (rd, wr)
}

fn read_diskstats_sysfs() -> (u64, u64) {
    let Ok(entries) = fs::read_dir("/sys/block") else {
        return (0, 0);
    };
    let mut rd = 0_u64;
    let mut wr = 0_u64;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !keep_block_device(&name) {
            continue;
        }
        let stat_path = entry.path().join("stat");
        let Ok(text) = fs::read_to_string(stat_path) else {
            continue;
        };
        let mut parts = text.split_whitespace();
        let Some(rd_sec) = parts.nth(2).and_then(|s| s.parse::<u64>().ok()) else {
            continue;
        };
        let Some(wr_sec) = parts.nth(3).and_then(|s| s.parse::<u64>().ok()) else {
            continue;
        };
        rd = rd.saturating_add(rd_sec);
        wr = wr.saturating_add(wr_sec);
    }
    (rd, wr)
}

fn keep_block_device(name: &str) -> bool {
    if name.starts_with("loop") || name.starts_with("ram") || name.starts_with("zram") {
        return false;
    }
    let last = name.chars().last();
    let is_partition = last.is_some_and(|c| c.is_ascii_digit())
        && (name.starts_with("sd")
            || name.starts_with("vd")
            || name.starts_with("nvme")
            || (name.starts_with("mmc") && name.contains('p')));
    !is_partition
}

fn read_proc_self_io() -> (u64, u64) {
    let Ok(text) = fs::read_to_string("/proc/self/io") else {
        return (0, 0);
    };
    let mut read_bytes = 0_u64;
    let mut write_bytes = 0_u64;
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("read_bytes:") {
            read_bytes = v.trim().parse().unwrap_or(0);
        } else if let Some(v) = line.strip_prefix("write_bytes:") {
            write_bytes = v.trim().parse().unwrap_or(0);
        }
    }
    (read_bytes, write_bytes)
}

fn read_avail_bytes() -> i64 {
    for candidate in ["/data", "/data/user/0", ".", "/"] {
        if let Some(n) = avail_bytes_path(Path::new(candidate)) {
            return n;
        }
    }
    -1
}

fn avail_bytes_path(path: &Path) -> Option<i64> {
    use std::ffi::CString;
    let c = CString::new(path.to_string_lossy().as_bytes()).ok()?;
    unsafe {
        let mut st: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c.as_ptr(), &mut st) != 0 {
            return None;
        }
        let bsize = st.f_frsize as u64;
        let avail = st.f_bavail as u64;
        Some((bsize.saturating_mul(avail)) as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_meminfo_line() {
        assert_eq!(
            parse_meminfo_kib("MemAvailable:   123456 kB", "MemAvailable:"),
            Some(123456)
        );
        assert_eq!(parse_meminfo_kib("MemTotal: 1 kB", "MemFree:"), None);
    }

    #[test]
    fn host_snapshot_line_is_nonempty() {
        let line = host_snapshot_line("test.phase");
        assert!(line.starts_with("HOST phase=test.phase "));
        assert!(line.contains("mem_avail_kb="));
        assert!(line.contains("disk_stats="));
        assert!(line.contains("proc_read_bytes="));
        assert!(line.contains("proc_write_bytes="));
    }

    #[test]
    fn contention_flags_slow_sqlite_write() {
        // 100 MiB over 400 s ⇒ 0.25 MB/s exactly at floor → not flagged (< floor)
        assert_eq!(
            suspected_disk_contention(
                "place_index.sqlite_insert_rows",
                400_000.0,
                0,
                100 * 1024 * 1024,
                0.0,
                0.24
            ),
            1
        );
        assert_eq!(
            suspected_disk_contention(
                "place_index.sqlite_insert_rows",
                400_000.0,
                0,
                100 * 1024 * 1024,
                0.0,
                1.0
            ),
            0
        );
        // Too short / too little I/O → no flag
        assert_eq!(
            suspected_disk_contention(
                "place_index.sqlite_insert_rows",
                5_000.0,
                0,
                100 * 1024 * 1024,
                0.0,
                0.01
            ),
            0
        );
    }

    #[test]
    fn io_proxy_logs_after_start_end() {
        let t0 = start("place_index.sqlite_insert_rows");
        end("place_index.sqlite_insert_rows", t0);
        // Anchor consumed; second end is a no-op for IO_PROXY.
        end("place_index.sqlite_insert_rows", t0);
    }
}
