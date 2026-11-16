//! Polite MWM + sidecar downloads (resume, backoff, mirror failover).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use thiserror::Error;

use super::catalog::{CatalogLeaf, MwmCatalog};
use super::checksum::{verify_path_sha1_base64, verify_path_sha256, ChecksumError};
use super::comaps::{percent_encode_leaf_id, ComapsDiscovery, CountriesTxt};
use super::fingerprint::{
    check_local_mwm_fingerprint, FingerprintStatus, FINGERPRINT_USER_WARNING,
};
use super::flag::mwm_client_enabled;
use super::release::{begin_staging, mark_staging_complete, ReleaseSwitchError};
use super::storage::{
    attribution_json_path, attribution_md_path, catalog_snapshot_path, leaf_mwm_path,
    leaf_sidecar_path,
};
use crate::download::http::{
    progress_label_for_resume, stream_get_to_file_blocking, StreamDownloadOpts, DEFAULT_RETRIES,
};
use crate::pack_server::{http_get_bytes, http_get_text, PackServerError};

/// Identify Navi to CoMaps mirrors / navi-server.
pub const NAVI_MWM_USER_AGENT: &str =
    "Navi/0.1.0 (+https://github.com/Supermagnum/Navi; contact=https://github.com/Supermagnum/Navi/issues)";

/// Max concurrent MWM downloads (navi-server uses 2).
pub const DEFAULT_MWM_CONCURRENCY: usize = 2;

#[derive(Debug, Error)]
pub enum MwmDownloadError {
    #[error("mwm client feature flag is off")]
    Disabled,
    #[error("http: {0}")]
    Http(#[from] PackServerError),
    #[error("checksum: {0}")]
    Checksum(#[from] ChecksumError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("release: {0}")]
    Release(#[from] ReleaseSwitchError),
    #[error("download failed: {0}")]
    Other(String),
    #[error("leaf {0} missing from catalog")]
    MissingLeaf(String),
    #[error("leaf {0} missing from countries.txt")]
    MissingCountriesLeaf(String),
}

#[derive(Debug, Clone, Default)]
pub struct DownloadProgress {
    pub completed_leaves: usize,
    pub total_leaves: usize,
    pub current_label: String,
}

#[derive(Debug, Clone)]
pub struct DownloadPlan {
    pub version: String,
    pub leaf_ids: Vec<String>,
    pub navi_origin: String,
}

#[derive(Debug, Clone)]
pub struct LeafDownloadResult {
    pub leaf_id: String,
    pub mwm_path: PathBuf,
    pub sidecar_path: Option<PathBuf>,
    pub fingerprint: FingerprintStatus,
    pub navi_layers_available: bool,
    pub warning: Option<String>,
}

fn ua_headers() -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(USER_AGENT, HeaderValue::from_static(NAVI_MWM_USER_AGENT));
    h
}

fn join_url(base: &str, rel: &str) -> String {
    let base = base.trim().trim_end_matches('/');
    let rel = rel.trim().trim_start_matches('/');
    format!("{base}/{rel}")
}

fn backoff_sleep(attempt: u32) {
    let base_ms = 200u64.saturating_mul(1u64 << attempt.min(5));
    let jitter = (attempt as u64).saturating_mul(37) % 100;
    thread::sleep(Duration::from_millis(base_ms + jitter));
}

fn download_with_mirrors(
    mirrors: &[String],
    relative_path: &str,
    dest: &Path,
    expected_bytes: Option<u64>,
    label: &str,
) -> Result<(), MwmDownloadError> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut last_err = None;
    for mirror in mirrors {
        let url = join_url(mirror, relative_path);
        for attempt in 0..DEFAULT_RETRIES {
            let partial = PathBuf::from(format!("{}.partial", dest.display()));
            let resume_from = if partial.is_file() {
                fs::metadata(&partial).map(|m| m.len()).unwrap_or(0)
            } else {
                0
            };
            let ui = progress_label_for_resume(label, resume_from);
            match stream_get_to_file_blocking(StreamDownloadOpts {
                url: &url,
                dest,
                headers: ua_headers(),
                resume_from,
                expected_bytes,
                retries: 1,
                progress_label: &ui,
                allow_not_found: false,
            }) {
                Ok(Some(_)) => return Ok(()),
                Ok(None) => {
                    last_err = Some(MwmDownloadError::Other(format!("not found: {url}")));
                }
                Err(e) => {
                    let msg = e.to_string();
                    last_err = Some(MwmDownloadError::Other(msg.clone()));
                    if msg.contains("429") || msg.contains("503") || msg.contains("502") {
                        backoff_sleep(attempt);
                        continue;
                    }
                    break;
                }
            }
            backoff_sleep(attempt);
        }
    }
    Err(last_err.unwrap_or_else(|| MwmDownloadError::Other("all mirrors failed".into())))
}

fn download_navi_file(origin: &str, relative: &str, dest: &Path) -> Result<(), MwmDownloadError> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let url = join_url(origin, &format!("mwm/{relative}"));
    let partial = PathBuf::from(format!("{}.partial", dest.display()));
    let resume_from = if partial.is_file() {
        fs::metadata(&partial).map(|m| m.len()).unwrap_or(0)
    } else {
        0
    };
    let ui = progress_label_for_resume(relative, resume_from);
    stream_get_to_file_blocking(StreamDownloadOpts {
        url: &url,
        dest,
        headers: ua_headers(),
        resume_from,
        expected_bytes: None,
        retries: DEFAULT_RETRIES,
        progress_label: &ui,
        allow_not_found: false,
    })
    .map_err(|e| MwmDownloadError::Other(e.to_string()))?
    .ok_or_else(|| MwmDownloadError::Other(format!("not found: {url}")))?;
    Ok(())
}

