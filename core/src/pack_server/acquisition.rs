//! Pure region-source routing + acquisition planning (Geofabrik fallback).
//!
//! Host chain: LAN → duckdns → local-bake. Pack binary fetch
//! ([`try_fetch_region_packs`]) is still stubbed pending review of the merged
//! module; when unimplemented it soft-fails to local convert.

use super::{
    check_connectivity_blocking, check_connectivity_chain_blocking, Connectivity, ReadyRegion,
    DEFAULT_PACK_SERVER_BASE_URL, FALLBACK_PACK_SERVER_BASE_URL,
};

/// Which hop ultimately supplied catalog data (or local bake).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackDataSource {
    ServerLan,
    ServerDuckdns,
    LocalBake,
}

impl PackDataSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ServerLan => "server-lan",
            Self::ServerDuckdns => "server-duckdns",
            Self::LocalBake => "local-bake",
        }
    }
}

/// Where to acquire a region after consulting the pack catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegionSource {
    /// Pack host lists this region as ready. Prefer pack-fetch when implemented.
    Server {
        region_id: String,
        generation: Option<String>,
        bytes: Option<u64>,
        /// Host that served `current.json` for this decision.
        data_source: PackDataSource,
        /// Base URL of that host (no trailing slash).
        base_url: String,
    },
    /// Use Geofabrik (or equivalent) extract download + on-device convert.
    Local {
        reason: String,
        data_source: PackDataSource,
    },
}

impl RegionSource {
    pub fn is_server(&self) -> bool {
        matches!(self, Self::Server { .. })
    }

    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local { .. })
    }

    pub fn data_source(&self) -> PackDataSource {
        match self {
            Self::Server { data_source, .. } | Self::Local { data_source, .. } => *data_source,
        }
    }
}

/// Normalize a Geofabrik-style region path for catalog lookup.
pub fn normalize_region_id(region_id: &str) -> String {
    region_id.trim().trim_matches('/').to_string()
}

/// Pure routing decision: no I/O. Unit-test without a network.
///
/// - [`Connectivity::Ready`] + region present -> [`RegionSource::Server`]
/// - Ready but region missing / empty catalog -> [`RegionSource::Local`]
/// - [`Connectivity::Unreachable`] -> Local
pub fn resolve_region_source(
    region_id: &str,
    connectivity: &Connectivity,
    data_source: PackDataSource,
) -> RegionSource {
    let region_id = normalize_region_id(region_id);
    match connectivity {
        Connectivity::Unreachable { reason } => RegionSource::Local {
            reason: format!("pack server unreachable, using local convert ({reason})"),
            data_source: PackDataSource::LocalBake,
        },
        Connectivity::Ready(catalog) if catalog.regions.is_empty() => RegionSource::Local {
            reason: "pack catalog empty / not published, using local convert".to_string(),
            data_source: PackDataSource::LocalBake,
        },
        Connectivity::Ready(catalog) => {
            match catalog
                .regions
                .iter()
                .find(|r| normalize_region_id(&r.region_id) == region_id)
            {
                Some(ready) => RegionSource::Server {
                    region_id: ready.region_id.clone(),
                    generation: ready.generation.clone(),
                    bytes: ready.bytes,
                    data_source,
                    base_url: catalog.served_from.clone(),
                },
                None => RegionSource::Local {
                    reason: format!(
                        "region not published on pack server ({region_id}), using local convert"
                    ),
                    data_source: PackDataSource::LocalBake,
                },
            }
        }
    }
}

