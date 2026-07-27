//! Disk free-space helpers for download diagnostics.

use std::path::Path;

/// Available bytes on the filesystem that contains `path` (or its parent).
pub fn available_bytes(path: &Path) -> Option<u64> {
    let probe = if path.exists() {
        path
    } else {
        path.parent().unwrap_or(path)
    };
    let c_path = std::ffi::CString::new(probe.to_string_lossy().as_bytes()).ok()?;
    unsafe {
        let mut s: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c_path.as_ptr(), &mut s) != 0 {
            return None;
        }
        Some(s.f_bavail * s.f_frsize)
    }
}

/// Enrich an I/O error with free-space context when it looks like ENOSPC.
pub fn enrich_io_error(err: std::io::Error, path: &Path) -> anyhow::Error {
    let avail = available_bytes(path);
    if err.raw_os_error() == Some(libc::ENOSPC) || err.kind() == std::io::ErrorKind::StorageFull {
        anyhow::anyhow!(
            "insufficient space writing {}: {} (available_bytes={:?})",
            path.display(),
            err,
            avail
        )
    } else {
        anyhow::anyhow!(
            "I/O on {}: {} (available_bytes={:?})",
            path.display(),
            err,
            avail
        )
    }
}
