//! UniFFI surface for Phase 7: migrating `.rkyv` downloads to CoMaps leaf sets, the pinned
//! migration state, and the "Map storage" data. Structured results are JSON strings (the Kotlin
//! side parses them with `org.json`); every function that touches the network blocks and must
//! run off the main thread.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use driver_break_core::mwm::map_storage::{
    delete_block_reason, delete_leaf, delete_old_releases, storage_summary,
};
use driver_break_core::mwm::migrate::{
    build_plan, discard_migration, fetch_plan_sources, forget_kept_region, kept_old_regions,
    later_until_ms, now_ms, read_state, record_old_maps_decision, remind_later, run_migration,
    set_status, should_offer, start_migration, update_offer, verify_migration, DetectedRkyvRegion,
    MigrationOutcome, MigrationStatus, OldMapsDecision, PlanInput, PlanSources,
};
use driver_break_core::mwm::{
    fetch_current_catalog_text, installed_leaf_ids, parse_catalog_json, read_active_version,
    read_world_release, DownloadProgress, MwmCatalog,
};

static CANCEL: AtomicBool = AtomicBool::new(false);
static RUNNING: AtomicBool = AtomicBool::new(false);

/// Plan sources are 5 MB of JSON; one fetch serves every toggle in the "Choose regions" list.
struct CachedSources {
    origin: String,
    at: Instant,
    sources: PlanSources,
}

static SOURCES: Mutex<Option<CachedSources>> = Mutex::new(None);
const SOURCES_TTL: Duration = Duration::from_secs(600);

fn sources_for(origin: &str, force: bool) -> Result<PlanSourcesView, String> {
    let mut g = SOURCES.lock().unwrap_or_else(|p| p.into_inner());
    let fresh = g
        .as_ref()
        .is_some_and(|c| c.origin == origin && c.at.elapsed() < SOURCES_TTL);
    if force || !fresh {
        let s = fetch_plan_sources(origin).map_err(|e| e.to_string())?;
        *g = Some(CachedSources {
            origin: origin.to_string(),
            at: Instant::now(),
            sources: s,
        });
    }
    let c = g.as_ref().expect("just set");
    Ok(PlanSourcesView {
        catalog: c.sources.catalog.clone(),
        catalog_text: c.sources.catalog_text.clone(),
        migration: c.sources.migration.clone(),
    })
}

struct PlanSourcesView {
    catalog: MwmCatalog,
    catalog_text: String,
    migration: driver_break_core::mwm::RegionMigration,
}

fn err_json(msg: impl std::fmt::Display) -> String {
    serde_json::json!({"ok": false, "error": msg.to_string()}).to_string()
}

fn parse_detected(json: &str) -> Result<Vec<DetectedRkyvRegion>, String> {
    serde_json::from_str(json).map_err(|e| format!("detected regions: {e}"))
}

fn parse_list(json: &str) -> Vec<String> {
    if json.trim().is_empty() || json.trim() == "null" {
        return Vec::new();
    }
    serde_json::from_str(json).unwrap_or_default()
}

fn plan_with(
    data_dir: &str,
    origin: &str,
    detected_json: &str,
    selected_json: &str,
    excluded_json: &str,
    free_bytes: u64,
    force_refresh: bool,
) -> Result<(driver_break_core::mwm::migrate::MigrationPlan, String), String> {
    let det = parse_detected(detected_json)?;
    let selected: Option<Vec<String>> = if selected_json.trim().is_empty() {
        None
    } else {
        Some(parse_list(selected_json))
    };
    let excluded = parse_list(excluded_json);
    let src = sources_for(origin, force_refresh)?;
    let dir = Path::new(data_dir);
    let active = read_active_version(dir).ok().flatten().map(|a| a.version);
    let installed: BTreeSet<String> = active
        .as_deref()
        .map(|v| installed_leaf_ids(dir, v).into_iter().collect())
        .unwrap_or_default();
    // Installed leaves only count when they belong to the release the plan targets.
    let installed = if active.as_deref() == Some(src.catalog.version.as_str()) {
        installed
    } else {
        BTreeSet::new()
    };
    let world_installed = active.as_deref() == Some(src.catalog.version.as_str())
        && read_world_release(dir)
            .ok()
            .flatten()
            .is_some_and(|w| w.version == src.catalog.version);
    let plan = build_plan(&PlanInput {
        catalog: &src.catalog,
        migration: &src.migration,
        detected: &det,
        selected_region_ids: selected.as_deref(),
        excluded_leaves: &excluded,
        installed_leaves: &installed,
        world_installed,
        free_bytes,
    });
    Ok((plan, src.catalog_text))
}

