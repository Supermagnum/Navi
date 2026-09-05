//! Pure region-source routing + acquisition planning (Geofabrik fallback).

use super::{
    check_connectivity_blocking, Connectivity, ReadyRegion, DEFAULT_PACK_SERVER_BASE_URL,
};

/// Where to acquire a region after consulting the pack catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegionSource {
    /// Pack host lists this region as ready. Prefer pack-fetch when implemented.
    Server {
        region_id: String,
        generation: Option<String>,
        bytes: Option<u64>,
    },
    /// Use Geofabrik (or equivalent) extract download + on-device convert.
    Local { reason: String },
}

impl RegionSource {
    pub fn is_server(&self) -> bool {
        matches!(self, Self::Server { .. })
    }

    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local { .. })
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
/// - [`Connectivity::Unreachable`] (timeout, DNS, non-2xx, malformed) -> Local
pub fn resolve_region_source(region_id: &str, connectivity: &Connectivity) -> RegionSource {
    let region_id = normalize_region_id(region_id);
    match connectivity {
        Connectivity::Unreachable { reason } => RegionSource::Local {
            reason: format!("pack server unreachable, using local convert ({reason})"),
        },
        Connectivity::Ready(catalog) if catalog.regions.is_empty() => RegionSource::Local {
            reason: "pack catalog empty / not published, using local convert".to_string(),
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
                },
                None => RegionSource::Local {
                    reason: format!(
                        "region not published on pack server ({region_id}), using local convert"
                    ),
                },
            }
        }
    }
}

/// Resolve base URL: `NAVI_PACK_SERVER_BASE_URL` env, else [`DEFAULT_PACK_SERVER_BASE_URL`].
pub fn pack_server_base_url() -> String {
    std::env::var("NAVI_PACK_SERVER_BASE_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_PACK_SERVER_BASE_URL.to_string())
}

/// Future pack download + manifest verify. Always soft-fails until implemented.
pub fn try_fetch_region_packs(_ready: &ReadyRegion, _base_url: &str) -> Result<(), String> {
    Err("pack fetch not implemented".to_string())
}

/// Outcome of acquisition planning (routing decision + what to execute now).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionAcquisitionPlan {
    /// Pure routing result ([`RegionSource::Server`] even when pack-fetch is stubbed).
    pub source: RegionSource,
    /// Whether callers should run Geofabrik download + on-device convert now.
    ///
    /// TODO: always `true` until [`try_fetch_region_packs`] is implemented — including
    /// when [`source`](RegionAcquisitionPlan::source) is [`RegionSource::Server`]. That
    /// means "stub deferred to local", not "server fetch succeeded and local convert
    /// also runs". Once pack-fetch is real, set this `false` on the Server Ok branch
    /// (and keep `true` only for Local / pack-fetch soft-fail).
    pub execute_local_convert: bool,
    pub log_message: String,
    pub catalog_generation: Option<String>,
}

