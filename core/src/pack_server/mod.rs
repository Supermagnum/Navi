//! navi-server pack host: connectivity + region discovery only.
//!
//! Contract: plain HTTP(S) `GET /current.json` (see Supermagnum/navi-server
//! `docs/client-fetch.md`). No auth, no custom protocol. Any failure is soft —
//! callers should fall through to Geofabrik (or equivalent), not surface a hard
//! error. Pack download / manifest verification are intentionally out of scope.
//!
//! ## Generation fields (important for future cache invalidation)
//!
//! `current.json` has two different "generation" strings:
//!
//! - **Catalog** (`PackCatalog::catalog_generation`): whatever the last script
//!   to rewrite `current.json` wrote — a real publish timestamp *or* a label
//!   like `migrate-geofabrik-paths`. Treat as "catalog last touched", **not** a
//!   monotonic / comparable bake id.
//! - **Per-region** ([`ReadyRegion::generation`]): the bake id under
//!   `/packs/<region_id>/<generation>/` (typically `20260904T…Z…`). This is the
//!   field to compare for "is this newer than what I have" once download/cache
//!   logic exists.

mod acquisition;

pub use acquisition::{
    normalize_region_id, pack_server_base_url, plan_region_acquisition, resolve_region_source,
    try_fetch_region_packs, RegionAcquisitionPlan, RegionSource,
};

use std::time::Duration;

use serde::Deserialize;

/// Default LAN pack host for development. Override via
/// [`check_connectivity`] / CLI / future app config — do not bake production
/// URLs into call sites. Intended public host later:
/// `https://navigate-me.duckdns.org`.
pub const DEFAULT_PACK_SERVER_BASE_URL: &str = "http://192.168.1.195";

/// Short timeout for the discovery GET (`/current.json`).
pub const CONNECTIVITY_TIMEOUT: Duration = Duration::from_secs(5);

/// One ready region from `current.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadyRegion {
    pub region_id: String,
    /// Bake / pack-tree generation for this region (trustworthy for
    /// freshness comparison). Absent only if the catalog omitted it.
    pub generation: Option<String>,
    pub bytes: Option<u64>,
}

/// Successful discovery payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackCatalog {
    /// Top-level `current.json` `generation`: catalog last-touched marker.
    /// **Do not** use for monotonic / "is newer" comparison — see module docs.
    pub catalog_generation: String,
    pub regions: Vec<ReadyRegion>,
}

/// Soft connectivity / discovery outcome. Never panics; failures are
/// `Unreachable` so callers can use the Geofabrik fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Connectivity {
    Ready(PackCatalog),
    /// Host unreachable, not published yet, bad JSON, non-2xx, etc.
    Unreachable { reason: String },
}

