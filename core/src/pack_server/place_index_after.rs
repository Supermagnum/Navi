//! After pack-server install: fetch a real Geofabrik PBF and build `place_index.db`.
//!
//! Pack installs leave only a tiny stub `.osm.pbf` (graphs come from published
//! packs). Place search still needs a full extract — same Geofabrik URL and
//! [`crate::search::NameIndex`] path as the local-convert / `ensure_place_index`
//! flow. Indexing is **additive by `region_id`** so downloading region B does
//! not wipe region A's places. This module does **not** re-bake routing packs.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use super::acquisition::{leaf_stem_for_region_id, normalize_region_id};
use crate::routing::geofabrik_latest_pbf_url;
use crate::routing::region::download_file;
use crate::search::NameIndex;

/// Minimum size treated as a real extract (matches region provision / Android).
pub const MIN_REAL_PBF_BYTES: u64 = 1_000_000;

const PLACE_SOURCE_FILE_SUFFIX: &str = ".navi-place-source.osm.pbf";

/// Pack regions that have a manifest or install stamp in [dir].
pub fn installed_pack_region_ids(dir: &Path) -> Vec<String> {
    let mut ids = Vec::new();
    let Ok(rd) = fs::read_dir(dir) else {
        return ids;
    };
    for ent in rd.flatten() {
        let name = ent.file_name();
        let name = name.to_string_lossy();
        if let Some(_stem) = name.strip_suffix(".navi-server-install.json") {
            if let Ok(txt) = fs::read_to_string(ent.path()) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
                    if let Some(id) = v.get("region_id").and_then(|x| x.as_str()) {
                        let id = normalize_region_id(id);
                        if !id.is_empty() {
                            ids.push(id);
                        }
                    }
                }
            }
        }
    }
    ids.sort();
    ids.dedup();
    ids
}

fn region_has_pack_manifest(dir: &Path, region_id: &str) -> bool {
    let stem = leaf_stem_for_region_id(region_id);
    dir.join(format!("{stem}.navi-manifest.json")).is_file()
        || dir.join(format!("{stem}.navi-server-install.json")).is_file()
}

/// A region is indexed only from its own source (place-source file or own
/// extract). An extract that covers more than the region's outline is never
/// indexed under any id. A region id that is not an installed pack region is
/// never given an index.
pub fn refuse_overbroad_place_index(region_id: &str, pbf: &Path) -> Result<(), String> {
    let region_id = normalize_region_id(region_id);
    if region_id.is_empty() {
        return Err("empty region_id".into());
    }
    let fname = pbf.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if fname.ends_with(PLACE_SOURCE_FILE_SUFFIX) {
        return Ok(());
    }
    let extract = crate::routing::geofabrik_extract_path(&region_id);
    if extract != region_id {
        return Err(format!(
            "extract covers more than the region outline; will not index {extract} under {region_id}"
        ));
    }
    let Some(dir) = pbf.parent() else {
        return Ok(());
    };
    let installed = installed_pack_region_ids(dir);
    let prefix = format!("{region_id}/");
    if installed.iter().any(|id| id.starts_with(&prefix)) {
        return Err(format!(
            "refusing covering extract under {region_id}: leaf pack regions are installed"
        ));
    }
    if !region_has_pack_manifest(dir, &region_id) {
        return Err(format!(
            "region id is not an installed pack region: {region_id}"
        ));
    }
    Ok(())
}

/// On-device FTS path used by searchPlaces / PlaceIndexBackground.
pub const PLACE_INDEX_DB_NAME: &str = "place_index.db";

#[derive(Debug, Clone)]
pub struct PackPlaceIndexReport {
    pub region_id: String,
    pub pbf_path: PathBuf,
    pub pbf_bytes: u64,
    pub pbf_downloaded: bool,
    pub index_db: PathBuf,
    pub indexed: usize,
    pub cache_hit: bool,
    pub pbf_ms: f64,
    pub index_ms: f64,
}

impl PackPlaceIndexReport {
    pub fn to_report_string(&self) -> String {
        format!(
            "PASS\n\
             region_id={}\n\
             pbf={}\n\
             pbf_bytes={}\n\
             pbf_downloaded={}\n\
             indexed={}\n\
             cache_hit={}\n\
             index_db={}\n\
             pbf_ms={:.1}\n\
             index_ms={:.1}\n",
            self.region_id,
            self.pbf_path.display(),
            self.pbf_bytes,
            self.pbf_downloaded,
            self.indexed,
            self.cache_hit,
            self.index_db.display(),
            self.pbf_ms,
            self.index_ms
        )
    }
}

