//! Atomic archive publish: write `.partial`, fsync, rename.
//!
//! Never mmap a `.partial` file. Cancel/fail must delete the partial and leave
//! any previous good archive untouched.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use super::header::{read_preamble, write_preamble, Preamble, PREAMBLE_LEN};

pub fn partial_path(final_path: &Path) -> PathBuf {
    let mut s = final_path.as_os_str().to_os_string();
    s.push(".partial");
    PathBuf::from(s)
}

/// Write `preamble || payload` to `final_path` via temp+fsync+rename.
pub fn write_archive_atomic(
    final_path: &Path,
    preamble: Preamble,
    payload: &[u8],
) -> anyhow::Result<()> {
    if let Some(parent) = final_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let partial = partial_path(final_path);
    // Orphan cleanup from a prior crash.
    let _ = fs::remove_file(&partial);

    {
        let mut f = File::create(&partial)?;
        write_preamble(&mut f, preamble)?;
        f.write_all(payload)?;
        f.flush()?;
        f.sync_all()?;
    }

    // Best-effort directory fsync (may no-op on some Android FS).
    if let Some(parent) = final_path.parent() {
        if let Ok(dir) = File::open(parent) {
            let _ = dir.sync_all();
        }
    }

    fs::rename(&partial, final_path)?;
    Ok(())
}

/// Delete a leftover `.partial` next to `final_path` if present.
pub fn discard_partial(final_path: &Path) {
    let _ = fs::remove_file(partial_path(final_path));
}

/// True when `path` is a complete archive with this magic + format version.
pub fn archive_matches_preamble(path: &Path, magic: u32, format_version: u32) -> bool {
    let Ok(mut f) = File::open(path) else {
        return false;
    };
    let Ok(p) = read_preamble(&mut f) else {
        return false;
    };
    p.magic == magic && p.format_version == format_version
}

/// Byte offset of the rkyv payload after the fixed preamble.
#[must_use]
pub const fn archive_payload_offset() -> usize {
    PREAMBLE_LEN
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::indexed::header::Preamble;

    #[test]
    fn atomic_write_no_partial_left() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("x.navi-graph-car.rkyv");
        write_archive_atomic(&dest, Preamble::new(1, 1), b"payload").unwrap();
        assert!(dest.is_file());
        assert!(!partial_path(&dest).exists());
        let bytes = fs::read(&dest).unwrap();
        assert_eq!(&bytes[8..], b"payload");
        assert!(archive_matches_preamble(&dest, 1, 1));
        assert!(!archive_matches_preamble(&dest, 1, 99));
        assert!(!archive_matches_preamble(&dir.path().join("missing"), 1, 1));
    }
}