fn unique_leaf_ids(ids: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for id in ids {
        if seen.insert(id.clone()) {
            out.push(id.clone());
        }
    }
    out
}

fn download_one_leaf(
    staging: &Path,
    leaf: &CatalogLeaf,
    countries: &CountriesTxt,
    discovery: &ComapsDiscovery,
    navi_origin: &str,
) -> Result<LeafDownloadResult, MwmDownloadError> {
    let mwm_dest = leaf_mwm_path(staging, &leaf.id);
    let countries_leaf = countries
        .leaves
        .get(&leaf.id)
        .ok_or_else(|| MwmDownloadError::MissingCountriesLeaf(leaf.id.clone()))?;

    let need_mwm = !(mwm_dest.is_file()
        && fs::metadata(&mwm_dest).map(|m| m.len()).unwrap_or(0) == countries_leaf.size);
    if need_mwm {
        let rel = format!(
            "maps/{}/{}/{}.mwm",
            discovery.map_series,
            discovery.release_version,
            percent_encode_leaf_id(&leaf.id)
        );
        download_with_mirrors(
            &discovery.mirrors,
            &rel,
            &mwm_dest,
            Some(countries_leaf.size),
            &format!("MWM {}", leaf.id),
        )?;
    }
    verify_path_sha1_base64(
        &mwm_dest,
        Some(countries_leaf.size),
        &countries_leaf.sha1_base64,
    )?;

    if leaf.no_sidecar {
        return Ok(LeafDownloadResult {
            leaf_id: leaf.id.clone(),
            mwm_path: mwm_dest,
            sidecar_path: None,
            fingerprint: FingerprintStatus::NoSidecar,
            navi_layers_available: false,
            warning: Some("no Navi sidecar for this overlay leaf".into()),
        });
    }

    let sc_dest = leaf_sidecar_path(staging, &leaf.id);
    let sc_url = leaf
        .sidecar_url
        .as_ref()
        .ok_or_else(|| MwmDownloadError::Other(format!("missing sidecar_url for {}", leaf.id)))?;
    let sc_sha = leaf.sidecar_sha256.as_ref().ok_or_else(|| {
        MwmDownloadError::Other(format!("missing sidecar_sha256 for {}", leaf.id))
    })?;
    let need_sc = !(sc_dest.is_file()
        && leaf
            .sidecar_bytes
            .map(|b| fs::metadata(&sc_dest).map(|m| m.len()).unwrap_or(0) == b)
            .unwrap_or(false));
    if need_sc {
        download_navi_file(navi_origin, sc_url, &sc_dest)?;
    }
    verify_path_sha256(&sc_dest, leaf.sidecar_bytes, sc_sha)?;

    let fp = leaf
        .fingerprint()
        .ok_or_else(|| MwmDownloadError::Other("missing fingerprint".into()))?;
    let status = check_local_mwm_fingerprint(&leaf.id, &mwm_dest, &fp)
        .map_err(|e| MwmDownloadError::Other(e.to_string()))?;
    let (navi_layers_available, warning) = match &status {
        FingerprintStatus::Match => (true, None),
        FingerprintStatus::Mismatch(_) => (false, Some(FINGERPRINT_USER_WARNING.to_string())),
        FingerprintStatus::NoSidecar => (false, None),
    };

    Ok(LeafDownloadResult {
        leaf_id: leaf.id.clone(),
        mwm_path: mwm_dest,
        sidecar_path: Some(sc_dest),
        fingerprint: status,
        navi_layers_available,
        warning,
    })
}

