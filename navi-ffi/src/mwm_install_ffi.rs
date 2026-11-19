//! UniFFI surface for the MWM leaf installer (catalog -> mirrors -> verified download -> register).
//!
//! `mwm_install_leaves` blocks the calling (background) thread; the UI polls
//! [`mwm_install_progress`] and the shared download progress channel
//! (`download_progress_snapshot`) for byte-level progress.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use driver_break_core::mwm::{
    install_leaf_set, installed_leaf_ids_active, installed_sidecar_count, resolve_mwm_catalog,
    DownloadProgress, MwmCatalog,
};

static CANCEL: AtomicBool = AtomicBool::new(false);
static STATE: Mutex<InstallState> = Mutex::new(InstallState::new());

struct InstallState {
    running: bool,
    finished: bool,
    ok: bool,
    error: Option<String>,
    version: String,
    downloaded: u32,
    already_installed: u32,
    sidecars_installed: u32,
    activated_new_release: bool,
}

impl InstallState {
    const fn new() -> Self {
        Self {
            running: false,
            finished: false,
            ok: false,
            error: None,
            version: String::new(),
            downloaded: 0,
            already_installed: 0,
            sidecars_installed: 0,
            activated_new_release: false,
        }
    }
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiMwmRegion {
    /// Catalog `region_to_leaves` key, e.g. `europe_norway_ostlandet`.
    pub id: String,
    pub display_name: String,
    pub leaf_ids: Vec<String>,
    /// MWM + sidecar bytes of the leaves (World overlays excluded).
    pub total_bytes: u64,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiMwmRegionList {
    pub ok: bool,
    pub error: Option<String>,
    pub version: String,
    pub regions: Vec<FfiMwmRegion>,
}

fn pretty_region_name(id: &str) -> String {
    let parts: Vec<&str> = id.split('_').collect();
    let skip = if parts.len() > 1 { 1 } else { 0 };
    parts[skip..]
        .iter()
        .map(|p| {
            let mut c = p.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn regions_from_catalog(cat: &MwmCatalog) -> Vec<FfiMwmRegion> {
    let mut regions: Vec<FfiMwmRegion> = if !cat.region_to_leaves.is_empty() {
        cat.region_to_leaves
            .iter()
            .map(|(id, leaves)| {
                let total_bytes = leaves
                    .iter()
                    .filter_map(|l| cat.leaf(l))
                    .map(|l| {
                        l.comaps_bytes.or(l.mwm_bytes).unwrap_or(0) + l.sidecar_bytes.unwrap_or(0)
                    })
                    .sum();
                FfiMwmRegion {
                    id: id.clone(),
                    display_name: pretty_region_name(id),
                    leaf_ids: leaves.clone(),
                    total_bytes,
                }
            })
            .collect()
    } else {
        // CoMaps-direct / sparse catalogs: one row per country group.
        cat.country_groups
            .iter()
            .filter_map(|g| {
                let leaf_ids: Vec<String> = g.leaves.iter().map(|l| l.id.clone()).collect();
                if leaf_ids.is_empty() {
                    return None;
                }
                let total_bytes = leaf_ids
                    .iter()
                    .filter_map(|l| cat.leaf(l))
                    .map(|l| l.comaps_bytes.or(l.mwm_bytes).unwrap_or(0))
                    .sum();
                Some(FfiMwmRegion {
                    id: g.id.to_ascii_lowercase(),
                    display_name: g.display_name.clone().unwrap_or_else(|| g.id.clone()),
                    leaf_ids,
                    total_bytes,
                })
            })
            .collect()
    };
    regions.sort_by(|a, b| a.id.cmp(&b.id));
    regions
}

/// List downloadable regions (blocking). Uses live navi catalog, then disk cache, then
/// CoMaps-direct fallback so Tools still works when navi leaves are empty/unreachable.
#[uniffi::export]
pub fn mwm_list_regions(navi_origin: String) -> FfiMwmRegionList {
    match resolve_mwm_catalog(&navi_origin, None) {
        Ok(resolved) => FfiMwmRegionList {
            ok: true,
            error: None,
            version: resolved.catalog.version.clone(),
            regions: regions_from_catalog(&resolved.catalog),
        },
        Err(e) => FfiMwmRegionList {
            ok: false,
            error: Some(e.to_string()),
            version: String::new(),
            regions: vec![],
        },
    }
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiMwmInstallProgress {
    pub running: bool,
    pub finished: bool,
    pub ok: bool,
    pub error: Option<String>,
    pub version: String,
    pub downloaded_leaves: u32,
    pub already_installed_leaves: u32,
    pub sidecars_installed: u32,
    pub activated_new_release: bool,
}

#[uniffi::export]
pub fn mwm_install_progress() -> FfiMwmInstallProgress {
    let s = STATE.lock().unwrap_or_else(|p| p.into_inner());
    FfiMwmInstallProgress {
        running: s.running,
        finished: s.finished,
        ok: s.ok,
        error: s.error.clone(),
        version: s.version.clone(),
        downloaded_leaves: s.downloaded,
        already_installed_leaves: s.already_installed,
        sidecars_installed: s.sidecars_installed,
        activated_new_release: s.activated_new_release,
    }
}

/// Ask the running install to stop between leaves (the staging tree is kept for resume).
#[uniffi::export]
pub fn mwm_install_cancel() {
    CANCEL.store(true, Ordering::SeqCst);
}

/// Download, verify and register `leaf_ids` into `data_dir/mwm` (blocking; call off the main
/// thread). Already installed leaves are skipped; an incremental install keeps the active
/// release (and its open files) intact. Returns the final progress state.
#[uniffi::export]
pub fn mwm_install_leaves(
    data_dir: String,
    navi_origin: String,
    leaf_ids: Vec<String>,
) -> FfiMwmInstallProgress {
    {
        let mut s = STATE.lock().unwrap_or_else(|p| p.into_inner());
        if s.running {
            drop(s);
            return mwm_install_progress();
        }
        *s = InstallState::new();
        s.running = true;
    }
    CANCEL.store(false, Ordering::SeqCst);
    let mut progress = DownloadProgress::default();
    let res = install_leaf_set(
        Path::new(&data_dir),
        &navi_origin,
        &leaf_ids,
        &mut progress,
        Some(&CANCEL),
    );
    {
        let mut s = STATE.lock().unwrap_or_else(|p| p.into_inner());
        s.running = false;
        s.finished = true;
        match res {
            Ok(r) => {
                s.ok = true;
                s.version = r.version;
                s.downloaded = r.downloaded.len() as u32;
                s.already_installed = r.already_installed.len() as u32;
                s.sidecars_installed = r.sidecars_installed as u32;
                s.activated_new_release = r.activated_new_release;
            }
            Err(e) => {
                s.ok = false;
                s.error = Some(e.to_string());
            }
        }
    }
    mwm_install_progress()
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiMwmInstalled {
    pub version: Option<String>,
    pub leaf_ids: Vec<String>,
    pub sidecar_count: u32,
}

/// Leaves of the active release under `data_dir/mwm` (no network).
#[uniffi::export]
pub fn mwm_installed(data_dir: String) -> FfiMwmInstalled {
    let (version, leaf_ids) = installed_leaf_ids_active(Path::new(&data_dir));
    let sidecar_count = version
        .as_deref()
        .map(|v| installed_sidecar_count(Path::new(&data_dir), v) as u32)
        .unwrap_or(0);
    FfiMwmInstalled {
        version,
        leaf_ids,
        sidecar_count,
    }
}
