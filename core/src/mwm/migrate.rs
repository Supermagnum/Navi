//! Phase 7: migrate a user's existing `.rkyv` region downloads to CoMaps leaf sets.
//!
//! The module has three parts, none of which touches the `.rkyv` files themselves:
//!
//! 1. [`build_plan`] -- pure planning. Detected `.rkyv` regions are mapped to leaves with
//!    `region-migration.json`; leaves shared between regions are counted once; the plan carries
//!    the download size, the free space needed and available, and the regions that have no
//!    mapping (those keep their old maps and are never offered for deletion).
//! 2. [`MigrationState`] -- a persisted state machine under `mwm/migration-cache/state.json`
//!    plus the catalog the plan was made with (pinned). A migration survives app restarts,
//!    network loss and pauses because [`run_migration`] always resumes from this state and the
//!    staging tree (`.partial` files resume with a Range request).
//! 3. [`run_migration`] -- download into staging (nothing is visible to the router), verify every
//!    leaf, switch atomically (`activate_release` / `merge_staging_into_release`), then verify the
//!    installed set again from disk ([`verify_installed`]). Only after that does the state become
//!    [`MigrationStatus::Verified`], which is the precondition for asking about the old `.rkyv`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::catalog::{CatalogLeaf, MwmCatalog};
use super::checksum::{verify_path_sha1_base64, verify_path_sha256};
use super::download::{
    fetch_current_catalog_text, join_url, DownloadProgress, MwmDownloadError, CANCELLED,
    WORLD_OVERLAY_LEAF_IDS,
};
use super::install::{install_leaf_set_with_catalog, installed_leaf_ids};
use super::migration::{parse_region_migration_json, RegionMigration};
use super::region_ui::{flatten_country_group_leaves, region_ui_from_catalog};
use super::release::{assert_world_matches_active, read_active_version};
use super::storage::{leaf_mwm_path, leaf_sidecar_path, release_dir, MwmPaths};
use crate::pack_server::http_get_text;

/// Downloads above this size need Wi-Fi unless the user overrides it.
pub const LARGE_MIGRATION_BYTES: u64 = 250 * 1024 * 1024;

/// Slack kept free on top of the download (filesystem overhead, World promotion copy, partials).
pub const FREE_SPACE_MARGIN_BYTES: u64 = 256 * 1024 * 1024;

/// A reminder set by "Later" lasts this long.
pub const LATER_REMINDER_MS: u64 = 3 * 24 * 3600 * 1000;

// ---------------------------------------------------------------------------------------------
// Planning
// ---------------------------------------------------------------------------------------------

