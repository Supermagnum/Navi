//! Atomic release switching: never mix CoMaps release versions.
//!
//! Country leaves live under `releases/{version}/leaves/`. World overlays
//! (`World.mwm`, `WorldCoasts.mwm`) live at the MWM storage root and are
//! promoted atomically with `active.json` via `world-release.json`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::storage::{
    active_path, leaf_mwm_path, release_dir, staging_dir, world_coasts_mwm_path, world_mwm_path,
    world_release_path, MwmPaths, WORLD_COASTS_MWM_FILE, WORLD_MWM_FILE,
};

const WORLD_OVERLAY_LEAF_IDS: &[&str] = &["World", "WorldCoasts"];

#[derive(Debug, Error)]
pub enum ReleaseSwitchError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("staging release {0} is incomplete: {1}")]
    IncompleteStaging(String, String),
    #[error("staging release {0} missing World overlay: {1}")]
    MissingWorldOverlay(String, String),
    #[error("World release {world} does not match active release {active}")]
    WorldVersionMismatch { active: String, world: String },
    #[error("World overlays missing at MWM root (expected World.mwm + WorldCoasts.mwm)")]
    WorldOverlaysAbsent,
    #[error("no active MWM release")]
    NoActiveRelease,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActiveRelease {
    pub version: String,
}

/// Which CoMaps release the root `World*.mwm` files belong to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorldRelease {
    pub version: String,
}

/// Read the active release pointer, if any.
pub fn read_active_version(data_dir: &Path) -> Result<Option<ActiveRelease>, ReleaseSwitchError> {
    let path = active_path(data_dir);
    if !path.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(path)?;
    Ok(Some(serde_json::from_str(&text)?))
}

pub fn write_active_version(data_dir: &Path, version: &str) -> Result<(), ReleaseSwitchError> {
    let paths = MwmPaths::new(data_dir);
    fs::create_dir_all(&paths.root)?;
    let tmp = paths.root.join("active.json.tmp");
    let active = ActiveRelease {
        version: version.to_string(),
    };
    fs::write(&tmp, serde_json::to_string_pretty(&active)?)?;
    fs::rename(&tmp, paths.active_file())?;
    Ok(())
}

/// Read which release the root World overlays belong to.
pub fn read_world_release(data_dir: &Path) -> Result<Option<WorldRelease>, ReleaseSwitchError> {
    let path = world_release_path(data_dir);
    if !path.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(path)?;
    Ok(Some(serde_json::from_str(&text)?))
}

fn write_world_release(data_dir: &Path, version: &str) -> Result<(), ReleaseSwitchError> {
    let paths = MwmPaths::new(data_dir);
    fs::create_dir_all(&paths.root)?;
    let tmp = paths.root.join("world-release.json.tmp");
    let wr = WorldRelease {
        version: version.to_string(),
    };
    fs::write(&tmp, serde_json::to_string_pretty(&wr)?)?;
    fs::rename(&tmp, paths.world_release_file())?;
    Ok(())
}

/// Create staging directories for a new release download.
pub fn begin_staging(data_dir: &Path, version: &str) -> Result<PathBuf, ReleaseSwitchError> {
    let staging = staging_dir(data_dir, version);
    fs::create_dir_all(staging.join("leaves"))?;
    fs::create_dir_all(staging.join("sidecars"))?;
    Ok(staging)
}

/// Delete an incomplete staging tree (interrupted / abandoned download).
pub fn abandon_staging(data_dir: &Path, version: &str) -> Result<(), ReleaseSwitchError> {
    let staging = staging_dir(data_dir, version);
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    Ok(())
}

fn marker_complete(staging: &Path) -> PathBuf {
    staging.join(".complete")
}

/// Mark staging as fully verified and ready to activate.
pub fn mark_staging_complete(staging: &Path) -> Result<(), ReleaseSwitchError> {
    fs::write(marker_complete(staging), b"ok\n")?;
    Ok(())
}

pub fn staging_is_complete(staging: &Path) -> bool {
    marker_complete(staging).is_file()
}

fn staging_world_path(staging: &Path, leaf_id: &str) -> PathBuf {
    leaf_mwm_path(staging, leaf_id)
}

