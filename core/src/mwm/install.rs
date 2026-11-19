//! One-call installer for the Phase 1 download layer: catalog -> mirrors -> verified
//! `countries.txt` -> leaf download (MWM + sidecar, checksum + fingerprint verified) -> register
//! (activate a new release, or merge into the active release of the same version).
//!
//! The app calls [`install_leaf_set`] from a background thread and polls
//! [`crate::download::progress`] plus the [`DownloadProgress`] it passes in. Already installed
//! leaves are never downloaded again; an incremental install into the active release does not
//! re-fetch the World overlays and never removes existing leaves, so a live router / drape keeps
//! working while new leaves arrive.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use super::catalog::MwmCatalog;
use super::comaps::{
    discover_mirrors, fetch_and_verify_countries_txt, CountriesTxt, COMAPS_COUNTRIES_TXT_PUBKEY_HEX,
};
use super::comaps_fallback::{
    catalog_snapshot_json, resolve_mwm_catalog, CatalogResolveSource, ResolvedCatalog,
};
use super::download::{
    download_leaf_set_with, DownloadProgress, LeafSetOptions, MwmDownloadError,
};
use super::flag::mwm_client_enabled;
use super::release::{
    activate_release, merge_staging_into_release, read_active_version, read_world_release,
};
use super::storage::{
    leaf_mwm_path, leaf_sidecar_path, release_dir, world_coasts_mwm_path, world_mwm_path,
};

#[derive(Debug, Clone, Default)]
pub struct InstallReport {
    pub version: String,
    /// Leaves requested by the caller (World overlays excluded).
    pub requested: Vec<String>,
    /// Leaves fetched by this call (World overlays excluded).
    pub downloaded: Vec<String>,
    /// Requested leaves that were already installed and verified by size.
    pub already_installed: Vec<String>,
    pub world_downloaded: bool,
    /// True when `active.json` was switched to a new release (not just merged).
    pub activated_new_release: bool,
    pub sidecars_installed: usize,
}