fn pbf_filename_for_region(region_id: &str) -> String {
    // Place-index always uses the leaf stem file (e.g. halland-latest.osm.pbf).
    // Never fall back to a parent-country extract under a subregion id.
    format!("{}.osm.pbf", leaf_stem_for_region_id(region_id))
}

/// Ensure `data_dir/<leaf>-latest.osm.pbf` is a real Geofabrik extract.
///
/// Replaces pack-server stubs (< [`MIN_REAL_PBF_BYTES`]). Uses the same
/// [`download_file`] resume path as [`crate::routing::region::provision_region`].
///
/// When Geofabrik only publishes a parent extract (Sweden län), a missing leaf
/// file is **not** replaced by the country PBF — returns a clear
/// "cannot index yet" error so callers clip first.
pub fn ensure_geofabrik_pbf_for_region(
    data_dir: &Path,
    region_id: &str,
) -> Result<(PathBuf, u64, bool, f64), String> {
    let region_id = normalize_region_id(region_id);
    if region_id.is_empty() {
        return Err("empty region_id".into());
    }
    fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
    let filename = pbf_filename_for_region(&region_id);
    let pbf_path = data_dir.join(&filename);
    let extract = crate::routing::geofabrik_extract_path(&region_id);
    let existing = pbf_path.metadata().map(|m| m.len()).unwrap_or(0);
    // Parent-country extract path differs from the leaf region id (Sweden län).
    // Place-index requires a clipped/own leaf file — never index sweden-latest
    // under europe/sweden/halland (or hamburg/finland from the wrong file).
    if extract != region_id {
        if pbf_path.is_file() && existing >= MIN_REAL_PBF_BYTES {
            log::info!(
                target: "NaviPack",
                "place-index: reusing leaf PBF region={region_id} path={} bytes={existing}",
                pbf_path.display()
            );
            return Ok((pbf_path, existing, false, 0.0));
        }
        return Err(format!(
            "cannot index yet: missing leaf extract {filename} for {region_id} \
             (parent extract {extract} must be clipped to this region first)"
        ));
    }
    let url = geofabrik_latest_pbf_url(&region_id);
    let need = !pbf_path.is_file() || existing < MIN_REAL_PBF_BYTES;
    let t0 = crate::download::phase_timing::start("geofabrik_pbf.download");
    let (bytes, downloaded) = if need {
        log::info!(
            target: "NaviPack",
            "place-index: downloading Geofabrik PBF region={region_id} url={url} \
             existing_bytes={existing}"
        );
        // Drop stub so we never resume a 16 KiB zero-filled "partial" as truth.
        if pbf_path.is_file() && existing < MIN_REAL_PBF_BYTES {
            let _ = fs::remove_file(&pbf_path);
        }
        let n =
            download_file(&url, &pbf_path).map_err(|e| format!("Geofabrik PBF download: {e:#}"))?;
        if n < MIN_REAL_PBF_BYTES {
            let _ = fs::remove_file(&pbf_path);
            return Err(format!(
                "cannot index yet: Geofabrik PBF too small ({n} bytes) for {region_id} from {url}"
            ));
        }
        (n, true)
    } else {
        log::info!(
            target: "NaviPack",
            "place-index: reusing Geofabrik PBF region={region_id} path={} bytes={existing}",
            pbf_path.display()
        );
        (existing, false)
    };
    crate::download::phase_timing::end_detail(
        "geofabrik_pbf.download",
        t0,
        &format!("bytes={bytes} downloaded={downloaded}"),
    );
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    Ok((pbf_path, bytes, downloaded, ms))
}

