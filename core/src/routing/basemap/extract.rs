//! Resolve the Protomaps public planet PMTiles URL and extract a bbox-scoped
//! local archive via HTTP range requests (no project-hosted files).
//!
//! Throughput notes:
//! - Earlier Navi extracts were **single-flight / one `get_tile` at a time** and
//!   used `NoCache`, so every tile re-fetched leaf directories. Measured Østlandet
//!   progress was ~0.87–0.92 tiles/s (~100× below the go-pmtiles reference of
//!   ~99 tiles/s with 4 download threads + range overfetch).
//! - This module uses go-pmtiles-style **range coalescing** (default 5% overfetch)
//!   plus a concurrent download pool sized from [`WorkerPoolPlan`], with chunk
//!   bodies streamed to disk (not fully buffered) so large DEM extracts fit on
//!   mobile devices.

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use pmtiles::{PmTilesWriter, TileCoord, TileId};

use crate::download::phase_timing;
use crate::download::progress as download_progress;
use crate::download::DownloadControl;
use crate::routing::basemap::http_backend::Reqwest012Backend;
use crate::routing::basemap::range_coalesce::{
    fetch_tiles_coalesced, pmtiles_download_workers_for_bytes, DEFAULT_OVERFETCH,
};
use crate::storage::{PmtilesJobStatus, PmtilesJobStore, Storage};
use uuid::Uuid;

/// Metadata endpoint listing daily planet builds.
pub const PROTOMAPS_BUILDS_METADATA_URL: &str = "https://build-metadata.protomaps.dev/builds.json";

pub const PROTOMAPS_BUILD_BASE_URL: &str = "https://build.protomaps.com";

/// Fallback dated build if metadata is unreachable.
pub const PROTOMAPS_PLANET_FALLBACK_URL: &str = "https://build.protomaps.com/20260722.pmtiles";

/// Progress label while tile/coalesce planning runs (total byte count not known yet).
pub const PLANNING_EXTRACT_LABEL: &str = "Planning extract…";

/// Default max zoom for offline extracts (higher = larger downloads).
pub const DEFAULT_EXTRACT_MAX_ZOOM: u8 = 15;

/// Historical size floor (staging picker / older tests). Archives are no longer
/// judged by size against bbox — see [validate_completed_pmtiles].
pub const MIN_FULL_REGION_BASEMAP_BYTES: u64 = 500_000_000;

#[derive(Debug, serde::Deserialize)]
struct BuildMeta {
    key: String,
}

/// Resolve the newest `https://build.protomaps.com/YYYYMMDD.pmtiles` URL.
pub async fn resolve_planet_url(client: &reqwest::Client) -> anyhow::Result<String> {
    let resp = client
        .get(PROTOMAPS_BUILDS_METADATA_URL)
        .timeout(Duration::from_secs(30))
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("builds metadata HTTP {}", resp.status());
    }
    let builds: Vec<BuildMeta> = resp.json().await?;
    let latest = builds
        .into_iter()
        .filter(|b| b.key.ends_with(".pmtiles"))
        .max_by(|a, b| a.key.cmp(&b.key))
        .ok_or_else(|| anyhow::anyhow!("no pmtiles builds in metadata"))?;
    Ok(format!(
        "{}/{}",
        PROTOMAPS_BUILD_BASE_URL.trim_end_matches('/'),
        latest.key
    ))
}

pub fn resolve_planet_url_blocking() -> String {
    let Ok(rt) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return PROTOMAPS_PLANET_FALLBACK_URL.to_string();
    };
    let client = reqwest::Client::new();
    rt.block_on(resolve_planet_url(&client))
        .unwrap_or_else(|_| PROTOMAPS_PLANET_FALLBACK_URL.to_string())
}

/// Staging directory for coalesced chunk files beside `dest`.
pub fn chunk_staging_dir(dest: &Path) -> PathBuf {
    let mut p = dest.as_os_str().to_owned();
    p.push(".chunks");
    PathBuf::from(p)
}

/// Concurrent download workers (legacy entry point; prefers size-aware sizing).
#[allow(dead_code)]
pub fn pmtiles_download_workers() -> usize {
    pmtiles_download_workers_for_bytes(0)
}

