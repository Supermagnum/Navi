//! Pure region-source routing + acquisition planning (Geofabrik fallback).
//!
//! Host chain: public pack host → local-bake. Pack binary fetch lives in
//! [`super::fetch`]; on failure the planner soft-falls to local convert.

use std::path::{Path, PathBuf};

use super::fetch::{try_fetch_region_packs, ServerInstallStamp};
use super::{
    check_connectivity_blocking, check_connectivity_blocking_timed,
    check_connectivity_chain_blocking, check_connectivity_chain_blocking_timed, Connectivity,
    ConnectivityFailureKind, ReadyRegion, CONNECTIVITY_RETRY_TIMEOUT, CONNECTIVITY_TIMEOUT,
    DEFAULT_PACK_SERVER_BASE_URL,
};

/// Which hop ultimately supplied catalog data (or local bake).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackDataSource {
    ServerDuckdns,
    LocalBake,
}

impl PackDataSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ServerDuckdns => "server-duckdns",
            Self::LocalBake => "local-bake",
        }
    }
}

/// Where to acquire a region after consulting the pack catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegionSource {
    /// Pack host lists this region as ready. Prefer pack-fetch when available.
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

/// Alternate pack-catalog `region_id`s for a client path (and the reverse).
///
/// **Permanent client-only exception — not a general hyphen/underscore
/// normalizer.** navi-server publishes Västra Götaland as
/// `europe/sweden/vastra_gotaland` (underscore); Tools chips use that same
/// published id. Keep the hyphen alias so typed / legacy
/// `europe/sweden/vastra-gotaland` still resolves. If the published
/// `region_id` is ever corrected independently, this mapping (and the Kotlin
/// mirror in `PackRegionAvailability`) can be removed.
pub fn pack_catalog_region_id_aliases(region_id: &str) -> Vec<&'static str> {
    match normalize_region_id(region_id).as_str() {
        "europe/sweden/vastra-gotaland" => vec!["europe/sweden/vastra_gotaland"],
        "europe/sweden/vastra_gotaland" => vec!["europe/sweden/vastra-gotaland"],
        _ => vec![],
    }
}

/// True when two region ids are the same path or a known catalog alias pair.
pub fn region_ids_match_for_catalog(a: &str, b: &str) -> bool {
    let a = normalize_region_id(a);
    let b = normalize_region_id(b);
    if a == b {
        return true;
    }
    pack_catalog_region_id_aliases(&a)
        .iter()
        .any(|alias| *alias == b)
        || pack_catalog_region_id_aliases(&b)
            .iter()
            .any(|alias| *alias == a)
}

/// Look up `region_id` (or a catalog alias) in `ready_ids`, returning the
/// **published** spelling from the catalog when present.
fn published_id_exact(region_id: &str, ready_ids: &[String]) -> Option<String> {
    let want = normalize_region_id(region_id);
    if want.is_empty() {
        return None;
    }
    ready_ids
        .iter()
        .find(|r| region_ids_match_for_catalog(r, &want))
        .map(|r| normalize_region_id(r))
}

/// Resolve a required area to a catalog entry that exists in `ready_ids`.
///
/// Exact match (including [`pack_catalog_region_id_aliases`]) wins. Otherwise
/// walk parents (`europe/denmark/syddanmark` → `europe/denmark`) until a
/// published id is found. Generic — not Denmark-specific. Returns `None` when
/// no ancestor is published.
pub fn resolve_area_to_catalog(region_id: &str, ready_ids: &[String]) -> Option<String> {
    let mut cur = normalize_region_id(region_id);
    if cur.is_empty() {
        return None;
    }
    loop {
        if let Some(hit) = published_id_exact(&cur, ready_ids) {
            return Some(hit);
        }
        match cur.rsplit_once('/') {
            Some((parent, _)) if !parent.is_empty() => cur = parent.to_string(),
            _ => return None,
        }
    }
}

/// Resolve required areas to published catalog ids.
///
/// Deduplicates while keeping first-occurrence order. Entries that cannot be
/// resolved to any published ancestor are dropped.
pub fn resolve_areas_to_catalog(required: &[String], ready_ids: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for req in required {
        let Some(id) = resolve_area_to_catalog(req, ready_ids) else {
            continue;
        };
        if seen.insert(id.clone()) {
            out.push(id);
        }
    }
    out
}

