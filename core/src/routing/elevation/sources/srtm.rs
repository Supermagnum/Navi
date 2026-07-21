use std::io::{Cursor, Read, Write};
use std::path::Path;

use reqwest::header::{AUTHORIZATION, RANGE};
use reqwest::Client;

use super::DownloadResult;
use crate::routing::elevation::tile_id::HgtTileId;

const CMR_SEARCH: &str = "https://cmr.earthdata.nasa.gov/search/granules.json";

pub async fn download_tile(
    client: &Client,
    tile: HgtTileId,
    dest_dir: &Path,
    resume_from: u64,
    earthdata_token: Option<&str>,
) -> anyhow::Result<Option<DownloadResult>> {
    let Some(token) = earthdata_token else {
        return Ok(None);
    };

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

    let bbox = format!(
        "{},{},{},{}",
        tile.lon_floor,
        tile.lat_floor,
        tile.lon_floor + 1,
        tile.lat_floor + 1
    );
    let search = client
        .get(CMR_SEARCH)
        .query(&[
            ("short_name", "SRTMGL1"),
            ("version", "003"),
            ("bounding_box", bbox.as_str()),
            ("page_size", "5"),
        ])
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    let download_url = search["feed"]["entry"]
        .as_array()
        .and_then(|entries| entries.first())
        .and_then(|entry| {
            entry["links"]
                .as_array()?
                .iter()
                .find(|link| link["rel"] == "http://esipfed.org/ns/fedsearch/1.1/data#")
                .and_then(|link| link["href"].as_str())
        });

    let Some(url) = download_url else {
        return Ok(None);
    };
    let url = url.to_string();

    let mut request = client
        .get(&url)
        .header(AUTHORIZATION, format!("Bearer {token}"));
    if resume_from > 0 {
        request = request.header(RANGE, format!("bytes={resume_from}-"));
    }
    let response = request.send().await?;
    if !response.status().is_success() {
        return Ok(None);
    }

    let bytes = response.bytes().await?;
    if url.ends_with(".zip") {
        extract_hgt_from_zip(&bytes, &dest)?;
    } else if resume_from > 0 {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&dest)?;
        file.write_all(&bytes)?;
    } else {
        std::fs::write(&dest, &bytes)?;
    }

    Ok(Some(DownloadResult {
        local_path: dest.clone(),
        bytes: std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0),
        total_bytes: None,
        etag: None,
    }))
}

fn extract_hgt_from_zip(bytes: &[u8], dest: &Path) -> anyhow::Result<()> {
    use zip::ZipArchive;
    let mut archive = ZipArchive::new(Cursor::new(bytes))?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        if file.name().ends_with(".hgt") {
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)?;
            std::fs::write(dest, buffer)?;
            return Ok(());
        }
    }
    anyhow::bail!("no hgt in srtm zip")
}