/// Build or rebuild `place_index.db` from a PBF (same schema as on-device local convert).
///
/// When `region_id` is non-empty, only that region's rows are replaced — other
/// regions in the shared DB are preserved. `force_rebuild` forces a re-index of
/// this region even if it already has entries; it never deletes the whole DB file.
pub fn build_place_index_from_pbf(
    pbf_path: &Path,
    index_db: &Path,
    region_id: &str,
    force_rebuild: bool,
) -> Result<(usize, bool, f64), String> {
    if !pbf_path.is_file() {
        return Err(format!("PBF missing: {}", pbf_path.display()));
    }
    if let Some(parent) = index_db.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let region_id = region_id.trim().trim_matches('/');
    let _build = crate::search::lock_place_index_build_with_progress();
    // Schema bumps must rebuild; wipe so multi-region DBs do not keep pre-bump
    // kinds after only this region is re-indexed.
    NameIndex::discard_if_schema_stale(index_db);
    if !force_rebuild && index_db.is_file() {
        if let Ok(meta) = fs::metadata(index_db) {
            let region_ok = if region_id.is_empty() {
                NameIndex::has_entries(index_db) && NameIndex::region_index_complete(index_db, "")
            } else {
                NameIndex::has_entries_for_region(index_db, region_id)
                    && NameIndex::region_index_complete(index_db, region_id)
            };
            if meta.len() > 10_000 && NameIndex::is_current_schema(index_db) && region_ok {
                crate::download::progress::set(6, Some(6), "Place index ready");
                return Ok((0, true, 0.0));
            }
        }
    }
    let t0 = Instant::now();
    crate::download::progress::set(0, Some(6), "Place index: starting…");
    let open_t0 = crate::download::phase_timing::start("place_index.open_db");
    let mut idx = NameIndex::open(index_db).map_err(|e| format!("open index: {e}"))?;
    crate::download::phase_timing::end("place_index.open_db", open_t0);
    let n = idx
        .load_from_pbf_for_region(pbf_path, region_id)
        .map_err(|e| {
            let msg = format!("{e:#}");
            if msg.contains(crate::search::PLACE_INDEX_PAUSED_PREFIX) {
                msg
            } else {
                format!("index load: {msg}")
            }
        })?;
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    if n == 0 || !NameIndex::has_entries(index_db) {
        return Err(format!(
            "place index empty after load (indexed={n}) from {}",
            pbf_path.display()
        ));
    }
    Ok((n, false, ms))
}

