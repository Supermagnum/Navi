//! Shared HTTP download helpers: stream-to-disk, size-scaled timeouts, retries,
//! and actionable error classification (used by region PBF, elevation tiles,
//! OSM updates, and PMTiles range fetches).

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, bail, Context};
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, RANGE};
use reqwest::{Client, Response, StatusCode};
use tokio::io::AsyncWriteExt;

use crate::download::fsutil::{available_bytes, enrich_io_error};
use crate::download::progress as download_progress;

/// Default retry count for transient network failures.
pub const DEFAULT_RETRIES: u32 = 3;

/// Stream a full (or ranged) GET to `dest`, never buffering the whole body in RAM.
#[derive(Debug, Clone)]
pub struct StreamDownloadOpts<'a> {
    pub url: &'a str,
    pub dest: &'a Path,
    /// Extra request headers (Authorization, etc.).
    pub headers: HeaderMap,
    /// Resume from this byte offset (sends `Range: bytes={n}-`).
    pub resume_from: u64,
    /// Known expected total size (for timeout + progress); `None` uses Content-Length.
    pub expected_bytes: Option<u64>,
    pub retries: u32,
    pub progress_label: &'a str,
    /// When true, treat HTTP 404 as `Ok(None)` instead of an error.
    pub allow_not_found: bool,
}

#[derive(Debug, Clone)]
pub struct StreamDownloadResult {
    pub bytes: u64,
    pub total_bytes: Option<u64>,
    pub etag: Option<String>,
    pub status: StatusCode,
}

/// Per-request timeout: assume ~256 KiB/s floor on mobile Wi‑Fi, clamped.
pub fn timeout_for_bytes(len: u64) -> Duration {
    let secs = (len / (256 * 1024)).saturating_add(90).clamp(90, 900);
    Duration::from_secs(secs)
}

/// Classify reqwest failures so UI/logs distinguish timeout vs reset vs other.
pub fn format_reqwest_error(
    err: &reqwest::Error,
    context: &str,
    bytes_received: u64,
    expected: Option<u64>,
) -> String {
    let kind = if err.is_timeout() {
        "timeout"
    } else if err.is_connect() {
        "connection_failed"
    } else if err.is_request() {
        "request_failed"
    } else if err.is_body() || err.is_decode() {
        "body_interrupted"
    } else if err.is_status() {
        "http_status"
    } else {
        "network_error"
    };
    let detail = {
        let s = err.to_string();
        if let Some(idx) = s.find(" for url (") {
            s[..idx].to_string()
        } else {
            s
        }
    };
    let detail = if detail.trim().is_empty() {
        kind.to_string()
    } else {
        detail
    };
    match expected {
        Some(total) => format!("{kind} {context} received={bytes_received}/{total}: {detail}"),
        None => format!("{kind} {context} received={bytes_received}: {detail}"),
    }
}

fn describe_status(status: StatusCode) -> String {
    format!("http_status status={status}")
}

/// Build a default client with a high ceiling; per-request timeouts still apply.
pub fn http_client() -> anyhow::Result<Client> {
    Ok(Client::builder()
        .timeout(Duration::from_secs(900))
        .tcp_keepalive(Duration::from_secs(30))
        .pool_idle_timeout(Duration::from_secs(90))
        .build()?)
}

/// Stream `opts.url` to `opts.dest` with retries and durable `.partial` progress.
pub async fn stream_get_to_file(
    client: &Client,
    opts: StreamDownloadOpts<'_>,
) -> anyhow::Result<Option<StreamDownloadResult>> {
    if let Some(parent) = opts.dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut last_err: Option<anyhow::Error> = None;
    let retries = opts.retries.max(1);

    for attempt in 1..=retries {
        match stream_get_to_file_once(client, &opts).await {
            Ok(v) => {
                if attempt > 1 {
                    log::info!(
                        target: "NaviDownload",
                        "[NaviDownload] download ok after retry attempt={attempt} dest={}",
                        opts.dest.display()
                    );
                }
                return Ok(v);
            }
            Err(e) => {
                log::warn!(
                    target: "NaviDownload",
                    "[NaviDownload] download failed attempt={attempt}/{retries} dest={} err={e}",
                    opts.dest.display()
                );
                last_err = Some(e);
                if attempt < retries {
                    let backoff_ms = 500u64 * 3u64.pow(attempt - 1);
                    tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                }
            }
        }
    }

    Err(last_err.unwrap_or_else(|| anyhow!("download failed")))
}