/// Plan the migration of the detected `.rkyv` regions (network: catalog + region mapping, cached
/// for ten minutes). `selected_json` is a JSON list of region ids (empty string = all mapped
/// regions); `excluded_json` a JSON list of leaf ids the user took off the list.
#[uniffi::export]
pub fn mwm_migration_plan(
    data_dir: String,
    navi_origin: String,
    detected_json: String,
    selected_json: String,
    excluded_json: String,
    free_bytes: u64,
    force_refresh: bool,
) -> String {
    match plan_with(
        &data_dir,
        &navi_origin,
        &detected_json,
        &selected_json,
        &excluded_json,
        free_bytes,
        force_refresh,
    ) {
        Ok((plan, _)) => serde_json::json!({"ok": true, "plan": plan}).to_string(),
        Err(e) => err_json(e),
    }
}

/// Persist the user's choice (state `queued`) and pin the catalog it was planned with.
#[uniffi::export]
pub fn mwm_migration_start(
    data_dir: String,
    navi_origin: String,
    detected_json: String,
    selected_json: String,
    excluded_json: String,
    free_bytes: u64,
    wifi_only: bool,
) -> String {
    let (plan, text) = match plan_with(
        &data_dir,
        &navi_origin,
        &detected_json,
        &selected_json,
        &excluded_json,
        free_bytes,
        false,
    ) {
        Ok(v) => v,
        Err(e) => return err_json(e),
    };
    if !plan.enough_space {
        return err_json(format!(
            "not enough free space: {} bytes short",
            plan.space_short_by
        ));
    }
    match start_migration(Path::new(&data_dir), &plan, &text, wifi_only) {
        Ok(st) => serde_json::json!({"ok": true, "state": st}).to_string(),
        Err(e) => err_json(e),
    }
}

/// Run (or resume) the pinned migration; blocks until verified, paused, or stopped by an error.
#[uniffi::export]
pub fn mwm_migration_run(data_dir: String, navi_origin: String) -> String {
    if RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return err_json("a migration is already running");
    }
    // Releases the flag on every exit, including a panic inside the download.
    struct RunningGuard;
    impl Drop for RunningGuard {
        fn drop(&mut self) {
            RUNNING.store(false, Ordering::SeqCst);
        }
    }
    let _running = RunningGuard;
    CANCEL.store(false, Ordering::SeqCst);
    let mut progress = DownloadProgress::default();
    let out = run_migration(
        Path::new(&data_dir),
        &navi_origin,
        &mut progress,
        Some(&CANCEL),
    );
    let (kind, msg) = match out {
        MigrationOutcome::Verified { .. } => ("verified", String::new()),
        MigrationOutcome::Paused => ("paused", String::new()),
        MigrationOutcome::WaitingNetwork(m) => ("waiting_network", m),
        MigrationOutcome::NoSpace(m) => ("no_space", m),
        MigrationOutcome::Failed(m) => ("failed", m),
    };
    serde_json::json!({"ok": kind == "verified", "kind": kind, "message": msg}).to_string()
}

/// Ask the running migration to stop (state becomes `paused`; the staging tree is kept).
#[uniffi::export]
pub fn mwm_migration_pause() {
    CANCEL.store(true, Ordering::SeqCst);
}

#[uniffi::export]
pub fn mwm_migration_running() -> bool {
    RUNNING.load(Ordering::SeqCst)
}

/// The persisted state as JSON, or `"null"`.
#[uniffi::export]
pub fn mwm_migration_state(data_dir: String) -> String {
    match read_state(Path::new(&data_dir)) {
        Some(st) => serde_json::to_string(&st).unwrap_or_else(|_| "null".into()),
        None => "null".into(),
    }
}

/// Overall progress: bytes of the pinned download that are on disk (staging, or everything once
/// switched) against the planned download size.
#[uniffi::export]
pub fn mwm_migration_progress(data_dir: String) -> String {
    let dir = Path::new(&data_dir);
    let Some(st) = read_state(dir) else {
        return serde_json::json!({"done": 0, "total": 0, "running": false}).to_string();
    };
    let done = match st.status {
        MigrationStatus::Verified | MigrationStatus::Done => st.download_bytes,
        _ => driver_break_core::mwm::map_storage::dir_size(&driver_break_core::mwm::staging_dir(
            dir,
            &st.version,
        ))
        .min(st.download_bytes),
    };
    serde_json::json!({
        "done": done,
        "total": st.download_bytes,
        "running": RUNNING.load(Ordering::SeqCst),
    })
    .to_string()
}

/// Deep verification (sizes + SHA-1 + sidecar SHA-256 + World) of the migrated set.
#[uniffi::export]
pub fn mwm_migration_verify(data_dir: String) -> String {
    match verify_migration(Path::new(&data_dir)) {
        Ok(()) => serde_json::json!({"ok": true}).to_string(),
        Err(e) => err_json(e),
    }
}

/// Set a status the Kotlin side owns (`paused`, `waiting_wifi`, `queued`).
#[uniffi::export]
pub fn mwm_migration_set_status(data_dir: String, status: String, error: String) -> bool {
    let s = match status.as_str() {
        "queued" => MigrationStatus::Queued,
        "paused" => MigrationStatus::Paused,
        "waiting_wifi" => MigrationStatus::WaitingWifi,
        "waiting_network" => MigrationStatus::WaitingNetwork,
        _ => return false,
    };
    let e = if error.is_empty() { None } else { Some(error) };
    set_status(Path::new(&data_dir), s, e).is_some()
}