/// Download a leaf set into staging, verify digests + fingerprints, mark complete.
///
/// Always includes `World` + `WorldCoasts` (prepended) so
/// [`super::release::activate_release`] can promote root overlays with the
/// release. Shared leaves are stored once under `leaves/` / `sidecars/`. Does
/// **not** activate the release (call [`super::release::activate_release`]
/// after).
///
/// Concurrency is capped at [`DEFAULT_MWM_CONCURRENCY`] conceptually; this
/// implementation downloads leaves sequentially (safe / polite). Callers may
/// parallelize later without changing on-disk layout.
pub fn download_leaf_set(
    data_dir: &Path,
    catalog: &MwmCatalog,
    countries: &CountriesTxt,
    discovery: &ComapsDiscovery,
    navi_origin: &str,
    leaf_ids: &[String],
    progress: &mut DownloadProgress,
) -> Result<Vec<LeafDownloadResult>, MwmDownloadError> {
    download_leaf_set_with(
        data_dir,
        catalog,
        countries,
        discovery,
        navi_origin,
        leaf_ids,
        progress,
        &LeafSetOptions::default(),
    )
}

/// Options for [`download_leaf_set_with`].
#[derive(Debug, Clone, Copy)]
pub struct LeafSetOptions<'a> {
    /// Prepend `World` + `WorldCoasts` (needed for a new release; an incremental leaf
    /// download into the already active release must not re-fetch them).
    pub include_world: bool,
    /// Checked between leaves and before each leaf; when set the download stops with
    /// [`MwmDownloadError::Other`] `"cancelled"` and keeps the staging tree for resume.
    pub cancel: Option<&'a std::sync::atomic::AtomicBool>,
}

impl Default for LeafSetOptions<'_> {
    fn default() -> Self {
        Self {
            include_world: true,
            cancel: None,
        }
    }
}

