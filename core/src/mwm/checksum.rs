//! Size + digest helpers for MWM (SHA-1 base64) and sidecars (SHA-256 hex).

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use base64::Engine;
use sha1::{Digest as Sha1Digest, Sha1};
use sha2::{Digest as Sha2Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ChecksumError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("size mismatch: got {got}, expected {expected}")]
    SizeMismatch { got: u64, expected: u64 },
    #[error("sha256 mismatch: got {got}, expected {expected}")]
    Sha256Mismatch { got: String, expected: String },
    #[error("sha1 mismatch: got {got}, expected {expected}")]
    Sha1Mismatch { got: String, expected: String },
}

pub fn sha256_hex_of_bytes(bytes: &[u8]) -> String {
    let mut h = <Sha256 as Sha2Digest>::new();
    Sha2Digest::update(&mut h, bytes);
    hex::encode(Sha2Digest::finalize(h))
}

pub fn sha256_hex_of_path(path: &Path) -> Result<String, ChecksumError> {
    let mut file = BufReader::new(File::open(path)?);
    let mut h = <Sha256 as Sha2Digest>::new();
    let mut buf = [0u8; 1024 * 64];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        Sha2Digest::update(&mut h, &buf[..n]);
    }
    Ok(hex::encode(Sha2Digest::finalize(h)))
}

pub fn sha1_base64_of_bytes(bytes: &[u8]) -> String {
    let mut h = <Sha1 as Sha1Digest>::new();
    Sha1Digest::update(&mut h, bytes);
    base64::engine::general_purpose::STANDARD.encode(Sha1Digest::finalize(h))
}

pub fn sha1_base64_of_path(path: &Path) -> Result<String, ChecksumError> {
    let mut file = BufReader::new(File::open(path)?);
    let mut h = <Sha1 as Sha1Digest>::new();
    let mut buf = [0u8; 1024 * 64];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        Sha1Digest::update(&mut h, &buf[..n]);
    }
    Ok(base64::engine::general_purpose::STANDARD.encode(Sha1Digest::finalize(h)))
}

pub fn verify_bytes_sha256(bytes: &[u8], expected_sha256: &str) -> Result<(), ChecksumError> {
    let got = sha256_hex_of_bytes(bytes);
    if !got.eq_ignore_ascii_case(expected_sha256.trim()) {
        return Err(ChecksumError::Sha256Mismatch {
            got,
            expected: expected_sha256.to_string(),
        });
    }
    Ok(())
}

pub fn verify_path_sha256(
    path: &Path,
    expected_bytes: Option<u64>,
    expected_sha256: &str,
) -> Result<(), ChecksumError> {
    let meta = std::fs::metadata(path)?;
    let got_size = meta.len();
    if let Some(expected) = expected_bytes {
        if got_size != expected {
            return Err(ChecksumError::SizeMismatch {
                got: got_size,
                expected,
            });
        }
    }
    let got = sha256_hex_of_path(path)?;
    if !got.eq_ignore_ascii_case(expected_sha256.trim()) {
        return Err(ChecksumError::Sha256Mismatch {
            got,
            expected: expected_sha256.to_string(),
        });
    }
    Ok(())
}

pub fn verify_path_sha1_base64(
    path: &Path,
    expected_bytes: Option<u64>,
    expected_sha1_base64: &str,
) -> Result<(), ChecksumError> {
    let meta = std::fs::metadata(path)?;
    let got_size = meta.len();
    if let Some(expected) = expected_bytes {
        if got_size != expected {
            return Err(ChecksumError::SizeMismatch {
                got: got_size,
                expected,
            });
        }
    }
    let got = sha1_base64_of_path(path)?;
    if got != expected_sha1_base64.trim() {
        return Err(ChecksumError::Sha1Mismatch {
            got,
            expected: expected_sha1_base64.to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn sha256_and_sha1_roundtrip() {
        let bytes = b"navi-mwm-checksum";
        let sha = sha256_hex_of_bytes(bytes);
        verify_bytes_sha256(bytes, &sha).unwrap();
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(bytes).unwrap();
        verify_path_sha256(f.path(), Some(bytes.len() as u64), &sha).unwrap();
        let s1 = sha1_base64_of_bytes(bytes);
        verify_path_sha1_base64(f.path(), Some(bytes.len() as u64), &s1).unwrap();
    }

    #[test]
    fn detects_tampered_sha256() {
        let err = verify_bytes_sha256(b"a", &sha256_hex_of_bytes(b"b")).unwrap_err();
        assert!(matches!(err, ChecksumError::Sha256Mismatch { .. }));
    }
}