fn require_staging_world_overlays(staging: &Path, version: &str) -> Result<(), ReleaseSwitchError> {
    for id in WORLD_OVERLAY_LEAF_IDS {
        let p = staging_world_path(staging, id);
        if !p.is_file() || fs::metadata(&p)?.len() == 0 {
            return Err(ReleaseSwitchError::MissingWorldOverlay(
                version.to_string(),
                format!("missing or empty {}", p.display()),
            ));
        }
    }
    Ok(())
}

/// Promote verified World overlays from a release/staging `leaves/` tree to the
/// MWM storage root (never leave them under `{version}/` for CoMaps routing).
fn promote_world_overlays_to_root(
    data_dir: &Path,
    source_release_or_staging: &Path,
    version: &str,
) -> Result<(), ReleaseSwitchError> {
    let paths = MwmPaths::new(data_dir);
    fs::create_dir_all(&paths.root)?;

    let src_world = staging_world_path(source_release_or_staging, "World");
    let src_coasts = staging_world_path(source_release_or_staging, "WorldCoasts");
    let tmp_world = paths.root.join(format!("{WORLD_MWM_FILE}.new"));
    let tmp_coasts = paths.root.join(format!("{WORLD_COASTS_MWM_FILE}.new"));

    fs::copy(&src_world, &tmp_world)?;
    fs::copy(&src_coasts, &tmp_coasts)?;

    // Pointer first so a crash after rename still reports the new World version.
    write_world_release(data_dir, version)?;
    fs::rename(&tmp_world, paths.world_mwm())?;
    fs::rename(&tmp_coasts, paths.world_coasts_mwm())?;

    // Keep World out of the versioned leaves dir (CoMaps Joints breakage).
    let _ = fs::remove_file(&src_world);
    let _ = fs::remove_file(&src_coasts);
    Ok(())
}

/// Atomically promote a complete staging tree to `releases/{version}`, promote
/// World overlays to the MWM root, and point `active.json` at the new release.
/// On incomplete staging the previous active release + root World files stay.
pub fn activate_release(data_dir: &Path, version: &str) -> Result<PathBuf, ReleaseSwitchError> {
    // Feature flag is enforced at catalog/download entry points
    // (`fetch_current_catalog` / `download_leaf_set`), not on the pure
    // filesystem switch helper (keeps unit tests race-free).
    let staging = staging_dir(data_dir, version);
    if !staging_is_complete(&staging) {
        return Err(ReleaseSwitchError::IncompleteStaging(
            version.to_string(),
            "missing .complete marker — refuse partial activate".into(),
        ));
    }
    require_staging_world_overlays(&staging, version)?;

    let dest = release_dir(data_dir, version);
    if dest.exists() {
        fs::remove_dir_all(&dest)?;
    }
    fs::create_dir_all(dest.parent().unwrap())?;
    fs::rename(&staging, &dest)?;

    // World promotion after the leaves rename: if this fails, active.json is
    // still the previous release and root World files are untouched.
    promote_world_overlays_to_root(data_dir, &dest, version)?;
    write_active_version(data_dir, version)?;
    Ok(dest)
}

/// Add the leaves + sidecars of a complete staging tree to the **already active** release of
/// the same version, file by file (atomic renames inside one filesystem). Existing leaves are
/// never removed, so a running router / drape that has them open keeps working, and an
/// interrupted merge leaves a valid (smaller) release. World overlays in the staging tree (only
/// present when they were downloaded) are promoted to the MWM root. The staging tree is removed.
pub fn merge_staging_into_release(
    data_dir: &Path,
    version: &str,
) -> Result<PathBuf, ReleaseSwitchError> {
    let staging = staging_dir(data_dir, version);
    if !staging_is_complete(&staging) {
        return Err(ReleaseSwitchError::IncompleteStaging(
            version.to_string(),
            "missing .complete marker — refuse partial merge".into(),
        ));
    }
    let dest = release_dir(data_dir, version);
    if !dest.is_dir() {
        return Err(ReleaseSwitchError::NoActiveRelease);
    }
    let has_world = WORLD_OVERLAY_LEAF_IDS
        .iter()
        .all(|id| staging_world_path(&staging, id).is_file());
    if has_world {
        promote_world_overlays_to_root(data_dir, &staging, version)?;
    }
    for sub in ["leaves", "sidecars"] {
        let src_dir = staging.join(sub);
        if !src_dir.is_dir() {
            continue;
        }
        let dst_dir = dest.join(sub);
        fs::create_dir_all(&dst_dir)?;
        for entry in fs::read_dir(&src_dir)? {
            let entry = entry?;
            let name = entry.file_name();
            if name.to_string_lossy().ends_with(".partial") {
                continue;
            }
            fs::rename(entry.path(), dst_dir.join(&name))?;
        }
    }
    for name in ["attribution.json", "ATTRIBUTION.md"] {
        let from = staging.join(name);
        let to = dest.join(name);
        if from.is_file() && !to.exists() {
            let _ = fs::rename(&from, &to);
        }
    }
    fs::remove_dir_all(&staging)?;
    Ok(dest)
}