/// [`download_leaf_set`] with [`LeafSetOptions`].
pub fn download_leaf_set_with(
    data_dir: &Path,
    catalog: &MwmCatalog,
    countries: &CountriesTxt,
    discovery: &ComapsDiscovery,
    navi_origin: &str,
    leaf_ids: &[String],
    progress: &mut DownloadProgress,
    opts: &LeafSetOptions<'_>,
) -> Result<Vec<LeafDownloadResult>, MwmDownloadError> {
    if !mwm_client_enabled() {
        return Err(MwmDownloadError::Disabled);
    }
    let version = &catalog.version;
    let staging = begin_staging(data_dir, version)?;
    // Snapshot catalog JSON as originally served when available via re-fetch;
    // here we persist a minimal version marker for offline debugging.
    fs::write(
        catalog_snapshot_path(&staging),
        format!("{{\"schema\":\"navi-mwm-catalog/v2\",\"version\":\"{version}\"}}\n"),
    )?;

    let attr_rel = format!("{version}/attribution.json");
    let attr_md_rel = format!("{version}/ATTRIBUTION.md");
    if let Ok(text) = http_get_text(
        &join_url(navi_origin, &format!("mwm/{attr_rel}")),
        Duration::from_secs(60),
    ) {
        fs::write(attribution_json_path(&staging), text)?;
    }
    if let Ok(bytes) = http_get_bytes(
        &join_url(navi_origin, &format!("mwm/{attr_md_rel}")),
        Duration::from_secs(60),
    ) {
        let _ = fs::write(attribution_md_path(&staging), bytes);
    }

    let leaves = if opts.include_world {
        unique_leaf_ids(&leaf_ids_with_world_overlays(leaf_ids))
    } else {
        unique_leaf_ids(leaf_ids)
    };
    progress.total_leaves = leaves.len();
    progress.completed_leaves = 0;

    let mut results = Vec::new();
    for leaf_id in &leaves {
        if opts
            .cancel
            .map(|c| c.load(std::sync::atomic::Ordering::SeqCst))
            .unwrap_or(false)
        {
            return Err(MwmDownloadError::Other("cancelled".into()));
        }
        progress.current_label = leaf_id.clone();
        let catalog_leaf = catalog
            .leaf(leaf_id)
            .ok_or_else(|| MwmDownloadError::MissingLeaf(leaf_id.clone()))?;
        let result = download_one_leaf(&staging, catalog_leaf, countries, discovery, navi_origin)?;
        progress.completed_leaves += 1;
        results.push(result);
    }

    mark_staging_complete(&staging)?;
    Ok(results)
}

/// Test-friendly install of already-fetched bytes into staging (shared-leaf +
/// fingerprint logic without network).
pub fn install_leaf_bytes_for_test(
    staging: &Path,
    leaf: &CatalogLeaf,
    mwm_bytes: &[u8],
    sidecar_bytes: Option<&[u8]>,
) -> Result<LeafDownloadResult, MwmDownloadError> {
    let mwm_dest = leaf_mwm_path(staging, &leaf.id);
    if let Some(parent) = mwm_dest.parent() {
        fs::create_dir_all(parent)?;
    }
    if !mwm_dest.is_file() {
        fs::write(&mwm_dest, mwm_bytes)?;
    }
    if let Some(exp) = leaf.comaps_bytes.or(leaf.mwm_bytes) {
        let got = fs::metadata(&mwm_dest)?.len();
        if got != exp {
            return Err(MwmDownloadError::Checksum(ChecksumError::SizeMismatch {
                got,
                expected: exp,
            }));
        }
    }
    if let Some(sha1) = &leaf.comaps_sha1_base64 {
        verify_path_sha1_base64(&mwm_dest, leaf.comaps_bytes.or(leaf.mwm_bytes), sha1)?;
    }

    if leaf.no_sidecar {
        return Ok(LeafDownloadResult {
            leaf_id: leaf.id.clone(),
            mwm_path: mwm_dest,
            sidecar_path: None,
            fingerprint: FingerprintStatus::NoSidecar,
            navi_layers_available: false,
            warning: None,
        });
    }

    let sc_dest = leaf_sidecar_path(staging, &leaf.id);
    if let Some(parent) = sc_dest.parent() {
        fs::create_dir_all(parent)?;
    }
    if let Some(bytes) = sidecar_bytes {
        if !sc_dest.is_file() {
            fs::write(&sc_dest, bytes)?;
        }
        if let Some(sha) = &leaf.sidecar_sha256 {
            verify_path_sha256(&sc_dest, leaf.sidecar_bytes, sha)?;
        }
    }

    let fp = leaf
        .fingerprint()
        .ok_or_else(|| MwmDownloadError::Other("missing fingerprint".into()))?;
    let status = check_local_mwm_fingerprint(&leaf.id, &mwm_dest, &fp)
        .map_err(|e| MwmDownloadError::Other(e.to_string()))?;
    let (navi_layers_available, warning) = match &status {
        FingerprintStatus::Match => (sidecar_bytes.is_some(), None),
        FingerprintStatus::Mismatch(_) => (false, Some(FINGERPRINT_USER_WARNING.to_string())),
        FingerprintStatus::NoSidecar => (false, None),
    };

    Ok(LeafDownloadResult {
        leaf_id: leaf.id.clone(),
        mwm_path: mwm_dest,
        sidecar_path: if sc_dest.is_file() {
            Some(sc_dest)
        } else {
            None
        },
        fingerprint: status,
        navi_layers_available,
        warning,
    })
}

