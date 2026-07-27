pub mod copernicus;
pub mod srtm;
pub mod viewfinder;

use std::path::{Path, PathBuf};

use crate::routing::elevation::tile_id::HgtTileId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemSource {
    Copernicus,
    Viewfinder,
    Srtm,
}

impl DemSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Copernicus => "copernicus",
            Self::Viewfinder => "viewfinder",
            Self::Srtm => "srtm",
        }
    }

    pub const FALLBACK_CHAIN: [DemSource; 3] = [Self::Copernicus, Self::Viewfinder, Self::Srtm];
}

#[derive(Debug, Clone)]
pub struct DownloadResult {
    pub local_path: PathBuf,
    pub bytes: u64,
    pub total_bytes: Option<u64>,
    pub etag: Option<String>,
}

pub async fn download_tile(
    source: DemSource,
    client: &reqwest::Client,
    tile: HgtTileId,
    dest_root: &Path,
    resume_from: u64,
    earthdata_token: Option<&str>,
) -> anyhow::Result<Option<DownloadResult>> {
    let dest_dir = dest_root.join(source.label());
    std::fs::create_dir_all(&dest_dir)?;
    match source {
        DemSource::Copernicus => {
            copernicus::download_tile(client, tile, &dest_dir, resume_from).await
        }
        DemSource::Viewfinder => {
            viewfinder::download_tile(client, tile, &dest_dir, resume_from).await
        }
        DemSource::Srtm => {
            srtm::download_tile(client, tile, &dest_dir, resume_from, earthdata_token).await
        }
    }
}