/// Device-side pack stem for a Geofabrik path (`europe/monaco` → `monaco-latest`).
pub fn leaf_stem_for_region_id(region_id: &str) -> String {
    let id = normalize_region_id(region_id);
    let leaf = id.rsplit('/').next().unwrap_or(id.as_str());
    format!("{leaf}-latest")
}

/// Pure routing decision: no I/O. Unit-test without a network.
///
/// - [`Connectivity::Ready`] + region present (exact, alias, or published
///   parent via [`resolve_area_to_catalog`]) -> [`RegionSource::Server`]
/// - Ready but region and all parents missing / empty catalog -> [`RegionSource::Local`]
/// - [`Connectivity::Unreachable`] -> Local
///
/// On a catalog hit (including [`pack_catalog_region_id_aliases`] and parent
/// fallback), the returned [`RegionSource::Server::region_id`] is the
/// **published** catalog id (may differ from the client/chip path for the
/// Västra Götaland alias or an unpublished leaf).
pub fn resolve_region_source(
    region_id: &str,
    connectivity: &Connectivity,
    data_source: PackDataSource,
) -> RegionSource {
    let region_id = normalize_region_id(region_id);
    match connectivity {
        Connectivity::Unreachable { reason, .. } => RegionSource::Local {
            reason: format!("pack server unreachable, using local convert ({reason})"),
            data_source: PackDataSource::LocalBake,
        },
        Connectivity::Ready(catalog) if catalog.regions.is_empty() => RegionSource::Local {
            reason: "pack catalog empty / not published, using local convert".to_string(),
            data_source: PackDataSource::LocalBake,
        },
        Connectivity::Ready(catalog) => {
            let ready_ids: Vec<String> = catalog
                .regions
                .iter()
                .map(|r| r.region_id.clone())
                .collect();
            match resolve_area_to_catalog(&region_id, &ready_ids).and_then(|published| {
                catalog
                    .regions
                    .iter()
                    .find(|r| region_ids_match_for_catalog(&r.region_id, &published))
            }) {
                Some(ready) => RegionSource::Server {
                    // Prefer the catalog's published id so pack URLs / stems
                    // match DocumentRoot (e.g. vastra_gotaland). Parent fallback
                    // maps unpublished leaves (e.g. europe/denmark/syddanmark)
                    // onto a published ancestor when one exists.
                    region_id: normalize_region_id(&ready.region_id),
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

/// Resolve a single override base URL: `NAVI_PACK_SERVER_BASE_URL` env, else public default.
///
/// Prefer [`pack_server_discovery_bases`] for discovery (same host unless overridden).
pub fn pack_server_base_url() -> String {
    std::env::var("NAVI_PACK_SERVER_BASE_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_PACK_SERVER_BASE_URL.to_string())
}

/// Ordered discovery bases for the host fallback chain.
///
/// Default: public pack host only. If `NAVI_PACK_SERVER_BASE_URL` is set, only
/// that host is probed (tagged [`PackDataSource::ServerDuckdns`]).
pub fn pack_server_discovery_bases() -> Vec<(PackDataSource, String)> {
    if let Ok(env) = std::env::var("NAVI_PACK_SERVER_BASE_URL") {
        let base = env.trim().trim_end_matches('/').to_string();
        if !base.is_empty() {
            return vec![(PackDataSource::ServerDuckdns, base)];
        }
    }
    vec![(
        PackDataSource::ServerDuckdns,
        DEFAULT_PACK_SERVER_BASE_URL.to_string(),
    )]
}

fn hop_tag_for_override_base(_base: &str) -> PackDataSource {
    PackDataSource::ServerDuckdns
}

/// Whether [path] is covered by a ready-region id from `current.json`.
///
/// Matches exact id (including [`pack_catalog_region_id_aliases`]), a published
/// child (`europe/norway` covers `europe/norway/ostlandet`), or a published
/// parent covering a deeper chip.
pub fn path_covered_by_ready_ids(path: &str, ready_ids: &[String]) -> bool {
    let p = normalize_region_id(path);
    if p.is_empty() {
        return false;
    }
    ready_ids.iter().any(|raw| {
        let r = normalize_region_id(raw);
        if r.is_empty() {
            return false;
        }
        region_ids_match_for_catalog(&r, &p)
            || r.starts_with(&(p.clone() + "/"))
            || p.starts_with(&(r.clone() + "/"))
    })
}

/// Snapshot of pack-host discovery for UI (region-pill greens).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackCatalogSnapshot {
    pub data_source: PackDataSource,
    pub ready_region_ids: Vec<String>,
    pub catalog_generation: Option<String>,
    pub served_from: Option<String>,
    pub unreachable_reason: Option<String>,
}

/// Probe the public pack host (or override) and return ready region ids for pill UI.
///
/// Soft-fail: unreachable hosts yield an empty ready list and
/// [`PackDataSource::LocalBake`] with a reason — never panics.
pub fn discover_pack_catalog(base_url_override: Option<&str>) -> PackCatalogSnapshot {
    let (connectivity, hop) = if let Some(base) = base_url_override
        .map(|s| s.trim().trim_end_matches('/'))
        .filter(|s| !s.is_empty())
    {
        let tag = hop_tag_for_override_base(base);
        let conn = check_connectivity_blocking(base);
        let hop = if conn.is_ready() { Some(tag) } else { None };
        (conn, hop)
    } else {
        let bases = pack_server_discovery_bases();
        check_connectivity_chain_blocking(&bases)
    };

    match connectivity {
        Connectivity::Ready(catalog) => {
            let data_source = hop.unwrap_or(PackDataSource::ServerDuckdns);
            PackCatalogSnapshot {
                data_source,
                ready_region_ids: catalog
                    .regions
                    .into_iter()
                    .map(|r| normalize_region_id(&r.region_id))
                    .filter(|s| !s.is_empty())
                    .collect(),
                catalog_generation: Some(catalog.catalog_generation),
                served_from: Some(catalog.served_from),
                unreachable_reason: None,
            }
        }
        Connectivity::Unreachable { reason, .. } => PackCatalogSnapshot {
            data_source: PackDataSource::LocalBake,
            ready_region_ids: Vec::new(),
            catalog_generation: None,
            served_from: None,
            unreachable_reason: Some(reason),
        },
    }
}

/// Outcome of acquisition planning (routing decision + what to execute now).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionAcquisitionPlan {
    /// Pure routing result ([`RegionSource::Server`] even when pack-fetch fails).
    pub source: RegionSource,
    /// Whether callers should run Geofabrik download + on-device convert now.
    ///
    /// `false` after a successful [`try_fetch_region_packs`]; `true` on Local
    /// or when pack-fetch soft-fails.
    pub execute_local_convert: bool,
    pub log_message: String,
    /// Short machine token for logs / UI:
    /// `ok` | `timeout` | `not_in_catalog` | `fetch_error` | `format_gate` |
    /// `network` | `http_404` | `malformed` | `catalog_empty` | …
    pub decision_reason: String,
    pub catalog_generation: Option<String>,
    /// Final hop tag for UI / logs (`server-duckdns` / `local-bake`).
    pub data_source: PackDataSource,
}

fn classify_unreachable_kind(kind: ConnectivityFailureKind) -> &'static str {
    match kind {
        ConnectivityFailureKind::Timeout => "timeout",
        ConnectivityFailureKind::Network => "network",
        ConnectivityFailureKind::HttpStatus(404) => "http_404",
        ConnectivityFailureKind::HttpStatus(_) => "http_status",
        ConnectivityFailureKind::Malformed => "malformed",
        ConnectivityFailureKind::Internal => "internal",
    }
}

fn classify_fetch_failure(fetch_reason: &str) -> &'static str {
    let lower = fetch_reason.to_ascii_lowercase();
    if lower.contains("graph_format_version")
        || lower.contains("wetland_format")
        || lower.contains("poi_barrier_format")
        || lower.contains("format check")
        || lower.contains("not installing")
    {
        "format_gate"
    } else if lower.contains("not ready") || lower.contains("packs not ready") {
        "install_not_ready"
    } else if lower.contains("data_dir") || lower.contains("no data dir") {
        "no_data_dir"
    } else {
        "fetch_error"
    }
}

fn classify_local_source_reason(reason: &str) -> &'static str {
    let lower = reason.to_ascii_lowercase();
    if lower.contains("not published") {
        "not_in_catalog"
    } else if lower.contains("catalog empty") {
        "catalog_empty"
    } else if lower.contains("timeout") {
        "timeout"
    } else if lower.contains("unreachable") {
        // Prefer more specific tokens from ConnectivityFailureKind when available.
        if lower.contains("http 404") || lower.contains("http_404") {
            "http_404"
        } else if lower.contains("malformed") {
            "malformed"
        } else {
            "network"
        }
    } else {
        "local"
    }
}

/// Probe pack catalog; on a plain timeout, retry once with a longer budget.
fn probe_catalog_with_timeout_retry(
    base_url_override: Option<&str>,
) -> (Connectivity, Option<PackDataSource>, f64) {
    let t_conn = std::time::Instant::now();
    let (mut connectivity, mut hop) = if let Some(base) = base_url_override {
        let tag = hop_tag_for_override_base(base);
        let conn = check_connectivity_blocking_timed(base, CONNECTIVITY_TIMEOUT);
        let hop = if conn.is_ready() { Some(tag) } else { None };
        (conn, hop)
    } else {
        let bases = pack_server_discovery_bases();
        check_connectivity_chain_blocking_timed(&bases, CONNECTIVITY_TIMEOUT)
    };

    if matches!(
        connectivity.failure_kind(),
        Some(ConnectivityFailureKind::Timeout)
    ) {
        log::info!(
            target: "NaviPack",
            "plan_region_acquisition catalog probe timed out ({}ms budget); retrying with {}ms",
            CONNECTIVITY_TIMEOUT.as_millis(),
            CONNECTIVITY_RETRY_TIMEOUT.as_millis()
        );
        let (retry_conn, retry_hop) = if let Some(base) = base_url_override {
            let tag = hop_tag_for_override_base(base);
            let conn = check_connectivity_blocking_timed(base, CONNECTIVITY_RETRY_TIMEOUT);
            let hop = if conn.is_ready() { Some(tag) } else { None };
            (conn, hop)
        } else {
            let bases = pack_server_discovery_bases();
            check_connectivity_chain_blocking_timed(&bases, CONNECTIVITY_RETRY_TIMEOUT)
        };
        connectivity = retry_conn;
        hop = retry_hop;
    }

    let connectivity_ms = t_conn.elapsed().as_secs_f64() * 1000.0;
    (connectivity, hop, connectivity_ms)
}

/// Check the pack host (or `base_url_override`), resolve source,
/// fetch packs into `data_dir` when Server, else fall back to local convert.
///
/// When `base_url_override` is `Some`, only that host is probed (tests /
/// UniFFI explicit URL). Silent automatic fallback — never panics.
///
/// A **timeout** on the first catalog probe is retried once with a longer
/// budget before local-bake. Confirmed misses (`not_in_catalog`) fall through
/// immediately. Soft failures still set [`RegionAcquisitionPlan::execute_local_convert`]
/// but [`RegionAcquisitionPlan::decision_reason`] distinguishes the cause.
pub fn plan_region_acquisition(
    region_id: &str,
    base_url_override: Option<&str>,
    data_dir: Option<&Path>,
) -> RegionAcquisitionPlan {
    let t0 = std::time::Instant::now();
    let region_id = normalize_region_id(region_id);

    crate::download::progress::set(0, None, "Checking pack server…");
    let override_base = base_url_override
        .map(|s| s.trim().trim_end_matches('/'))
        .filter(|s| !s.is_empty());
    let (connectivity, hop, connectivity_ms) = probe_catalog_with_timeout_retry(override_base);

    let catalog_generation = connectivity.catalog().map(|c| c.catalog_generation.clone());
    let hop_for_resolve = hop.unwrap_or(PackDataSource::LocalBake);
    let source = resolve_region_source(&region_id, &connectivity, hop_for_resolve);
    let data_source = source.data_source();

    let plan = match &source {
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
                        .find(|r| region_ids_match_for_catalog(&r.region_id, rid))
                        .and_then(|r| r.manifest_url.clone())
                }),
            };
            let t_fetch = std::time::Instant::now();
            match try_fetch_region_packs(&ready, base_url, data_dir) {
                Ok(()) => {
                    let fetch_ms = t_fetch.elapsed().as_secs_f64() * 1000.0;
                    let total_ms = t0.elapsed().as_secs_f64() * 1000.0;
                    let log_message = format!(
                        "source={} pack server ready for {rid}; installed packs (generation={generation:?}) \
                         connectivity_ms={connectivity_ms:.1} fetch_ms={fetch_ms:.1} total_ms={total_ms:.1}",
                        data_source.as_str()
                    );
                    RegionAcquisitionPlan {
                        source: source.clone(),
                        execute_local_convert: false,
                        log_message,
                        decision_reason: "ok".into(),
                        catalog_generation,
                        data_source,
                    }
                }
                Err(fetch_reason) => {
                    let fetch_ms = t_fetch.elapsed().as_secs_f64() * 1000.0;
                    let decision_reason = classify_fetch_failure(&fetch_reason).to_string();
                    let log_message = format!(
                        "source={} pack server has region {rid} (generation={generation:?}); pack fetch failed ({fetch_reason}) — using local convert \
                         connectivity_ms={connectivity_ms:.1} fetch_ms={fetch_ms:.1} decision_reason={decision_reason}",
                        data_source.as_str()
                    );
                    RegionAcquisitionPlan {
                        source: source.clone(),
                        execute_local_convert: true,
                        log_message,
                        decision_reason,
                        catalog_generation,
                        data_source: PackDataSource::LocalBake,
                    }
                }
            }
        }
        RegionSource::Local { reason, .. } => {
            let total_ms = t0.elapsed().as_secs_f64() * 1000.0;
            let decision_reason = match connectivity.failure_kind() {
                Some(kind) => classify_unreachable_kind(kind).to_string(),
                None => classify_local_source_reason(reason).to_string(),
            };
            let log_message = format!(
                "source={} {reason} connectivity_ms={connectivity_ms:.1} total_ms={total_ms:.1} \
                 decision_reason={decision_reason}",
                PackDataSource::LocalBake.as_str()
            );
            RegionAcquisitionPlan {
                source: source.clone(),
                execute_local_convert: true,
                log_message,
                decision_reason,
                catalog_generation,
                data_source: PackDataSource::LocalBake,
            }
        }
    };

    log::info!(
        target: "NaviPack",
        "plan_region_acquisition region={region_id} execute_local={} reason={} \
         connectivity_ms={connectivity_ms:.1} data_dir={}",
        plan.execute_local_convert,
        plan.decision_reason,
        data_dir.is_some()
    );
    log::info!(target: "NaviPack", "{}", plan.log_message);
    plan
}