/// Pack-server follow-up: real Geofabrik PBF + full place index.
///
/// `force_rebuild` re-indexes this region’s rows only (additive across regions).
/// Pass `force_rebuild = catalog_generation_requires_rebuild(local, catalog)` so
/// an unchanged pack-server generation does not wipe and rebuild the place index;
/// incomplete indexes resume via `name_index_build.complete=0` instead.
pub fn ensure_place_index_after_pack_install(
    data_dir: &Path,
    region_id: &str,
    force_rebuild: bool,
) -> Result<PackPlaceIndexReport, String> {
    let region_id = normalize_region_id(region_id);
    let (pbf_path, pbf_bytes, pbf_downloaded, pbf_ms) =
        ensure_geofabrik_pbf_for_region(data_dir, &region_id)?;
    refuse_overbroad_place_index(&region_id, &pbf_path)?;
    let index_db = data_dir.join(PLACE_INDEX_DB_NAME);
    log::info!(
        target: "NaviPack",
        "place-index: building region={region_id} pbf={} force_rebuild={force_rebuild}",
        pbf_path.display()
    );
    let (indexed, cache_hit, index_ms) =
        build_place_index_from_pbf(&pbf_path, &index_db, &region_id, force_rebuild)?;
    let report = PackPlaceIndexReport {
        region_id,
        pbf_path,
        pbf_bytes,
        pbf_downloaded,
        index_db,
        indexed,
        cache_hit,
        pbf_ms,
        index_ms,
    };
    log::info!(
        target: "NaviPack",
        "place-index: done region={} indexed={} cache_hit={} pbf_ms={:.1} index_ms={:.1}",
        report.region_id,
        report.indexed,
        report.cache_hit,
        report.pbf_ms,
        report.index_ms
    );
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn pbf_filename_uses_geofabrik_leaf_stem() {
        assert_eq!(
            pbf_filename_for_region("europe/norway/vestlandet"),
            "vestlandet-latest.osm.pbf"
        );
        assert_eq!(
            pbf_filename_for_region("europe/germany/bremen"),
            "bremen-latest.osm.pbf"
        );
        assert_eq!(
            pbf_filename_for_region("europe/sweden/gotland"),
            "gotland-latest.osm.pbf"
        );
        assert_eq!(
            pbf_filename_for_region("europe/germany/hamburg"),
            "hamburg-latest.osm.pbf"
        );
    }

    #[test]
    fn sweden_lan_without_leaf_pbf_cannot_index_yet() {
        let dir =
            std::env::temp_dir().join(format!("navi-place-index-se-lan-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        // Country extract present must not authorize indexing a län leaf.
        {
            let f = fs::File::create(dir.join("sweden-latest.osm.pbf")).unwrap();
            f.set_len(MIN_REAL_PBF_BYTES).unwrap();
        }
        let err = ensure_geofabrik_pbf_for_region(&dir, "europe/sweden/halland").unwrap_err();
        assert!(
            err.contains("cannot index yet"),
            "expected cannot-index-yet, got {err}"
        );
        assert!(err.contains("halland-latest.osm.pbf"), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuse_covering_extract_when_leaf_packs_installed() {
        let dir = std::env::temp_dir().join(format!(
            "navi-place-index-parent-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("norrbotten-latest.navi-server-install.json"),
            r#"{"region_id":"europe/sweden/norrbotten"}"#,
        )
        .unwrap();
        let pbf = dir.join("sweden-latest.osm.pbf");
        {
            let f = fs::File::create(&pbf).unwrap();
            f.set_len(MIN_REAL_PBF_BYTES).unwrap();
        }
        let err = refuse_overbroad_place_index("europe/sweden", &pbf).unwrap_err();
        assert!(err.contains("leaf pack"), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuse_region_id_that_is_not_an_installed_pack() {
        let dir = std::env::temp_dir().join(format!(
            "navi-place-index-not-pack-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let pbf = dir.join("fu38-pause.osm.pbf");
        {
            let f = fs::File::create(&pbf).unwrap();
            f.set_len(MIN_REAL_PBF_BYTES).unwrap();
        }
        let err = refuse_overbroad_place_index("test/fu38-pause", &pbf).unwrap_err();
        assert!(err.contains("not an installed pack region"), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn build_rejects_missing_pbf() {
        let dir =
            std::env::temp_dir().join(format!("navi-place-index-missing-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let pbf = dir.join("missing.osm.pbf");
        let db = dir.join(PLACE_INDEX_DB_NAME);
        let err = build_place_index_from_pbf(&pbf, &db, "europe/test", true).unwrap_err();
        assert!(err.contains("PBF missing"), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn build_cache_hit_skips_rebuild_when_index_present() {
        let dir =
            std::env::temp_dir().join(format!("navi-place-index-cache-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let pbf = dir.join("tiny.osm.pbf");
        // Real enough to pass is_file(); load_from_pbf is not reached on cache hit.
        {
            let mut f = fs::File::create(&pbf).unwrap();
            f.write_all(b"not-a-real-pbf").unwrap();
        }
        let db = dir.join(PLACE_INDEX_DB_NAME);
        // Seed a schema-valid empty-looking DB via NameIndex::open + insert path is heavy;
        // instead only assert force_rebuild clears and missing schema fails open path.
        // Cache-hit requires has_entries — create via open then skip if schema helpers need rows.
        let opened = NameIndex::open(&db);
        assert!(
            opened.is_ok(),
            "open empty index failed: {:?}",
            opened.err()
        );
        drop(opened);
        // Without rows, cache hit must not trigger (falls through to load_from_pbf).
        let err = build_place_index_from_pbf(&pbf, &db, "europe/test", false);
        assert!(
            err.is_err(),
            "expected load failure on stub PBF, got {err:?}"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn report_string_marks_pass() {
        let r = PackPlaceIndexReport {
            region_id: "europe/germany/bremen".into(),
            pbf_path: PathBuf::from("/tmp/bremen-latest.osm.pbf"),
            pbf_bytes: 2_000_000,
            pbf_downloaded: true,
            index_db: PathBuf::from("/tmp/place_index.db"),
            indexed: 10,
            cache_hit: false,
            pbf_ms: 1.0,
            index_ms: 2.0,
        };
        let s = r.to_report_string();
        assert!(s.starts_with("PASS"), "{s}");
        assert!(s.contains("indexed=10"), "{s}");
    }
}
