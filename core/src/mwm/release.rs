//! Atomic release switching: never mix CoMaps release versions.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::storage::{active_path, release_dir, staging_dir, MwmPaths};

#[derive(Debug, Error)]
pub enum ReleaseSwitchError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("staging release {0} is incomplete: {1}")]
    IncompleteStaging(String, String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActiveRelease {
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

/// Atomically promote a complete staging tree to `releases/{version}` and
/// point `active.json` at it. Leaves the previous release on disk for cleanup.
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
    let dest = release_dir(data_dir, version);
    if dest.exists() {
        fs::remove_dir_all(&dest)?;
    }
    fs::create_dir_all(dest.parent().unwrap())?;
    fs::rename(&staging, &dest)?;
    write_active_version(data_dir, version)?;
    Ok(dest)
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

    #[test]
    fn switch_atomic_and_interrupted_staging() {
        let dir = tempdir().unwrap();
        let data = dir.path();

        // First release.
        let s1 = begin_staging(data, "261105").unwrap();
        fs::write(s1.join("leaves").join("A.mwm"), b"a").unwrap();
        mark_staging_complete(&s1).unwrap();
        activate_release(data, "261105").unwrap();
        assert_eq!(
            read_active_version(data).unwrap().unwrap().version,
            "261105"
        );

        // Interrupted new release: no .complete → activate refused; old stays.
        let s2 = begin_staging(data, "261112").unwrap();
        fs::write(s2.join("leaves").join("A.mwm"), b"partial").unwrap();
        let err = activate_release(data, "261112").unwrap_err();
        assert!(matches!(err, ReleaseSwitchError::IncompleteStaging(_, _)));
        assert_eq!(
            read_active_version(data).unwrap().unwrap().version,
            "261105"
        );
        abandon_staging(data, "261112").unwrap();
        assert!(!staging_dir(data, "261112").exists());

        // Complete switch + cleanup.
        let s3 = begin_staging(data, "261112").unwrap();
        fs::write(s3.join("leaves").join("A.mwm"), b"b").unwrap();
        mark_staging_complete(&s3).unwrap();
        activate_release(data, "261112").unwrap();
        let removed = cleanup_old_releases(data, "261112").unwrap();
        assert!(removed.contains(&"261105".to_string()));
        assert!(!release_dir(data, "261105").exists());
        assert!(release_dir(data, "261112")
            .join("leaves")
            .join("A.mwm")
            .is_file());
    }
}