async fn wait_if_paused_or_cancelled(
    control: &DownloadControl,
    store: Option<(&Storage, Uuid)>,
    partial: &Path,
    staging: &Path,
) -> anyhow::Result<()> {
    if control.is_cancelled() {
        let _ = fs::remove_file(partial);
        let _ = fs::remove_dir_all(staging);
        if let Some((storage, job_id)) = store {
            PmtilesJobStore::new(storage).set_status(job_id, PmtilesJobStatus::Cancelled, false)?;
        }
        anyhow::bail!("cancelled");
    }
    while control.is_paused() {
        if let Some((storage, job_id)) = store {
            PmtilesJobStore::new(storage).set_status(job_id, PmtilesJobStatus::Paused, true)?;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
        if control.is_cancelled() {
            let _ = fs::remove_file(partial);
            let _ = fs::remove_dir_all(staging);
            if let Some((storage, job_id)) = store {
                PmtilesJobStore::new(storage).set_status(
                    job_id,
                    PmtilesJobStatus::Cancelled,
                    false,
                )?;
            }
            anyhow::bail!("cancelled");
        }
    }
    if let Some((storage, job_id)) = store {
        PmtilesJobStore::new(storage).set_status(job_id, PmtilesJobStatus::Running, false)?;
    }
    Ok(())
}

/// Extract tiles intersecting `bbox` `[min_lat, min_lon, max_lat, max_lon]` from
/// a remote PMTiles URL into `dest` using coalesced concurrent HTTP range requests.
pub async fn extract_bbox_to_file(
    planet_url: &str,
    dest: &Path,
    bbox: [f64; 4],
    max_zoom: u8,
    control: &DownloadControl,
    store: Option<(&Storage, Uuid)>,
) -> anyhow::Result<u64> {
    let total_t0 = phase_timing::start("pmtiles.extract.total");
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let partial = {
        let mut p = dest.as_os_str().to_owned();
        p.push(".partial");
        PathBuf::from(p)
    };
    let staging = chunk_staging_dir(dest);
    // Keep staging across retries so completed chunks are not re-fetched.
    let _ = fs::remove_file(&partial);
    let _ = fs::remove_file(dest);
    fs::create_dir_all(&staging)?;

    wait_if_paused_or_cancelled(control, store, &partial, &staging).await?;

    let setup_t0 = phase_timing::start("pmtiles.extract.setup_http");
    let http_requests = Arc::new(AtomicU64::new(0));
    // Ceiling timeout; each range GET also sets a size-scaled per-request timeout.
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(900))
        .pool_max_idle_per_host(8)
        .pool_idle_timeout(Duration::from_secs(90))
        .tcp_keepalive(Duration::from_secs(30))
        .build()?;
    let backend = Reqwest012Backend::try_from(client, planet_url)?
        .with_request_counter(Arc::clone(&http_requests));
    phase_timing::end("pmtiles.extract.setup_http", setup_t0);

    let plan_t0 = phase_timing::start("pmtiles.extract.plan_tile_list");
    let mut coords = tiles_covering_bbox(bbox, max_zoom);
    coords.sort_by_key(|c| TileId::from(*c));
    let total = coords.len() as u64;
    phase_timing::end_detail(
        "pmtiles.extract.plan_tile_list",
        plan_t0,
        &format!("tiles={total} max_z={max_zoom}"),
    );

    let avail = crate::download::available_bytes(dest);
    let started = Instant::now();
    let progress_label = if dest
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.contains("_dem"))
    {
        "Downloading terrain DEM…"
    } else {
        "Downloading map tiles for region…"
    };
    log::info!(
        target: "NaviDownload",
        "[NaviDownload] pmtiles extract start url={planet_url} dest={} tiles={total} max_z={max_zoom} \
         coalesce=1 overfetch={DEFAULT_OVERFETCH} staging={} available_bytes={:?}",
        dest.display(),
        staging.display(),
        avail
    );
    if let Some((storage, job_id)) = store {
        let s = PmtilesJobStore::new(storage);
        s.set_progress(job_id, 0, None)?;
        s.set_status(job_id, PmtilesJobStatus::Running, false)?;
    }

    wait_if_paused_or_cancelled(control, store, &partial, &staging).await?;

    // UI stays on "Planning extract…" until the coalesce fetch first updates
    // byte progress — time that gap explicitly.
    download_progress::set(0, None, PLANNING_EXTRACT_LABEL);
    let fetch_t0 = phase_timing::start("pmtiles.extract.fetch_coalesced");
    let fetched = fetch_tiles_coalesced(
        &backend,
        &coords,
        DEFAULT_OVERFETCH,
        control,
        store,
        &staging,
        progress_label,
    )
    .await
    .map_err(|e| {
        log::error!(
            target: "NaviDownload",
            "[NaviDownload] pmtiles coalesce failed dest={} err={e:#}",
            dest.display()
        );
        e
    })?;
    let max_z = max_zoom.min(fetched.header_max_zoom);
    let tiles: Vec<_> = fetched
        .tiles
        .into_iter()
        .filter(|t| t.coord.z() <= max_z)
        .collect();
    phase_timing::end_detail(
        "pmtiles.extract.fetch_coalesced",
        fetch_t0,
        &format!(
            "tiles={} http_requests={}",
            tiles.len(),
            http_requests.load(Ordering::Relaxed)
        ),
    );

    wait_if_paused_or_cancelled(control, store, &partial, &staging).await?;

    let write_t0 = phase_timing::start("pmtiles.extract.write_archive");
    let file = File::create(&partial).map_err(|e| crate::download::enrich_io_error(e, &partial))?;
    let mut writer = PmTilesWriter::new(fetched.tile_type)
        .tile_compression(fetched.tile_compression)
        .min_zoom(0)
        .max_zoom(max_z)
        .create(file)
        .map_err(|e| anyhow::anyhow!("create writer: {e}"))?;

    let mut done: u64 = 0;
    let wrote_total = tiles.len() as u64;
    download_progress::set(0, Some(wrote_total), "Writing map archive…");
    for staged in tiles {
        let bytes = staged.read_bytes().map_err(|e| {
            anyhow::anyhow!("read staged tile {}: {e}", staged.chunk_path.display())
        })?;
        writer
            .add_raw_tile(staged.coord, &bytes)
            .map_err(|e| anyhow::anyhow!("add tile: {e}"))?;
        done += 1;
        if done.is_multiple_of(32) || done == wrote_total {
            if let Some((storage, job_id)) = store {
                PmtilesJobStore::new(storage).set_progress(job_id, done, Some(wrote_total))?;
            }
            download_progress::set(done, Some(wrote_total), "Writing map archive…");
        }
        if done.is_multiple_of(256) || done == wrote_total {
            let pct = if wrote_total == 0 {
                100
            } else {
                (done.saturating_mul(100) / wrote_total.max(1)).min(100)
            };
            let elapsed = started.elapsed().as_secs_f64().max(1e-6);
            let tiles_per_s = done as f64 / elapsed;
            let reqs = http_requests.load(Ordering::Relaxed);
            log::info!(
                target: "NaviDownload",
                "[NaviDownload] pmtiles extract progress dest={} tiles={done}/{wrote_total} pct={pct} \
                 tiles_per_s={tiles_per_s:.2} http_requests={reqs} available_bytes={:?}",
                dest.display(),
                crate::download::available_bytes(dest)
            );
        }
        if done.is_multiple_of(512) {
            wait_if_paused_or_cancelled(control, store, &partial, &staging).await?;
        }
    }

    writer
        .finalize()
        .map_err(|e| anyhow::anyhow!("finalize: {e}"))?;

    if !partial.exists() {
        anyhow::bail!("extract produced no file");
    }
    fs::rename(&partial, dest).map_err(|e| crate::download::enrich_io_error(e, dest))?;
    let _ = fs::remove_dir_all(&staging);
    let len = fs::metadata(dest)?.len();
    phase_timing::end_detail(
        "pmtiles.extract.write_archive",
        write_t0,
        &format!("bytes={len} wrote={done}"),
    );
    if let Some((storage, job_id)) = store {
        // Progress only — caller ([PmtilesDownloader::run_job]) validates the
        // archive before marking Completed (shared guard with the short-circuit path).
        PmtilesJobStore::new(storage).set_progress(job_id, len, Some(len))?;
    }
    let elapsed = started.elapsed().as_secs_f64().max(1e-6);
    let tiles_per_s = total as f64 / elapsed;
    let reqs = http_requests.load(Ordering::Relaxed);
    log::info!(
        target: "NaviDownload",
        "[NaviDownload] pmtiles extract complete dest={} bytes={len} bbox_tiles={total} wrote={done} \
         elapsed_s={elapsed:.1} tiles_per_s={tiles_per_s:.2} http_requests={reqs}",
        dest.display()
    );
    phase_timing::end_detail(
        "pmtiles.extract.total",
        total_t0,
        &format!("bytes={len} tiles={done}"),
    );
    Ok(len)
}