impl Connectivity {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready(_))
    }

    pub fn catalog(&self) -> Option<&PackCatalog> {
        match self {
            Self::Ready(c) => Some(c),
            Self::Unreachable { .. } => None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct CurrentJson {
    generation: String,
    #[serde(default)]
    regions: Vec<CurrentRegion>,
}

#[derive(Debug, Deserialize)]
struct CurrentRegion {
    region_id: String,
    #[serde(default)]
    generation: Option<String>,
    #[serde(default)]
    bytes: Option<u64>,
}

fn current_json_url(base_url: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    format!("{base}/current.json")
}

fn unreachable(reason: impl Into<String>) -> Connectivity {
    Connectivity::Unreachable {
        reason: reason.into(),
    }
}

fn parse_current_json(body: &str) -> Connectivity {
    let parsed: CurrentJson = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return unreachable(format!("malformed current.json: {e}")),
    };
    if parsed.generation.trim().is_empty() {
        return unreachable("malformed current.json: empty catalog generation");
    }
    let regions = parsed
        .regions
        .into_iter()
        .filter(|r| !r.region_id.trim().is_empty())
        .map(|r| ReadyRegion {
            region_id: r.region_id,
            generation: r
                .generation
                .map(|g| g.trim().to_string())
                .filter(|g| !g.is_empty()),
            bytes: r.bytes,
        })
        .collect();
    Connectivity::Ready(PackCatalog {
        catalog_generation: parsed.generation,
        regions,
    })
}

/// `GET {base_url}/current.json` with a short timeout.
///
/// On HTTP 200 + valid JSON returns [`Connectivity::Ready`]. On timeout, DNS,
/// connect failure, non-2xx (including 404), or malformed JSON returns
/// [`Connectivity::Unreachable`] — never panics.
pub async fn check_connectivity(base_url: &str) -> Connectivity {
    let url = current_json_url(base_url);
    let client = match reqwest::Client::builder()
        .timeout(CONNECTIVITY_TIMEOUT)
        .build()
    {
        Ok(c) => c,
        Err(e) => return unreachable(format!("http client: {e}")),
    };

    let response = match client.get(&url).timeout(CONNECTIVITY_TIMEOUT).send().await {
        Ok(r) => r,
        Err(e) => {
            let kind = if e.is_timeout() {
                "timeout"
            } else if e.is_connect() {
                "connection_failed"
            } else {
                "network_error"
            };
            return unreachable(format!("{kind}: {e}"));
        }
    };

    let status = response.status();
    if !status.is_success() {
        return unreachable(format!(
            "not ready (HTTP {status}) — use Geofabrik fallback"
        ));
    }

    let body = match response.text().await {
        Ok(t) => t,
        Err(e) => return unreachable(format!("read body failed: {e}")),
    };

    parse_current_json(&body)
}

/// Blocking wrapper around [`check_connectivity`].
pub fn check_connectivity_blocking(base_url: &str) -> Connectivity {
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => return unreachable(format!("runtime: {e}")),
    };
    rt.block_on(check_connectivity(base_url))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn serve_once(status_line: &str, body: &str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let status = status_line.to_string();
        let body = body.to_string();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let resp = format!(
                "{status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
        });
        format!("http://{addr}")
    }

    #[test]
    fn joins_current_json_without_double_slash() {
        assert_eq!(
            current_json_url("http://example.com"),
            "http://example.com/current.json"
        );
        assert_eq!(
            current_json_url("http://example.com/"),
            "http://example.com/current.json"
        );
    }

    #[test]
    fn parses_minimal_catalog() {
        let json = r#"{
            "schema": 1,
            "generation": "20260904T120000Z",
            "packs_base": "/packs",
            "regions": [
                {
                    "region_id": "asia/china/anhui",
                    "generation": "20260904T120000Z",
                    "manifest_url": "/packs/asia/china/anhui/20260904T120000Z/manifest.json",
                    "bytes": 12345678
                },
                { "region_id": "europe/andorra" }
            ]
        }"#;
        match parse_current_json(json) {
            Connectivity::Ready(c) => {
                assert_eq!(c.catalog_generation, "20260904T120000Z");
                assert_eq!(c.regions.len(), 2);
                assert_eq!(c.regions[0].region_id, "asia/china/anhui");
                assert_eq!(
                    c.regions[0].generation.as_deref(),
                    Some("20260904T120000Z")
                );
                assert_eq!(c.regions[0].bytes, Some(12_345_678));
                assert_eq!(c.regions[1].region_id, "europe/andorra");
                assert_eq!(c.regions[1].generation, None);
                assert_eq!(c.regions[1].bytes, None);
            }
            Connectivity::Unreachable { reason } => panic!("expected Ready, got {reason}"),
        }
    }

    #[test]
    fn migration_catalog_generation_is_not_region_bake_id() {
        // Live host after migrate-published-to-geofabrik-paths.sh: top-level
        // generation is a script label; per-region generations stay bake ids.
        let json = r#"{
            "schema": 1,
            "generation": "migrate-geofabrik-paths",
            "regions": [
                {
                    "region_id": "europe/monaco",
                    "generation": "20260904T113619Z-2762746-europe_monaco-9df05929",
                    "bytes": 3063609
                }
            ]
        }"#;
        match parse_current_json(json) {
            Connectivity::Ready(c) => {
                assert_eq!(c.catalog_generation, "migrate-geofabrik-paths");
                assert_eq!(
                    c.regions[0].generation.as_deref(),
                    Some("20260904T113619Z-2762746-europe_monaco-9df05929")
                );
                // Future cache logic must compare region.generation, not this.
                assert_ne!(
                    c.catalog_generation,
                    c.regions[0].generation.as_deref().unwrap()
                );
            }
            Connectivity::Unreachable { reason } => panic!("expected Ready, got {reason}"),
        }
    }

    #[test]
    fn malformed_json_is_unreachable() {
        match parse_current_json("{not-json") {
            Connectivity::Unreachable { reason } => {
                assert!(reason.contains("malformed"), "{reason}");
            }
            Connectivity::Ready(_) => panic!("expected Unreachable"),
        }
    }

    #[tokio::test]
    async fn mock_ready_returns_regions() {
        let base = serve_once(
            "HTTP/1.1 200 OK",
            r#"{"generation":"gen-1","regions":[{"region_id":"europe/monaco","generation":"20260904T113619Z","bytes":99}]}"#,
        );
        match check_connectivity(&base).await {
            Connectivity::Ready(c) => {
                assert_eq!(c.catalog_generation, "gen-1");
                assert_eq!(c.regions.len(), 1);
                assert_eq!(c.regions[0].region_id, "europe/monaco");
                assert_eq!(c.regions[0].generation.as_deref(), Some("20260904T113619Z"));
                assert_eq!(c.regions[0].bytes, Some(99));
            }
            Connectivity::Unreachable { reason } => panic!("expected Ready: {reason}"),
        }
    }

    #[tokio::test]
    async fn mock_404_is_soft_unreachable() {
        let base = serve_once("HTTP/1.1 404 Not Found", "missing");
        match check_connectivity(&base).await {
            Connectivity::Unreachable { reason } => {
                assert!(reason.contains("404") || reason.contains("not ready"), "{reason}");
            }
            Connectivity::Ready(_) => panic!("404 must not be Ready"),
        }
    }

    #[tokio::test]
    async fn unreachable_host_fails_soft() {
        // Reserved TEST-NET — should fail to connect quickly.
        let status = check_connectivity("http://192.0.2.1:9").await;
        match status {
            Connectivity::Unreachable { reason } => {
                assert!(!reason.is_empty());
            }
            Connectivity::Ready(_) => panic!("bogus host must not be Ready"),
        }
    }

    /// Manual check against the LAN pack host (or `NAVI_PACK_SERVER_BASE_URL`).
    #[tokio::test]
    #[ignore = "network: live navi-server pack host"]
    async fn live_pack_host_discovery() {
        let base = std::env::var("NAVI_PACK_SERVER_BASE_URL")
            .unwrap_or_else(|_| DEFAULT_PACK_SERVER_BASE_URL.to_string());
        let status = check_connectivity(&base).await;
        match &status {
            Connectivity::Ready(c) => {
                eprintln!(
                    "reachable catalog_generation={} (not for freshness compare)",
                    c.catalog_generation
                );
                for r in &c.regions {
                    let gen = r.generation.as_deref().unwrap_or("(missing)");
                    match r.bytes {
                        Some(b) => eprintln!("  {}  generation={gen}  bytes={b}", r.region_id),
                        None => eprintln!("  {}  generation={gen}", r.region_id),
                    }
                }
                assert!(!c.catalog_generation.is_empty());
            }
            Connectivity::Unreachable { reason } => {
                eprintln!("unreachable / not ready: {reason}");
                // Soft outcome is valid (nothing published / host down).
            }
        }
    }
}
