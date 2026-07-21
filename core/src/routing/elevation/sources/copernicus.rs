use std::io::Write;
use std::path::Path;

use reqwest::header::{RANGE, ETAG};
use reqwest::Client;

use super::{DownloadResult, DemSource};
use crate::routing::elevation::tile_id::HgtTileId;

const COPERNICUS_BUCKET: &str = "https://copernicus-dem-30m.s3.eu-central-1.amazonaws.com";

fn prefix(tile: HgtTileId) -> String {
    let ns = if tile.lat_floor >= 0 { 'N' } else { 'S' };
    let ew = if tile.lon_floor >= 0 { 'E' } else { 'W' };
    format!(
        "Copernicus_DSM_COG_10_{ns}{:02}_00_{ew}{:03}_00_DEM",
        tile.lat_floor.abs(),
        tile.lon_floor.abs()
    )
}

pub async fn download_tile(
    client: &Client,
    tile: HgtTileId,
    dest_dir: &Path,
    resume_from: u64,
) -> anyhow::Result<Option<DownloadResult>> {
    let p = prefix(tile);
    let url = format!("{COPERNICUS_BUCKET}/{p}/{p}.tif");
    let tile_dir = dest_dir.join(tile.to_string());
    std::fs::create_dir_all(&tile_dir)?;
    let dest = tile_dir.join(format!("{p}.tif"));

    if dest.exists() && resume_from == 0 {
        return Ok(Some(DownloadResult {
            local_path: dest.clone(),
            bytes: std::fs::metadata(&dest)?.len(),
            total_bytes: None,
            etag: None,
        }));
    }

    let mut request = client.get(&url);
    if resume_from > 0 {
        request = request.header(RANGE, format!("bytes={resume_from}-"));
    }

    let response = request.send().await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        anyhow::bail!("copernicus HTTP {}", response.status());
    }

    let etag = response
        .headers()
        .get(ETAG)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let bytes_body = response.bytes().await?;

    if resume_from > 0 {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&dest)?;
        file.write_all(&bytes_body)?;
    } else {
        std::fs::write(&dest, &bytes_body)?;
    }

    Ok(Some(DownloadResult {
        local_path: dest.clone(),
        bytes: std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0),
        total_bytes: None,
        etag,
    }))
}

#[allow(dead_code)]
pub fn source_label() -> &'static str {
    DemSource::Copernicus.label()
}