async fn stream_get_to_file_once(
    client: &Client,
    opts: &StreamDownloadOpts<'_>,
) -> anyhow::Result<Option<StreamDownloadResult>> {
    let mut partial = opts.dest.as_os_str().to_owned();
    partial.push(".partial");
    let partial_path = PathBuf::from(partial);

    let mut resume_from = opts.resume_from;
    if resume_from == 0 {
        if let Ok(meta) = std::fs::metadata(&partial_path) {
            if meta.len() > 0 {
                resume_from = meta.len();
                log::info!(
                    target: "NaviDownload",
                    "[NaviDownload] resume partial dest={} from={resume_from}",
                    opts.dest.display()
                );
            }
        }
    }

    let timeout_hint = opts
        .expected_bytes
        .unwrap_or(32 * 1024 * 1024)
        .saturating_sub(resume_from)
        .max(1);
    let timeout = timeout_for_bytes(timeout_hint);

    let mut request = client.get(opts.url).timeout(timeout);
    for (k, v) in opts.headers.iter() {
        request = request.header(k, v);
    }
    if resume_from > 0 {
        request = request.header(RANGE, format!("bytes={resume_from}-"));
    }

    let response = request.send().await.map_err(|e| {
        anyhow!(format_reqwest_error(
            &e,
            &format!("GET {}", short_url(opts.url)),
            resume_from,
            opts.expected_bytes
        ))
    })?;

    let status = response.status();
    if status == StatusCode::NOT_FOUND && opts.allow_not_found {
        return Ok(None);
    }
    if resume_from > 0 && status != StatusCode::PARTIAL_CONTENT && status != StatusCode::OK {
        // Server rejected resume — restart from scratch on next attempt.
        let _ = std::fs::remove_file(&partial_path);
        return Err(anyhow!(
            "{} resume rejected for {} (got {status}); will retry from 0",
            describe_status(status),
            short_url(opts.url)
        ));
    }
    if resume_from == 0 && !status.is_success() {
        bail!("{} for {}", describe_status(status), short_url(opts.url));
    }
    if resume_from > 0 && status == StatusCode::OK {
        // Some servers ignore Range and return 200 with full body — rewrite file.
        let _ = std::fs::remove_file(&partial_path);
        resume_from = 0;
    }

    let etag = response
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let content_len = response.content_length();
    let expected = opts.expected_bytes.or_else(|| {
        content_len.map(|n| {
            if resume_from > 0 && status == StatusCode::PARTIAL_CONTENT {
                resume_from.saturating_add(n)
            } else {
                n
            }
        })
    });

    if let (Some(need), Some(free)) = (expected, available_bytes(opts.dest)) {
        let remaining = need.saturating_sub(resume_from);
        if free < remaining {
            bail!(
                "insufficient space for download: need {remaining} bytes, available {free} bytes at {}",
                opts.dest.display()
            );
        }
    }

    log::info!(
        target: "NaviDownload",
        "[NaviDownload] start url={} dest={} resume_from={resume_from} expected_bytes={:?} \
         timeout_s={} available_bytes={:?}",
        short_url(opts.url),
        opts.dest.display(),
        expected,
        timeout.as_secs(),
        available_bytes(opts.dest)
    );

    let mut file = if resume_from > 0 {
        tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&partial_path)
            .await
            .map_err(|e| enrich_io_error(e, &partial_path))?
    } else {
        let _ = std::fs::remove_file(&partial_path);
        tokio::fs::File::create(&partial_path)
            .await
            .map_err(|e| enrich_io_error(e, &partial_path))?
    };

    let mut written = resume_from;
    let mut last_logged = resume_from;
    let mut last_ui = resume_from;
    download_progress::set(written, expected, opts.progress_label);

    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| {
            anyhow!(format_reqwest_error(
                &e,
                &format!("body {}", short_url(opts.url)),
                written,
                expected
            ))
        })?;
        if let Err(e) = file.write_all(&chunk).await {
            return Err(enrich_io_error(e, &partial_path));
        }
        written += chunk.len() as u64;
        if written - last_ui >= 256 * 1024 || expected.is_some_and(|t| written >= t) {
            download_progress::set(written, expected, opts.progress_label);
            last_ui = written;
        }
        if written - last_logged >= 5 * 1024 * 1024 || expected.is_some_and(|t| written >= t) {
            let pct = expected.map(|t| {
                if t == 0 {
                    100
                } else {
                    (written.saturating_mul(100) / t).min(100)
                }
            });
            log::info!(
                target: "NaviDownload",
                "progress dest={} written={written} expected={:?} pct={:?} available_bytes={:?}",
                opts.dest.display(),
                expected,
                pct,
                available_bytes(opts.dest)
            );
            last_logged = written;
        }
    }

    file.flush()
        .await
        .map_err(|e| enrich_io_error(e, &partial_path))?;
    drop(file);

    if let Some(total) = expected {
        if written < total {
            // Keep partial for resume; surface a clear short-body error.
            bail!(
                "body_interrupted short body for {}: received {written}/{total} (connection closed early)",
                short_url(opts.url)
            );
        }
    }

    std::fs::rename(&partial_path, opts.dest).map_err(|e| enrich_io_error(e, opts.dest))?;
    download_progress::set(written, Some(written), opts.progress_label);
    log::info!(
        target: "NaviDownload",
        "[NaviDownload] complete dest={} bytes={written}",
        opts.dest.display()
    );

    Ok(Some(StreamDownloadResult {
        bytes: written,
        total_bytes: expected.or(Some(written)),
        etag,
        status,
    }))
}

