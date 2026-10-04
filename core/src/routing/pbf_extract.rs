//! Gate OSM PBF files used as graph-build input (not pack handles).
//!
//! Pack-server installs leave a 16 KiB zero `{stem}-latest.osm.pbf` beside Ready
//! graph packs. Those paths may still be passed into planning as a region handle,
//! but must never be parsed with osmpbf for cold graph build.
//!
//! Size alone is not enough: checked-in corridor extracts can be a few hundred
//! KiB (below Geofabrik's 1 MiB floor) while still carrying a real OSM PBF
//! BlobHeader. Stubs stay rejected by the 16 KiB cap plus a prefix/header check.

use std::fs::File;
use std::io::Read;
use std::path::Path;

pub use crate::pack_server::MIN_REAL_PBF_BYTES;

/// Pack-server `write_stub_pbf` size. Files this small are never graph input.
const PACK_STUB_BYTES: u64 = 16 * 1024;

/// True when `path` is a usable OSM extract for graph build (not a pack stub).
pub fn pbf_is_real_extract(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    let Ok(meta) = path.metadata() else {
        return false;
    };
    let len = meta.len();
    if len == 0 || len <= PACK_STUB_BYTES {
        return false;
    }
    if pbf_has_osm_blob_header(path) {
        return true;
    }
    // Large Geofabrik-class files: keep the existing 1 MiB floor even if the
    // first blob header cannot be read (partial copy still bigger than a stub).
    len >= MIN_REAL_PBF_BYTES
}

/// OSM PBF starts with a network-endian BlobHeader length, then protobuf
/// `BlobHeader.type` of `OSMHeader` or `OSMData`.
fn pbf_has_osm_blob_header(path: &Path) -> bool {
    let mut f = match File::open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let mut len_buf = [0u8; 4];
    if f.read_exact(&mut len_buf).is_err() {
        return false;
    }
    let header_len = u32::from_be_bytes(len_buf) as usize;
    // Spec: BlobHeader is at most 64 KiB; zero is the all-zero stub prefix.
    if header_len == 0 || header_len > 64 * 1024 {
        return false;
    }
    let mut header = vec![0u8; header_len];
    if f.read_exact(&mut header).is_err() {
        return false;
    }
    contains_subslice(&header, b"OSMHeader") || contains_subslice(&header, b"OSMData")
}

fn contains_subslice(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
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
        "OSM PBF is not a real extract (pack-server stub or not a usable OSM PBF at {len} bytes): {}",
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

    fn osm_blob_header_prefix() -> Vec<u8> {
        // BlobHeader protobuf: field 1 type="OSMHeader", field 3 datasize=0.
        let header = b"\x0a\x09OSMHeader\x18\x00";
        let mut out = (header.len() as u32).to_be_bytes().to_vec();
        out.extend_from_slice(header);
        out
    }

    #[test]
    fn stub_sized_file_is_never_real_even_with_blob_header() {
        let dir = tempdir().expect("tempdir");
        let stub = dir.path().join("ostlandet-latest.osm.pbf");
        let mut body = osm_blob_header_prefix();
        body.resize(16 * 1024, 0);
        std::fs::write(&stub, &body).expect("write");
        assert!(!pbf_is_real_extract(&stub));
    }

    #[test]
    fn sixteen_kib_nonzero_garbage_is_not_real_extract() {
        let dir = tempdir().expect("tempdir");
        let stub = dir.path().join("ostlandet-latest.osm.pbf");
        std::fs::write(&stub, vec![0xAAu8; 16 * 1024]).expect("write");
        assert!(!pbf_is_real_extract(&stub));
    }

    #[test]
    fn midsize_zeros_without_blob_header_are_not_real() {
        let dir = tempdir().expect("tempdir");
        let pbf = dir.path().join("fake.osm.pbf");
        std::fs::write(&pbf, vec![0u8; 196_000]).expect("write");
        assert!(!pbf_is_real_extract(&pbf));
    }

    #[test]
    fn small_file_with_osm_blob_header_is_real_extract() {
        let dir = tempdir().expect("tempdir");
        let pbf = dir.path().join("tiny-corridor.osm.pbf");
        let mut body = osm_blob_header_prefix();
        body.resize(196_000, 0x20);
        std::fs::write(&pbf, &body).expect("write");
        assert!(pbf_is_real_extract(&pbf));
        ensure_pbf_usable_for_graph_build(&pbf).expect("ok");
    }

    #[test]
    fn stai_bru_fixture_is_real_extract() {
        let pbf = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/stai-bru-limits.osm.pbf");
        assert!(pbf.is_file(), "missing {}", pbf.display());
        let len = pbf.metadata().expect("meta").len();
        assert!(len < MIN_REAL_PBF_BYTES, "fixture unexpectedly huge: {len}");
        assert!(
            len > PACK_STUB_BYTES,
            "fixture smaller than pack stub: {len}"
        );
        assert!(pbf_is_real_extract(&pbf));
        ensure_pbf_usable_for_graph_build(&pbf).expect("ok");
    }
}
