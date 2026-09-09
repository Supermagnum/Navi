//! navi-server DocumentRoot client: catalog discovery + shared HTTP helpers.
//!
//! Merges two prior workstreams:
//! - **package-test** catalog-aware [`check_connectivity`] (`GET /current.json` →
//!   [`Connectivity`] / [`PackCatalog`]) and acquisition routing
//! - **DATEX** read-only GET helpers ([`http_get_text`], [`http_get_bytes`],
//!   [`base_url`], [`USER_AGENT`], [`PackServerError`]) plus a cheap liveness
//!   probe renamed to [`probe_current_json`] so it does not shadow catalog
//!   discovery
//!
//! Contract: plain HTTP(S) GET/HEAD (navi-server `docs/client-fetch.md`). No
//! auth, no custom protocol. Discovery failures are soft —
//! [`Connectivity::Unreachable`] — so callers fall through to Geofabrik.
//!
//! ## Host fallback chain
//!
//! [`check_connectivity_chain`] / [`plan_region_acquisition`] probe:
//! 1. Public pack host — [`DEFAULT_PACK_SERVER_BASE_URL`]
//!    (`https://navigate-me.duckdns.org`)
//! 2. Callers then run local Geofabrik + on-device convert (`local-bake`)
//!
//! Per-host connect timeout is [`CONNECTIVITY_TIMEOUT`] (3s). Resolved source
//! tags: `server-duckdns` / `local-bake` ([`PackDataSource`]).
//!
//! ## Generation fields
//!
//! `current.json` has two different "generation" strings:
//!
//! - **Catalog** (`PackCatalog::catalog_generation`): last rewrite of
//!   `current.json` (publish timestamp *or* a label like
//!   `migrate-geofabrik-paths`). Not for freshness compare.
//! - **Per-region** ([`ReadyRegion::generation`]): bake id under
//!   `/packs/<region_id>/<generation>/`. Use this for cache invalidation.

mod acquisition;
mod fetch;
mod place_index_after;

pub use acquisition::{
    discover_pack_catalog, leaf_stem_for_region_id, normalize_region_id,
    pack_catalog_region_id_aliases, pack_server_base_url, pack_server_discovery_bases,
    path_covered_by_ready_ids, plan_region_acquisition, region_ids_match_for_catalog,
    resolve_region_source, PackCatalogSnapshot, PackDataSource, RegionAcquisitionPlan,
    RegionSource,
};
pub use fetch::{try_fetch_region_packs, ServerInstallStamp};
pub use place_index_after::{
    ensure_geofabrik_pbf_for_region, ensure_place_index_after_pack_install, PackPlaceIndexReport,
    MIN_REAL_PBF_BYTES, PLACE_INDEX_DB_NAME,
};

use std::time::Duration;

use serde::Deserialize;
use thiserror::Error;

/// Identifying User-Agent for pack / DATEX GETs against a navi-server.
pub const USER_AGENT: &str = "Navi/0.1.0 https://github.com/Supermagnum/Navi";

/// Public pack / DATEX host (sole default in the discovery chain).
pub const DEFAULT_PACK_SERVER_BASE_URL: &str = "https://navigate-me.duckdns.org";

/// Per-host connect + discovery GET timeout (short so the chain stays snappy).
pub const CONNECTIVITY_TIMEOUT: Duration = Duration::from_secs(3);

/// Default body read timeout for larger GETs (DATEX XML, future pack files).
const BODY_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Error)]
pub enum PackServerError {
    #[error("http {0}")]
    Http(u16),
    #[error("timeout")]
    Timeout,
    #[error("{0}")]
    Other(String),
}

/// Build `http://{host}:{port}` with no trailing slash. Port `80` omits `:80`.
pub fn base_url(host: &str, port: u16) -> String {
    let host = host.trim().trim_end_matches('/');
    if port == 80 {
        format!("http://{host}")
    } else {
        format!("http://{host}:{port}")
    }
}

/// Cheap path probe: `GET {base}{path}` with [`CONNECTIVITY_TIMEOUT`].
///
/// Returns `Ok(true)` on HTTP 200, `Ok(false)` on 404 (host up, resource
/// missing), and `Err` on transport / other HTTP failures. Used by
/// [`probe_current_json`] and DATEX `/datex/source.json` checks — do not
/// duplicate this HTTP shape elsewhere.
pub fn probe_path(base: &str, path: &str) -> Result<bool, PackServerError> {
    let base = base.trim().trim_end_matches('/');
    let path = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    let url = format!("{base}{path}");
    match http_get_text(&url, CONNECTIVITY_TIMEOUT) {
        Ok(_) => Ok(true),
        Err(PackServerError::Http(404)) => Ok(false),
        Err(e) => Err(e),
    }
}

/// Cheap liveness probe: `GET {base}/current.json`.
///
/// Renamed from the DATEX-era `check_connectivity` bool probe so it does not
/// shadow catalog-aware [`check_connectivity`].
pub fn probe_current_json(base: &str) -> Result<bool, PackServerError> {
    probe_path(base, "/current.json")
}