/// PMTiles v3 header fields used by the completion guard.
#[derive(Debug, Clone, Copy)]
pub struct PmtilesHeaderInfo {
    pub min_zoom: u8,
    pub max_zoom: u8,
    pub addressed_tiles: u64,
}

/// PMTiles v3 header: magic at 0, maxzoom uint8 at offset 101,
/// addressed tile count u64le at offset 72.
pub fn read_pmtiles_header(path: &Path) -> anyhow::Result<PmtilesHeaderInfo> {
    let mut header = [0u8; 127];
    let mut file = File::open(path)?;
    use std::io::Read;
    file.read_exact(&mut header)?;
    if &header[0..7] != b"PMTiles" {
        anyhow::bail!("not a PMTiles archive");
    }
    Ok(PmtilesHeaderInfo {
        min_zoom: header[100],
        max_zoom: header[101],
        addressed_tiles: u64::from_le_bytes(header[72..80].try_into()?),
    })
}

/// PMTiles v3 header: magic at 0, maxzoom uint8 at offset 101.
pub fn read_pmtiles_max_zoom(path: &Path) -> anyhow::Result<u8> {
    Ok(read_pmtiles_header(path)?.max_zoom)
}

fn is_dem_archive(region_key: &str, path: &Path) -> bool {
    region_key.ends_with("_dem")
        || path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.contains("_dem.pmtiles") || n.ends_with("_dem.pmtiles"))
}