/// Change the Wi-Fi-only choice of the pinned migration.
#[uniffi::export]
pub fn mwm_migration_set_wifi_only(data_dir: String, wifi_only: bool) -> bool {
    let dir = Path::new(&data_dir);
    match read_state(dir) {
        Some(mut st) => {
            st.wifi_only = wifi_only;
            driver_break_core::mwm::migrate::write_state(dir, &st).is_ok()
        }
        None => false,
    }
}

/// `deleted` / `kept`: finishes the migration (only valid once the new maps are verified).
#[uniffi::export]
pub fn mwm_migration_old_maps_decision(
    data_dir: String,
    decision: String,
    bytes_freed: u64,
) -> bool {
    let d = match decision.as_str() {
        "deleted" => OldMapsDecision::Deleted,
        "kept" => OldMapsDecision::Kept,
        _ => return false,
    };
    record_old_maps_decision(Path::new(&data_dir), d, bytes_freed).is_some()
}

/// Old regions the user migrated and kept (JSON list of {region_id, old_bytes}).
#[uniffi::export]
pub fn mwm_migration_kept_regions(data_dir: String) -> String {
    serde_json::to_string(&kept_old_regions(Path::new(&data_dir))).unwrap_or_else(|_| "[]".into())
}

/// The old maps of a kept region were deleted: stop listing it.
#[uniffi::export]
pub fn mwm_migration_forget_kept(data_dir: String, region_id: String) -> bool {
    forget_kept_region(Path::new(&data_dir), &region_id).is_ok()
}

/// Cancel the migration for good (partial downloads are removed, installed maps stay).
#[uniffi::export]
pub fn mwm_migration_discard(data_dir: String) -> bool {
    discard_migration(Path::new(&data_dir)).is_ok()
}

/// "Later": returns the epoch millis before which the offer is not shown again.
#[uniffi::export]
pub fn mwm_migration_remind_later(data_dir: String) -> u64 {
    remind_later(Path::new(&data_dir)).unwrap_or(0)
}

#[uniffi::export]
pub fn mwm_migration_should_offer(data_dir: String) -> bool {
    should_offer(Path::new(&data_dir), now_ms())
}

#[uniffi::export]
pub fn mwm_migration_later_until(data_dir: String) -> u64 {
    later_until_ms(Path::new(&data_dir)).unwrap_or(0)
}

fn catalog_cache_path(data_dir: &str) -> std::path::PathBuf {
    Path::new(data_dir).join("mwm").join("catalog-cache.json")
}

/// Installed leaves per country group, sizes, release version, and (when `navi_origin` is
/// reachable) whether a newer CoMaps release is available. Falls back to the cached catalog
/// offline. `refresh` forces a network fetch.
#[uniffi::export]
pub fn mwm_storage_summary(data_dir: String, navi_origin: String, refresh: bool) -> String {
    let dir = Path::new(&data_dir);
    let cache = catalog_cache_path(&data_dir);
    let mut catalog: Option<MwmCatalog> = None;
    let mut online = false;
    if refresh && !navi_origin.is_empty() {
        if let Ok((c, text)) = fetch_current_catalog_text(&navi_origin) {
            let _ = std::fs::create_dir_all(cache.parent().unwrap());
            let _ = std::fs::write(&cache, text);
            catalog = Some(c);
            online = true;
        }
    }
    if catalog.is_none() {
        catalog = std::fs::read_to_string(&cache)
            .ok()
            .and_then(|t| parse_catalog_json(&t).ok());
    }
    let summary = storage_summary(dir, catalog.as_ref());
    let update = catalog
        .as_ref()
        .map(|c| update_offer(dir, c, summary.version.as_deref()));
    serde_json::json!({
        "ok": true,
        "summary": summary,
        "update": update,
        "online": online,
    })
    .to_string()
}

#[uniffi::export]
pub fn mwm_storage_delete_block_reason(data_dir: String, leaf_id: String) -> String {
    delete_block_reason(Path::new(&data_dir), &leaf_id).unwrap_or_default()
}

#[uniffi::export]
pub fn mwm_storage_delete_leaf(data_dir: String, leaf_id: String) -> String {
    match delete_leaf(Path::new(&data_dir), &leaf_id) {
        Ok(freed) => serde_json::json!({"ok": true, "freed": freed}).to_string(),
        Err(e) => err_json(e),
    }
}

#[uniffi::export]
pub fn mwm_storage_delete_old_releases(data_dir: String) -> String {
    match delete_old_releases(Path::new(&data_dir)) {
        Ok(freed) => serde_json::json!({"ok": true, "freed": freed}).to_string(),
        Err(e) => err_json(e),
    }
}
