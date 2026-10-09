//! On-device place index from the pack server's optional place-source PBF.
//!
//! Find the file by the manifest key suffix [`PLACE_SOURCE_SUFFIX`]. Never
//! invent the name from the app stem. The client builds the index; the file
//! is deleted after the intact check. Absent key → existing Geofabrik path.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::acquisition::normalize_region_id;
use super::fetch::ServerInstallStamp;
use super::place_index_after::{
    build_place_index_from_pbf, ensure_geofabrik_pbf_for_region, PackPlaceIndexReport,
    PLACE_INDEX_DB_NAME,
};
use super::{http_get_text, PackServerError};
use crate::download::progress as download_progress;
use crate::search::NameIndex;

/// Manifest key / filename suffix. Look up by this ending; do not construct.
pub const PLACE_SOURCE_SUFFIX: &str = ".navi-place-source.osm.pbf";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceSourceRef {
    pub filename: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceIndexSource {
    PlaceSource,
    OwnExtract,
    None,
}

impl PlaceIndexSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PlaceSource => "place-source",
            Self::OwnExtract => "own-extract",
            Self::None => "none",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlaceIndexEnsureReport {
    pub region_id: String,
    pub source: PlaceIndexSource,
    pub action: String,
    pub indexed: usize,
    pub cache_hit: bool,
    pub sha256: String,
    pub download_bytes: u64,
    pub index_ms: f64,
    pub reason: String,
    pub file_deleted: bool,
}

impl PlaceIndexEnsureReport {
    pub fn to_report_string(&self) -> String {
        if self.action == "missing" || self.action == "rejected" {
            return format!(
                "FAIL\nregion_id={}\nsource={}\naction={}\nreason={}\n",
                self.region_id,
                self.source.as_str(),
                self.action,
                self.reason
            );
        }
        format!(
            "PASS\n\
             region_id={}\n\
             source={}\n\
             action={}\n\
             indexed={}\n\
             cache_hit={}\n\
             sha256={}\n\
             download_bytes={}\n\
             index_ms={:.1}\n\
             file_deleted={}\n\
             reason={}\n",
            self.region_id,
            self.source.as_str(),
            self.action,
            self.indexed,
            self.cache_hit,
            self.sha256,
            self.download_bytes,
            self.index_ms,
            self.file_deleted,
            self.reason
        )
    }
}

#[derive(Debug, Deserialize)]
struct ManifestFiles {
    #[serde(default)]
    files: BTreeMap<String, ManifestFileMeta>,
}

#[derive(Debug, Deserialize)]
struct ManifestFileMeta {
    sha256: String,
    #[serde(default)]
    bytes: Option<u64>,
}

/// First manifest `files` key that ends with [`PLACE_SOURCE_SUFFIX`].
pub fn place_source_from_manifest_json(body: &str) -> Result<Option<PlaceSourceRef>, String> {
    let man: ManifestFiles =
        serde_json::from_str(body).map_err(|e| format!("manifest.json parse: {e}"))?;
    Ok(place_source_from_files(&man.files))
}

fn place_source_from_files(files: &BTreeMap<String, ManifestFileMeta>) -> Option<PlaceSourceRef> {
    for (name, meta) in files {
        if name.ends_with(PLACE_SOURCE_SUFFIX) {
            let bytes = meta.bytes.unwrap_or(0);
            return Some(PlaceSourceRef {
                filename: name.clone(),
                sha256: meta.sha256.trim().to_string(),
                bytes,
            });
        }
    }
    None
}

fn file_sha256_hex(path: &Path) -> Result<String, String> {
    let mut f = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 1024 * 256];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Verify a complete file. A `.partial` path is never accepted.
pub fn verify_place_source_file(path: &Path, expect: &PlaceSourceRef) -> Result<(), String> {
    if path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("partial"))
        || path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(".partial"))
    {
        return Err("partial file is not indexed".into());
    }
    if !path.is_file() {
        return Err(format!("place-source missing: {}", path.display()));
    }
    let len = path.metadata().map(|m| m.len()).unwrap_or(0);
    if expect.bytes > 0 && len != expect.bytes {
        return Err(format!(
            "place-source size mismatch: got {len}, expect {}",
            expect.bytes
        ));
    }
    if len == 0 {
        return Err("place-source empty".into());
    }
    let got = file_sha256_hex(path)?;
    if !got.eq_ignore_ascii_case(expect.sha256.trim()) {
        return Err(format!(
            "place-source sha256 mismatch: got {got}, expect {}",
            expect.sha256
        ));
    }
    Ok(())
}