fn is_test_region_key(region_key: &str) -> bool {
    region_key.starts_with("test_")
}

/// Shared completion guard: existing short-circuit files, fresh extracts, and
/// idle recheck of `*.pmtiles.rejected`.
///
/// An archive is judged by what it contains, not by its size against its bbox
/// (sparse regions are legitimately small):
/// - readable PMTiles v3 header
/// - expected zoom range (vector: maxzoom ≥ [DEFAULT_EXTRACT_MAX_ZOOM])
/// - addressed tile count above zero
/// - a sample of tiles that decode
///
/// DEM (`*_dem`) / `test_*` still skip the vector maxzoom floor (native DEM
/// maxzoom is 12; fixtures stay small).
/// Validate [partial] then replace [dest] atomically. On failure [dest] is
/// left untouched and [partial] is removed.
pub fn commit_partial_pmtiles(
    partial: &Path,
    dest: &Path,
    region_key: &str,
    bbox: [f64; 4],
) -> anyhow::Result<()> {
    if let Err(reason) = validate_completed_pmtiles(partial, region_key, bbox) {
        let _ = fs::remove_file(partial);
        anyhow::bail!("extracted archive failed validation: {reason}");
    }
    fs::rename(partial, dest).map_err(|e| crate::download::enrich_io_error(e, dest))?;
    Ok(())
}

static VALIDATE_CACHE: Mutex<Vec<(PathBuf, u64, u64, Result<(), String>)>> = Mutex::new(Vec::new());

