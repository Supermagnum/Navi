//! Exclusive ownership of a region's PBF-backed build work.
//!
//! Convert (graph / POI+barrier / wetland) and pack-miss plan fallback share
//! one lock **per region identity**, not per filesystem path. A production
//! `…/files/ostlandet-latest.osm.pbf` and a fixture clone under
//! `/data/local/tmp/navi_fixtures/` therefore cannot both walk the extract.
//!
//! Android convert and `plan_car_route` / `plan_hiking_route` run in the same
//! process (`libnavi.so`). The in-process map serializes those callers; the
//! sibling lock file plus checkpoint owner fields recover after force-stop
//! (Drop does not run on SIGKILL). Host `navi_indexed_convert` is a separate
//! process and uses the same file.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::routing::pbf_stem_to_geofabrik_path;

/// `convert_region_packs` returns this when another living convert owns the region.
pub const REGION_CONVERT_IN_PROGRESS: &str = "region convert already in progress";

thread_local! {
    static HOLDING_CONVERT_LOCK: AtomicBool = const { AtomicBool::new(false) };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionLockKind {
    Convert,
    PlanFallback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionLockPhase {
    Graphs,
    PoiBarrier,
    Wetland,
    PlanFallback,
}

impl RegionLockPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Graphs => "graphs",
            Self::PoiBarrier => "poi_barrier",
            Self::Wetland => "wetland",
            Self::PlanFallback => "plan_fallback",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionLockRecord {
    pub region_id: String,
    pub pid: u32,
    pub kind: RegionLockKind,
    pub phase: RegionLockPhase,
    pub started_unix_secs: u64,
}

struct LiveEntry {
    kind: RegionLockKind,
    pid: u32,
}

struct Registry {
    map: HashMap<String, LiveEntry>,
}

static REGISTRY: LazyLock<Mutex<Registry>> = LazyLock::new(|| {
    Mutex::new(Registry {
        map: HashMap::new(),
    })
});

fn registry() -> std::sync::MutexGuard<'static, Registry> {
    REGISTRY.lock().unwrap_or_else(|e| e.into_inner())
}

/// In-process registry key: pack dir + region identity (not PBF path).
fn registry_key(data_dir: &Path, region_id: &str) -> String {
    format!("{}::{}", data_dir.display(), region_id)
}

/// Region identity for locking: Geofabrik path when known, else PBF stem.
///
/// Paths are ignored. `ostlandet-latest.osm.pbf` next to packs and the same
/// leaf under `navi_fixtures` share a lock. `oppland-latest` aliases Ostlandet.
pub fn region_id_for_pbf(pbf: &Path) -> String {
    let name = pbf.file_name().and_then(|s| s.to_str()).unwrap_or("region");
    let stem = name
        .strip_suffix(".osm.pbf")
        .or_else(|| name.strip_suffix(".pbf"))
        .unwrap_or(name);
    pbf_stem_to_geofabrik_path(stem).unwrap_or_else(|| stem.to_ascii_lowercase())
}

fn sanitize_region_id(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub fn region_lock_path(data_dir: &Path, region_id: &str) -> PathBuf {
    data_dir.join(format!(
        "{}.navi-region-lock.json",
        sanitize_region_id(region_id)
    ))
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn pid_is_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    if pid == std::process::id() {
        return true;
    }
    #[cfg(unix)]
    {
        // Signal 0: existence check, no signal delivered.
        let rc = unsafe { libc::kill(pid as i32, 0) };
        if rc == 0 {
            return true;
        }
        let err = std::io::Error::last_os_error().raw_os_error();
        // ESRCH = no such process. EPERM = alive but not signalable.
        err != Some(libc::ESRCH)
    }
    #[cfg(not(unix))]
    {
        false
    }
}

/// Delete leaked `navi-tiled-ways*` spill files left by `pid`.
pub fn cleanup_spills_for_pid(data_dir: &Path, pid: u32) {
    let needle = format!("-{pid}-");
    let Ok(rd) = fs::read_dir(data_dir) else {
        return;
    };
    for ent in rd.flatten() {
        let name = ent.file_name();
        let Some(s) = name.to_str() else { continue };
        if s.starts_with("navi-tiled-ways") && s.contains(&needle) && s.ends_with(".bin") {
            let _ = fs::remove_file(ent.path());
        }
    }
}

fn read_lock_file(path: &Path) -> Option<RegionLockRecord> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_lock_file(path: &Path, rec: &RegionLockRecord) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let Ok(text) = serde_json::to_string_pretty(rec) else {
        return;
    };
    let tmp = path.with_extension("json.partial");
    if fs::write(&tmp, text).is_ok() {
        let _ = fs::rename(&tmp, path);
    }
}

/// Clear owner metadata on a convert checkpoint when the holder releases or dies.
pub fn clear_checkpoint_owner_fields(data_dir: &Path, region_id: &str, pid: u32) {
    let Ok(rd) = fs::read_dir(data_dir) else {
        return;
    };
    for ent in rd.flatten() {
        let name = ent.file_name();
        let Some(s) = name.to_str() else { continue };
        if !s.ends_with(".navi-convert-progress.json") {
            continue;
        }
        let Ok(text) = fs::read_to_string(ent.path()) else {
            continue;
        };
        let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let ck_region = v
            .get("region_id")
            .and_then(|x| x.as_str())
            .map(str::to_string)
            .or_else(|| {
                v.get("pbf_filename")
                    .and_then(|x| x.as_str())
                    .map(|n| region_id_for_pbf(Path::new(n)))
            });
        if ck_region.as_deref() != Some(region_id) {
            continue;
        }
        let owner = v
            .get("owner_pid")
            .and_then(|x| x.as_u64())
            .map(|p| p as u32);
        if owner != Some(pid) {
            continue;
        }
        if let Some(map) = v.as_object_mut() {
            map.insert("owner_pid".into(), serde_json::Value::Null);
            map.insert("owner_kind".into(), serde_json::Value::Null);
            map.insert("owner_phase".into(), serde_json::Value::Null);
            map.insert("owner_started_unix_secs".into(), serde_json::Value::Null);
        }
        if let Ok(out) = serde_json::to_string_pretty(&v) {
            let _ = fs::write(ent.path(), out);
        }
    }
}

pub fn recover_stale(data_dir: &Path, region_id: &str) {
    let path = region_lock_path(data_dir, region_id);
    if let Some(rec) = read_lock_file(&path) {
        if rec.region_id == region_id && !pid_is_alive(rec.pid) {
            log::info!(
                target: "NaviConvert",
                "region lock stale pid={} region={region_id} — reclaiming, cleaning spills",
                rec.pid
            );
            cleanup_spills_for_pid(data_dir, rec.pid);
            clear_checkpoint_owner_fields(data_dir, region_id, rec.pid);
            let _ = fs::remove_file(&path);
        }
    }
    let Ok(rd) = fs::read_dir(data_dir) else {
        return;
    };
    for ent in rd.flatten() {
        let name = ent.file_name();
        let Some(s) = name.to_str() else { continue };
        if !s.ends_with(".navi-convert-progress.json") {
            continue;
        }
        let Ok(text) = fs::read_to_string(ent.path()) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let ck_region = v
            .get("region_id")
            .and_then(|x| x.as_str())
            .map(str::to_string)
            .or_else(|| {
                v.get("pbf_filename")
                    .and_then(|x| x.as_str())
                    .map(|n| region_id_for_pbf(Path::new(n)))
            });
        let Some(ck_region) = ck_region else { continue };
        if ck_region != region_id {
            continue;
        }
        let Some(pid) = v.get("owner_pid").and_then(|x| x.as_u64()) else {
            continue;
        };
        let pid = pid as u32;
        if pid == 0 || pid_is_alive(pid) {
            continue;
        }
        log::info!(
            target: "NaviConvert",
            "checkpoint owner pid={pid} dead region={region_id} — cleaning spills"
        );
        cleanup_spills_for_pid(data_dir, pid);
        clear_checkpoint_owner_fields(data_dir, region_id, pid);
    }
}

/// True when a living **convert** owns this region (file or in-process).
pub fn convert_lock_held(data_dir: &Path, pbf: &Path) -> bool {
    let id = region_id_for_pbf(pbf);
    recover_stale(data_dir, &id);
    let key = registry_key(data_dir, &id);
    {
        let g = registry();
        if let Some(e) = g.map.get(&key) {
            if e.kind == RegionLockKind::Convert && pid_is_alive(e.pid) {
                return true;
            }
        }
    }
    let path = region_lock_path(data_dir, &id);
    match read_lock_file(&path) {
        Some(rec) => {
            rec.kind == RegionLockKind::Convert && rec.region_id == id && pid_is_alive(rec.pid)
        }
        None => false,
    }
}

/// Convert thread is inside `convert_region_packs` holding the region lock.
pub fn holding_convert_lock_on_thread() -> bool {
    HOLDING_CONVERT_LOCK.with(|c| c.load(Ordering::SeqCst))
}

pub enum ConvertAcquire {
    Held(RegionLockGuard),
    AlreadyConverting,
}

pub struct RegionLockGuard {
    data_dir: PathBuf,
    region_id: String,
    kind: RegionLockKind,
    phase: RegionLockPhase,
    pid: u32,
    started_unix_secs: u64,
}

impl RegionLockGuard {
    pub fn region_id(&self) -> &str {
        &self.region_id
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn started_unix_secs(&self) -> u64 {
        self.started_unix_secs
    }

    pub fn kind(&self) -> RegionLockKind {
        self.kind
    }

    pub fn phase(&self) -> RegionLockPhase {
        self.phase
    }

    pub fn set_phase(&mut self, phase: RegionLockPhase) {
        self.phase = phase;
        self.persist();
    }

    fn persist(&self) {
        let rec = RegionLockRecord {
            region_id: self.region_id.clone(),
            pid: self.pid,
            kind: self.kind,
            phase: self.phase,
            started_unix_secs: self.started_unix_secs,
        };
        write_lock_file(&region_lock_path(&self.data_dir, &self.region_id), &rec);
    }
}

impl Drop for RegionLockGuard {
    fn drop(&mut self) {
        if self.kind == RegionLockKind::Convert {
            HOLDING_CONVERT_LOCK.with(|c| c.store(false, Ordering::SeqCst));
            cleanup_spills_for_pid(&self.data_dir, self.pid);
        }
        clear_checkpoint_owner_fields(&self.data_dir, &self.region_id, self.pid);
        let _ = fs::remove_file(region_lock_path(&self.data_dir, &self.region_id));
        let key = registry_key(&self.data_dir, &self.region_id);
        let mut g = registry();
        if g.map
            .get(&key)
            .is_some_and(|e| e.pid == self.pid && e.kind == self.kind)
        {
            g.map.remove(&key);
        }
    }
}

fn install(
    data_dir: &Path,
    region_id: String,
    kind: RegionLockKind,
    phase: RegionLockPhase,
) -> RegionLockGuard {
    let pid = std::process::id();
    let started = now_unix();
    let key = registry_key(data_dir, &region_id);
    {
        let mut g = registry();
        g.map.insert(key, LiveEntry { kind, pid });
    }
    if kind == RegionLockKind::Convert {
        HOLDING_CONVERT_LOCK.with(|c| c.store(true, Ordering::SeqCst));
    }
    let guard = RegionLockGuard {
        data_dir: data_dir.to_path_buf(),
        region_id,
        kind,
        phase,
        pid,
        started_unix_secs: started,
    };
    guard.persist();
    guard
}

/// Background convert: refuse if another living convert owns the region; wait if a plan holds.
pub fn try_acquire_convert(
    data_dir: &Path,
    pbf: &Path,
    phase: RegionLockPhase,
) -> anyhow::Result<ConvertAcquire> {
    let id = region_id_for_pbf(pbf);
    recover_stale(data_dir, &id);
    let key = registry_key(data_dir, &id);
    loop {
        crate::download::plan_cancel::abort_if_cancelled()?;
        recover_stale(data_dir, &id);
        {
            let g = registry();
            if let Some(e) = g.map.get(&key) {
                if e.kind == RegionLockKind::Convert && pid_is_alive(e.pid) {
                    return Ok(ConvertAcquire::AlreadyConverting);
                }
                if e.kind == RegionLockKind::PlanFallback && pid_is_alive(e.pid) {
                    drop(g);
                    thread::sleep(Duration::from_millis(50));
                    continue;
                }
            }
        }
        if let Some(rec) = read_lock_file(&region_lock_path(data_dir, &id)) {
            if rec.region_id == id && pid_is_alive(rec.pid) {
                if rec.kind == RegionLockKind::Convert {
                    return Ok(ConvertAcquire::AlreadyConverting);
                }
                thread::sleep(Duration::from_millis(50));
                continue;
            }
        }
        return Ok(ConvertAcquire::Held(install(
            data_dir,
            id,
            RegionLockKind::Convert,
            phase,
        )));
    }
}

/// Pack-miss plan: wait until convert (or another plan) releases, then take.
pub fn acquire_plan_fallback(data_dir: &Path, pbf: &Path) -> anyhow::Result<RegionLockGuard> {
    let id = region_id_for_pbf(pbf);
    recover_stale(data_dir, &id);
    let key = registry_key(data_dir, &id);
    loop {
        crate::download::plan_cancel::abort_if_cancelled()?;
        recover_stale(data_dir, &id);
        let blocked = {
            let g = registry();
            matches!(g.map.get(&key), Some(e) if pid_is_alive(e.pid))
        };
        if blocked {
            thread::sleep(Duration::from_millis(50));
            continue;
        }
        if let Some(rec) = read_lock_file(&region_lock_path(data_dir, &id)) {
            if rec.region_id == id && pid_is_alive(rec.pid) {
                thread::sleep(Duration::from_millis(50));
                continue;
            }
        }
        return Ok(install(
            data_dir,
            id,
            RegionLockKind::PlanFallback,
            RegionLockPhase::PlanFallback,
        ));
    }
}

impl ConvertAcquire {
    pub fn into_held(self) -> Option<RegionLockGuard> {
        match self {
            Self::Held(g) => Some(g),
            Self::AlreadyConverting => None,
        }
    }
}

pub fn is_convert_in_progress_err(err: &anyhow::Error) -> bool {
    err.to_string().contains(REGION_CONVERT_IN_PROGRESS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    fn touch_pbf(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        File::create(&p).unwrap();
        p
    }

    #[test]
    fn fixture_and_production_paths_share_lock_id() {
        let prod = Path::new("/data/user/0/no.navi.app/files/ostlandet-latest.osm.pbf");
        let fixture = Path::new("/data/local/tmp/navi_fixtures/ostlandet-latest.osm.pbf");
        assert_eq!(region_id_for_pbf(prod), region_id_for_pbf(fixture));
        assert_eq!(region_id_for_pbf(prod), "europe/norway/ostlandet");
    }

    #[test]
    fn oppland_aliases_ostlandet() {
        assert_eq!(
            region_id_for_pbf(Path::new("oppland-latest.osm.pbf")),
            region_id_for_pbf(Path::new("ostlandet-latest.osm.pbf"))
        );
    }

    #[test]
    fn second_convert_refuses() {
        let dir = tempfile::tempdir().unwrap();
        let pbf = touch_pbf(dir.path(), "locktest-alpha-latest.osm.pbf");
        let _a = match try_acquire_convert(dir.path(), &pbf, RegionLockPhase::Graphs).unwrap() {
            ConvertAcquire::Held(g) => g,
            ConvertAcquire::AlreadyConverting => panic!("first convert must acquire"),
        };
        match try_acquire_convert(dir.path(), &pbf, RegionLockPhase::Graphs).unwrap() {
            ConvertAcquire::AlreadyConverting => {}
            ConvertAcquire::Held(_) => panic!("second convert must refuse"),
        }
    }

    #[test]
    fn plan_waits_until_convert_releases() {
        let dir = tempfile::tempdir().unwrap();
        let pbf = touch_pbf(dir.path(), "locktest-wait-latest.osm.pbf");
        let convert =
            match try_acquire_convert(dir.path(), &pbf, RegionLockPhase::PoiBarrier).unwrap() {
                ConvertAcquire::Held(g) => g,
                ConvertAcquire::AlreadyConverting => panic!("convert"),
            };
        let started = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let started_t = started.clone();
        let dir_s = dir.path().to_path_buf();
        let pbf_s = pbf.clone();
        let h = thread::spawn(move || {
            let g = acquire_plan_fallback(&dir_s, &pbf_s).expect("plan lock");
            started_t.store(true, Ordering::SeqCst);
            g
        });
        thread::sleep(Duration::from_millis(120));
        assert!(
            !started.load(Ordering::SeqCst),
            "plan must wait while convert holds"
        );
        drop(convert);
        let plan = h.join().expect("join");
        assert!(started.load(Ordering::SeqCst));
        drop(plan);
    }

    #[test]
    fn dead_pid_reclaims_and_cleans_spills() {
        let dir = tempfile::tempdir().unwrap();
        let pbf = touch_pbf(dir.path(), "locktest-stale-latest.osm.pbf");
        let id = region_id_for_pbf(&pbf);
        let dead_pid = 2_147_483_646u32;
        assert!(!pid_is_alive(dead_pid));
        let spill = dir
            .path()
            .join(format!("navi-tiled-ways-t0-{dead_pid}-111.bin"));
        fs::write(&spill, b"x").unwrap();
        let rec = RegionLockRecord {
            region_id: id.clone(),
            pid: dead_pid,
            kind: RegionLockKind::Convert,
            phase: RegionLockPhase::PoiBarrier,
            started_unix_secs: 1,
        };
        write_lock_file(&region_lock_path(dir.path(), &id), &rec);
        match try_acquire_convert(dir.path(), &pbf, RegionLockPhase::Graphs).unwrap() {
            ConvertAcquire::Held(g) => drop(g),
            ConvertAcquire::AlreadyConverting => panic!("stale lock must be stolen"),
        }
        assert!(!spill.exists(), "stale owner spills must be deleted");
    }

    /// File lock stale recovery must work when the holder was another binary
    /// (e.g. host `navi_indexed_convert`), not only in-process callers.
    #[test]
    fn stale_lock_from_exited_subprocess_is_reclaimed() {
        let dir = tempfile::tempdir().unwrap();
        let pbf = touch_pbf(dir.path(), "locktest-xproc-latest.osm.pbf");
        let id = region_id_for_pbf(&pbf);
        let lock_path = region_lock_path(dir.path(), &id);
        let lock_s = lock_path.display().to_string();
        let script = format!(
            r#"
import json, os, sys
rec = {{
    "region_id": {id:?},
    "pid": os.getpid(),
    "kind": "convert",
    "phase": "graphs",
    "started_unix_secs": 1,
}}
open({lock_s:?}, "w").write(json.dumps(rec))
"#
        );
        let status = std::process::Command::new("python3")
            .arg("-c")
            .arg(&script)
            .status()
            .expect("spawn python lock writer");
        assert!(status.success(), "subprocess must write lock file");
        let written = read_lock_file(&lock_path).expect("lock file");
        assert!(
            !pid_is_alive(written.pid),
            "subprocess must have exited before reclaim"
        );
        match try_acquire_convert(dir.path(), &pbf, RegionLockPhase::Graphs).unwrap() {
            ConvertAcquire::Held(g) => drop(g),
            ConvertAcquire::AlreadyConverting => {
                panic!("exited subprocess lock must be reclaimed via kill(0)")
            }
        }
        assert!(
            !lock_path.exists(),
            "reclaimed lock file must be removed on acquire"
        );
    }
}