fn fetch_manifest_json(pack_dir: &Path, region_id: &str) -> Result<String, String> {
    let stem = super::acquisition::leaf_stem_for_region_id(region_id);
    let stamp = ServerInstallStamp::load_for_leaf(pack_dir, &stem)
        .map_err(|e| format!("server install stamp: {e}"))?;
    let generation = stamp
        .generation
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "server install stamp missing generation".to_string())?;
    let base = stamp.base_url.trim().trim_end_matches('/');
    let url = format!("{base}/packs/{region_id}/{generation}/manifest.json");
    http_get_text(&url, std::time::Duration::from_secs(60)).map_err(|e| match e {
        PackServerError::Http(code) => format!("manifest HTTP {code}"),
        PackServerError::Timeout => "manifest timeout".into(),
        PackServerError::Other(s) => s,
    })
}

fn download_place_source(
    pack_dir: &Path,
    region_id: &str,
    listed: &PlaceSourceRef,
    allow_network: bool,
) -> Result<PathBuf, String> {
    let dest = pack_dir.join(&listed.filename);
    if dest.is_file() {
        match verify_place_source_file(&dest, listed) {
            Ok(()) => return Ok(dest),
            Err(e) => {
                let _ = fs::remove_file(&dest);
                if !allow_network {
                    return Err(e);
                }
            }
        }
    }
    let partial = pack_dir.join(format!("{}.partial", listed.filename));
    if !allow_network {
        return Err("place-source not on disk and network disabled".into());
    }
    let stem = super::acquisition::leaf_stem_for_region_id(region_id);
    let stamp = ServerInstallStamp::load_for_leaf(pack_dir, &stem)
        .map_err(|e| format!("server install stamp: {e}"))?;
    let generation = stamp
        .generation
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "server install stamp missing generation".to_string())?;
    let base = stamp.base_url.trim().trim_end_matches('/');
    let url = format!("{base}/packs/{region_id}/{generation}/{}", listed.filename);
    download_progress::set(
        0,
        Some(listed.bytes.max(1)),
        &format!("Downloading place-source for {region_id}…"),
    );
    let n = crate::routing::region::download_file(&url, &partial)
        .map_err(|e| format!("place-source download: {e:#}"))?;
    if listed.bytes > 0 && n != listed.bytes {
        let _ = fs::remove_file(&partial);
        return Err(format!(
            "place-source size mismatch: got {n}, expect {}",
            listed.bytes
        ));
    }
    match verify_place_source_file(&partial, listed) {
        Ok(()) => {
            // verify rejects .partial — check sha on partial then rename.
            Err("internal: partial path rejected by verify".into())
        }
        Err(e) if e.contains("partial file") => {
            let got = file_sha256_hex(&partial)?;
            if listed.bytes > 0 {
                let len = partial.metadata().map(|m| m.len()).unwrap_or(0);
                if len != listed.bytes {
                    let _ = fs::remove_file(&partial);
                    return Err(format!(
                        "place-source size mismatch: got {len}, expect {}",
                        listed.bytes
                    ));
                }
            }
            if !got.eq_ignore_ascii_case(listed.sha256.trim()) {
                let _ = fs::remove_file(&partial);
                return Err(format!(
                    "place-source sha256 mismatch: got {got}, expect {}",
                    listed.sha256
                ));
            }
            fs::rename(&partial, &dest).map_err(|e| {
                let _ = fs::remove_file(&partial);
                format!("promote place-source: {e}")
            })?;
            verify_place_source_file(&dest, listed)?;
            Ok(dest)
        }
        Err(e) => {
            let _ = fs::remove_file(&partial);
            Err(e)
        }
    }
}

fn fail(
    region_id: &str,
    source: PlaceIndexSource,
    action: &str,
    reason: String,
) -> PlaceIndexEnsureReport {
    PlaceIndexEnsureReport {
        region_id: region_id.to_string(),
        source,
        action: action.into(),
        indexed: 0,
        cache_hit: false,
        sha256: String::new(),
        download_bytes: 0,
        index_ms: 0.0,
        reason,
        file_deleted: false,
    }
}