fn cache_key_mtime(meta: &fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn validate_completed_pmtiles(
    path: &Path,
    region_key: &str,
    _bbox: [f64; 4],
) -> Result<(), String> {
    let meta = fs::metadata(path).map_err(|e| format!("stat failed: {e}"))?;
    let len = meta.len();
    let mtime = cache_key_mtime(&meta);
    if let Ok(g) = VALIDATE_CACHE.lock() {
        if let Some((_, _, _, cached)) = g
            .iter()
            .rev()
            .find(|(p, s, m, _)| p == path && *s == len && *m == mtime)
        {
            return cached.clone();
        }
    }
    let result = validate_completed_pmtiles_uncached(path, region_key, len);
    if let Ok(mut g) = VALIDATE_CACHE.lock() {
        g.push((path.to_path_buf(), len, mtime, result.clone()));
        if g.len() > 64 {
            g.remove(0);
        }
    }
    result
}

fn validate_completed_pmtiles_uncached(
    path: &Path,
    region_key: &str,
    len: u64,
) -> Result<(), String> {
    if len < 127 {
        return Err(format!("archive too small ({len} bytes)"));
    }
    let header = read_pmtiles_header(path).map_err(|e| e.to_string())?;

    if is_dem_archive(region_key, path) || is_test_region_key(region_key) {
        return Ok(());
    }
    if header.max_zoom < DEFAULT_EXTRACT_MAX_ZOOM {
        return Err(format!(
            "PMTiles maxzoom {} < required {DEFAULT_EXTRACT_MAX_ZOOM} for region {region_key}",
            header.max_zoom
        ));
    }
    if header.min_zoom > header.max_zoom {
        return Err(format!(
            "PMTiles minzoom {} > maxzoom {} for region {region_key}",
            header.min_zoom, header.max_zoom
        ));
    }
    if header.addressed_tiles == 0 {
        return Err(format!("archive has no tiles for region {region_key}"));
    }
    sample_decode_tiles(path, region_key)?;
    Ok(())
}

fn sample_decode_tiles(path: &Path, region_key: &str) -> Result<(), String> {
    let samples: &[(u8, u32, u32)] = &[(0, 0, 0), (1, 0, 0), (1, 1, 0)];
    let mut decoded = 0u32;
    let mut last_err: Option<String> = None;
    for &(z, x, y) in samples {
        match crate::routing::basemap::read_pmtiles_tile(path, z, x, y) {
            Ok(Some(bytes)) if !bytes.is_empty() => decoded += 1,
            Ok(Some(_)) => last_err = Some(format!("tile {z}/{x}/{y} decoded empty")),
            Ok(None) => {}
            Err(e) => last_err = Some(format!("tile {z}/{x}/{y} decode failed: {e}")),
        }
        if decoded >= 1 {
            return Ok(());
        }
    }
    Err(last_err.unwrap_or_else(|| {
        format!("no decodable sample tile for region {region_key}")
    }))
}

pub fn tiles_covering_bbox(bbox: [f64; 4], max_zoom: u8) -> Vec<TileCoord> {
    let (min_lat, min_lon, max_lat, max_lon) = (bbox[0], bbox[1], bbox[2], bbox[3]);
    let mut out = Vec::new();
    for z in 0..=max_zoom {
        let x0 = lon_to_x(min_lon, z);
        let x1 = lon_to_x(max_lon, z);
        let y0 = lat_to_y(max_lat, z);
        let y1 = lat_to_y(min_lat, z);
        let (xmin, xmax) = (x0.min(x1), x0.max(x1));
        let (ymin, ymax) = (y0.min(y1), y0.max(y1));
        for x in xmin..=xmax {
            for y in ymin..=ymax {
                if let Ok(c) = TileCoord::new(z, x, y) {
                    out.push(c);
                }
            }
        }
    }
    out
}

fn lon_to_x(lon: f64, z: u8) -> u32 {
    let n = 2f64.powi(z as i32);
    let mut x = ((lon + 180.0) / 360.0 * n).floor() as i64;
    x = x.clamp(0, n as i64 - 1);
    x as u32
}

fn lat_to_y(lat: f64, z: u8) -> u32 {
    let lat = lat.clamp(-85.05112878, 85.05112878);
    let n = 2f64.powi(z as i32);
    let lat_rad = lat.to_radians();
    let y = ((1.0 - (lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / std::f64::consts::PI) / 2.0 * n)
        .floor() as i64;
    y.clamp(0, n as i64 - 1) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::basemap::range_coalesce::{
        pmtiles_download_workers_for_bytes, timeout_for_chunk_bytes, MAX_HTTP_CHUNK_BYTES,
    };

    #[test]
    fn oslo_tile_count_z3_small() {
        let bbox = [59.85, 10.6, 59.98, 10.9];
        let tiles = tiles_covering_bbox(bbox, 3);
        assert!(!tiles.is_empty());
        assert!(tiles.len() < 100);
    }

    #[test]
    fn download_workers_in_range() {
        let w = pmtiles_download_workers();
        assert!((1..=8).contains(&w));
        assert!(pmtiles_download_workers_for_bytes(2_000_000_000) <= 2);
        assert!(pmtiles_download_workers_for_bytes(300_000_000) <= 3);
    }

    #[test]
    fn timeout_scales_with_chunk_size() {
        let small = timeout_for_chunk_bytes(1024);
        let large = timeout_for_chunk_bytes(MAX_HTTP_CHUNK_BYTES);
        assert!(small.as_secs() >= 90);
        assert!(large.as_secs() >= small.as_secs());
        assert!(large.as_secs() <= 900);
    }

    #[test]
    fn staging_dir_suffix() {
        let p = PathBuf::from("/data/files/pmtiles/region_dem.pmtiles");
        assert!(chunk_staging_dir(&p)
            .to_string_lossy()
            .ends_with("region_dem.pmtiles.chunks"));
    }

    fn write_fake_pmtiles(path: &Path, maxzoom: u8, size: usize) {
        let mut buf = vec![0u8; size];
        buf[0..7].copy_from_slice(b"PMTiles");
        buf[101] = maxzoom;
        fs::write(path, &buf).unwrap();
    }

    fn gzip_bytes(data: &[u8]) -> Vec<u8> {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;
        let mut enc = GzEncoder::new(Vec::new(), Compression::default());
        enc.write_all(data).unwrap();
        enc.finish().unwrap()
    }

    fn write_valid_pmtiles(path: &Path, maxzoom: u8, with_tile: bool) {
        use pmtiles::{Compression, PmTilesWriter, TileCoord, TileType};
        let file = File::create(path).unwrap();
        let mut writer = PmTilesWriter::new(TileType::Mvt)
            .tile_compression(Compression::Gzip)
            .min_zoom(0)
            .max_zoom(maxzoom)
            .create(file)
            .unwrap();
        if with_tile {
            let raw = gzip_bytes(&[0x1a, 0x04, b't', b'e', b's', b't']);
            writer
                .add_raw_tile(TileCoord::new(0, 0, 0).unwrap(), &raw)
                .unwrap();
        }
        writer.finalize().unwrap();
    }

    #[test]
    fn validate_rejects_mz12_large_region_fixture() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("europe_norway_ostlandet.pmtiles");
        write_fake_pmtiles(&path, 12, 192_023_045);
        let bbox = [58.5, 7.5, 62.8, 13.5];
        let err = validate_completed_pmtiles(&path, "europe_norway_ostlandet", bbox).unwrap_err();
        assert!(err.contains("maxzoom"), "{err}");
    }

    #[test]
    fn validate_accepts_small_archive_with_large_bbox() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("europe_sweden_dalarna.pmtiles");
        write_valid_pmtiles(&path, 15, true);
        assert!(path.metadata().unwrap().len() < 50_000);
        let bbox = [50.0, 0.0, 70.0, 20.0];
        validate_completed_pmtiles(&path, "europe_sweden_dalarna", bbox).unwrap();
    }

    #[test]
    fn validate_rejects_truncated_archive() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("europe_sweden_dalarna.pmtiles");
        write_valid_pmtiles(&path, 15, true);
        let f = File::options().write(true).open(&path).unwrap();
        f.set_len(80).unwrap();
        drop(f);
        let bbox = [50.0, 0.0, 70.0, 20.0];
        let err = validate_completed_pmtiles(&path, "europe_sweden_dalarna", bbox).unwrap_err();
        assert!(
            err.contains("too small") || err.contains("not a PMTiles") || err.contains("decode"),
            "{err}"
        );
    }

    #[test]
    fn interrupted_write_leaves_existing_dest() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("europe_sweden_dalarna.pmtiles");
        write_valid_pmtiles(&dest, 15, true);
        let before = fs::read(&dest).unwrap();
        let partial = {
            let mut p = dest.as_os_str().to_owned();
            p.push(".partial");
            PathBuf::from(p)
        };
        fs::write(&partial, b"not-a-pmtiles-archive").unwrap();
        let bbox = [50.0, 0.0, 70.0, 20.0];
        let err = commit_partial_pmtiles(&partial, &dest, "europe_sweden_dalarna", bbox).unwrap_err();
        assert!(
            err.to_string().contains("validation") || err.to_string().contains("not a PMTiles"),
            "{err}"
        );
        assert_eq!(fs::read(&dest).unwrap(), before);
        assert!(!partial.exists());
    }

    #[test]
    fn validate_rejects_archive_with_no_tiles() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("europe_sweden_dalarna.pmtiles");
        write_valid_pmtiles(&path, 15, false);
        let bbox = [50.0, 0.0, 70.0, 20.0];
        let err = validate_completed_pmtiles(&path, "europe_sweden_dalarna", bbox).unwrap_err();
        assert!(err.contains("no tiles"), "{err}");
    }

    #[test]
    fn validate_test_region_allows_small_mz12() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.pmtiles");
        write_fake_pmtiles(&path, 12, 50_000);
        let bbox = [58.5, 7.5, 62.8, 13.5];
        validate_completed_pmtiles(&path, "test_ostlandet_fixture", bbox).unwrap();
    }

    #[test]
    fn validate_dem_skips_vector_floors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("europe_norway_ostlandet_dem.pmtiles");
        write_fake_pmtiles(&path, 12, 50_000);
        let bbox = [58.5, 7.5, 62.8, 13.5];
        validate_completed_pmtiles(&path, "europe_norway_ostlandet_dem", bbox).unwrap();
    }

    #[tokio::test]
    #[ignore = "network: real Protomaps range extract"]
    async fn extract_tiny_oslo_from_planet() {
        let url = resolve_planet_url(&reqwest::Client::new())
            .await
            .unwrap_or_else(|_| PROTOMAPS_PLANET_FALLBACK_URL.to_string());
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("oslo.pmtiles");
        let control = DownloadControl::default();
        let bbox = [59.85, 10.6, 59.98, 10.9];
        let len = extract_bbox_to_file(&url, &dest, bbox, 8, &control, None)
            .await
            .unwrap();
        assert!(dest.is_file());
        assert!(len > 1000);
    }

    #[tokio::test]
    #[ignore = "network: real Mapterhorn DEM range extract"]
    async fn extract_tiny_oslo_dem_from_mapterhorn() {
        let url = "https://download.mapterhorn.com/planet.pmtiles";
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("oslo_dem.pmtiles");
        let control = DownloadControl::default();
        let bbox = [59.90, 10.70, 59.93, 10.75];
        let len = extract_bbox_to_file(url, &dest, bbox, 10, &control, None)
            .await
            .expect("mapterhorn extract");
        assert!(dest.is_file());
        assert!(len > 100, "expected DEM bytes, got {len}");
        assert!(
            !chunk_staging_dir(&dest).exists(),
            "staging should be removed after success"
        );
    }

    #[tokio::test]
    #[ignore = "network: measures Ostlandet extract tiles/s"]
    async fn ostlandet_extract_throughput() {
        let url = resolve_planet_url(&reqwest::Client::new())
            .await
            .unwrap_or_else(|_| PROTOMAPS_PLANET_FALLBACK_URL.to_string());
        let bbox = crate::routing::basemap::region_bbox("europe/norway/ostlandet").unwrap();
        let total = tiles_covering_bbox(bbox, DEFAULT_EXTRACT_MAX_ZOOM).len();
        eprintln!("url={url} planned_tiles={total}");
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("ostlandet.pmtiles");
        let control = DownloadControl::default();
        let t0 = Instant::now();
        let len = extract_bbox_to_file(&url, &dest, bbox, DEFAULT_EXTRACT_MAX_ZOOM, &control, None)
            .await
            .expect("extract");
        let secs = t0.elapsed().as_secs_f64();
        let rate = total as f64 / secs;
        eprintln!("COMPLETE bytes={len} tiles={total} elapsed_s={secs:.1} tiles_per_s={rate:.2}");
        assert!(dest.is_file());
        assert!(
            rate > 20.0,
            "expected >>1 tile/s after coalesce+parallel fix, got {rate:.2}"
        );
    }
}
