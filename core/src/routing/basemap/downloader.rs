//! PMTiles region jobs: bbox extract from the Protomaps public planet build.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use reqwest::Client;
use uuid::Uuid;

use crate::download::DownloadControl;
use crate::routing::basemap::extract::{
    extract_bbox_to_file, resolve_planet_url, DEFAULT_EXTRACT_MAX_ZOOM,
    PROTOMAPS_PLANET_FALLBACK_URL,
};
use crate::routing::basemap::regions::{
    geofabrik_path_to_region_key, region_bbox, sanitize_region_key,
};
use crate::storage::{PmtilesJobRecord, PmtilesJobStatus, PmtilesJobStore, Storage};

pub struct PmtilesJob {
    pub id: Uuid,
    pub record: PmtilesJobRecord,
}

pub struct PmtilesDownloader {
    storage: Storage,
    data_dir: PathBuf,
    client: Client,
    max_zoom: u8,
}

impl PmtilesDownloader {
    pub fn new(storage: Storage, data_dir: impl Into<PathBuf>) -> Self {
        Self {
            storage,
            data_dir: data_dir.into(),
            client: Client::builder()
                .timeout(Duration::from_secs(900))
                .build()
                .unwrap_or_else(|_| Client::new()),
            max_zoom: DEFAULT_EXTRACT_MAX_ZOOM,
        }
    }

    pub fn with_max_zoom(mut self, max_zoom: u8) -> Self {
        self.max_zoom = max_zoom;
        self
    }

    pub fn pmtiles_dir(&self) -> PathBuf {
        self.data_dir.join("pmtiles")
    }

    pub fn local_path_for_key(&self, region_key: &str) -> PathBuf {
        self.pmtiles_dir()
            .join(format!("{}.pmtiles", sanitize_region_key(region_key)))
    }

