use std::io::Read;
use std::path::Path;

use reqwest::Client;

use super::DownloadResult;
use crate::download::{bearer_headers, stream_get_to_file, StreamDownloadOpts, DEFAULT_RETRIES};
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
    let headers = bearer_headers(token)?;

    let is_zip = url.ends_with(".zip");
    let fetch_dest = if is_zip {
        let mut p = dest.as_os_str().to_owned();
        p.push(".zip");
        std::path::PathBuf::from(p)
    } else {
        dest.clone()
    };

    let Some(result) = stream_get_to_file(
        client,
        StreamDownloadOpts {
            url: &url,
            dest: &fetch_dest,
            headers,
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

    if is_zip {
        extract_hgt_from_zip_file(&fetch_dest, &dest)?;
        let _ = std::fs::remove_file(&fetch_dest);
    }

    Ok(Some(DownloadResult {
        local_path: dest.clone(),
        bytes: std::fs::metadata(&dest)
            .map(|m| m.len())
            .unwrap_or(result.bytes),
        total_bytes: result.total_bytes,
        etag: result.etag,
    }))
}

fn extract_hgt_from_zip_file(zip_path: &Path, dest: &Path) -> anyhow::Result<()> {
    use zip::ZipArchive;
    let file = std::fs::File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;
    for i in 0..archive.len() {
        let mut zf = archive.by_index(i)?;
        if zf.name().ends_with(".hgt") {
            let mut buffer = Vec::new();
            zf.read_to_end(&mut buffer)?;
            std::fs::write(dest, buffer)?;
            return Ok(());
        }
    }
    anyhow::bail!("no hgt in srtm zip")
}
