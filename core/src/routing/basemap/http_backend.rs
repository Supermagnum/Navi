//! HTTP Range backend using workspace `reqwest` 0.12 (Android-safe TLS).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use pmtiles::{AsyncBackend, BackendResponse, PmtError, PmtResult};
use reqwest::header::RANGE;
use reqwest::{Client, StatusCode, Url};

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
}

impl AsyncBackend for Reqwest012Backend {
    async fn read(&self, offset: usize, length: usize) -> PmtResult<BackendResponse> {
        if let Some(c) = &self.request_counter {
            c.fetch_add(1, Ordering::Relaxed);
        }
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
