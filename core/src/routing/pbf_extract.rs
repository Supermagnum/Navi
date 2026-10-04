//! Size gate for OSM PBF files used as graph-build input (not pack handles).
//!
//! Pack-server installs leave a 16 KiB zero `{stem}-latest.osm.pbf` beside Ready
//! graph packs. Those paths may still be passed into planning as a region handle,
//! but must never be parsed with osmpbf for cold graph build.

use std::fs::File;
use std::io::Read;
use std::path::Path;

pub use crate::pack_server::MIN_REAL_PBF_BYTES;

/// True when `path` is large enough to be a real Geofabrik extract (not a pack stub).
pub fn pbf_is_real_extract(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    let Ok(meta) = path.metadata() else {
        return false;
    };
    let len = meta.len();
    if len >= MIN_REAL_PBF_BYTES {
        return true;
    }
    // Reject known pack-server zero stubs even if the threshold constant changes.
    if len > 0 && len <= 64 * 1024 {
        return !pbf_prefix_all_zero(path, len).unwrap_or(true);
    }
    false
}

fn pbf_prefix_all_zero(path: &Path, len: u64) -> std::io::Result<bool> {
    let mut f = File::open(path)?;
    let mut buf = vec![0u8; len.min(4096) as usize];
    let n = f.read(&mut buf)?;
    Ok(n > 0 && buf[..n].iter().all(|&b| b == 0))
}

/// Refuse cold graph build / osmpbf parse on pack-server stubs and tiny placeholders.
pub fn ensure_pbf_usable_for_graph_build(path: &Path) -> anyhow::Result<()> {
    if !path.is_file() {
        anyhow::bail!("OSM PBF missing: {}", path.display());
    }
    if pbf_is_real_extract(path) {
        return Ok(());
    }
    let len = path.metadata().map(|m| m.len()).unwrap_or(0);
    anyhow::bail!(
        "OSM PBF is not a real extract (pack-server stub or too small at {len} bytes; \
         need >= {MIN_REAL_PBF_BYTES}): {}",
        path.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn pack_server_zero_stub_is_not_real_extract() {
        let dir = tempdir().expect("tempdir");
        let stub = dir.path().join("ostlandet-latest.osm.pbf");
        std::fs::write(&stub, vec![0u8; 16 * 1024]).expect("stub");
        assert!(!pbf_is_real_extract(&stub));
        let err = ensure_pbf_usable_for_graph_build(&stub).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("not a real extract"), "{msg}");
        assert!(!msg.contains("BlobHeader"), "{msg}");
    }

    #[test]
    fn large_file_counts_as_real_extract() {
        let dir = tempdir().expect("tempdir");
        let pbf = dir.path().join("norway-latest.osm.pbf");
        let mut f = std::fs::File::create(&pbf).expect("create");
        f.write_all(&vec![1u8; MIN_REAL_PBF_BYTES as usize])
            .expect("write");
        assert!(pbf_is_real_extract(&pbf));
        ensure_pbf_usable_for_graph_build(&pbf).expect("ok");
    }
}
