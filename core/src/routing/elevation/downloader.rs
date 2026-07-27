use reqwest::Client;
use uuid::Uuid;

use crate::download::DownloadControl;
use crate::routing::elevation::cache::ElevationCache;
use crate::routing::elevation::sources::{download_tile, DemSource};
use crate::routing::elevation::tile_id::{bbox_to_tiles, country_bbox, HgtTileId};
use crate::storage::{
    ElevationJobRecord, ElevationJobStore, JobScope, JobStatus, Storage, TileStatus,
};

pub struct ElevationJob {
    pub id: Uuid,
    pub record: ElevationJobRecord,
}

pub struct ElevationDownloader {
    storage: Storage,
    cache: ElevationCache,
    client: Client,
    earthdata_token: Option<String>,
}

impl ElevationDownloader {
    pub fn new(storage: Storage, cache: ElevationCache) -> Self {
        Self {
            storage,
            cache,
            client: Client::new(),
            earthdata_token: None,
        }
    }

    pub fn with_earthdata_token(mut self, token: impl Into<String>) -> Self {
        self.earthdata_token = Some(token.into());
        self
    }

    pub fn queue_region(&self, bbox: [f64; 4]) -> anyhow::Result<ElevationJob> {
        let tiles = bbox_to_tiles(bbox);
        let store = ElevationJobStore::new(&self.storage);
        let record = store.create_job(JobScope::Region, None, bbox, &tiles)?;
        Ok(ElevationJob {
            id: record.id,
            record,
        })
    }

    pub fn queue_country(&self, country_code: &str) -> anyhow::Result<ElevationJob> {
        let bbox = country_bbox(country_code)
            .ok_or_else(|| anyhow::anyhow!("unknown country code: {country_code}"))?;
        let tiles = bbox_to_tiles(bbox);
        let store = ElevationJobStore::new(&self.storage);
        let record = store.create_job(JobScope::Country, Some(country_code), bbox, &tiles)?;
        Ok(ElevationJob {
            id: record.id,
            record,
        })
    }

    pub async fn run_job(
        &self,
        job_id: Uuid,
        control: &DownloadControl,
    ) -> anyhow::Result<ElevationJobRecord> {
        let store = ElevationJobStore::new(&self.storage);
        store.set_job_status(job_id, JobStatus::Running)?;
        store.set_paused(job_id, false)?;

        loop {
            if control.is_cancelled() {
                store.set_job_status(job_id, JobStatus::Cancelled)?;
                break;
            }
            while control.is_paused() {
                store.set_paused(job_id, true)?;
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                if control.is_cancelled() {
                    store.set_job_status(job_id, JobStatus::Cancelled)?;
                    return store
                        .get_job(job_id)?
                        .ok_or_else(|| anyhow::anyhow!("job missing"));
                }
            }
            store.set_paused(job_id, false)?;

            let pending = store.pending_tiles(job_id)?;
            if pending.is_empty() {
                store.set_job_status(job_id, JobStatus::Completed)?;
                break;
            }

            let tile = pending[0].clone();
            let tile_id: HgtTileId = tile
                .tile_id
                .parse()
                .map_err(|_| anyhow::anyhow!("invalid tile id {}", tile.tile_id))?;

            if self.cache.tile_exists(tile_id) {
                if let Some(path) = self.cache.resolve_local_path(tile_id) {
                    store.update_tile(
                        job_id,
                        &tile.tile_id,
                        TileStatus::Completed,
                        None,
                        0,
                        None,
                        None,
                        Some(path.to_string_lossy().as_ref()),
                    )?;
                    continue;
                }
            }

            let mut downloaded = false;
            for source in DemSource::FALLBACK_CHAIN {
                store.set_active_source(job_id, source.label())?;
                store.update_tile(
                    job_id,
                    &tile.tile_id,
                    TileStatus::Downloading,
                    Some(source.label()),
                    tile.bytes_received,
                    tile.total_bytes,
                    tile.etag.as_deref(),
                    tile.local_path.as_deref(),
                )?;

                let result = download_tile(
                    source,
                    &self.client,
                    tile_id,
                    self.cache.data_dir(),
                    tile.bytes_received,
                    self.earthdata_token.as_deref(),
                )
                .await?;

                if let Some(result) = result {
                    self.cache.invalidate(tile_id);
                    store.update_tile(
                        job_id,
                        &tile.tile_id,
                        TileStatus::Completed,
                        Some(source.label()),
                        result.bytes,
                        result.total_bytes,
                        result.etag.as_deref(),
                        Some(result.local_path.to_string_lossy().as_ref()),
                    )?;
                    downloaded = true;
                    break;
                }
            }

            if !downloaded {
                store.update_tile(
                    job_id,
                    &tile.tile_id,
                    TileStatus::Failed,
                    None,
                    tile.bytes_received,
                    tile.total_bytes,
                    tile.etag.as_deref(),
                    tile.local_path.as_deref(),
                )?;
            }
        }

        store
            .get_job(job_id)?
            .ok_or_else(|| anyhow::anyhow!("job missing after run"))
    }

    pub fn resume_job(&self, job_id: Uuid) -> ElevationJob {
        ElevationJob {
            id: job_id,
            record: ElevationJobStore::new(&self.storage)
                .get_job(job_id)
                .ok()
                .flatten()
                .expect("job must exist to resume"),
        }
    }
}
