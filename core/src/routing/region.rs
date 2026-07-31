//! Host-facing region download helpers (OSM extract + elevation tiles).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::bail;
use reqwest::header::HeaderMap;

use crate::download::{
    stream_get_to_file_blocking, DownloadControl, StreamDownloadOpts, DEFAULT_RETRIES,
};
use crate::routing::elevation::{bbox_to_tiles, ElevationCache, ElevationDownloader};
use crate::storage::{ElevationJobStore, JobStatus, Storage};

/// Espa -> Atnbrufossen corridor bbox [min_lat, min_lon, max_lat, max_lon].
pub const CORRIDOR_BBOX: [f64; 4] = [60.40, 10.00, 62.00, 11.50];

/// Download a file from `url` to `dest`, creating parent directories.
///
/// Streams in bounded chunks (never buffers the whole body in RAM), retries
/// transient failures, and resumes from a sibling `.partial` when present.
pub fn download_file(url: &str, dest: &Path) -> anyhow::Result<u64> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    // Local file copy (instrumented tests stage fixtures under /data/local/tmp).
    if let Some(path) = url.strip_prefix("file://") {
        let src = Path::new(path);
        log::info!(
            target: "NaviDownload",
            "[NaviDownload] copy start src={path} dest={}",
            dest.display()
        );
        let bytes = fs::copy(src, dest)
            .map_err(|e| anyhow::anyhow!("copy {path} -> {dest:?}: {e}"))?;
        log::info!(
            target: "NaviDownload",
            "[NaviDownload] copy complete dest={} bytes={bytes}",
            dest.display()
        );
        return Ok(bytes);
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        let src = Path::new(url);
        if src.is_file() {
            log::info!(
                target: "NaviDownload",
                "[NaviDownload] copy start src={url} dest={}",
                dest.display()
            );
            let bytes = fs::copy(src, dest)
                .map_err(|e| anyhow::anyhow!("copy {url} -> {dest:?}: {e}"))?;
            log::info!(
                target: "NaviDownload",
                "[NaviDownload] copy complete dest={} bytes={bytes}",
                dest.display()
            );
            return Ok(bytes);
        }
    }

    let result = stream_get_to_file_blocking(StreamDownloadOpts {
        url,
        dest,
        headers: HeaderMap::new(),
        resume_from: 0,
        expected_bytes: None,
        retries: DEFAULT_RETRIES,
        progress_label: "Downloading region…",
        allow_not_found: false,
    })?
    .ok_or_else(|| anyhow::anyhow!("download returned empty for {url}"))?;
    Ok(result.bytes)
}

/// Ensure corridor DEM tiles exist under `elev_dir`, downloading any missing ones.
pub fn ensure_corridor_dem(elev_dir: &Path, db_path: &Path) -> anyhow::Result<(usize, usize, f64)> {
    fs::create_dir_all(elev_dir)?;
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let storage = Storage::open(db_path)?;
    let cache = ElevationCache::new(elev_dir);
    let tiles = bbox_to_tiles(CORRIDOR_BBOX);
    if tiles.iter().all(|t| cache.tile_exists(*t)) {
        return Ok((tiles.len(), tiles.len(), 0.0));
    }
    let started = std::time::Instant::now();
    let downloader = ElevationDownloader::new(storage.clone(), cache);
    let job = downloader.queue_region(CORRIDOR_BBOX)?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let record = rt.block_on(downloader.run_job(job.id, &DownloadControl::default()))?;
    if !matches!(record.status, JobStatus::Completed) {
        bail!("elevation job status: {:?}", record.status);
    }
    let store = ElevationJobStore::new(&storage);
    let (done, total) = store.progress(job.id)?;
    Ok((
        done as usize,
        total as usize,
        started.elapsed().as_secs_f64(),
    ))
}

/// Result of provisioning a test/app region directory.
#[derive(Debug, Clone)]
pub struct RegionProvision {
    pub pbf_path: PathBuf,
    pub elev_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub osm_downloaded_bytes: u64,
    pub dem_download_s: f64,
}

/// Provision region data into `data_dir`:
/// - download OSM PBF from `pbf_url` unless already present
/// - ensure DEM tiles for [`CORRIDOR_BBOX`] (network download, or optional
///   `elevation_tar_url` sibling for offline/test seeding)
pub fn provision_region(
    data_dir: &Path,
    pbf_url: &str,
    pbf_filename: &str,
) -> anyhow::Result<RegionProvision> {
    provision_region_with_elev_tar(data_dir, pbf_url, pbf_filename, None)
}

/// Like [`provision_region`], but if `elevation_tar_url` is set, download and
/// extract that tarball under `data_dir` before falling back to live DEM fetch.
pub fn provision_region_with_elev_tar(
    data_dir: &Path,
    pbf_url: &str,
    pbf_filename: &str,
    elevation_tar_url: Option<&str>,
) -> anyhow::Result<RegionProvision> {
    fs::create_dir_all(data_dir)?;
    let pbf_path = data_dir.join(pbf_filename);
    let elev_dir = data_dir.join("elevation");
    let cache_dir = data_dir.join("graph-cache");
    fs::create_dir_all(&elev_dir)?;
    fs::create_dir_all(&cache_dir)?;

    let mut osm_bytes = 0u64;
    let need_pbf = !pbf_path.is_file() || fs::metadata(&pbf_path)?.len() < 1_000_000;
    if need_pbf {
        osm_bytes = download_file(pbf_url, &pbf_path)?;
        if osm_bytes < 1_000_000 {
            bail!(
                "downloaded PBF too small ({osm_bytes} bytes) from {pbf_url} — refuse stub/empty"
            );
        }
    }

    if let Some(tar_url) = elevation_tar_url {
        let tar_path = data_dir.join("elevation-corridor.tar");
        let need_tar = !elev_dir.join("copernicus").is_dir()
            || fs::read_dir(elev_dir.join("copernicus"))
                .map(|rd| rd.count() == 0)
                .unwrap_or(true);
        if need_tar {
            let _ = download_file(tar_url, &tar_path)?;
            extract_tar_to(data_dir, &tar_path)?;
        }
        // When a fixture tar was supplied, never fall through to live Copernicus
        // (emulator may be offline). Require at least one corridor tile on disk.
        let cache = ElevationCache::new(&elev_dir);
        let tiles = bbox_to_tiles(CORRIDOR_BBOX);
        let present = tiles.iter().filter(|t| cache.tile_exists(**t)).count();
        if present == 0 {
            bail!(
                "elevation tar produced 0/{} corridor DEM tiles under {}",
                tiles.len(),
                elev_dir.display()
            );
        }
        return Ok(RegionProvision {
            pbf_path,
            elev_dir,
            cache_dir,
            osm_downloaded_bytes: osm_bytes,
            dem_download_s: 0.0,
        });
    }

    let (_, _, dem_s) = ensure_corridor_dem(&elev_dir, &data_dir.join("navi.db"))?;
    Ok(RegionProvision {
        pbf_path,
        elev_dir,
        cache_dir,
        osm_downloaded_bytes: osm_bytes,
        dem_download_s: dem_s,
    })
}

fn extract_tar_to(data_dir: &Path, tar_path: &Path) -> anyhow::Result<()> {
    let f = fs::File::open(tar_path)?;
    let mut archive = tar::Archive::new(f);
    archive.unpack(data_dir)?;
    Ok(())
}
