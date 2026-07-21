use std::io::Read;
use std::path::Path;

use reqwest::header::RANGE;
use reqwest::Client;
use zip::ZipArchive;

use super::DownloadResult;
use crate::routing::elevation::tile_id::HgtTileId;

const INDEX_BASE: &str = "https://viewfinderpanoramas.org/dem3";

pub async fn download_tile(
    client: &Client,
    tile: HgtTileId,
    dest_dir: &Path,
    resume_from: u64,
) -> anyhow::Result<Option<DownloadResult>> {
    let stem = tile.to_string();
    let dest = dest_dir.join(format!("{stem}.hgt"));
    if dest.exists() && resume_from == 0 {
        return Ok(Some(DownloadResult {
            local_path: dest.clone(),
            bytes: std::fs::metadata(&dest)?.len(),
            total_bytes: None,
            etag: None,
        }));
    }

    let index_url = format!("{INDEX_BASE}/{stem}.hgt.zip");
    let mut request = client.get(&index_url);
    if resume_from > 0 {
        request = request.header(RANGE, format!("bytes={resume_from}-"));
    }
    let response = request.send().await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        anyhow::bail!("viewfinder HTTP {}", response.status());
    }

    let zip_bytes = response.bytes().await?;
    let mut archive = ZipArchive::new(std::io::Cursor::new(zip_bytes.as_ref()))?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        if file.name().ends_with(".hgt") {
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)?;
            std::fs::write(&dest, &buffer)?;
            return Ok(Some(DownloadResult {
                local_path: dest,
                bytes: buffer.len() as u64,
                total_bytes: Some(buffer.len() as u64),
                etag: None,
            }));
        }
    }
    Ok(None)
}