/// Resolve a single override base URL: `NAVI_PACK_SERVER_BASE_URL` env, else LAN default.
///
/// Prefer [`pack_server_discovery_bases`] for the full LAN → duckdns chain.
pub fn pack_server_base_url() -> String {
    std::env::var("NAVI_PACK_SERVER_BASE_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_PACK_SERVER_BASE_URL.to_string())
}

/// Ordered discovery bases for the host fallback chain.
///
/// If `NAVI_PACK_SERVER_BASE_URL` is set, only that host is probed (tagged
/// `server-lan` when it matches the LAN default, otherwise `server-duckdns`
/// when it matches the public fallback, else `server-lan` as a custom override
/// tag for logging).
pub fn pack_server_discovery_bases() -> Vec<(PackDataSource, String)> {
    if let Ok(env) = std::env::var("NAVI_PACK_SERVER_BASE_URL") {
        let base = env.trim().trim_end_matches('/').to_string();
        if !base.is_empty() {
            let tag = if base == DEFAULT_PACK_SERVER_BASE_URL {
                PackDataSource::ServerLan
            } else if base == FALLBACK_PACK_SERVER_BASE_URL {
                PackDataSource::ServerDuckdns
            } else {
                // Custom override: treat as primary hop for logging.
                PackDataSource::ServerLan
            };
            return vec![(tag, base)];
        }
    }
    vec![
        (
            PackDataSource::ServerLan,
            DEFAULT_PACK_SERVER_BASE_URL.to_string(),
        ),
        (
            PackDataSource::ServerDuckdns,
            FALLBACK_PACK_SERVER_BASE_URL.to_string(),
        ),
    ]
}

/// Future pack download + manifest verify. Always soft-fails until implemented.
///
/// Next step (after merge review): GET `manifest.json` / files under
/// `/packs/<region_id>/<generation>/`, verify sha256, atomic install into
/// `data_dir`, confirm local loaders accept the packs.
pub fn try_fetch_region_packs(
    _ready: &ReadyRegion,
    _base_url: &str,
    _data_dir: Option<&std::path::Path>,
) -> Result<(), String> {
    Err("pack fetch not implemented".to_string())
}

/// Outcome of acquisition planning (routing decision + what to execute now).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionAcquisitionPlan {
    /// Pure routing result ([`RegionSource::Server`] even when pack-fetch is stubbed).
    pub source: RegionSource,
    /// Whether callers should run Geofabrik download + on-device convert now.
    ///
    /// TODO: always `true` until [`try_fetch_region_packs`] is implemented —
    /// including when [`source`](RegionAcquisitionPlan::source) is
    /// [`RegionSource::Server`]. That means "stub deferred to local", not
    /// "server fetch succeeded and local convert also runs". Once pack-fetch
    /// is real, set this `false` on the Server Ok branch.
    pub execute_local_convert: bool,
    pub log_message: String,
    pub catalog_generation: Option<String>,
    /// Final hop tag for UI / logs (`server-lan` / `server-duckdns` / `local-bake`).
    pub data_source: PackDataSource,
}