/// Tag string for UI / logs. DATEX uses `None` → `"none"` (no local-bake).
pub fn data_source_tag(src: Option<PackDataSource>) -> &'static str {
    match src {
        Some(s) => s.as_str(),
        None => "none",
    }
}

/// `GET` a URL and return the response body as UTF-8 text.
pub fn http_get_text(url: &str, timeout: Duration) -> Result<String, PackServerError> {
    let bytes = http_get_bytes(url, timeout)?;
    String::from_utf8(bytes).map_err(|e| PackServerError::Other(format!("utf-8: {e}")))
}

/// `GET` a URL and return the raw body bytes.
pub fn http_get_bytes(url: &str, timeout: Duration) -> Result<Vec<u8>, PackServerError> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| PackServerError::Other(e.to_string()))?;
    rt.block_on(http_get_bytes_async(url, timeout))
}

async fn http_get_bytes_async(url: &str, timeout: Duration) -> Result<Vec<u8>, PackServerError> {
    let connect = timeout.min(CONNECTIVITY_TIMEOUT);
    let client = reqwest::Client::builder()
        .timeout(timeout.max(BODY_TIMEOUT))
        .connect_timeout(connect)
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| PackServerError::Other(e.to_string()))?;

    let resp = client.get(url).send().await.map_err(map_reqwest_err)?;
    let status = resp.status();
    if !status.is_success() {
        return Err(PackServerError::Http(status.as_u16()));
    }
    resp.bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(map_reqwest_err)
}

fn map_reqwest_err(e: reqwest::Error) -> PackServerError {
    if e.is_timeout() || e.is_connect() {
        PackServerError::Timeout
    } else if let Some(status) = e.status() {
        PackServerError::Http(status.as_u16())
    } else {
        PackServerError::Other(e.to_string())
    }
}

/// One ready region from `current.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadyRegion {
    pub region_id: String,
    /// Bake / pack-tree generation for this region (trustworthy for freshness).
    pub generation: Option<String>,
    pub bytes: Option<u64>,
    /// Relative or absolute manifest URL from the catalog (when published).
    pub manifest_url: Option<String>,
}

/// Successful discovery payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackCatalog {
    /// Top-level `current.json` `generation`: catalog last-touched marker.
    pub catalog_generation: String,
    pub regions: Vec<ReadyRegion>,
    /// Host base URL that served this catalog (no trailing slash).
    pub served_from: String,
}

/// Soft connectivity / discovery outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Connectivity {
    Ready(PackCatalog),
    /// Host unreachable, not published yet, bad JSON, non-2xx, etc.
    Unreachable {
        reason: String,
    },
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
    #[serde(default)]
    manifest_url: Option<String>,
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

fn parse_current_json(body: &str, served_from: &str) -> Connectivity {
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
            manifest_url: r
                .manifest_url
                .map(|u| u.trim().to_string())
                .filter(|u| !u.is_empty()),
        })
        .collect();
    Connectivity::Ready(PackCatalog {
        catalog_generation: parsed.generation,
        regions,
        served_from: served_from.trim().trim_end_matches('/').to_string(),
    })
}

/// `GET {base_url}/current.json` with a short timeout; parse into a catalog.
///
/// On HTTP 200 + valid JSON returns [`Connectivity::Ready`]. On timeout, DNS,
/// connect failure, non-2xx (including 404), or malformed JSON returns
/// [`Connectivity::Unreachable`] — never panics.
pub async fn check_connectivity(base_url: &str) -> Connectivity {
    let t0 = std::time::Instant::now();
    let base = base_url.trim().trim_end_matches('/');
    let url = current_json_url(base);
    let client = match reqwest::Client::builder()
        .timeout(CONNECTIVITY_TIMEOUT)
        .connect_timeout(CONNECTIVITY_TIMEOUT)
        .user_agent(USER_AGENT)
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
            let ms = t0.elapsed().as_secs_f64() * 1000.0;
            log::info!(
                target: "NaviPack",
                "check_connectivity host={base} outcome={kind} ms={ms:.1} err={e}"
            );
            return unreachable(format!("{kind}: {e}"));
        }
    };

    let status = response.status();
    if !status.is_success() {
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        log::info!(
            target: "NaviPack",
            "check_connectivity host={base} outcome=http_{status} ms={ms:.1}"
        );
        return unreachable(format!(
            "not ready (HTTP {status}) — use Geofabrik fallback"
        ));
    }

    let body = match response.text().await {
        Ok(t) => t,
        Err(e) => return unreachable(format!("read body failed: {e}")),
    };

    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    let conn = parse_current_json(&body, base);
    let n = conn.catalog().map(|c| c.regions.len()).unwrap_or(0);
    log::info!(
        target: "NaviPack",
        "check_connectivity host={base} outcome=ready regions={n} body_bytes={} ms={ms:.1}",
        body.len()
    );
    conn
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