/// Check pack host, resolve source, stub pack-fetch, fall back to local convert.
///
/// Silent automatic fallback — never panics, never retries the server in a loop.
pub fn plan_region_acquisition(region_id: &str, base_url: &str) -> RegionAcquisitionPlan {
    let region_id = normalize_region_id(region_id);
    let connectivity = check_connectivity_blocking(base_url);
    let catalog_generation = connectivity
        .catalog()
        .map(|c| c.catalog_generation.clone());
    let source = resolve_region_source(&region_id, &connectivity);

    match &source {
        RegionSource::Server {
            region_id: rid,
            generation,
            bytes,
        } => {
            let ready = ReadyRegion {
                region_id: rid.clone(),
                generation: generation.clone(),
                bytes: *bytes,
            };
            match try_fetch_region_packs(&ready, base_url) {
                Ok(()) => {
                    let log_message = format!(
                        "pack server ready for {rid}; using pack fetch (generation={generation:?})"
                    );
                    log::info!(target: "NaviPack", "{log_message}");
                    RegionAcquisitionPlan {
                        source,
                        execute_local_convert: false,
                        log_message,
                        catalog_generation,
                    }
                }
                Err(stub_reason) => {
                    let log_message = format!(
                        "pack server has region {rid} (generation={generation:?}); {stub_reason} — using local convert"
                    );
                    log::info!(target: "NaviPack", "{log_message}");
                    RegionAcquisitionPlan {
                        source,
                        // TODO: always true while try_fetch_region_packs is a stub.
                        execute_local_convert: true,
                        log_message,
                        catalog_generation,
                    }
                }
            }
        }
        RegionSource::Local { reason } => {
            let log_message = reason.clone();
            log::info!(target: "NaviPack", "{log_message}");
            RegionAcquisitionPlan {
                source,
                execute_local_convert: true,
                log_message,
                catalog_generation,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack_server::PackCatalog;

    #[test]
    fn resolve_reachable_region_ready() {
        let conn = Connectivity::Ready(PackCatalog {
            catalog_generation: "migrate-geofabrik-paths".into(),
            regions: vec![ReadyRegion {
                region_id: "australia-oceania/australia/christmas-island".into(),
                generation: Some("20260904T113616Z".into()),
                bytes: Some(1_074_714),
            }],
        });
        match resolve_region_source("australia-oceania/australia/christmas-island", &conn) {
            RegionSource::Server {
                region_id,
                generation,
                bytes,
            } => {
                assert_eq!(region_id, "australia-oceania/australia/christmas-island");
                assert_eq!(generation.as_deref(), Some("20260904T113616Z"));
                assert_eq!(bytes, Some(1_074_714));
            }
            RegionSource::Local { reason } => panic!("expected Server: {reason}"),
        }
    }

    #[test]
    fn resolve_reachable_region_missing() {
        let conn = Connectivity::Ready(PackCatalog {
            catalog_generation: "20260904T120000Z".into(),
            regions: vec![ReadyRegion {
                region_id: "europe/monaco".into(),
                generation: Some("g".into()),
                bytes: None,
            }],
        });
        match resolve_region_source("europe/norway/ostlandet", &conn) {
            RegionSource::Local { reason } => {
                assert!(reason.contains("not published"), "{reason}");
                assert!(reason.contains("local convert"), "{reason}");
            }
            RegionSource::Server { .. } => panic!("missing region must be Local"),
        }
    }

    #[test]
    fn resolve_unreachable() {
        let conn = Connectivity::Unreachable {
            reason: "timeout".into(),
        };
        match resolve_region_source("europe/monaco", &conn) {
            RegionSource::Local { reason } => {
                assert!(reason.contains("unreachable"), "{reason}");
            }
            RegionSource::Server { .. } => panic!("unreachable must be Local"),
        }
    }

    #[test]
    fn resolve_empty_catalog() {
        let conn = Connectivity::Ready(PackCatalog {
            catalog_generation: "migrate-geofabrik-paths".into(),
            regions: vec![],
        });
        match resolve_region_source("europe/monaco", &conn) {
            RegionSource::Local { reason } => {
                assert!(
                    reason.contains("empty") || reason.contains("not published"),
                    "{reason}"
                );
            }
            RegionSource::Server { .. } => panic!("empty catalog must be Local"),
        }
    }

    #[test]
    fn resolve_malformed_connectivity_is_local() {
        // parse failures surface as Unreachable from check_connectivity.
        let conn = Connectivity::Unreachable {
            reason: "malformed current.json: expected value".into(),
        };
        assert!(resolve_region_source("europe/monaco", &conn).is_local());
    }

    #[test]
    fn resolve_normalizes_slashes() {
        let conn = Connectivity::Ready(PackCatalog {
            catalog_generation: "g".into(),
            regions: vec![ReadyRegion {
                region_id: "europe/monaco".into(),
                generation: None,
                bytes: None,
            }],
        });
        assert!(resolve_region_source("/europe/monaco/", &conn).is_server());
    }
}