/// Resolve Geofabrik / pack-catalog region id for a leaf stem under `data_dir`.
pub fn resolve_region_id_for_leaf(
    data_dir: &Path,
    leaf_stem: &str,
    region_id_hint: Option<&str>,
) -> Option<String> {
    if let Some(h) = region_id_hint.map(str::trim).filter(|s| !s.is_empty()) {
        return Some(normalize_region_id(h));
    }
    if let Ok(stamp) = ServerInstallStamp::load_for_leaf(data_dir, leaf_stem) {
        let id = normalize_region_id(&stamp.region_id);
        if !id.is_empty() {
            return Some(id);
        }
    }
    // Avoid importing osm_update (pack_server ↔ osm_update cycle); read meta JSON directly.
    let meta_path = data_dir.join("region_meta.json");
    if let Ok(text) = std::fs::read_to_string(meta_path) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(id) = v
                .get("geofabrik_region")
                .and_then(|x| x.as_str())
                .map(normalize_region_id)
                .filter(|s| !s.is_empty())
            {
                return Some(id);
            }
        }
    }
    None
}

/// Outcome of [`ensure_indexed_packs_prefer_server`].
#[derive(Debug, Clone, PartialEq)]
pub struct EnsureIndexedPacksResult {
    /// `server-duckdns` when packs were installed from the pack host; else `local-bake`.
    pub data_source: PackDataSource,
    /// True when packs were already Ready (no work).
    pub cache_hit: bool,
    pub log_message: String,
    /// Optional convert report when a local bake ran.
    pub convert_ms: Option<f64>,
}