    /// Queue a bbox extract from the Protomaps planet (or override URL).
    ///
    /// `planet_url_override`: if set, use that URL; otherwise resolve latest from
    /// Protomaps builds metadata (or fall back to a known dated build).
    pub fn queue_geofabrik_region(
        &self,
        geofabrik_path: &str,
        planet_url_override: Option<&str>,
    ) -> anyhow::Result<PmtilesJob> {
        let region_key = geofabrik_path_to_region_key(geofabrik_path);
        let bbox = region_bbox(geofabrik_path)
            .ok_or_else(|| anyhow::anyhow!("no bbox for geofabrik path {geofabrik_path}"))?;
        let url = planet_url_override
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                // Prefer a fast sync fallback URL for queue; resolve at run time if empty.
                PROTOMAPS_PLANET_FALLBACK_URL.to_string()
            });
        self.queue_url(&region_key, &url, Some(bbox))
    }

    pub fn queue_url(
        &self,
        region_key: &str,
        planet_url: &str,
        bbox: Option<[f64; 4]>,
    ) -> anyhow::Result<PmtilesJob> {
        let key = sanitize_region_key(region_key);
        let local = self.local_path_for_key(&key);
        fs::create_dir_all(self.pmtiles_dir())?;
        let store = PmtilesJobStore::new(&self.storage);
        let record = store.create_job(&key, planet_url, &local, bbox)?;
        Ok(PmtilesJob {
            id: record.id,
            record,
        })
    }

    pub fn get_job(&self, job_id: Uuid) -> anyhow::Result<Option<PmtilesJobRecord>> {
        Ok(PmtilesJobStore::new(&self.storage).get_job(job_id)?)
    }

    pub fn list_jobs(&self) -> anyhow::Result<Vec<PmtilesJobRecord>> {
        Ok(PmtilesJobStore::new(&self.storage).list_jobs()?)
    }

    pub fn list_completed_covering(
        &self,
        lat: f64,
        lon: f64,
    ) -> anyhow::Result<Vec<PmtilesJobRecord>> {
        Ok(PmtilesJobStore::new(&self.storage).list_completed_covering(lat, lon)?)
    }

    pub fn delete_job(&self, job_id: Uuid) -> anyhow::Result<()> {
        let store = PmtilesJobStore::new(&self.storage);
        if let Some(job) = store.get_job(job_id)? {
            let path = PathBuf::from(&job.local_path);
            let _ = fs::remove_file(&path);
            let mut partial = path.as_os_str().to_owned();
            partial.push(".partial");
            let _ = fs::remove_file(Path::new(&partial));
            let staging = crate::routing::basemap::extract::chunk_staging_dir(&path);
            let _ = fs::remove_dir_all(&staging);
        }
        store.delete_job(job_id)?;
        Ok(())
    }

    pub fn run_job_blocking(
        &self,
        job_id: Uuid,
        control: &DownloadControl,
    ) -> anyhow::Result<PmtilesJobRecord> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        rt.block_on(self.run_job(job_id, control))
    }

    /// Range-extract the job bbox from the planet URL into the local `.pmtiles` path.
    pub async fn run_job(
        &self,
        job_id: Uuid,
        control: &DownloadControl,
    ) -> anyhow::Result<PmtilesJobRecord> {
        let store = PmtilesJobStore::new(&self.storage);
        let job = store
            .get_job(job_id)?
            .ok_or_else(|| anyhow::anyhow!("pmtiles job missing"))?;

        let bbox = match (job.min_lat, job.min_lon, job.max_lat, job.max_lon) {
            (Some(a), Some(b), Some(c), Some(d)) => [a, b, c, d],
            _ => anyhow::bail!("pmtiles job has no bbox"),
        };

        let final_path = PathBuf::from(&job.local_path);
        if final_path.is_file() && fs::metadata(&final_path)?.len() > 1000 {
            let len = fs::metadata(&final_path)?.len();
            store.set_progress(job_id, len, Some(len))?;
            store.set_status(job_id, PmtilesJobStatus::Completed, false)?;
            return store
                .get_job(job_id)?
                .ok_or_else(|| anyhow::anyhow!("job missing"));
        }

        let mut planet_url = job.url.clone();
        // Mapterhorn DEM uses a fixed third-party planet URL; Protomaps jobs always
        // resolve the current dated build at run time (queued URLs may be stale).
        let is_mapterhorn = planet_url.contains("mapterhorn.com");
        if !is_mapterhorn {
            match resolve_planet_url(&self.client).await {
                Ok(resolved) => {
                    if resolved != planet_url {
                        log::info!(
                            target: "NaviDownload",
                            "[NaviDownload] resolved protomaps planet url {planet_url} -> {resolved}"
                        );
                    }
                    planet_url = resolved;
                }
                Err(e) => {
                    log::warn!(
                        target: "NaviDownload",
                        "[NaviDownload] protomaps metadata resolve failed: {e:#}; using fallback"
                    );
                    planet_url = PROTOMAPS_PLANET_FALLBACK_URL.to_string();
                }
            }
        }

        let max_zoom = if job.region_key.starts_with("test_") {
            10
        } else {
            self.max_zoom
        };

        match extract_bbox_to_file(
            &planet_url,
            &final_path,
            bbox,
            max_zoom,
            control,
            Some((&self.storage, job_id)),
        )
        .await
        {
            Ok(_) => {}
            Err(e) => {
                if control.is_cancelled() {
                    store.set_status(job_id, PmtilesJobStatus::Cancelled, false)?;
                } else if !control.is_paused() {
                    store.set_status(job_id, PmtilesJobStatus::Failed, false)?;
                }
                return Err(e);
            }
        }

        store
            .get_job(job_id)?
            .ok_or_else(|| anyhow::anyhow!("job missing after extract"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[tokio::test]
    #[ignore = "network: real Protomaps extract + pause"]
    async fn extract_with_pause_resume_flags() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path().join("t.db")).unwrap();
        let dl = PmtilesDownloader::new(storage, dir.path()).with_max_zoom(7);
        let job = dl
            .queue_url(
                "oslo_test",
                PROTOMAPS_PLANET_FALLBACK_URL,
                Some([59.85, 10.6, 59.98, 10.9]),
            )
            .unwrap();
        let control = DownloadControl::default();
        let control2 = control.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(400));
            control2.pause();
            thread::sleep(Duration::from_millis(600));
            control2.resume();
        });
        let done = dl.run_job(job.id, &control).await.unwrap();
        assert_eq!(done.status, PmtilesJobStatus::Completed);
        assert!(PathBuf::from(&done.local_path).is_file());
    }

    #[test]
    fn pause_cancel_flags() {
        let c = DownloadControl::default();
        c.pause();
        assert!(c.is_paused());
        c.resume();
        assert!(!c.is_paused());
        c.cancel();
        assert!(c.is_cancelled());
        let _ = Arc::new(());
    }
}