/// Fetch catalog from navi-server DocumentRoot (`/mwm/current.json`).
pub fn fetch_current_catalog(navi_origin: &str) -> Result<MwmCatalog, MwmDownloadError> {
    if !mwm_client_enabled() {
        return Err(MwmDownloadError::Disabled);
    }
    let url = join_url(navi_origin, "mwm/current.json");
    let text = http_get_text(&url, Duration::from_secs(120))?;
    super::catalog::parse_catalog_json(&text).map_err(|e| MwmDownloadError::Other(e.to_string()))
}

/// World + WorldCoasts overlay leaf ids (catalog `non_routable_leaves`, no sidecar).
pub const WORLD_OVERLAY_LEAF_IDS: &[&str] = &["World", "WorldCoasts"];

/// Prepend World / WorldCoasts to a leaf set so every release also fetches the
/// non-routable overlays (checksum-verified like other leaves; `no_sidecar`).
pub fn leaf_ids_with_world_overlays(leaf_ids: &[String]) -> Vec<String> {
    let mut out: Vec<String> = WORLD_OVERLAY_LEAF_IDS
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    for id in leaf_ids {
        if !out.iter().any(|x| x == id) {
            out.push(id.clone());
        }
    }
    out
}

/// Download `World` + `WorldCoasts` into staging (Phase 1 no_sidecar path).
pub fn download_world_overlays(
    data_dir: &Path,
    catalog: &MwmCatalog,
    countries: &CountriesTxt,
    discovery: &ComapsDiscovery,
    navi_origin: &str,
    progress: &mut DownloadProgress,
) -> Result<Vec<LeafDownloadResult>, MwmDownloadError> {
    let ids: Vec<String> = WORLD_OVERLAY_LEAF_IDS
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    download_leaf_set(
        data_dir,
        catalog,
        countries,
        discovery,
        navi_origin,
        &ids,
        progress,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mwm::catalog::parse_catalog_json;
    use crate::mwm::checksum::{sha1_base64_of_bytes, sha256_hex_of_bytes};
    use crate::mwm::flag::with_mwm_client_override;
    use crate::mwm::release::begin_staging;
    use tempfile::tempdir;

    #[test]
    fn shared_leaf_downloaded_once() {
        with_mwm_client_override(Some(true), || {
            let dir = tempdir().unwrap();
            let staging = begin_staging(dir.path(), "261105").unwrap();
            let mwm = b"shared-mwm-bytes-xxxx";
            let leaf = CatalogLeaf {
                id: "Norway_Southern".into(),
                sidecar_url: Some("261105/Norway_Southern.navi-sidecar.v1.zst".into()),
                sidecar_bytes: Some(4),
                sidecar_sha256: Some(sha256_hex_of_bytes(b"side")),
                comaps_file: Some("Norway_Southern.mwm".into()),
                comaps_bytes: Some(mwm.len() as u64),
                comaps_sha1_base64: Some(sha1_base64_of_bytes(mwm)),
                mwm_bytes: Some(mwm.len() as u64),
                mwm_sha256: Some(sha256_hex_of_bytes(mwm)),
                mwm_fingerprint: Some(crate::mwm::catalog::MwmFingerprint {
                    size: mwm.len() as u64,
                    sha256: sha256_hex_of_bytes(mwm),
                }),
                shared: true,
                shared_with_regions: vec![
                    "europe_norway_ostlandet".into(),
                    "europe_norway_sorlandet".into(),
                ],
                no_sidecar: false,
                elevation_policy_version: Some(1),
            };
            let r1 = install_leaf_bytes_for_test(&staging, &leaf, mwm, Some(b"side")).unwrap();
            assert!(r1.navi_layers_available);
            let mtime1 = fs::metadata(&r1.mwm_path).unwrap().modified().unwrap();
            let r2 = install_leaf_bytes_for_test(&staging, &leaf, mwm, Some(b"side")).unwrap();
            let mtime2 = fs::metadata(&r2.mwm_path).unwrap().modified().unwrap();
            assert_eq!(r1.mwm_path, r2.mwm_path);
            assert_eq!(mtime1, mtime2);
        });
    }

    #[test]
    fn fingerprint_mismatch_disables_navi_layers() {
        with_mwm_client_override(Some(true), || {
            let dir = tempdir().unwrap();
            let staging = begin_staging(dir.path(), "261105").unwrap();
            let mwm = b"local-mwm";
            let leaf = CatalogLeaf {
                id: "Norway_Hedmark".into(),
                sidecar_url: Some("x".into()),
                sidecar_bytes: Some(4),
                sidecar_sha256: Some(sha256_hex_of_bytes(b"side")),
                comaps_file: None,
                comaps_bytes: Some(mwm.len() as u64),
                comaps_sha1_base64: Some(sha1_base64_of_bytes(mwm)),
                mwm_bytes: Some(mwm.len() as u64),
                mwm_sha256: Some(sha256_hex_of_bytes(mwm)),
                mwm_fingerprint: Some(crate::mwm::catalog::MwmFingerprint {
                    size: mwm.len() as u64,
                    sha256: "ab".repeat(32),
                }),
                shared: false,
                shared_with_regions: vec![],
                no_sidecar: false,
                elevation_policy_version: Some(1),
            };
            let r = install_leaf_bytes_for_test(&staging, &leaf, mwm, Some(b"side")).unwrap();
            assert!(!r.navi_layers_available);
            assert_eq!(r.warning.as_deref(), Some(FINGERPRINT_USER_WARNING));
        });
    }

    #[test]
    fn parse_example_catalog_for_download_plan() {
        let c = parse_catalog_json(include_str!(
            "../../tests/fixtures/mwm/catalog-v2.example.json"
        ))
        .unwrap();
        let leaves = c.leaves_for_bake_id("europe_norway_ostlandet").unwrap();
        assert!(leaves.contains(&"Norway_Hedmark".to_string()));
        assert!(leaves.contains(&"Norway_Southern".to_string()));
    }

    #[test]
    fn flag_off_blocks_fetch() {
        with_mwm_client_override(Some(false), || {
            let err = fetch_current_catalog("https://example.invalid").unwrap_err();
            assert!(matches!(err, MwmDownloadError::Disabled));
        });
    }

    #[test]
    fn world_overlays_prepended_once() {
        let ids = leaf_ids_with_world_overlays(&[
            "World".into(),
            "Norway_Hedmark".into(),
            "WorldCoasts".into(),
        ]);
        assert_eq!(
            ids,
            vec![
                "World".to_string(),
                "WorldCoasts".to_string(),
                "Norway_Hedmark".to_string(),
            ]
        );
    }
}
