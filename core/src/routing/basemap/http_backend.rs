//! HTTP Range backend using workspace `reqwest` 0.12 (Android-safe TLS).

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use pmtiles::{AsyncBackend, BackendResponse, PmtError, PmtResult};
use reqwest::header::RANGE;
use reqwest::{Client, StatusCode, Url};
use tokio::io::AsyncWriteExt;

#[derive(Clone)]
pub struct Reqwest012Backend {
    client: Client,
    url: Url,
    request_counter: Option<Arc<AtomicU64>>,
}

impl Reqwest012Backend {
    pub fn try_from(client: Client, url: &str) -> anyhow::Result<Self> {
        Ok(Self {
            client,
            url: Url::parse(url)?,
            request_counter: None,
        })
    }

    pub fn with_request_counter(mut self, counter: Arc<AtomicU64>) -> Self {
        self.request_counter = Some(counter);
        self
    }

    fn bump_counter(&self) {
        if let Some(c) = &self.request_counter {
            c.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Stream a Range GET into `dest`, writing incrementally (never buffers the full body).
    ///
    /// If `dest` already exists with exactly `length` bytes, the download is skipped (resume).
    /// Incomplete files are written via a sibling `.partial` then renamed.
    pub async fn read_range_to_path(
        &self,
        offset: usize,
        length: usize,
        dest: &Path,
        timeout: Duration,
    ) -> anyhow::Result<u64> {
        if length == 0 {
            anyhow::bail!("range length is 0");
        }
        if dest.is_file() {
            if let Ok(meta) = std::fs::metadata(dest) {
                if meta.len() == length as u64 {
                    return Ok(length as u64);
                }
            }
            let _ = std::fs::remove_file(dest);
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut partial = dest.as_os_str().to_owned();
        partial.push(".partial");
        let partial_path = std::path::PathBuf::from(partial);
        let _ = std::fs::remove_file(&partial_path);

        self.bump_counter();
        let end = offset + length - 1;
        let range = format!("bytes={offset}-{end}");

        let response = self
            .client
            .get(self.url.clone())
            .header(RANGE, &range)
            .timeout(timeout)
            .send()
            .await
            .map_err(|e| {
                anyhow::anyhow!(crate::download::format_reqwest_error(
                    &e,
                    &range,
                    0,
                    Some(length as u64)
                ))
            })?;

        if response.status() != StatusCode::PARTIAL_CONTENT {
            anyhow::bail!(
                "range unsupported or unexpected status {} for {range} (wanted {length} bytes)",
                response.status()
            );
        }

        let mut file = tokio::fs::File::create(&partial_path)
            .await
            .map_err(|e| anyhow::anyhow!("create chunk file {}: {e}", partial_path.display()))?;
        let mut stream = response.bytes_stream();
        let mut written: u64 = 0;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| {
                anyhow::anyhow!(crate::download::format_reqwest_error(
                    &e,
                    &range,
                    written,
                    Some(length as u64)
                ))
            })?;
            file.write_all(&chunk).await.map_err(|e| {
                anyhow::anyhow!(
                    "write chunk file {} at {written}/{length}: {e}",
                    partial_path.display()
                )
            })?;
            written += chunk.len() as u64;
            if written > length as u64 {
                let _ = std::fs::remove_file(&partial_path);
                anyhow::bail!("response body too long for {range}: got {written} > {length}");
            }
        }
        file.flush()
            .await
            .map_err(|e| anyhow::anyhow!("flush chunk file {}: {e}", partial_path.display()))?;
        drop(file);

        if written != length as u64 {
            let _ = std::fs::remove_file(&partial_path);
            anyhow::bail!(
                "short body for {range}: received {written}/{length} (connection closed early)"
            );
        }
        std::fs::rename(&partial_path, dest).map_err(|e| {
            anyhow::anyhow!(
                "rename chunk {} -> {}: {e}",
                partial_path.display(),
                dest.display()
            )
        })?;
        Ok(written)
    }
}

impl AsyncBackend for Reqwest012Backend {
    async fn read(&self, offset: usize, length: usize) -> PmtResult<BackendResponse> {
        self.bump_counter();
        let end = offset + length - 1;
        let range = format!("bytes={offset}-{end}");

        let response = self
            .client
            .get(self.url.clone())
            .header(RANGE, range)
            .send()
            .await
            .map_err(|e| PmtError::Reading(std::io::Error::other(e.to_string())))?;

        if response.status() != StatusCode::PARTIAL_CONTENT {
            return Err(PmtError::Reading(std::io::Error::other(format!(
                "range unsupported or unexpected status {}",
                response.status()
            ))));
        }

        let etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .or_else(|| response.headers().get(reqwest::header::LAST_MODIFIED))
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);

        let bytes = response
            .bytes()
            .await
            .map_err(|e| PmtError::Reading(std::io::Error::other(e.to_string())))?;

        if bytes.len() > length {
            return Err(PmtError::Reading(std::io::Error::other(format!(
                "response body too long {} > {length}",
                bytes.len()
            ))));
        }

        // Keep the reqwest Bytes buffer; do not allocate a second full copy.
        Ok(match etag {
            Some(v) => BackendResponse::new_with_version(bytes, v),
            None => BackendResponse::new(bytes),
        })
    }
}