/// Blocking wrapper around [`stream_get_to_file`].
pub fn stream_get_to_file_blocking(
    opts: StreamDownloadOpts<'_>,
) -> anyhow::Result<Option<StreamDownloadResult>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let client = http_client()?;
    rt.block_on(stream_get_to_file(&client, opts))
}

/// Convenience: set a single Authorization bearer header.
pub fn bearer_headers(token: &str) -> anyhow::Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static("authorization"),
        HeaderValue::from_str(&format!("Bearer {token}"))?,
    );
    Ok(headers)
}

fn short_url(url: &str) -> String {
    // Keep host + last path segment for logs.
    if let Ok(u) = reqwest::Url::parse(url) {
        let host = u.host_str().unwrap_or("");
        let last = u
            .path_segments()
            .and_then(|mut s| s.next_back())
            .unwrap_or("");
        if last.is_empty() {
            host.to_string()
        } else {
            format!("{host}/…/{last}")
        }
    } else {
        url.chars().take(80).collect()
    }
}

/// Drain a response body into memory only when the payload is known-small
/// (metadata / JSON). Prefer [`stream_get_to_file`] for tile/file bodies.
pub async fn read_body_text(response: Response, max_bytes: usize) -> anyhow::Result<bytes::Bytes> {
    let bytes = response.bytes().await.context("read body")?;
    if bytes.len() > max_bytes {
        bail!(
            "response body too large {} > {max_bytes} (refusing to buffer)",
            bytes.len()
        );
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_scales() {
        let small = timeout_for_bytes(1024);
        let large = timeout_for_bytes(64 * 1024 * 1024);
        assert!(small.as_secs() >= 90);
        assert!(large.as_secs() >= small.as_secs());
        assert!(large.as_secs() <= 900);
    }

    #[test]
    fn short_url_keeps_host_and_leaf() {
        let s = short_url("https://example.com/a/b/c/file.tif");
        assert!(s.contains("example.com"));
        assert!(s.contains("file.tif"));
    }

    #[tokio::test]
    #[ignore = "network: stream a small public object to disk"]
    async fn streams_small_http_object() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("tiny.bin");
        // Tiny known object from a CDN that supports GET (Mapterhorn header bytes via range
        // is covered elsewhere; use httpbin-like isn't always available — use Geofabrik robots).
        let url = "https://download.geofabrik.de/robots.txt";
        let client = http_client().unwrap();
        let result = stream_get_to_file(
            &client,
            StreamDownloadOpts {
                url,
                dest: &dest,
                headers: HeaderMap::new(),
                resume_from: 0,
                expected_bytes: None,
                retries: 2,
                progress_label: "test…",
                allow_not_found: false,
            },
        )
        .await
        .unwrap()
        .expect("body");
        assert!(dest.is_file());
        assert!(result.bytes > 0);
        assert_eq!(std::fs::metadata(&dest).unwrap().len(), result.bytes);
    }
}
