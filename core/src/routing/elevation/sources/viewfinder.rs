use std::io::Read;
use std::path::Path;

use reqwest::header::HeaderMap;
use reqwest::Client;
use zip::ZipArchive;

use super::DownloadResult;
use crate::download::{stream_get_to_file, StreamDownloadOpts, DEFAULT_RETRIES};
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
    let zip_dest = {
        let mut p = dest.as_os_str().to_owned();
        p.push(".zip");
        std::path::PathBuf::from(p)
    };

    let Some(_) = stream_get_to_file(
        client,
        StreamDownloadOpts {
            url: &index_url,
            dest: &zip_dest,
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

    let file = std::fs::File::open(&zip_dest)?;
    let mut archive = ZipArchive::new(file)?;
    for i in 0..archive.len() {
        let mut zf = archive.by_index(i)?;
        if zf.name().ends_with(".hgt") {
            let mut buffer = Vec::new();
            zf.read_to_end(&mut buffer)?;
            std::fs::write(&dest, &buffer)?;
            let _ = std::fs::remove_file(&zip_dest);
            return Ok(Some(DownloadResult {
                local_path: dest,
                bytes: buffer.len() as u64,
                total_bytes: Some(buffer.len() as u64),
                etag: None,
            }));
        }
    }
    let _ = std::fs::remove_file(&zip_dest);
    Ok(None)
}
