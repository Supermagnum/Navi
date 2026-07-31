use std::path::Path;

use reqwest::header::HeaderMap;
use reqwest::Client;

use super::{DemSource, DownloadResult};
use crate::download::{stream_get_to_file, StreamDownloadOpts, DEFAULT_RETRIES};
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

    let Some(result) = stream_get_to_file(
        client,
        StreamDownloadOpts {
            url: &url,
            dest: &dest,
            headers: HeaderMap::new(),
            resume_from,
            expected_bytes: None,
            retries: DEFAULT_RETRIES,
            progress_label: "Downloading elevation…",
            allow_not_found: true,
        },
    )
    .await?
    else {
        return Ok(None);
    };

    Ok(Some(DownloadResult {
        local_path: dest,
        bytes: result.bytes,
        total_bytes: result.total_bytes,
        etag: result.etag,
    }))
}

#[allow(dead_code)]
pub fn source_label() -> &'static str {
    DemSource::Copernicus.label()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::download::http_client;
    use crate::routing::elevation::tile_id::HgtTileId;

    #[tokio::test]
    #[ignore = "network: live Copernicus COG stream-to-disk"]
    async fn streams_one_copernicus_tile() {
        let dir = tempfile::tempdir().unwrap();
        let tile = HgtTileId {
            lat_floor: 59,
            lon_floor: 10,
        };
        let client = http_client().unwrap();
        let result = download_tile(&client, tile, dir.path(), 0)
            .await
            .expect("download")
            .expect("tile present");
        assert!(result.local_path.is_file());
        assert!(
            result.bytes > 1_000_000,
            "expected multi-MB COG, got {}",
            result.bytes
        );
    }
}