/// Refuse routing when active release and root World overlays disagree (or
/// World files are absent). Call before opening a CoMaps session.
pub fn assert_world_matches_active(data_dir: &Path) -> Result<ActiveRelease, ReleaseSwitchError> {
    let active = read_active_version(data_dir)?.ok_or(ReleaseSwitchError::NoActiveRelease)?;
    let world = read_world_release(data_dir)?.ok_or(ReleaseSwitchError::WorldOverlaysAbsent)?;
    if active.version != world.version {
        return Err(ReleaseSwitchError::WorldVersionMismatch {
            active: active.version,
            world: world.version,
        });
    }
    if !world_mwm_path(data_dir).is_file() || !world_coasts_mwm_path(data_dir).is_file() {
        return Err(ReleaseSwitchError::WorldOverlaysAbsent);
    }
    Ok(active)
}

/// Remove release trees other than `keep_version` (and any staging).
pub fn cleanup_old_releases(
    data_dir: &Path,
    keep_version: &str,
) -> Result<Vec<String>, ReleaseSwitchError> {
    let paths = MwmPaths::new(data_dir);
    let mut removed = Vec::new();
    let releases = paths.releases();
    if releases.is_dir() {
        for entry in fs::read_dir(&releases)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name != keep_version && entry.path().is_dir() {
                fs::remove_dir_all(entry.path())?;
                removed.push(name);
            }
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_staging_with_world(staging: &Path, leaf_bytes: &[u8], world_tag: &[u8]) {
        fs::write(staging.join("leaves").join("A.mwm"), leaf_bytes).unwrap();
        fs::write(staging_world_path(staging, "World"), world_tag).unwrap();
        fs::write(
            staging_world_path(staging, "WorldCoasts"),
            format!("{:?}-coasts", world_tag).as_bytes(),
        )
        .unwrap();
    }

    #[test]
    fn switch_atomic_and_interrupted_staging() {
        let dir = tempdir().unwrap();
        let data = dir.path();

        // First release.
        let s1 = begin_staging(data, "261105").unwrap();
        write_staging_with_world(&s1, b"a", b"world-261105");
        mark_staging_complete(&s1).unwrap();
        activate_release(data, "261105").unwrap();
        assert_eq!(
            read_active_version(data).unwrap().unwrap().version,
            "261105"
        );
        assert_eq!(read_world_release(data).unwrap().unwrap().version, "261105");
        assert_eq!(
            fs::read(world_mwm_path(data)).unwrap(),
            b"world-261105".as_slice()
        );
        assert!(!release_dir(data, "261105")
            .join("leaves")
            .join("World.mwm")
            .exists());
        assert_world_matches_active(data).unwrap();

        // Interrupted new release: no .complete → activate refused; old stays.
        let s2 = begin_staging(data, "261112").unwrap();
        write_staging_with_world(&s2, b"partial", b"world-partial");
        let err = activate_release(data, "261112").unwrap_err();
        assert!(matches!(err, ReleaseSwitchError::IncompleteStaging(_, _)));
        assert_eq!(
            read_active_version(data).unwrap().unwrap().version,
            "261105"
        );
        assert_eq!(
            fs::read(world_mwm_path(data)).unwrap(),
            b"world-261105".as_slice()
        );
        abandon_staging(data, "261112").unwrap();
        assert!(!staging_dir(data, "261112").exists());

        // Complete switch 261105 → fake newer 261112 + cleanup.
        let s3 = begin_staging(data, "261112").unwrap();
        write_staging_with_world(&s3, b"b", b"world-261112");
        mark_staging_complete(&s3).unwrap();
        activate_release(data, "261112").unwrap();
        let removed = cleanup_old_releases(data, "261112").unwrap();
        assert!(removed.contains(&"261105".to_string()));
        assert!(!release_dir(data, "261105").exists());
        assert!(release_dir(data, "261112")
            .join("leaves")
            .join("A.mwm")
            .is_file());
        assert_eq!(
            fs::read(world_mwm_path(data)).unwrap(),
            b"world-261112".as_slice()
        );
        assert_eq!(read_world_release(data).unwrap().unwrap().version, "261112");
        assert_world_matches_active(data).unwrap();
    }

    #[test]
    fn activate_refuses_missing_world_overlays() {
        let dir = tempdir().unwrap();
        let data = dir.path();
        let s = begin_staging(data, "261105").unwrap();
        fs::write(s.join("leaves").join("A.mwm"), b"a").unwrap();
        mark_staging_complete(&s).unwrap();
        let err = activate_release(data, "261105").unwrap_err();
        assert!(matches!(err, ReleaseSwitchError::MissingWorldOverlay(_, _)));
        assert!(read_active_version(data).unwrap().is_none());
    }

    #[test]
    fn world_version_mismatch_detected() {
        let dir = tempdir().unwrap();
        let data = dir.path();
        let s = begin_staging(data, "261105").unwrap();
        write_staging_with_world(&s, b"a", b"world-261105");
        mark_staging_complete(&s).unwrap();
        activate_release(data, "261105").unwrap();

        // Simulate operator corruption: active bumped without World promote.
        write_active_version(data, "261199").unwrap();
        let err = assert_world_matches_active(data).unwrap_err();
        assert!(matches!(
            err,
            ReleaseSwitchError::WorldVersionMismatch { .. }
        ));
    }

    #[test]
    fn merge_adds_leaves_and_keeps_the_active_release() {
        let dir = tempdir().unwrap();
        let data = dir.path();
        let s1 = begin_staging(data, "261105").unwrap();
        write_staging_with_world(&s1, b"a", b"world-261105");
        fs::write(s1.join("sidecars").join("A.navi-sidecar.v1.zst"), b"sa").unwrap();
        mark_staging_complete(&s1).unwrap();
        activate_release(data, "261105").unwrap();
        let world_before = fs::read(world_mwm_path(data)).unwrap();

        // Incremental install of leaf B into the same release: no World files in staging.
        let s2 = begin_staging(data, "261105").unwrap();
        fs::write(s2.join("leaves").join("B.mwm"), b"bb").unwrap();
        fs::write(s2.join("sidecars").join("B.navi-sidecar.v1.zst"), b"sb").unwrap();
        mark_staging_complete(&s2).unwrap();
        let dest = merge_staging_into_release(data, "261105").unwrap();

        assert_eq!(fs::read(dest.join("leaves/A.mwm")).unwrap(), b"a");
        assert_eq!(fs::read(dest.join("leaves/B.mwm")).unwrap(), b"bb");
        assert_eq!(
            fs::read(dest.join("sidecars/B.navi-sidecar.v1.zst")).unwrap(),
            b"sb"
        );
        assert!(!s2.exists(), "staging removed after the merge");
        assert_eq!(fs::read(world_mwm_path(data)).unwrap(), world_before);
        assert_eq!(
            read_active_version(data).unwrap().unwrap().version,
            "261105"
        );
    }

    #[test]
    fn merge_refuses_incomplete_staging_and_missing_release() {
        let dir = tempdir().unwrap();
        let data = dir.path();
        let s = begin_staging(data, "261105").unwrap();
        fs::write(s.join("leaves").join("B.mwm"), b"b").unwrap();
        assert!(matches!(
            merge_staging_into_release(data, "261105"),
            Err(ReleaseSwitchError::IncompleteStaging(_, _))
        ));
        mark_staging_complete(&s).unwrap();
        assert!(matches!(
            merge_staging_into_release(data, "261105"),
            Err(ReleaseSwitchError::NoActiveRelease)
        ));
    }
}