/// Check pack hosts (LAN → duckdns unless `base_url_override`), resolve source,
/// stub pack-fetch, fall back to local convert.
///
/// When `base_url_override` is `Some`, only that host is probed (tests /
/// UniFFI explicit URL). Silent automatic fallback — never panics.
pub fn plan_region_acquisition(
    region_id: &str,
    base_url_override: Option<&str>,
) -> RegionAcquisitionPlan {
    let region_id = normalize_region_id(region_id);

    let (connectivity, hop) = if let Some(base) = base_url_override
        .map(|s| s.trim().trim_end_matches('/'))
        .filter(|s| !s.is_empty())
    {
        let tag = if base == DEFAULT_PACK_SERVER_BASE_URL {
            PackDataSource::ServerLan
        } else if base == FALLBACK_PACK_SERVER_BASE_URL {
            PackDataSource::ServerDuckdns
        } else {
            PackDataSource::ServerLan
        };
        let conn = check_connectivity_blocking(base);
        let hop = if conn.is_ready() { Some(tag) } else { None };
        (conn, hop)
    } else {
        let bases = pack_server_discovery_bases();
        check_connectivity_chain_blocking(&bases)
    };

    let catalog_generation = connectivity.catalog().map(|c| c.catalog_generation.clone());
    let hop_for_resolve = hop.unwrap_or(PackDataSource::LocalBake);
    let source = resolve_region_source(&region_id, &connectivity, hop_for_resolve);
    let data_source = source.data_source();

    match &source {
        RegionSource::Server {
            region_id: rid,
            generation,
            bytes,
            base_url,
            ..
        } => {
            let ready = ReadyRegion {
                region_id: rid.clone(),
                generation: generation.clone(),
                bytes: *bytes,
                manifest_url: connectivity.catalog().and_then(|c| {
                    c.regions
                        .iter()
                        .find(|r| normalize_region_id(&r.region_id) == *rid)
                        .and_then(|r| r.manifest_url.clone())
                }),
            };
            match try_fetch_region_packs(&ready, base_url, None) {
                Ok(()) => {
                    let log_message = format!(
                        "source={} pack server ready for {rid}; using pack fetch (generation={generation:?})",
                        data_source.as_str()
                    );
                    log::info!(target: "NaviPack", "{log_message}");
                    RegionAcquisitionPlan {
                        source,
                        execute_local_convert: false,
                        log_message,
                        catalog_generation,
                        data_source,
                    }
                }
                Err(stub_reason) => {
                    let log_message = format!(
                        "source={} pack server has region {rid} (generation={generation:?}); {stub_reason} — using local convert",
                        data_source.as_str()
                    );
                    log::info!(target: "NaviPack", "{log_message}");
                    RegionAcquisitionPlan {
                        source,
                        // TODO: always true while try_fetch_region_packs is a stub.
                        execute_local_convert: true,
                        log_message,
                        catalog_generation,
                        data_source: PackDataSource::LocalBake,
                    }
                }
            }
        }
        RegionSource::Local { reason, .. } => {
            let log_message = format!("source={} {reason}", PackDataSource::LocalBake.as_str());
            log::info!(target: "NaviPack", "{log_message}");
            RegionAcquisitionPlan {
                source,
                execute_local_convert: true,
                log_message,
                catalog_generation,
                data_source: PackDataSource::LocalBake,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack_server::PackCatalog;

    #[test]
    fn data_source_tags() {
        assert_eq!(PackDataSource::ServerLan.as_str(), "server-lan");
        assert_eq!(PackDataSource::ServerDuckdns.as_str(), "server-duckdns");
        assert_eq!(PackDataSource::LocalBake.as_str(), "local-bake");
    }

    #[test]
    fn resolve_reachable_region_ready() {
        let conn = Connectivity::Ready(PackCatalog {
            catalog_generation: "migrate-geofabrik-paths".into(),
            served_from: "http://192.168.1.195".into(),
            regions: vec![ReadyRegion {
                region_id: "australia-oceania/australia/christmas-island".into(),
                generation: Some("20260904T113616Z".into()),
                bytes: Some(1_074_714),
                manifest_url: None,
            }],
        });
        match resolve_region_source(
            "australia-oceania/australia/christmas-island",
            &conn,
            PackDataSource::ServerLan,
        ) {
            RegionSource::Server {
                region_id,
                generation,
                bytes,
                data_source,
                base_url,
            } => {
                assert_eq!(region_id, "australia-oceania/australia/christmas-island");
                assert_eq!(generation.as_deref(), Some("20260904T113616Z"));
                assert_eq!(bytes, Some(1_074_714));
                assert_eq!(data_source, PackDataSource::ServerLan);
                assert_eq!(base_url, "http://192.168.1.195");
            }
            RegionSource::Local { reason, .. } => panic!("expected Server: {reason}"),
        }
    }

    #[test]
    fn resolve_reachable_region_missing() {
        let conn = Connectivity::Ready(PackCatalog {
            catalog_generation: "20260904T120000Z".into(),
            served_from: "https://navigate-me.duckdns.org".into(),
            regions: vec![ReadyRegion {
                region_id: "europe/monaco".into(),
                generation: Some("g".into()),
                bytes: None,
                manifest_url: None,
            }],
        });
        match resolve_region_source(
            "europe/norway/ostlandet",
            &conn,
            PackDataSource::ServerDuckdns,
        ) {
            RegionSource::Local {
                reason,
                data_source,
            } => {
                assert!(reason.contains("not published"), "{reason}");
                assert!(reason.contains("local convert"), "{reason}");
                assert_eq!(data_source, PackDataSource::LocalBake);
            }
            RegionSource::Server { .. } => panic!("missing region must be Local"),
        }
    }

    #[test]
    fn resolve_unreachable() {
        let conn = Connectivity::Unreachable {
            reason: "timeout".into(),
        };
        match resolve_region_source("europe/monaco", &conn, PackDataSource::ServerLan) {
            RegionSource::Local {
                reason,
                data_source,
            } => {
                assert!(reason.contains("unreachable"), "{reason}");
                assert_eq!(data_source, PackDataSource::LocalBake);
            }
            RegionSource::Server { .. } => panic!("unreachable must be Local"),
        }
    }

    #[test]
    fn resolve_empty_catalog() {
        let conn = Connectivity::Ready(PackCatalog {
            catalog_generation: "migrate-geofabrik-paths".into(),
            served_from: "http://192.168.1.195".into(),
            regions: vec![],
        });
        match resolve_region_source("europe/monaco", &conn, PackDataSource::ServerLan) {
            RegionSource::Local { reason, .. } => {
                assert!(
                    reason.contains("empty") || reason.contains("not published"),
                    "{reason}"
                );
            }
            RegionSource::Server { .. } => panic!("empty catalog must be Local"),
        }
    }

    #[test]
    fn resolve_normalizes_slashes() {
        let conn = Connectivity::Ready(PackCatalog {
            catalog_generation: "g".into(),
            served_from: "http://192.168.1.195".into(),
            regions: vec![ReadyRegion {
                region_id: "europe/monaco".into(),
                generation: None,
                bytes: None,
                manifest_url: None,
            }],
        });
        assert!(
            resolve_region_source("/europe/monaco/", &conn, PackDataSource::ServerLan).is_server()
        );
    }

    #[test]
    fn discovery_bases_default_chain() {
        // Clear override for this process if present — only assert defaults when unset.
        if std::env::var("NAVI_PACK_SERVER_BASE_URL").is_err() {
            let bases = pack_server_discovery_bases();
            assert_eq!(bases.len(), 2);
            assert_eq!(bases[0].0, PackDataSource::ServerLan);
            assert_eq!(bases[0].1, DEFAULT_PACK_SERVER_BASE_URL);
            assert_eq!(bases[1].0, PackDataSource::ServerDuckdns);
            assert_eq!(bases[1].1, FALLBACK_PACK_SERVER_BASE_URL);
        }
    }
}