/// Make indexed packs Ready for `pbf`: **pack server first**, local convert fallback.
///
/// Preference order:
/// 1. Already Ready → no-op
/// 2. Fetch / install published packs via [`plan_region_acquisition`] when a
///    region id is known and the host has a client-compatible format
/// 3. Otherwise run on-device [`convert_region_packs`] from the local PBF
pub fn ensure_indexed_packs_prefer_server(
    data_dir: &Path,
    pbf: &Path,
    elev_dir: Option<&Path>,
    region_id_hint: Option<&str>,
) -> Result<EnsureIndexedPacksResult, String> {
    use crate::routing::graph::RoutingProfile;
    use crate::routing::indexed::{
        convert_region_packs, manifest_path, server_install_present, ConvertOptions, NaviManifest,
        PackStatus,
    };

    let stem = pbf
        .file_name()
        .and_then(|s| s.to_str())
        .map(|name| {
            name.strip_suffix(".osm.pbf")
                .or_else(|| name.strip_suffix(".pbf"))
                .unwrap_or(name)
                .to_string()
        })
        .unwrap_or_else(|| "region".into());

    let man_path = manifest_path(data_dir, &stem);
    if man_path.is_file() {
        if let Ok(man) = NaviManifest::load(&man_path) {
            let ready = if server_install_present(data_dir, &stem) {
                man.status_pack_files(data_dir) == PackStatus::Ready
            } else if let Ok(packed) =
                crate::routing::indexed::fingerprint_pbf_for_packs(data_dir, pbf, &man)
            {
                man.status_for_pbf(data_dir, &packed) == PackStatus::Ready
            } else {
                false
            };
            if ready {
                crate::download::progress::set_on(
                    crate::download::progress::ProgressChannel::Convert,
                    100,
                    Some(100),
                    "Indexed maps ready",
                );
                return Ok(EnsureIndexedPacksResult {
                    data_source: if server_install_present(data_dir, &stem) {
                        PackDataSource::ServerDuckdns
                    } else {
                        PackDataSource::LocalBake
                    },
                    cache_hit: true,
                    log_message: "packs already ready".into(),
                    convert_ms: None,
                });
            }
        }
    }

    let region_id = resolve_region_id_for_leaf(data_dir, &stem, region_id_hint);
    let mut rebuild_reason = "no_region_id".to_string();
    if let Some(ref rid) = region_id {
        crate::download::progress::set(0, None, "Downloading updated pack from server…");
        log::info!(
            target: "NaviPack",
            "ensure_indexed_packs: trying pack server first region={rid} stem={stem}"
        );
        let plan = plan_region_acquisition(rid, None, Some(data_dir));
        if !plan.execute_local_convert {
            // Re-check Ready after install (format gate already applied in fetch).
            if let Ok(man) = NaviManifest::load(&manifest_path(data_dir, &stem)) {
                if man.status_pack_files(data_dir) == PackStatus::Ready {
                    let msg = format!("downloaded updated pack from server ({})", plan.log_message);
                    log::info!(target: "NaviPack", "{msg}");
                    crate::download::progress::set_on(
                        crate::download::progress::ProgressChannel::Convert,
                        100,
                        Some(100),
                        "Indexed maps ready",
                    );
                    return Ok(EnsureIndexedPacksResult {
                        data_source: PackDataSource::ServerDuckdns,
                        cache_hit: false,
                        log_message: msg,
                        convert_ms: None,
                    });
                }
            }
            rebuild_reason = "install_not_ready".into();
            log::info!(
                target: "NaviPack",
                "ensure_indexed_packs: server install reported success but packs not Ready — local rebuild reason={rebuild_reason}"
            );
        } else {
            rebuild_reason = plan.decision_reason.clone();
            log::info!(
                target: "NaviPack",
                "ensure_indexed_packs: server path unavailable reason={rebuild_reason} ({}) — rebuilding locally",
                plan.log_message
            );
        }
    } else {
        log::info!(
            target: "NaviPack",
            "ensure_indexed_packs: no region id for stem={stem} — rebuilding locally from PBF reason={rebuild_reason}"
        );
    }

    let progress_label = format!("Rebuilding locally ({rebuild_reason})…");
    crate::download::progress::set(0, None, &progress_label);
    let mut opts = ConvertOptions::new(data_dir, pbf);
    opts.elev_dir = elev_dir.map(PathBuf::from);
    opts.profiles = vec![
        RoutingProfile::Car,
        RoutingProfile::Truck,
        RoutingProfile::Foot,
        RoutingProfile::Bicycle,
    ];
    match convert_region_packs(&opts) {
        Ok(r) => {
            let msg = format!(
                "rebuilding locally ({rebuild_reason}); convert_ms={:.1}",
                r.convert_ms
            );
            log::info!(target: "NaviPack", "{msg}");
            Ok(EnsureIndexedPacksResult {
                data_source: PackDataSource::LocalBake,
                cache_hit: false,
                log_message: msg,
                convert_ms: Some(r.convert_ms),
            })
        }
        Err(e) if crate::routing::region_lock::is_convert_in_progress_err(&e) => {
            Ok(EnsureIndexedPacksResult {
                data_source: PackDataSource::LocalBake,
                cache_hit: false,
                log_message: "skipped=convert_in_progress".into(),
                convert_ms: None,
            })
        }
        Err(e) => Err(format!("indexed convert ({rebuild_reason}): {e:#}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack_server::PackCatalog;

    #[test]
    fn path_covered_exact_and_child() {
        let ready = vec![
            "europe/norway/ostlandet".into(),
            "europe/norway/vestlandet".into(),
        ];
        assert!(path_covered_by_ready_ids("europe/norway/ostlandet", &ready));
        assert!(path_covered_by_ready_ids("europe/norway", &ready));
        assert!(path_covered_by_ready_ids("europe", &ready));
        assert!(!path_covered_by_ready_ids("europe/sweden", &ready));
        assert!(!path_covered_by_ready_ids(
            "europe/norway/trondelag",
            &ready
        ));
    }

    #[test]
    fn classify_unreachable_and_fetch_tokens() {
        assert_eq!(
            classify_unreachable_kind(ConnectivityFailureKind::Timeout),
            "timeout"
        );
        assert_eq!(
            classify_unreachable_kind(ConnectivityFailureKind::HttpStatus(404)),
            "http_404"
        );
        assert_eq!(
            classify_fetch_failure(
                "server pack graph_format_version=6 (client needs 8) — not installing"
            ),
            "format_gate"
        );
        assert_eq!(
            classify_fetch_failure("installed packs not Ready for monaco-latest: Missing"),
            "install_not_ready"
        );
        assert_eq!(
            classify_local_source_reason(
                "region not published on pack server (europe/foo), using local convert"
            ),
            "not_in_catalog"
        );
    }

    #[test]
    fn resolve_reachable_region_ready() {
        let conn = Connectivity::Ready(PackCatalog {
            catalog_generation: "migrate-geofabrik-paths".into(),
            served_from: "https://navigate-me.duckdns.org".into(),
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
            PackDataSource::ServerDuckdns,
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
                assert_eq!(data_source, PackDataSource::ServerDuckdns);
                assert_eq!(base_url, "https://navigate-me.duckdns.org");
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
            kind: crate::pack_server::ConnectivityFailureKind::Timeout,
            reason: "timeout".into(),
        };
        match resolve_region_source("europe/monaco", &conn, PackDataSource::ServerDuckdns) {
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
            served_from: "https://navigate-me.duckdns.org".into(),
            regions: vec![],
        });
        match resolve_region_source("europe/monaco", &conn, PackDataSource::ServerDuckdns) {
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
            served_from: "https://navigate-me.duckdns.org".into(),
            regions: vec![ReadyRegion {
                region_id: "europe/monaco".into(),
                generation: None,
                bytes: None,
                manifest_url: None,
            }],
        });
        assert!(
            resolve_region_source("/europe/monaco/", &conn, PackDataSource::ServerDuckdns)
                .is_server()
        );
    }

    #[test]
    fn discovery_bases_default_chain() {
        // Clear override for this process if present — only assert defaults when unset.
        if std::env::var("NAVI_PACK_SERVER_BASE_URL").is_err() {
            let bases = pack_server_discovery_bases();
            assert_eq!(bases.len(), 1);
            assert_eq!(bases[0].0, PackDataSource::ServerDuckdns);
            assert_eq!(bases[0].1, DEFAULT_PACK_SERVER_BASE_URL);
        }
    }

    #[test]
    fn leaf_stem_helpers() {
        assert_eq!(leaf_stem_for_region_id("europe/monaco"), "monaco-latest");
        assert_eq!(
            leaf_stem_for_region_id("/europe/norway/ostlandet/"),
            "ostlandet-latest"
        );
    }

    #[test]
    fn vastra_gotaland_catalog_alias_resolves_to_published_underscore_id() {
        // Client/PMT chip: hyphen. navi-server publishes underscore permanently.
        let conn = Connectivity::Ready(PackCatalog {
            catalog_generation: "g".into(),
            served_from: "https://navigate-me.duckdns.org".into(),
            regions: vec![ReadyRegion {
                region_id: "europe/sweden/vastra_gotaland".into(),
                generation: Some("bake".into()),
                bytes: Some(42),
                manifest_url: Some(
                    "/packs/europe/sweden/vastra_gotaland/bake/manifest.json".into(),
                ),
            }],
        });
        match resolve_region_source(
            "europe/sweden/vastra-gotaland",
            &conn,
            PackDataSource::ServerDuckdns,
        ) {
            RegionSource::Server {
                region_id,
                generation,
                bytes,
                ..
            } => {
                assert_eq!(region_id, "europe/sweden/vastra_gotaland");
                assert_eq!(generation.as_deref(), Some("bake"));
                assert_eq!(bytes, Some(42));
            }
            RegionSource::Local { reason, .. } => panic!("expected Server via alias: {reason}"),
        }
        let ready = vec!["europe/sweden/vastra_gotaland".into()];
        assert!(path_covered_by_ready_ids(
            "europe/sweden/vastra-gotaland",
            &ready
        ));
        assert!(region_ids_match_for_catalog(
            "europe/sweden/vastra-gotaland",
            "europe/sweden/vastra_gotaland"
        ));
        assert_eq!(
            pack_catalog_region_id_aliases("europe/sweden/vastra-gotaland"),
            vec!["europe/sweden/vastra_gotaland"]
        );
    }

    #[test]
    fn catalog_parent_fallback_resolves_danish_leaves_to_country() {
        // Live pack host publishes europe/denmark, not Syddanmark / Sjælland leaves.
        let ready = vec![
            "europe/germany/hamburg".into(),
            "europe/denmark".into(),
            "europe/sweden/skane".into(),
            "europe/sweden/vastra_gotaland".into(),
        ];
        assert_eq!(
            resolve_area_to_catalog("europe/denmark/syddanmark", &ready).as_deref(),
            Some("europe/denmark")
        );
        assert_eq!(
            resolve_area_to_catalog("europe/denmark/sjaelland", &ready).as_deref(),
            Some("europe/denmark")
        );
        assert_eq!(
            resolve_area_to_catalog("europe/denmark/hovedstaden", &ready).as_deref(),
            Some("europe/denmark")
        );
        // Exact catalog hit stays on the leaf when published.
        let with_leaves = vec![
            "europe/denmark/syddanmark".into(),
            "europe/denmark/sjaelland".into(),
            "europe/denmark".into(),
        ];
        assert_eq!(
            resolve_area_to_catalog("europe/denmark/syddanmark", &with_leaves).as_deref(),
            Some("europe/denmark/syddanmark")
        );
        // Alias → published underscore spelling.
        assert_eq!(
            resolve_area_to_catalog("europe/sweden/vastra-gotaland", &ready).as_deref(),
            Some("europe/sweden/vastra_gotaland")
        );
        // Deduplicate while keeping first-occurrence order.
        let required = vec![
            "europe/denmark/syddanmark".into(),
            "europe/germany/hamburg".into(),
            "europe/denmark/sjaelland".into(),
            "europe/sweden/vastra-gotaland".into(),
        ];
        assert_eq!(
            resolve_areas_to_catalog(&required, &ready),
            vec![
                "europe/denmark".to_string(),
                "europe/germany/hamburg".to_string(),
                "europe/sweden/vastra_gotaland".to_string(),
            ]
        );
        // Generic parent walk — not Denmark-only.
        let de = vec!["europe/germany".into()];
        assert_eq!(
            resolve_area_to_catalog("europe/germany/bayern/oberbayern", &de).as_deref(),
            Some("europe/germany")
        );
        assert!(resolve_area_to_catalog("europe/norway/ostlandet", &ready).is_none());
    }

    #[test]
    fn resolve_region_source_uses_parent_fallback() {
        let conn = Connectivity::Ready(PackCatalog {
            catalog_generation: "g".into(),
            served_from: "https://navigate-me.duckdns.org".into(),
            regions: vec![ReadyRegion {
                region_id: "europe/denmark".into(),
                generation: Some("bake".into()),
                bytes: Some(1),
                manifest_url: None,
            }],
        });
        match resolve_region_source(
            "europe/denmark/syddanmark",
            &conn,
            PackDataSource::ServerDuckdns,
        ) {
            RegionSource::Server { region_id, .. } => {
                assert_eq!(region_id, "europe/denmark");
            }
            RegionSource::Local { reason, .. } => {
                panic!("expected parent fallback to europe/denmark, got Local: {reason}")
            }
        }
    }
}