/// Try each `(source, base_url)` in order; return the first ready catalog.
///
/// If every host fails, returns the last [`Connectivity::Unreachable`] reason
/// (or a generic one if the list was empty).
pub async fn check_connectivity_chain(
    bases: &[(PackDataSource, String)],
) -> (Connectivity, Option<PackDataSource>) {
    let t_chain = std::time::Instant::now();
    let mut last = unreachable("no pack server bases configured");
    for (source, base) in bases {
        let t_hop = std::time::Instant::now();
        match check_connectivity(base).await {
            ready @ Connectivity::Ready(_) => {
                let hop_ms = t_hop.elapsed().as_secs_f64() * 1000.0;
                let chain_ms = t_chain.elapsed().as_secs_f64() * 1000.0;
                log::info!(
                    target: "NaviPack",
                    "check_connectivity_chain selected={} ({}) hop_ms={hop_ms:.1} chain_ms={chain_ms:.1}",
                    source.as_str(),
                    base
                );
                return (ready, Some(*source));
            }
            bad => {
                let hop_ms = t_hop.elapsed().as_secs_f64() * 1000.0;
                log::info!(
                    target: "NaviPack",
                    "pack host {} ({}) unreachable in {hop_ms:.1}ms: {}",
                    source.as_str(),
                    base,
                    match &bad {
                        Connectivity::Unreachable { reason } => reason.as_str(),
                        Connectivity::Ready(_) => unreachable!(),
                    }
                );
                last = bad;
            }
        }
    }
    let chain_ms = t_chain.elapsed().as_secs_f64() * 1000.0;
    log::info!(
        target: "NaviPack",
        "check_connectivity_chain all hops failed chain_ms={chain_ms:.1}"
    );
    (last, None)
}

/// Blocking wrapper around [`check_connectivity_chain`].
pub fn check_connectivity_chain_blocking(
    bases: &[(PackDataSource, String)],
) -> (Connectivity, Option<PackDataSource>) {
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => return (unreachable(format!("runtime: {e}")), None),
    };
    rt.block_on(check_connectivity_chain(bases))
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
    fn base_url_omits_default_http_port() {
        assert_eq!(base_url("192.168.1.195", 80), "http://192.168.1.195");
        assert_eq!(base_url("navi.local", 8080), "http://navi.local:8080");
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
        match parse_current_json(json, "http://192.168.1.195") {
            Connectivity::Ready(c) => {
                assert_eq!(c.catalog_generation, "20260904T120000Z");
                assert_eq!(c.served_from, "http://192.168.1.195");
                assert_eq!(c.regions.len(), 2);
                assert_eq!(c.regions[0].region_id, "asia/china/anhui");
                assert_eq!(c.regions[0].generation.as_deref(), Some("20260904T120000Z"));
                assert_eq!(c.regions[0].bytes, Some(12_345_678));
                assert_eq!(
                    c.regions[0].manifest_url.as_deref(),
                    Some("/packs/asia/china/anhui/20260904T120000Z/manifest.json")
                );
                assert_eq!(c.regions[1].region_id, "europe/andorra");
                assert_eq!(c.regions[1].generation, None);
                assert_eq!(c.regions[1].bytes, None);
            }
            Connectivity::Unreachable { reason } => panic!("expected Ready, got {reason}"),
        }
    }

    #[test]
    fn migration_catalog_generation_is_not_region_bake_id() {
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
        match parse_current_json(json, "http://example.com") {
            Connectivity::Ready(c) => {
                assert_eq!(c.catalog_generation, "migrate-geofabrik-paths");
                assert_eq!(
                    c.regions[0].generation.as_deref(),
                    Some("20260904T113619Z-2762746-europe_monaco-9df05929")
                );
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
        match parse_current_json("{not-json", "http://example.com") {
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
                assert!(
                    reason.contains("404") || reason.contains("not ready"),
                    "{reason}"
                );
            }
            Connectivity::Ready(_) => panic!("404 must not be Ready"),
        }
    }

    #[tokio::test]
    async fn unreachable_host_fails_soft() {
        let status = check_connectivity("http://192.0.2.1:9").await;
        match status {
            Connectivity::Unreachable { reason } => {
                assert!(!reason.is_empty());
            }
            Connectivity::Ready(_) => panic!("bogus host must not be Ready"),
        }
    }

    #[tokio::test]
    async fn chain_skips_dead_first_host() {
        let ok = serve_once(
            "HTTP/1.1 200 OK",
            r#"{"generation":"g2","regions":[{"region_id":"europe/norway/ostlandet"}]}"#,
        );
        let bases = [
            (
                PackDataSource::ServerDuckdns,
                "http://192.0.2.1:9".to_string(),
            ),
            (PackDataSource::ServerDuckdns, ok),
        ];
        let (conn, src) = check_connectivity_chain(&bases).await;
        assert_eq!(src, Some(PackDataSource::ServerDuckdns));
        match conn {
            Connectivity::Ready(c) => {
                assert_eq!(c.regions[0].region_id, "europe/norway/ostlandet");
            }
            Connectivity::Unreachable { reason } => panic!("expected Ready: {reason}"),
        }
    }

    /// Manual check against the public pack host (or `NAVI_PACK_SERVER_BASE_URL`).
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
            }
        }
    }
}