fn build_from_place_source(
    region_id: &str,
    pbf: &Path,
    index_db: &Path,
    listed: &PlaceSourceRef,
    force: bool,
) -> Result<PlaceIndexEnsureReport, String> {
    verify_place_source_file(pbf, listed)?;
    download_progress::set(
        0,
        Some(6),
        &format!("Place index: {region_id} from place-source…"),
    );
    let t0 = Instant::now();
    let (indexed, cache_hit, index_ms) =
        build_place_index_from_pbf(pbf, index_db, region_id, force)?;
    if !NameIndex::region_index_intact(index_db, region_id) {
        return Err("place index intact check failed after place-source build".into());
    }
    NameIndex::set_place_source_sha256(index_db, region_id, &listed.sha256)?;
    NameIndex::set_place_index_source(index_db, region_id, "place-source")?;
    let _ = fs::remove_file(pbf);
    Ok(PlaceIndexEnsureReport {
        region_id: region_id.to_string(),
        source: PlaceIndexSource::PlaceSource,
        action: if cache_hit {
            "cache_hit".into()
        } else {
            "built".into()
        },
        indexed,
        cache_hit,
        sha256: listed.sha256.clone(),
        download_bytes: listed.bytes,
        index_ms: if index_ms > 0.0 {
            index_ms
        } else {
            t0.elapsed().as_secs_f64() * 1000.0
        },
        reason: String::new(),
        file_deleted: !pbf.is_file(),
    })
}

/// Ensure this pack-server region's index from the place-source file when
/// listed, otherwise the existing Geofabrik extract path.
pub fn ensure_place_index_for_pack_region(
    pack_dir: &Path,
    index_db: &Path,
    region_id: &str,
    manifest_json: Option<&str>,
    allow_network: bool,
) -> Result<PlaceIndexEnsureReport, String> {
    let region_id = normalize_region_id(region_id);
    if region_id.is_empty() {
        return Err("empty region_id".into());
    }
    fs::create_dir_all(pack_dir).map_err(|e| e.to_string())?;
    if let Some(parent) = index_db.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let body = match manifest_json {
        Some(s) => s.to_string(),
        None => fetch_manifest_json(pack_dir, &region_id)?,
    };
    let listed = place_source_from_manifest_json(&body)?;
    let intact = NameIndex::region_index_intact(index_db, &region_id);
    let recorded = NameIndex::place_source_sha256(index_db, &region_id).unwrap_or_default();

    if intact {
        if let Some(ps) = listed {
            if recorded.is_empty() {
                NameIndex::set_place_source_sha256(index_db, &region_id, &ps.sha256)?;
                NameIndex::set_place_index_source(index_db, &region_id, "own-extract")?;
                return Ok(PlaceIndexEnsureReport {
                    region_id,
                    source: PlaceIndexSource::OwnExtract,
                    action: "recorded_sha".into(),
                    indexed: 0,
                    cache_hit: true,
                    sha256: ps.sha256,
                    download_bytes: 0,
                    index_ms: 0.0,
                    reason: "intact index from another source; recorded first place-source sha256"
                        .into(),
                    file_deleted: false,
                });
            }
            if recorded.eq_ignore_ascii_case(&ps.sha256) {
                return Ok(PlaceIndexEnsureReport {
                    region_id,
                    source: PlaceIndexSource::PlaceSource,
                    action: "unchanged".into(),
                    indexed: 0,
                    cache_hit: true,
                    sha256: recorded,
                    download_bytes: 0,
                    index_ms: 0.0,
                    reason: String::new(),
                    file_deleted: false,
                });
            }
            let dest = download_place_source(pack_dir, &region_id, &ps, allow_network)
                .map_err(|e| format!("missing: {e}"))?;
            return build_from_place_source(&region_id, &dest, index_db, &ps, true);
        }
        return Ok(PlaceIndexEnsureReport {
            region_id,
            source: PlaceIndexSource::OwnExtract,
            action: "unchanged".into(),
            indexed: 0,
            cache_hit: true,
            sha256: recorded,
            download_bytes: 0,
            index_ms: 0.0,
            reason: String::new(),
            file_deleted: false,
        });
    }

    if let Some(ps) = listed {
        match download_place_source(pack_dir, &region_id, &ps, allow_network) {
            Ok(dest) => build_from_place_source(&region_id, &dest, index_db, &ps, false),
            Err(e) => Ok(fail(&region_id, PlaceIndexSource::None, "missing", e)),
        }
    } else {
        match ensure_geofabrik_pbf_for_region(pack_dir, &region_id) {
            Ok((pbf, bytes, downloaded, pbf_ms)) => {
                let (indexed, cache_hit, index_ms) =
                    build_place_index_from_pbf(&pbf, index_db, &region_id, false)?;
                NameIndex::set_place_index_source(index_db, &region_id, "own-extract")?;
                Ok(PlaceIndexEnsureReport {
                    region_id,
                    source: PlaceIndexSource::OwnExtract,
                    action: if cache_hit {
                        "cache_hit".into()
                    } else {
                        "built".into()
                    },
                    indexed,
                    cache_hit,
                    sha256: String::new(),
                    download_bytes: if downloaded { bytes } else { 0 },
                    index_ms: index_ms + pbf_ms,
                    reason: String::new(),
                    file_deleted: false,
                })
            }
            Err(e) => Ok(fail(&region_id, PlaceIndexSource::None, "missing", e)),
        }
    }
}