/// One `.rkyv` region found on the device (id as the old path knows it, e.g.
/// `europe/norway/ostlandet`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectedRkyvRegion {
    pub region_id: String,
    /// Bytes the old pack files occupy on disk.
    pub old_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MigrationRegionRow {
    pub region_id: String,
    pub bake_id: Option<String>,
    pub display_name: String,
    pub old_bytes: u64,
    /// False when `region-migration.json` has no (usable) leaf set for this region.
    pub mapped: bool,
    pub selected: bool,
    /// Leaves of this region that are part of the plan (after exclusions).
    pub leaf_ids: Vec<String>,
    /// Size of those leaves, MWM + sidecar, shared leaves included in full.
    pub region_bytes: u64,
    pub uncovered_pct: Option<f64>,
    /// Human note: why a region is unmapped / not selected.
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MigrationLeafRow {
    pub id: String,
    pub display_name: String,
    pub bytes: u64,
    pub installed: bool,
    /// Selected regions that need this leaf (more than one means it is stored once).
    pub shared_by: Vec<String>,
    /// `region-migration.json` flag: most of the leaf lies outside the region (border roads).
    pub mostly_outside: bool,
    pub excluded: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MigrationPlan {
    pub version: String,
    pub rows: Vec<MigrationRegionRow>,
    /// Union of the leaves of all selected, mapped regions (deduplicated), excluded ones removed.
    pub leaves: Vec<MigrationLeafRow>,
    /// Leaves named by the mapping but absent from the catalog (reported, never downloaded).
    pub unavailable_leaves: Vec<String>,
    pub world_needed: bool,
    pub world_bytes: u64,
    /// Bytes still to download: leaves not yet installed plus World when needed.
    pub download_bytes: u64,
    /// Size of the whole new map set (installed + new), World included.
    pub total_bytes: u64,
    /// What the per-region sizes add up to without sharing; `total - leaf_bytes_union` is saved.
    pub per_region_sum_bytes: u64,
    pub shared_saved_bytes: u64,
    pub free_bytes_needed: u64,
    pub free_bytes_available: u64,
    pub enough_space: bool,
    pub space_short_by: u64,
    pub old_bytes_selected: u64,
    /// Old maps of regions without a mapping: kept, never offered for deletion.
    pub old_bytes_unmapped: u64,
    pub large: bool,
    /// Default for "Wi-Fi only": on for large migrations.
    pub wifi_only_default: bool,
}

pub struct PlanInput<'a> {
    pub catalog: &'a MwmCatalog,
    pub migration: &'a RegionMigration,
    pub detected: &'a [DetectedRkyvRegion],
    /// Region ids to migrate; `None` selects every mapped region.
    pub selected_region_ids: Option<&'a [String]>,
    /// Leaves the user removed from the list.
    pub excluded_leaves: &'a [String],
    pub installed_leaves: &'a BTreeSet<String>,
    pub world_installed: bool,
    pub free_bytes: u64,
}

/// Bytes of one leaf (MWM + sidecar).
pub fn leaf_total_bytes(leaf: &CatalogLeaf) -> u64 {
    leaf.comaps_bytes.or(leaf.mwm_bytes).unwrap_or(0) + leaf.sidecar_bytes.unwrap_or(0)
}

fn normalize_region_id(id: &str) -> String {
    id.trim().trim_matches('/').to_ascii_lowercase()
}

fn pretty(id: &str) -> String {
    id.replace('_', " ")
}

/// Readable name for a region without a mapping entry: the path below the continent, words capitalised
/// (`europe/atlantis/east` -> `Atlantis East`).
fn unmapped_name(region_id: &str) -> String {
    let parts: Vec<&str> = region_id.trim_matches('/').split('/').collect();
    let tail = if parts.len() > 1 {
        &parts[1..]
    } else {
        &parts[..]
    };
    tail.iter()
        .flat_map(|p| p.split(['_', '-']))
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Map a detected `.rkyv` region to its `region-migration.json` entry.
fn lookup<'a>(
    migration: &'a RegionMigration,
    region_id: &str,
) -> Option<(&'a str, &'a super::migration::RegionMigrationEntry)> {
    let norm = normalize_region_id(region_id);
    if let Some((bake, e)) = migration.regions.iter().find_map(|(bake, e)| {
        (normalize_region_id(&e.region_id) == norm).then_some((bake.as_str(), e))
    }) {
        return Some((bake, e));
    }
    let derived: String = norm
        .chars()
        .map(|c| if c == '/' || c == '-' { '_' } else { c })
        .collect();
    migration
        .regions
        .get_key_value(&derived)
        .map(|(b, e)| (b.as_str(), e))
}

pub fn build_plan(input: &PlanInput<'_>) -> MigrationPlan {
    let PlanInput {
        catalog,
        migration,
        detected,
        selected_region_ids,
        excluded_leaves,
        installed_leaves,
        world_installed,
        free_bytes,
    } = input;
    let excluded: BTreeSet<&str> = excluded_leaves.iter().map(|s| s.as_str()).collect();
    let wanted_regions: Option<BTreeSet<String>> = selected_region_ids.map(|v| {
        v.iter()
            .map(|s| normalize_region_id(s))
            .collect::<BTreeSet<_>>()
    });

    let ui = flatten_country_group_leaves(&region_ui_from_catalog(catalog));
    let names: BTreeMap<&str, &str> = ui
        .iter()
        .map(|l| (l.id.as_str(), l.display_name.as_str()))
        .collect();
    let leaf_name = |id: &str| -> String {
        names
            .get(id)
            .map(|s| (*s).to_string())
            .unwrap_or_else(|| pretty(id))
    };

    let mut rows = Vec::new();
    let mut unavailable: BTreeSet<String> = BTreeSet::new();
    // leaf id -> selected region ids needing it
    let mut leaf_regions: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut mostly_outside: BTreeMap<String, bool> = BTreeMap::new();
    let mut old_selected = 0u64;
    let mut old_unmapped = 0u64;
    let mut per_region_sum = 0u64;

    let mut sorted: Vec<&DetectedRkyvRegion> = detected.iter().collect();
    sorted.sort_by(|a, b| a.region_id.cmp(&b.region_id));
    for d in sorted {
        let entry = lookup(migration, &d.region_id);
        let display = entry
            .and_then(|(_, e)| e.display_name.clone())
            .unwrap_or_else(|| unmapped_name(&d.region_id));
        let mut leaf_ids: Vec<String> = Vec::new();
        if let Some((_, e)) = entry {
            for l in &e.comaps_leaves {
                if catalog.leaf(&l.leaf).is_none() {
                    unavailable.insert(l.leaf.clone());
                    continue;
                }
                mostly_outside.insert(
                    l.leaf.clone(),
                    l.coverage_of_region_pct.unwrap_or(100.0) < 1.0
                        || l.overspill_pct.unwrap_or(0.0) > 90.0,
                );
                if !leaf_ids.contains(&l.leaf) {
                    leaf_ids.push(l.leaf.clone());
                }
            }
        }
        let mapped = !leaf_ids.is_empty();
        let in_selection = wanted_regions
            .as_ref()
            .is_none_or(|w| w.contains(&normalize_region_id(&d.region_id)));
        let selected = mapped && in_selection;
        let kept: Vec<String> = leaf_ids
            .iter()
            .filter(|l| !excluded.contains(l.as_str()))
            .cloned()
            .collect();
        let region_bytes: u64 = kept
            .iter()
            .filter_map(|l| catalog.leaf(l))
            .map(leaf_total_bytes)
            .sum();
        let note = if !mapped {
            Some("no map area mapping for this region; the old map is kept".to_string())
        } else if !in_selection {
            Some("not selected".to_string())
        } else {
            None
        };
        if selected {
            old_selected += d.old_bytes;
            per_region_sum += region_bytes;
            for l in &kept {
                leaf_regions
                    .entry(l.clone())
                    .or_default()
                    .push(d.region_id.clone());
            }
        }
        if !mapped {
            old_unmapped += d.old_bytes;
        }
        rows.push(MigrationRegionRow {
            region_id: d.region_id.clone(),
            bake_id: entry.map(|(b, _)| b.to_string()),
            display_name: display,
            old_bytes: d.old_bytes,
            mapped,
            selected,
            leaf_ids: if mapped { leaf_ids } else { Vec::new() },
            region_bytes,
            uncovered_pct: entry.and_then(|(_, e)| e.uncovered_pct),
            note,
        });
    }

    let mut leaves = Vec::new();
    let mut union_bytes = 0u64;
    let mut download_bytes = 0u64;
    for (id, shared_by) in &leaf_regions {
        let bytes = catalog.leaf(id).map(leaf_total_bytes).unwrap_or(0);
        let installed = installed_leaves.contains(id);
        union_bytes += bytes;
        if !installed {
            download_bytes += bytes;
        }
        leaves.push(MigrationLeafRow {
            id: id.clone(),
            display_name: leaf_name(id),
            bytes,
            installed,
            shared_by: shared_by.clone(),
            mostly_outside: mostly_outside.get(id).copied().unwrap_or(false),
            excluded: false,
        });
    }
    // Excluded leaves stay visible (greyed) so the user can put them back.
    for row in &rows {
        if !row.selected {
            continue;
        }
        for id in &row.leaf_ids {
            if excluded.contains(id.as_str()) && !leaves.iter().any(|l| &l.id == id) {
                leaves.push(MigrationLeafRow {
                    id: id.clone(),
                    display_name: leaf_name(id),
                    bytes: catalog.leaf(id).map(leaf_total_bytes).unwrap_or(0),
                    installed: installed_leaves.contains(id),
                    shared_by: Vec::new(),
                    mostly_outside: mostly_outside.get(id).copied().unwrap_or(false),
                    excluded: true,
                });
            }
        }
    }
    leaves.sort_by(|a, b| a.id.cmp(&b.id));

    let world_bytes: u64 = WORLD_OVERLAY_LEAF_IDS
        .iter()
        .filter_map(|id| catalog.leaf(id))
        .map(leaf_total_bytes)
        .sum();
    let any_selected = rows.iter().any(|r| r.selected);
    let world_needed = any_selected && !*world_installed;
    if world_needed {
        download_bytes += world_bytes;
    }
    let total_bytes = union_bytes + if any_selected { world_bytes } else { 0 };
    // The World overlays are copied to the storage root on activation, so they exist twice for
    // a moment; the margin covers filesystem slack and partial files.
    let free_bytes_needed = if download_bytes == 0 {
        0
    } else {
        download_bytes + if world_needed { world_bytes } else { 0 } + FREE_SPACE_MARGIN_BYTES
    };
    let enough_space = *free_bytes >= free_bytes_needed;
    let large = download_bytes > LARGE_MIGRATION_BYTES;
    MigrationPlan {
        version: catalog.version.clone(),
        rows,
        leaves,
        unavailable_leaves: unavailable.into_iter().collect(),
        world_needed,
        world_bytes,
        download_bytes,
        total_bytes,
        per_region_sum_bytes: per_region_sum,
        shared_saved_bytes: per_region_sum.saturating_sub(union_bytes),
        free_bytes_needed,
        free_bytes_available: *free_bytes,
        enough_space,
        space_short_by: free_bytes_needed.saturating_sub(*free_bytes),
        old_bytes_selected: old_selected,
        old_bytes_unmapped: old_unmapped,
        large,
        wifi_only_default: large,
    }
}

// ---------------------------------------------------------------------------------------------
// Persistent state
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationStatus {
    /// Chosen by the user, not started (or restarted after an app restart).
    Queued,
    Running,
    /// Paused by the user.
    Paused,
    /// Wi-Fi only is on and the device is not on an unmetered network.
    WaitingWifi,
    /// The network dropped; resumes when it is back.
    WaitingNetwork,
    Failed,
    /// New maps downloaded, switched and verified; the old `.rkyv` question is open.
    Verified,
    /// Finished (old maps deleted or kept by the user's decision).
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OldMapsDecision {
    Pending,
    Deleted,
    Kept,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OldRegion {
    pub region_id: String,
    pub old_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MigrationState {
    pub schema: u32,
    /// Catalog version the migration is pinned to.
    pub version: String,
    pub status: MigrationStatus,
    pub selected_region_ids: Vec<String>,
    pub leaves: Vec<String>,
    pub old_regions: Vec<OldRegion>,
    pub wifi_only: bool,
    pub download_bytes: u64,
    pub created_ms: u64,
    pub updated_ms: u64,
    pub switched_ms: Option<u64>,
    pub error: Option<String>,
    pub old_maps_decision: OldMapsDecision,
    pub bytes_freed: u64,
}

pub const STATE_SCHEMA: u32 = 1;

fn cache_dir(data_dir: &Path) -> PathBuf {
    MwmPaths::new(data_dir).migration_cache()
}

fn state_path(data_dir: &Path) -> PathBuf {
    cache_dir(data_dir).join("state.json")
}

fn pinned_catalog_path(data_dir: &Path, version: &str) -> PathBuf {
    cache_dir(data_dir).join(format!("catalog-{version}.json"))
}

fn later_path(data_dir: &Path) -> PathBuf {
    cache_dir(data_dir).join("later.json")
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)
}

pub fn read_state(data_dir: &Path) -> Option<MigrationState> {
    let text = fs::read_to_string(state_path(data_dir)).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn write_state(data_dir: &Path, st: &MigrationState) -> std::io::Result<()> {
    let mut st = st.clone();
    st.updated_ms = now_ms();
    write_atomic(
        &state_path(data_dir),
        serde_json::to_string_pretty(&st)
            .map_err(std::io::Error::other)?
            .as_bytes(),
    )
}

pub fn set_status(
    data_dir: &Path,
    status: MigrationStatus,
    error: Option<String>,
) -> Option<MigrationState> {
    let mut st = read_state(data_dir)?;
    st.status = status;
    st.error = error;
    write_state(data_dir, &st).ok()?;
    Some(st)
}

/// Forget the migration (user cancelled it). Partial downloads in staging are removed with it;
/// installed maps and the old `.rkyv` files are not touched.
pub fn discard_migration(data_dir: &Path) -> std::io::Result<()> {
    if let Some(st) = read_state(data_dir) {
        let staging = super::storage::staging_dir(data_dir, &st.version);
        if staging.exists() && st.status != MigrationStatus::Verified {
            fs::remove_dir_all(staging)?;
        }
        let _ = fs::remove_file(pinned_catalog_path(data_dir, &st.version));
    }
    let _ = fs::remove_file(state_path(data_dir));
    Ok(())
}

/// "Later": do not offer the migration again before `now + LATER_REMINDER_MS`.
pub fn remind_later(data_dir: &Path) -> std::io::Result<u64> {
    let until = now_ms() + LATER_REMINDER_MS;
    write_atomic(
        &later_path(data_dir),
        format!("{{\"until_ms\":{until}}}").as_bytes(),
    )?;
    Ok(until)
}

pub fn later_until_ms(data_dir: &Path) -> Option<u64> {
    let text = fs::read_to_string(later_path(data_dir)).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    v.get("until_ms")?.as_u64()
}

/// True when the start-up offer should be shown (no migration in flight, no reminder pending).
pub fn should_offer(data_dir: &Path, now: u64) -> bool {
    if let Some(st) = read_state(data_dir) {
        if st.status != MigrationStatus::Done {
            return false;
        }
    }
    later_until_ms(data_dir).is_none_or(|t| now >= t)
}

// ---------------------------------------------------------------------------------------------
// Network helpers
// ---------------------------------------------------------------------------------------------

/// Fetch and parse `region-migration.json` for `catalog`.
pub fn fetch_region_migration(
    navi_origin: &str,
    catalog: &MwmCatalog,
) -> Result<RegionMigration, MwmDownloadError> {
    let rel = catalog
        .region_migration_url
        .clone()
        .unwrap_or_else(|| format!("{}/region-migration.json", catalog.version));
    let url = join_url(navi_origin, &format!("mwm/{}", rel.trim_start_matches('/')));
    let text = http_get_text(&url, Duration::from_secs(120))?;
    parse_region_migration_json(&text).map_err(|e| MwmDownloadError::Other(e.to_string()))
}

/// Everything the plan screen needs from the network: catalog + region mapping.
pub struct PlanSources {
    pub catalog: MwmCatalog,
    pub catalog_text: String,
    pub migration: RegionMigration,
}

pub fn fetch_plan_sources(navi_origin: &str) -> Result<PlanSources, MwmDownloadError> {
    let (catalog, catalog_text) = fetch_current_catalog_text(navi_origin)?;
    let migration = fetch_region_migration(navi_origin, &catalog)?;
    Ok(PlanSources {
        catalog,
        catalog_text,
        migration,
    })
}

/// Create the persisted migration (status `Queued`) and pin the catalog it was planned with.
pub fn start_migration(
    data_dir: &Path,
    plan: &MigrationPlan,
    catalog_text: &str,
    wifi_only: bool,
) -> Result<MigrationState, MwmDownloadError> {
    if let Some(st) = read_state(data_dir) {
        if !matches!(st.status, MigrationStatus::Done) {
            return Err(MwmDownloadError::Other(
                "a migration is already in progress".into(),
            ));
        }
    }
    let leaves: Vec<String> = plan
        .leaves
        .iter()
        .filter(|l| !l.excluded)
        .map(|l| l.id.clone())
        .collect();
    if leaves.is_empty() {
        return Err(MwmDownloadError::Other("no leaves selected".into()));
    }
    write_atomic(
        &pinned_catalog_path(data_dir, &plan.version),
        catalog_text.as_bytes(),
    )?;
    let now = now_ms();
    let st = MigrationState {
        schema: STATE_SCHEMA,
        version: plan.version.clone(),
        status: MigrationStatus::Queued,
        selected_region_ids: plan
            .rows
            .iter()
            .filter(|r| r.selected)
            .map(|r| r.region_id.clone())
            .collect(),
        leaves,
        old_regions: plan
            .rows
            .iter()
            .filter(|r| r.selected)
            .map(|r| OldRegion {
                region_id: r.region_id.clone(),
                old_bytes: r.old_bytes,
            })
            .collect(),
        wifi_only,
        download_bytes: plan.download_bytes,
        created_ms: now,
        updated_ms: now,
        switched_ms: None,
        error: None,
        old_maps_decision: OldMapsDecision::Pending,
        bytes_freed: 0,
    };
    write_state(data_dir, &st)?;
    let _ = fs::remove_file(later_path(data_dir));
    Ok(st)
}

// ---------------------------------------------------------------------------------------------
// Running
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    Cancelled,
    /// Connection lost / timed out: retry when the network is back.
    Network,
    NoSpace,
    /// Anything else (checksum, missing leaf, release no longer served).
    Other,
}

pub fn classify_error(e: &MwmDownloadError) -> FailureKind {
    let msg = match e {
        MwmDownloadError::Other(m) => m.clone(),
        MwmDownloadError::Io(io) => {
            if io.raw_os_error() == Some(28) {
                return FailureKind::NoSpace;
            }
            io.to_string()
        }
        other => other.to_string(),
    };
    if msg == CANCELLED {
        return FailureKind::Cancelled;
    }
    let m = msg.to_ascii_lowercase();
    if m.contains("insufficient space") || m.contains("no space left") {
        return FailureKind::NoSpace;
    }
    const NETWORK: &[&str] = &[
        "timeout",
        "connection_failed",
        "request_failed",
        "body_interrupted",
        "network_error",
        "dns",
        "unreachable",
        "connection reset",
        "error sending request",
        "mirror discovery",
        "all mirrors failed",
    ];
    if NETWORK.iter().any(|k| m.contains(k)) {
        FailureKind::Network
    } else {
        FailureKind::Other
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum MigrationOutcome {
    /// New maps are downloaded, switched in and verified.
    Verified {
        version: String,
        leaves_installed: usize,
    },
    Paused,
    WaitingNetwork(String),
    NoSpace(String),
    Failed(String),
}

fn load_pinned_catalog(data_dir: &Path, version: &str) -> Result<MwmCatalog, MwmDownloadError> {
    let text = fs::read_to_string(pinned_catalog_path(data_dir, version))?;
    super::catalog::parse_catalog_json(&text).map_err(|e| MwmDownloadError::Other(e.to_string()))
}

/// Download, verify and switch the pinned leaf set. Blocks; run it off the main thread. Safe to
/// call again after any outcome except `Verified` (resumes the staging tree).
pub fn run_migration(
    data_dir: &Path,
    navi_origin: &str,
    progress: &mut DownloadProgress,
    cancel: Option<&AtomicBool>,
) -> MigrationOutcome {
    let Some(mut st) = read_state(data_dir) else {
        return MigrationOutcome::Failed("no migration is planned".into());
    };
    if matches!(st.status, MigrationStatus::Verified | MigrationStatus::Done) {
        return MigrationOutcome::Verified {
            version: st.version.clone(),
            leaves_installed: st.leaves.len(),
        };
    }
    st.status = MigrationStatus::Running;
    st.error = None;
    let _ = write_state(data_dir, &st);

    let catalog = match load_pinned_catalog(data_dir, &st.version) {
        Ok(c) => c,
        Err(e) => {
            let msg = format!("pinned catalog unreadable: {e}");
            let _ = set_status(data_dir, MigrationStatus::Failed, Some(msg.clone()));
            return MigrationOutcome::Failed(msg);
        }
    };

    let res = install_leaf_set_with_catalog(
        data_dir,
        navi_origin,
        &catalog,
        &st.leaves,
        progress,
        cancel,
    );
    match res {
        Ok(_) => match verify_installed(data_dir, &catalog, &st.leaves) {
            Ok(()) => {
                if let Some(mut s) = read_state(data_dir) {
                    s.status = MigrationStatus::Verified;
                    s.switched_ms = Some(now_ms());
                    s.error = None;
                    let _ = write_state(data_dir, &s);
                }
                MigrationOutcome::Verified {
                    version: st.version,
                    leaves_installed: st.leaves.len(),
                }
            }
            Err(msg) => {
                let _ = set_status(data_dir, MigrationStatus::Failed, Some(msg.clone()));
                MigrationOutcome::Failed(msg)
            }
        },
        Err(e) => match classify_error(&e) {
            FailureKind::Cancelled => {
                let _ = set_status(data_dir, MigrationStatus::Paused, None);
                MigrationOutcome::Paused
            }
            FailureKind::Network => {
                let msg = e.to_string();
                let _ = set_status(data_dir, MigrationStatus::WaitingNetwork, Some(msg.clone()));
                MigrationOutcome::WaitingNetwork(msg)
            }
            FailureKind::NoSpace => {
                let msg = e.to_string();
                let _ = set_status(data_dir, MigrationStatus::Failed, Some(msg.clone()));
                MigrationOutcome::NoSpace(msg)
            }
            FailureKind::Other => {
                let msg = e.to_string();
                let _ = set_status(data_dir, MigrationStatus::Failed, Some(msg.clone()));
                MigrationOutcome::Failed(msg)
            }
        },
    }
}

/// Re-verify an installed leaf set from disk: MWM size + SHA-1, sidecar size + SHA-256, World
/// overlays present and matching the active release. This is the gate for deleting old maps.
pub fn verify_installed(
    data_dir: &Path,
    catalog: &MwmCatalog,
    leaves: &[String],
) -> Result<(), String> {
    let active = read_active_version(data_dir)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "no active release after the switch".to_string())?;
    if active.version != catalog.version {
        return Err(format!(
            "active release is {} but the migration is for {}",
            active.version, catalog.version
        ));
    }
    assert_world_matches_active(data_dir).map_err(|e| e.to_string())?;
    let release = release_dir(data_dir, &catalog.version);
    for id in leaves {
        let leaf = catalog
            .leaf(id)
            .ok_or_else(|| format!("leaf {id} missing from the pinned catalog"))?;
        let mwm = leaf_mwm_path(&release, id);
        let size = leaf.comaps_bytes.or(leaf.mwm_bytes);
        let sha1 = leaf
            .comaps_sha1_base64
            .as_deref()
            .ok_or_else(|| format!("leaf {id} has no SHA-1 in the catalog"))?;
        verify_path_sha1_base64(&mwm, size, sha1).map_err(|e| format!("{id}: {e}"))?;
        if !leaf.no_sidecar {
            let sc = leaf_sidecar_path(&release, id);
            let sha = leaf
                .sidecar_sha256
                .as_deref()
                .ok_or_else(|| format!("leaf {id} has no sidecar SHA-256 in the catalog"))?;
            verify_path_sha256(&sc, leaf.sidecar_bytes, sha).map_err(|e| format!("{id}: {e}"))?;
        }
    }
    Ok(())
}

/// Verify the migration recorded in the state against the disk (used before offering to delete).
pub fn verify_migration(data_dir: &Path) -> Result<(), String> {
    let st = read_state(data_dir).ok_or_else(|| "no migration recorded".to_string())?;
    let catalog = load_pinned_catalog(data_dir, &st.version).map_err(|e| e.to_string())?;
    verify_installed(data_dir, &catalog, &st.leaves)
}

fn kept_path(data_dir: &Path) -> PathBuf {
    cache_dir(data_dir).join("kept.json")
}

/// Old `.rkyv` regions the user migrated and chose to keep. Survives later migrations (the state file
/// is replaced by each new migration), so a kept region is never offered for migration again and the
/// "Map storage" cleanup list stays complete.
pub fn kept_old_regions(data_dir: &Path) -> Vec<OldRegion> {
    fs::read_to_string(kept_path(data_dir))
        .ok()
        .and_then(|t| serde_json::from_str::<Vec<OldRegion>>(&t).ok())
        .unwrap_or_default()
}

fn write_kept(data_dir: &Path, list: &[OldRegion]) -> std::io::Result<()> {
    write_atomic(
        &kept_path(data_dir),
        serde_json::to_string_pretty(list)
            .map_err(std::io::Error::other)?
            .as_bytes(),
    )
}

/// Forget a kept region (its old maps were deleted).
pub fn forget_kept_region(data_dir: &Path, region_id: &str) -> std::io::Result<()> {
    let list: Vec<OldRegion> = kept_old_regions(data_dir)
        .into_iter()
        .filter(|r| r.region_id != region_id)
        .collect();
    write_kept(data_dir, &list)
}

/// Record the user's decision about the old `.rkyv` maps and finish the migration.
pub fn record_old_maps_decision(
    data_dir: &Path,
    decision: OldMapsDecision,
    bytes_freed: u64,
) -> Option<MigrationState> {
    let mut st = read_state(data_dir)?;
    st.old_maps_decision = decision;
    st.bytes_freed = bytes_freed;
    st.status = MigrationStatus::Done;
    write_state(data_dir, &st).ok()?;
    if decision == OldMapsDecision::Kept {
        let mut kept = kept_old_regions(data_dir);
        for r in &st.old_regions {
            if !kept.iter().any(|k| k.region_id == r.region_id) {
                kept.push(r.clone());
            }
        }
        write_kept(data_dir, &kept).ok()?;
    }
    Some(st)
}

// ---------------------------------------------------------------------------------------------
// Update offer (a newer CoMaps release than the installed one)
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateOffer {
    pub installed_version: Option<String>,
    pub current_version: String,
    pub update_available: bool,
    /// Download size of the installed leaves in the newer release (+ World).
    pub update_bytes: u64,
}

pub fn update_offer(
    data_dir: &Path,
    catalog: &MwmCatalog,
    installed_version: Option<&str>,
) -> UpdateOffer {
    let installed: Vec<String> = installed_version
        .map(|v| installed_leaf_ids(data_dir, v))
        .unwrap_or_default();
    let mut bytes: u64 = installed
        .iter()
        .filter_map(|id| catalog.leaf(id))
        .map(leaf_total_bytes)
        .sum();
    bytes += WORLD_OVERLAY_LEAF_IDS
        .iter()
        .filter_map(|id| catalog.leaf(id))
        .map(leaf_total_bytes)
        .sum::<u64>();
    UpdateOffer {
        installed_version: installed_version.map(str::to_string),
        current_version: catalog.version.clone(),
        update_available: installed_version.is_some_and(|v| v != catalog.version),
        update_bytes: bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mwm::catalog::parse_catalog_json;
    use crate::mwm::migration::parse_region_migration_json;
    use tempfile::tempdir;

    /// Catalog with the leaves of `region-migration.example.json`, World overlays and sizes.
    fn catalog() -> MwmCatalog {
        let mut leaves = Vec::new();
        for (id, mb) in [
            ("Norway_Hedmark", 100u64),
            ("Norway_Southern", 300),
            ("Norway_Oppland", 100),
            ("Denmark_North Denmark Region", 50),
            ("Norway_Rogaland", 200),
            ("World", 70),
            ("WorldCoasts", 30),
        ] {
            leaves.push(serde_json::json!({
                "id": id,
                "comaps_bytes": mb * 1_000_000,
                "mwm_bytes": mb * 1_000_000,
                "sidecar_bytes": if id.starts_with("World") { 0 } else { 1_000_000 },
                "comaps_sha1_base64": "x",
                "no_sidecar": id.starts_with("World"),
            }));
        }
        let mut base: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/mwm/catalog-v2.example.json"
        ))
        .unwrap();
        base["leaves"] = serde_json::Value::Array(leaves);
        parse_catalog_json(&base.to_string()).unwrap()
    }

    fn migration() -> RegionMigration {
        parse_region_migration_json(include_str!(
            "../../tests/fixtures/mwm/region-migration.example.json"
        ))
        .unwrap()
    }

    fn detected(ids: &[(&str, u64)]) -> Vec<DetectedRkyvRegion> {
        ids.iter()
            .map(|(id, b)| DetectedRkyvRegion {
                region_id: (*id).into(),
                old_bytes: *b,
            })
            .collect()
    }

    fn plan_for(
        det: &[DetectedRkyvRegion],
        sel: Option<&[String]>,
        excl: &[String],
        installed: &BTreeSet<String>,
        world: bool,
        free: u64,
    ) -> MigrationPlan {
        build_plan(&PlanInput {
            catalog: &catalog(),
            migration: &migration(),
            detected: det,
            selected_region_ids: sel,
            excluded_leaves: excl,
            installed_leaves: installed,
            world_installed: world,
            free_bytes: free,
        })
    }

    #[test]
    fn ostlandet_and_sorlandet_count_shared_leaf_once() {
        let det = detected(&[
            ("europe/norway/ostlandet", 2_549_326_770),
            ("europe/norway/sorlandet", 331_098_569),
        ]);
        let p = plan_for(&det, None, &[], &BTreeSet::new(), false, u64::MAX);
        let ids: Vec<&str> = p.leaves.iter().map(|l| l.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "Denmark_North Denmark Region",
                "Norway_Hedmark",
                "Norway_Oppland",
                "Norway_Rogaland",
                "Norway_Southern"
            ]
        );
        let southern = p.leaves.iter().find(|l| l.id == "Norway_Southern").unwrap();
        assert_eq!(southern.shared_by.len(), 2);
        // The fixture gives Ostlandet 3 leaves and Sorlandet 3; only Southern is common.
        let denmark = p
            .leaves
            .iter()
            .find(|l| l.id.starts_with("Denmark"))
            .unwrap();
        assert_eq!(denmark.shared_by.len(), 1);
        // union: 101 + 301 + 101 + 51 + 201 = 755 MB; World+Coasts 100 MB on top.
        assert_eq!(p.download_bytes, 755_000_000 + 100_000_000);
        assert_eq!(p.total_bytes, 855_000_000);
        // Per region: Ostlandet 503 MB, Sorlandet 553 MB; Southern (301 MB) is common.
        assert_eq!(p.per_region_sum_bytes, (503 + 553) * 1_000_000);
        assert_eq!(p.shared_saved_bytes, p.per_region_sum_bytes - 755_000_000);
        assert!(p.world_needed);
        assert_eq!(p.old_bytes_selected, 2_549_326_770 + 331_098_569);
        assert!(p.large && p.wifi_only_default);
    }

    #[test]
    fn only_shared_leaf_regions() {
        // Sorlandet alone: Southern + Denmark + Rogaland.
        let det = detected(&[("europe/norway/sorlandet", 100)]);
        let p = plan_for(&det, None, &[], &BTreeSet::new(), true, u64::MAX);
        assert_eq!(p.leaves.len(), 3);
        assert!(!p.world_needed);
        assert_eq!(p.download_bytes, (301 + 51 + 201) * 1_000_000);
    }

    #[test]
    fn unmapped_region_is_reported_and_kept() {
        let det = detected(&[
            ("europe/norway/ostlandet", 10),
            ("europe/atlantis/east", 777),
        ]);
        let p = plan_for(&det, None, &[], &BTreeSet::new(), true, u64::MAX);
        let row = p
            .rows
            .iter()
            .find(|r| r.region_id.contains("atlantis"))
            .unwrap();
        assert!(!row.mapped && !row.selected && row.bake_id.is_none());
        assert!(row.note.as_deref().unwrap().contains("old map is kept"));
        assert_eq!(p.old_bytes_unmapped, 777);
        assert_eq!(row.display_name, "Atlantis East");
        assert_eq!(p.old_bytes_selected, 10);
    }

    #[test]
    fn only_unmapped_regions_download_nothing() {
        let det = detected(&[("europe/atlantis/east", 5)]);
        let p = plan_for(&det, None, &[], &BTreeSet::new(), false, 0);
        assert!(p.leaves.is_empty());
        assert_eq!(p.download_bytes, 0);
        assert!(!p.world_needed);
        assert!(p.enough_space);
        assert_eq!(p.free_bytes_needed, 0);
    }

    #[test]
    fn selection_and_exclusions_edit_the_list() {
        let det = detected(&[
            ("europe/norway/ostlandet", 1),
            ("europe/norway/sorlandet", 1),
        ]);
        let only_sor = vec!["europe/norway/sorlandet".to_string()];
        let p = plan_for(&det, Some(&only_sor), &[], &BTreeSet::new(), true, u64::MAX);
        assert_eq!(p.rows.iter().filter(|r| r.selected).count(), 1);
        assert_eq!(p.leaves.len(), 3);
        let excl = vec!["Denmark_North Denmark Region".to_string()];
        let p = plan_for(&det, None, &excl, &BTreeSet::new(), true, u64::MAX);
        assert!(p
            .leaves
            .iter()
            .any(|l| l.excluded && l.id.starts_with("Denmark")));
        assert_eq!(p.download_bytes, (101 + 301 + 101 + 201) * 1_000_000);
    }

    #[test]
    fn installed_leaves_are_not_downloaded_again() {
        let det = detected(&[("europe/norway/sorlandet", 1)]);
        let have: BTreeSet<String> = ["Norway_Southern".to_string()].into();
        let p = plan_for(&det, None, &[], &have, true, u64::MAX);
        assert_eq!(p.download_bytes, (51 + 201) * 1_000_000);
        assert_eq!(p.total_bytes, (301 + 51 + 201 + 100) * 1_000_000);
        assert!(
            p.leaves
                .iter()
                .find(|l| l.id == "Norway_Southern")
                .unwrap()
                .installed
        );
    }

    #[test]
    fn free_space_gate() {
        let det = detected(&[("europe/norway/ostlandet", 1)]);
        let p = plan_for(&det, None, &[], &BTreeSet::new(), false, 600_000_000);
        // 503 MB leaves + 100 MB World, World exists twice during promotion, + margin.
        assert_eq!(
            p.free_bytes_needed,
            503_000_000 + 100_000_000 + 100_000_000 + FREE_SPACE_MARGIN_BYTES
        );
        assert!(!p.enough_space);
        assert_eq!(p.space_short_by, p.free_bytes_needed - 600_000_000);
        let ok = plan_for(
            &det,
            None,
            &[],
            &BTreeSet::new(),
            false,
            p.free_bytes_needed,
        );
        assert!(ok.enough_space);
    }

    #[test]
    fn small_migration_is_not_wifi_only() {
        let det = detected(&[("europe/norway/ostlandet", 1)]);
        let sel = det.iter().map(|d| d.region_id.clone()).collect::<Vec<_>>();
        let excl: Vec<String> = [
            "Norway_Hedmark",
            "Norway_Southern",
            "Norway_Oppland",
            "Denmark_North Denmark Region",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let have: BTreeSet<String> = BTreeSet::new();
        let p = plan_for(&det, Some(&sel), &excl, &have, true, u64::MAX);
        assert!(p.leaves.iter().all(|l| l.excluded));
        assert_eq!(p.download_bytes, 0);
        assert!(!p.large);
    }

    #[test]
    fn many_regions_stay_deterministic() {
        let mut m = migration();
        let mut cat = catalog();
        // 40 synthetic regions each sharing leaf "Norway_Southern" plus a private leaf.
        let mut det = Vec::new();
        for i in 0..40 {
            let bake = format!("europe_zz_r{i}");
            let leaf = format!("Zz_{i}");
            let mut entry = m.regions["europe_norway_sorlandet"].clone();
            entry.region_id = format!("europe/zz/r{i}");
            entry.display_name = Some(format!("R{i}"));
            entry.comaps_leaves = vec![
                crate::mwm::migration::MigrationLeaf {
                    leaf: "Norway_Southern".into(),
                    coverage_of_region_pct: Some(50.0),
                    overspill_pct: Some(10.0),
                    include_reason: None,
                    shared_with_navi_regions: vec![],
                },
                crate::mwm::migration::MigrationLeaf {
                    leaf: leaf.clone(),
                    coverage_of_region_pct: Some(50.0),
                    overspill_pct: Some(10.0),
                    include_reason: None,
                    shared_with_navi_regions: vec![],
                },
            ];
            m.regions.insert(bake, entry);
            let mut l = cat.leaf("Norway_Oppland").unwrap().clone();
            l.id = leaf;
            cat.leaves.push(l);
            det.push(DetectedRkyvRegion {
                region_id: format!("europe/zz/r{i}"),
                old_bytes: 1000,
            });
        }
        let p = build_plan(&PlanInput {
            catalog: &cat,
            migration: &m,
            detected: &det,
            selected_region_ids: None,
            excluded_leaves: &[],
            installed_leaves: &BTreeSet::new(),
            world_installed: false,
            free_bytes: u64::MAX,
        });
        assert_eq!(p.rows.len(), 40);
        assert_eq!(p.leaves.len(), 41);
        assert_eq!(
            p.leaves
                .iter()
                .find(|l| l.id == "Norway_Southern")
                .unwrap()
                .shared_by
                .len(),
            40
        );
        assert_eq!(p.download_bytes, (301 + 40 * 101 + 100) * 1_000_000);
        assert_eq!(p.old_bytes_selected, 40_000);
    }

    #[test]
    fn leaf_missing_from_catalog_is_reported() {
        let mut cat = catalog();
        cat.leaves.retain(|l| l.id != "Norway_Rogaland");
        let det = detected(&[("europe/norway/sorlandet", 1)]);
        let p = build_plan(&PlanInput {
            catalog: &cat,
            migration: &migration(),
            detected: &det,
            selected_region_ids: None,
            excluded_leaves: &[],
            installed_leaves: &BTreeSet::new(),
            world_installed: true,
            free_bytes: u64::MAX,
        });
        assert_eq!(p.unavailable_leaves, vec!["Norway_Rogaland".to_string()]);
        assert_eq!(p.leaves.len(), 2);
    }

    #[test]
    fn region_ids_match_by_bake_id_fallback() {
        let det = detected(&[("europe_norway_sorlandet", 1)]);
        let p = plan_for(&det, None, &[], &BTreeSet::new(), true, u64::MAX);
        assert!(p.rows[0].mapped);
    }

    #[test]
    fn state_roundtrip_later_and_offer() {
        let dir = tempdir().unwrap();
        let d = dir.path();
        assert!(should_offer(d, now_ms()));
        let det = detected(&[("europe/norway/sorlandet", 10)]);
        let p = plan_for(&det, None, &[], &BTreeSet::new(), false, u64::MAX);
        let cat_text = serde_json::to_string(&serde_json::json!({"x": 1})).unwrap();
        let st = start_migration(d, &p, &cat_text, true).unwrap();
        assert_eq!(st.status, MigrationStatus::Queued);
        assert_eq!(st.leaves.len(), 3);
        assert_eq!(read_state(d).unwrap(), st);
        // A second migration cannot start while one is open.
        assert!(start_migration(d, &p, &cat_text, true).is_err());
        assert!(!should_offer(d, now_ms()));
        set_status(d, MigrationStatus::Paused, None).unwrap();
        assert_eq!(read_state(d).unwrap().status, MigrationStatus::Paused);
        discard_migration(d).unwrap();
        assert!(read_state(d).is_none());
        // "Later" suppresses the offer for three days.
        remind_later(d).unwrap();
        assert!(!should_offer(d, now_ms()));
        assert!(should_offer(d, now_ms() + LATER_REMINDER_MS + 1));
    }

    #[test]
    fn kept_regions_accumulate_and_can_be_forgotten() {
        let dir = tempdir().unwrap();
        let d = dir.path();
        let det = detected(&[("europe/norway/sorlandet", 10)]);
        let p = plan_for(&det, None, &[], &BTreeSet::new(), false, u64::MAX);
        start_migration(d, &p, "{}", false).unwrap();
        record_old_maps_decision(d, OldMapsDecision::Kept, 0).unwrap();
        let det2 = detected(&[("europe/norway/ostlandet", 20)]);
        let p2 = plan_for(&det2, None, &[], &BTreeSet::new(), false, u64::MAX);
        start_migration(d, &p2, "{}", false).unwrap();
        record_old_maps_decision(d, OldMapsDecision::Kept, 0).unwrap();
        let ids: Vec<String> = kept_old_regions(d)
            .into_iter()
            .map(|r| r.region_id)
            .collect();
        assert_eq!(
            ids,
            vec!["europe/norway/sorlandet", "europe/norway/ostlandet"]
        );
        forget_kept_region(d, "europe/norway/sorlandet").unwrap();
        assert_eq!(kept_old_regions(d).len(), 1);
        // A decision to delete never lists anything as kept.
        start_migration(d, &p, "{}", false).unwrap();
        record_old_maps_decision(d, OldMapsDecision::Deleted, 5).unwrap();
        assert_eq!(kept_old_regions(d).len(), 1);
    }

    #[test]
    fn done_state_allows_a_new_offer_and_migration() {
        let dir = tempdir().unwrap();
        let d = dir.path();
        let det = detected(&[("europe/norway/sorlandet", 10)]);
        let p = plan_for(&det, None, &[], &BTreeSet::new(), false, u64::MAX);
        start_migration(d, &p, "{}", false).unwrap();
        record_old_maps_decision(d, OldMapsDecision::Kept, 0).unwrap();
        assert!(should_offer(d, now_ms()));
        assert!(start_migration(d, &p, "{}", false).is_ok());
    }

    #[test]
    fn error_classification() {
        let net = MwmDownloadError::Other(
            "connection_failed GET host/…/x.mwm received=0/10: dns error".into(),
        );
        assert_eq!(classify_error(&net), FailureKind::Network);
        let space = MwmDownloadError::Other(
            "insufficient space for download: need 5 bytes, available 1 bytes at /x".into(),
        );
        assert_eq!(classify_error(&space), FailureKind::NoSpace);
        assert_eq!(
            classify_error(&MwmDownloadError::Other(CANCELLED.into())),
            FailureKind::Cancelled
        );
        let sum = MwmDownloadError::Other("checksum mismatch".into());
        assert_eq!(classify_error(&sum), FailureKind::Other);
        let enospc = MwmDownloadError::Io(std::io::Error::from_raw_os_error(28));
        assert_eq!(classify_error(&enospc), FailureKind::NoSpace);
    }

    #[test]
    fn update_offer_when_release_differs() {
        let dir = tempdir().unwrap();
        let d = dir.path();
        let lv = release_dir(d, "261005").join("leaves");
        fs::create_dir_all(&lv).unwrap();
        fs::write(lv.join("Norway_Hedmark.mwm"), b"x").unwrap();
        let cat = catalog();
        let o = update_offer(d, &cat, Some("261005"));
        assert!(o.update_available);
        assert_eq!(o.update_bytes, (101 + 100) * 1_000_000);
        let same = update_offer(d, &cat, Some("261105"));
        assert!(!same.update_available);
        assert!(!update_offer(d, &cat, None).update_available);
    }

    /// The Kotlin side (`MwmMigration.parsePlan` / `parsePersisted`) reads exactly these keys.
    #[test]
    fn json_shape_matches_what_the_app_reads() {
        let det = detected(&[("europe/norway/sorlandet", 10), ("europe/atlantis/x", 1)]);
        let p = plan_for(&det, None, &[], &BTreeSet::new(), false, 10);
        let v = serde_json::to_value(&p).unwrap();
        for k in [
            "version",
            "rows",
            "leaves",
            "unavailable_leaves",
            "world_needed",
            "download_bytes",
            "total_bytes",
            "shared_saved_bytes",
            "free_bytes_needed",
            "free_bytes_available",
            "enough_space",
            "space_short_by",
            "old_bytes_selected",
            "old_bytes_unmapped",
            "large",
            "wifi_only_default",
        ] {
            assert!(v.get(k).is_some(), "plan key {k}");
        }
        for k in [
            "region_id",
            "display_name",
            "old_bytes",
            "mapped",
            "selected",
            "leaf_ids",
            "region_bytes",
            "note",
        ] {
            assert!(v["rows"][0].get(k).is_some(), "row key {k}");
        }
        for k in [
            "id",
            "display_name",
            "bytes",
            "installed",
            "shared_by",
            "mostly_outside",
            "excluded",
        ] {
            assert!(v["leaves"][0].get(k).is_some(), "leaf key {k}");
        }
        let dir = tempdir().unwrap();
        start_migration(dir.path(), &p, "{}", true).unwrap();
        let st = serde_json::to_value(read_state(dir.path()).unwrap()).unwrap();
        for k in [
            "version",
            "status",
            "leaves",
            "old_regions",
            "wifi_only",
            "download_bytes",
            "error",
            "old_maps_decision",
            "bytes_freed",
        ] {
            assert!(st.get(k).is_some(), "state key {k}");
        }
        assert_eq!(st["status"], "queued");
        assert_eq!(st["old_maps_decision"], "pending");
    }

    #[test]
    fn run_without_state_fails_cleanly() {
        let dir = tempdir().unwrap();
        let mut p = DownloadProgress::default();
        let out = run_migration(dir.path(), "http://127.0.0.1:9", &mut p, None);
        assert!(matches!(out, MigrationOutcome::Failed(_)));
    }
}