/// Leaf ids of the MWMs present under `releases/{version}/leaves/` (World overlays excluded).
pub fn installed_leaf_ids(data_dir: &Path, version: &str) -> Vec<String> {
    let dir = release_dir(data_dir, version).join("leaves");
    let mut out: Vec<String> = fs::read_dir(&dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().into_owned();
                    let stem = name.strip_suffix(".mwm")?;
                    let meta = e.metadata().ok()?;
                    (meta.is_file() && meta.len() > 0 && stem != "World" && stem != "WorldCoasts")
                        .then(|| stem.to_string())
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// Installed leaves of the active release (empty without one).
pub fn installed_leaf_ids_active(data_dir: &Path) -> (Option<String>, Vec<String>) {
    match read_active_version(data_dir) {
        Ok(Some(a)) => {
            let ids = installed_leaf_ids(data_dir, &a.version);
            (Some(a.version), ids)
        }
        _ => (None, Vec::new()),
    }
}

/// Number of verified `*.navi-sidecar.v1.zst` files in the active release.
pub fn installed_sidecar_count(data_dir: &Path, version: &str) -> usize {
    fs::read_dir(release_dir(data_dir, version).join("sidecars"))
        .map(|rd| {
            rd.flatten()
                .filter(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .ends_with(".navi-sidecar.v1.zst")
                })
                .count()
        })
        .unwrap_or(0)
}

fn leaf_mwm_present(release: &Path, countries: &CountriesTxt, id: &str) -> bool {
    let Some(c) = countries.leaves.get(id) else {
        return false;
    };
    fs::metadata(leaf_mwm_path(release, id))
        .map(|m| m.is_file() && m.len() == c.size)
        .unwrap_or(false)
}

fn leaf_sidecar_present(release: &Path, catalog: &MwmCatalog, id: &str) -> bool {
    match catalog.leaf(id) {
        Some(l) if !l.no_sidecar => {
            let path = leaf_sidecar_path(release, id);
            let meta = match fs::metadata(&path) {
                Ok(m) if m.is_file() => m,
                _ => return false,
            };
            if let Some(b) = l.sidecar_bytes {
                if meta.len() != b {
                    return false;
                }
            }
            // Prefer SHA when catalog carries one so same-size updates refresh.
            if let Some(want) = l.sidecar_sha256.as_ref() {
                return verify_sidecar_sha256(&path, want);
            }
            true
        }
        _ => true,
    }
}

fn verify_sidecar_sha256(path: &Path, want_hex: &str) -> bool {
    use sha2::{Digest, Sha256};
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    let got = Sha256::digest(&bytes);
    let got_hex: String = got.iter().map(|b| format!("{b:02x}")).collect();
    got_hex.eq_ignore_ascii_case(want_hex)
}

fn leaf_present(release: &Path, countries: &CountriesTxt, catalog: &MwmCatalog, id: &str) -> bool {
    leaf_mwm_present(release, countries, id) && leaf_sidecar_present(release, catalog, id)
}

/// Process-wide guard: one leaf install / migration at a time (they share the staging tree).
static INSTALL_RUNNING: AtomicBool = AtomicBool::new(false);

struct InstallGuard;

impl InstallGuard {
    fn acquire() -> Result<Self, MwmDownloadError> {
        INSTALL_RUNNING
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map(|_| InstallGuard)
            .map_err(|_| MwmDownloadError::Other("another map install is already running".into()))
    }
}

impl Drop for InstallGuard {
    fn drop(&mut self) {
        INSTALL_RUNNING.store(false, Ordering::SeqCst);
    }
}

/// Install `wanted` leaves (and, for a new release, World + WorldCoasts and the leaves of the
/// previous release). `cancel` stops the download (between leaves and inside a leaf, within about
/// a quarter of a second) and keeps the staging tree for resume.
///
/// Catalog resolve: live navi → disk cache → CoMaps-direct (see [`resolve_mwm_catalog`]). Disk /
/// CoMaps-direct soft-fail missing sidecars so maps still install; vehicle-limits honesty banners
/// cover leaves without a matching sidecar.
pub fn install_leaf_set(
    data_dir: &Path,
    navi_origin: &str,
    wanted: &[String],
    progress: &mut DownloadProgress,
    cancel: Option<&AtomicBool>,
) -> Result<InstallReport, MwmDownloadError> {
    if !mwm_client_enabled() {
        return Err(MwmDownloadError::Disabled);
    }
    let resolved = resolve_mwm_catalog(navi_origin, Some(data_dir))?;
    install_leaf_set_resolved(data_dir, navi_origin, &resolved, wanted, progress, cancel)
}

/// [`install_leaf_set`] against a catalog the caller already holds. A migration pins the catalog
/// it planned with, so a release published while it runs does not change what it downloads.
pub fn install_leaf_set_with_catalog(
    data_dir: &Path,
    navi_origin: &str,
    catalog: &MwmCatalog,
    wanted: &[String],
    progress: &mut DownloadProgress,
    cancel: Option<&AtomicBool>,
) -> Result<InstallReport, MwmDownloadError> {
    let resolved = ResolvedCatalog {
        catalog: catalog.clone(),
        source: CatalogResolveSource::NaviServer,
        raw_json: None,
    };
    install_leaf_set_resolved(data_dir, navi_origin, &resolved, wanted, progress, cancel)
}

fn install_leaf_set_resolved(
    data_dir: &Path,
    navi_origin: &str,
    resolved: &ResolvedCatalog,
    wanted: &[String],
    progress: &mut DownloadProgress,
    cancel: Option<&AtomicBool>,
) -> Result<InstallReport, MwmDownloadError> {
    if !mwm_client_enabled() {
        return Err(MwmDownloadError::Disabled);
    }
    let _guard = InstallGuard::acquire()?;
    let catalog = &resolved.catalog;
    let countries_sha = catalog.comaps.countries_txt_sha256.as_str();
    let countries_bytes = catalog.comaps.countries_txt_bytes;
    // CoMaps-direct / empty sha: verify signature only (no pinned digest from navi).
    let expect_sha = (!countries_sha.is_empty()).then_some(countries_sha);
    let expect_bytes = (countries_bytes > 0).then_some(countries_bytes);

    let discovery = discover_mirrors(
        &catalog.comaps.metaserver_url,
        &catalog.comaps.map_series,
        Some(catalog.comaps.release_version),
    )
    .map_err(|e| MwmDownloadError::Other(format!("mirror discovery: {e}")))?;
    let pubkey = if catalog.comaps.countries_txt_pubkey_hex.is_empty() {
        COMAPS_COUNTRIES_TXT_PUBKEY_HEX
    } else {
        catalog.comaps.countries_txt_pubkey_hex.as_str()
    };
    let countries = fetch_and_verify_countries_txt(&discovery, expect_sha, expect_bytes, pubkey)
        .map_err(|e| MwmDownloadError::Other(format!("countries.txt: {e}")))?;

    for id in wanted {
        if catalog.leaf(id).is_none() {
            return Err(MwmDownloadError::MissingLeaf(id.clone()));
        }
    }

    let active = read_active_version(data_dir)
        .map_err(MwmDownloadError::Release)?
        .map(|a| a.version);
    let same_release = active.as_deref() == Some(catalog.version.as_str());
    let world_root_ok = world_mwm_path(data_dir).is_file()
        && world_coasts_mwm_path(data_dir).is_file()
        && read_world_release(data_dir)
            .ok()
            .flatten()
            .map(|w| w.version == catalog.version)
            .unwrap_or(false);

    let mut want: BTreeSet<String> = wanted.iter().cloned().collect();
    if !same_release {
        // Upgrade: the new release replaces the old tree, so carry the old leaves over.
        if let Some(old) = active.as_deref() {
            for id in installed_leaf_ids(data_dir, old) {
                if catalog.leaf(&id).is_some() {
                    want.insert(id);
                }
            }
        }
    }

    let release = release_dir(data_dir, &catalog.version);
    let mut report = InstallReport {
        version: catalog.version.clone(),
        requested: wanted.to_vec(),
        ..Default::default()
    };
    let mut to_download = Vec::new();
    for id in &want {
        if same_release && leaf_present(&release, &countries, catalog, id) {
            if wanted.contains(id) {
                report.already_installed.push(id.clone());
            }
        } else {
            to_download.push(id.clone());
        }
    }
    let need_world = !same_release || !world_root_ok;
    if to_download.is_empty() && !need_world {
        report.sidecars_installed = installed_sidecar_count(data_dir, &catalog.version);
        return Ok(report);
    }

    let snapshot = catalog_snapshot_json(resolved)?;
    let opts = LeafSetOptions {
        include_world: need_world,
        cancel,
        allow_missing_sidecar: resolved.source.allow_missing_sidecar(),
        catalog_json: Some(snapshot.as_str()),
    };
    download_leaf_set_with(
        data_dir,
        catalog,
        &countries,
        &discovery,
        navi_origin,
        &to_download,
        progress,
        &opts,
    )?;
    report.downloaded = to_download.clone();
    report.world_downloaded = need_world;

    if same_release {
        merge_staging_into_release(data_dir, &catalog.version)
            .map_err(MwmDownloadError::Release)?;
    } else {
        activate_release(data_dir, &catalog.version).map_err(MwmDownloadError::Release)?;
        report.activated_new_release = true;
    }
    report.sidecars_installed = installed_sidecar_count(data_dir, &catalog.version);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn installed_leaves_skip_world_empty_and_foreign_files() {
        let dir = tempdir().unwrap();
        let leaves = release_dir(dir.path(), "261105").join("leaves");
        fs::create_dir_all(&leaves).unwrap();
        fs::write(leaves.join("Norway_Hedmark.mwm"), b"x").unwrap();
        fs::write(leaves.join("Norway_Oppland.mwm"), b"y").unwrap();
        fs::write(leaves.join("World.mwm"), b"w").unwrap();
        fs::write(leaves.join("WorldCoasts.mwm"), b"w").unwrap();
        fs::write(leaves.join("Empty.mwm"), b"").unwrap();
        fs::write(leaves.join("Norway_Hedmark.mwm.partial"), b"p").unwrap();
        assert_eq!(
            installed_leaf_ids(dir.path(), "261105"),
            vec!["Norway_Hedmark".to_string(), "Norway_Oppland".to_string()]
        );
        assert!(installed_leaf_ids(dir.path(), "999999").is_empty());
    }

    #[test]
    fn sidecar_count_counts_only_sidecars() {
        let dir = tempdir().unwrap();
        let sc = release_dir(dir.path(), "261105").join("sidecars");
        fs::create_dir_all(&sc).unwrap();
        fs::write(sc.join("A.navi-sidecar.v1.zst"), b"a").unwrap();
        fs::write(sc.join("B.navi-sidecar.v1.zst"), b"b").unwrap();
        fs::write(sc.join("notes.txt"), b"n").unwrap();
        assert_eq!(installed_sidecar_count(dir.path(), "261105"), 2);
    }
}