/// Pack-server follow-up used by FFI: same as [`ensure_place_index_for_pack_region`]
/// with network on and no injected manifest.
pub fn ensure_place_index_for_installed_region(
    pack_dir: &Path,
    index_db: &Path,
    region_id: &str,
) -> Result<PlaceIndexEnsureReport, String> {
    ensure_place_index_for_pack_region(pack_dir, index_db, region_id, None, true)
}

#[allow(dead_code)]
pub fn pack_place_report_from_ensure(r: &PlaceIndexEnsureReport) -> PackPlaceIndexReport {
    PackPlaceIndexReport {
        region_id: r.region_id.clone(),
        pbf_path: PathBuf::from(PLACE_INDEX_DB_NAME),
        pbf_bytes: r.download_bytes,
        pbf_downloaded: r.download_bytes > 0,
        index_db: PathBuf::new(),
        indexed: r.indexed,
        cache_hit: r.cache_hit,
        pbf_ms: 0.0,
        index_ms: r.index_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/place-source-tiny.osm.pbf")
    }

    fn sha_of(path: &Path) -> String {
        let b = fs::read(path).unwrap();
        hex::encode(Sha256::digest(&b))
    }

    fn manifest_with(name: &str, sha: &str, bytes: u64) -> String {
        format!(r#"{{"files":{{"{name}":{{"sha256":"{sha}","bytes":{bytes}}}}}}}"#)
    }

    fn work(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "navi-place-source-{}-{}",
            label,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn finds_manifest_entry_by_suffix() {
        let body = r#"{
            "files": {
                "europe_sweden_halland-latest.navi-graph-car.rkyv": {"sha256":"aa","bytes":1},
                "europe_sweden_halland-latest.navi-place-source.osm.pbf": {"sha256":"abcd","bytes":99}
            }
        }"#;
        let got = place_source_from_manifest_json(body).unwrap().unwrap();
        assert_eq!(
            got.filename,
            "europe_sweden_halland-latest.navi-place-source.osm.pbf"
        );
        assert_eq!(got.sha256, "abcd");
        assert_eq!(got.bytes, 99);
    }

    #[test]
    fn manifest_without_key_is_none() {
        let body = r#"{"files":{"halland-latest.navi-manifest.json":{"sha256":"aa","bytes":2}}}"#;
        assert!(place_source_from_manifest_json(body).unwrap().is_none());
    }

    #[test]
    fn build_and_intact_records_sha256() {
        let dir = work("build");
        let pbf = fixture();
        let sha = sha_of(&pbf);
        let dest = dir.join("europe_test_leaf-latest.navi-place-source.osm.pbf");
        fs::copy(&pbf, &dest).unwrap();
        let db = dir.join(PLACE_INDEX_DB_NAME);
        let man = manifest_with(
            "europe_test_leaf-latest.navi-place-source.osm.pbf",
            &sha,
            dest.metadata().unwrap().len(),
        );
        let r =
            ensure_place_index_for_pack_region(&dir, &db, "europe/test/leaf", Some(&man), false)
                .unwrap();
        assert_eq!(r.action, "built");
        assert!(NameIndex::region_index_intact(&db, "europe/test/leaf"));
        assert_eq!(
            NameIndex::place_source_sha256(&db, "europe/test/leaf").as_deref(),
            Some(sha.as_str())
        );
        assert!(r.file_deleted);
        assert!(!dest.is_file());
        assert_eq!(
            NameIndex::place_index_source(&db, "europe/test/leaf").as_deref(),
            Some("place-source")
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unchanged_sha256_does_not_rebuild() {
        let dir = work("unchanged");
        let pbf = fixture();
        let sha = sha_of(&pbf);
        let dest = dir.join("europe_test_leaf-latest.navi-place-source.osm.pbf");
        fs::copy(&pbf, &dest).unwrap();
        let db = dir.join(PLACE_INDEX_DB_NAME);
        let man = manifest_with(
            "europe_test_leaf-latest.navi-place-source.osm.pbf",
            &sha,
            dest.metadata().unwrap().len(),
        );
        ensure_place_index_for_pack_region(&dir, &db, "europe/test/leaf", Some(&man), false)
            .unwrap();
        let first = NameIndex::has_entries_for_region(&db, "europe/test/leaf");
        assert!(first);
        let r =
            ensure_place_index_for_pack_region(&dir, &db, "europe/test/leaf", Some(&man), false)
                .unwrap();
        assert_eq!(r.action, "unchanged");
        assert!(r.cache_hit);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn changed_sha256_rebuilds_only_that_region() {
        let dir = work("changed");
        let pbf = fixture();
        let sha = sha_of(&pbf);
        let dest = dir.join("europe_test_a-latest.navi-place-source.osm.pbf");
        fs::copy(&pbf, &dest).unwrap();
        let db = dir.join(PLACE_INDEX_DB_NAME);
        let man_a = manifest_with(
            "europe_test_a-latest.navi-place-source.osm.pbf",
            &sha,
            dest.metadata().unwrap().len(),
        );
        ensure_place_index_for_pack_region(&dir, &db, "europe/test/a", Some(&man_a), false)
            .unwrap();
        fs::copy(&pbf, &dest).unwrap();
        let man_b = manifest_with(
            "europe_test_a-latest.navi-place-source.osm.pbf",
            &sha,
            dest.metadata().unwrap().len(),
        );
        ensure_place_index_for_pack_region(&dir, &db, "europe/test/b", Some(&man_b), false)
            .unwrap();
        assert!(NameIndex::region_index_intact(&db, "europe/test/a"));
        assert!(NameIndex::region_index_intact(&db, "europe/test/b"));
        NameIndex::set_place_source_sha256(&db, "europe/test/a", &"ab".repeat(32)).unwrap();
        fs::copy(&pbf, &dest).unwrap();
        let r = ensure_place_index_for_pack_region(&dir, &db, "europe/test/a", Some(&man_a), false)
            .unwrap();
        assert_eq!(r.action, "built");
        assert_eq!(
            NameIndex::place_source_sha256(&db, "europe/test/a").as_deref(),
            Some(sha.as_str())
        );
        assert!(NameIndex::region_index_intact(&db, "europe/test/b"));
        assert_eq!(
            NameIndex::place_source_sha256(&db, "europe/test/b").as_deref(),
            Some(sha.as_str())
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn existing_intact_not_rebuilt_when_file_first_appears() {
        let dir = work("first-appear");
        let pbf = fixture();
        let sha = sha_of(&pbf);
        let db = dir.join(PLACE_INDEX_DB_NAME);
        // Build from the fixture as if it were an own extract (no place-source key).
        let extract = dir.join("leaf-latest.osm.pbf");
        fs::copy(&pbf, &extract).unwrap();
        build_place_index_from_pbf(&extract, &db, "europe/test/leaf", true).unwrap();
        assert!(NameIndex::region_index_intact(&db, "europe/test/leaf"));
        assert!(NameIndex::place_source_sha256(&db, "europe/test/leaf").is_none());
        let man = manifest_with(
            "europe_test_leaf-latest.navi-place-source.osm.pbf",
            &sha,
            pbf.metadata().unwrap().len(),
        );
        let r =
            ensure_place_index_for_pack_region(&dir, &db, "europe/test/leaf", Some(&man), false)
                .unwrap();
        assert_eq!(r.action, "recorded_sha");
        assert_eq!(r.source, PlaceIndexSource::OwnExtract);
        assert_eq!(
            NameIndex::place_index_source(&db, "europe/test/leaf").as_deref(),
            Some("own-extract")
        );
        assert_eq!(
            NameIndex::place_source_sha256(&db, "europe/test/leaf").as_deref(),
            Some(sha.as_str())
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_removes_only_that_region() {
        let dir = work("delete");
        let pbf = fixture();
        let sha = sha_of(&pbf);
        let dest = dir.join("europe_test_a-latest.navi-place-source.osm.pbf");
        fs::copy(&pbf, &dest).unwrap();
        let db = dir.join(PLACE_INDEX_DB_NAME);
        let man = manifest_with(
            "europe_test_a-latest.navi-place-source.osm.pbf",
            &sha,
            dest.metadata().unwrap().len(),
        );
        ensure_place_index_for_pack_region(&dir, &db, "europe/test/a", Some(&man), false).unwrap();
        fs::copy(&pbf, &dest).unwrap();
        ensure_place_index_for_pack_region(&dir, &db, "europe/test/b", Some(&man), false).unwrap();
        let mut idx = NameIndex::open(&db).unwrap();
        idx.clear_region("europe/test/a").unwrap();
        drop(idx);
        assert!(!NameIndex::has_entries_for_region(&db, "europe/test/a"));
        assert!(NameIndex::place_source_sha256(&db, "europe/test/a").is_none());
        assert!(NameIndex::region_index_intact(&db, "europe/test/b"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn checksum_mismatch_is_rejected() {
        let dir = work("mismatch");
        let dest = dir.join("europe_test_leaf-latest.navi-place-source.osm.pbf");
        fs::write(&dest, b"not-the-bytes").unwrap();
        let db = dir.join(PLACE_INDEX_DB_NAME);
        let man = manifest_with(
            "europe_test_leaf-latest.navi-place-source.osm.pbf",
            &"ab".repeat(32),
            dest.metadata().unwrap().len(),
        );
        let r =
            ensure_place_index_for_pack_region(&dir, &db, "europe/test/leaf", Some(&man), false)
                .unwrap();
        assert_eq!(r.action, "missing");
        assert!(r.reason.contains("sha256 mismatch") || r.reason.contains("size mismatch"));
        assert!(!NameIndex::has_entries_for_region(&db, "europe/test/leaf"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn partial_file_is_not_indexed() {
        let dir = work("partial");
        let pbf = fixture();
        let sha = sha_of(&pbf);
        let dest = dir.join("europe_test_leaf-latest.navi-place-source.osm.pbf.partial");
        fs::copy(&pbf, &dest).unwrap();
        let listed = PlaceSourceRef {
            filename: "europe_test_leaf-latest.navi-place-source.osm.pbf".into(),
            sha256: sha,
            bytes: dest.metadata().unwrap().len(),
        };
        let err = verify_place_source_file(&dest, &listed).unwrap_err();
        assert!(err.contains("partial"), "{err}");
        let db = dir.join(PLACE_INDEX_DB_NAME);
        let man = manifest_with(
            "europe_test_leaf-latest.navi-place-source.osm.pbf",
            &listed.sha256,
            listed.bytes,
        );
        let r =
            ensure_place_index_for_pack_region(&dir, &db, "europe/test/leaf", Some(&man), false)
                .unwrap();
        assert_eq!(r.action, "missing");
        assert!(!NameIndex::has_entries_for_region(&db, "europe/test/leaf"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn manifest_without_key_falls_back() {
        let dir = work("fallback");
        let db = dir.join(PLACE_INDEX_DB_NAME);
        let man = r#"{"files":{"leaf-latest.navi-manifest.json":{"sha256":"aa","bytes":2}}}"#;
        let r = ensure_place_index_for_pack_region(
            &dir,
            &db,
            "europe/sweden/halland",
            Some(man),
            false,
        )
        .unwrap();
        assert_eq!(r.action, "missing");
        assert!(r.reason.contains("cannot index yet"), "{}", r.reason);
        let _ = fs::remove_dir_all(&dir);
    }
}
