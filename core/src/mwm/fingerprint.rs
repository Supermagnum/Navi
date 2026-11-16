//! MWM fingerprint rule: never apply a sidecar when local MWM mismatches.

use std::path::Path;

use thiserror::Error;

use super::catalog::{CatalogLeaf, MwmFingerprint};
use super::checksum::{sha256_hex_of_path, ChecksumError};

/// User-facing warning when Navi layers are skipped for a leaf.
pub const FINGERPRINT_USER_WARNING: &str = "Navi layers unavailable — map build mismatch";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FingerprintStatus {
    Match,
    Mismatch(FingerprintMismatch),
    /// Leaf is World / WorldCoasts (or `no_sidecar`); no sidecar expected.
    NoSidecar,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FingerprintMismatch {
    pub leaf_id: String,
    pub local_size: u64,
    pub expected_size: u64,
    pub local_sha256: String,
    pub expected_sha256: String,
}

impl FingerprintMismatch {
    pub fn user_warning(&self) -> &'static str {
        FINGERPRINT_USER_WARNING
    }
}

#[derive(Debug, Error)]
pub enum FingerprintError {
    #[error("checksum: {0}")]
    Checksum(#[from] ChecksumError),
    #[error("catalog leaf missing mwm fingerprint fields")]
    MissingFingerprint,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

pub fn fingerprint_from_catalog_leaf(leaf: &CatalogLeaf) -> Option<MwmFingerprint> {
    leaf.fingerprint()
}

/// Compare local MWM file to catalog (or sidecar-header) fingerprint.
///
/// On mismatch: callers must discard/ignore the sidecar for that leaf and
/// route without Navi wetland/elevation layers, surfacing
/// [`FINGERPRINT_USER_WARNING`].
pub fn check_local_mwm_fingerprint(
    leaf_id: &str,
    local_mwm: &Path,
    expected: &MwmFingerprint,
) -> Result<FingerprintStatus, FingerprintError> {
    let meta = std::fs::metadata(local_mwm)?;
    let local_size = meta.len();
    let local_sha256 = sha256_hex_of_path(local_mwm)?;
    if local_size == expected.size && local_sha256.eq_ignore_ascii_case(&expected.sha256) {
        Ok(FingerprintStatus::Match)
    } else {
        Ok(FingerprintStatus::Mismatch(FingerprintMismatch {
            leaf_id: leaf_id.to_string(),
            local_size,
            expected_size: expected.size,
            local_sha256,
            expected_sha256: expected.sha256.clone(),
        }))
    }
}

/// Convenience: `no_sidecar` leaves always return [`FingerprintStatus::NoSidecar`].
pub fn check_catalog_leaf_against_local(
    leaf: &CatalogLeaf,
    local_mwm: &Path,
) -> Result<FingerprintStatus, FingerprintError> {
    if leaf.no_sidecar {
        return Ok(FingerprintStatus::NoSidecar);
    }
    let fp = fingerprint_from_catalog_leaf(leaf).ok_or(FingerprintError::MissingFingerprint)?;
    check_local_mwm_fingerprint(&leaf.id, local_mwm, &fp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn match_and_mismatch() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"hello-mwm").unwrap();
        let sha = sha256_hex_of_path(f.path()).unwrap();
        let fp = MwmFingerprint {
            size: 9,
            sha256: sha.clone(),
        };
        assert_eq!(
            check_local_mwm_fingerprint("Test", f.path(), &fp).unwrap(),
            FingerprintStatus::Match
        );
        let bad = MwmFingerprint {
            size: 9,
            sha256: "0".repeat(64),
        };
        let st = check_local_mwm_fingerprint("Test", f.path(), &bad).unwrap();
        match st {
            FingerprintStatus::Mismatch(m) => {
                assert_eq!(m.user_warning(), FINGERPRINT_USER_WARNING);
                assert_eq!(m.local_sha256, sha);
            }
            other => panic!("expected mismatch, got {other:?}"),
        }
    }
}
