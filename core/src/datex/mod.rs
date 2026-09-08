//! Host-owned DATEX II client for cached NPRA snapshots on navi-server.
//!
//! Host discovery reuses [`crate::pack_server::check_connectivity_chain`]
//! (LAN → duckdns, 3s/host, `server-lan` / `server-duckdns` tags). There is
//! **no** local-bake equivalent for live traffic — both hosts failing yields
//! `data_source = none` and no overlay.
//!
//! DATEX availability on a resolved host uses
//! [`crate::pack_server::probe_path`] via [`fetch::probe_datex_source`] (same
//! HTTP probe shape as [`crate::pack_server::probe_current_json`]).
//!
//! Default: **disabled**. Failures must not block routing.

mod config;
mod fetch;
mod filter;
mod impact;
mod parse;
mod session;

pub use config::{
    DatexConfig, DATEX_PLUGIN_DEFAULT_ENABLED, DATEX_SERVER_SITUATION_POLL_SECS,
    DATEX_SETTINGS_DEFAULT_HOST, DATEX_SETTINGS_DEFAULT_PORT, DATEX_SITUATION_PATH,
    DATEX_SOURCE_PATH, DATEX_WIFI_ONLY_DEFAULT,
};
pub use fetch::{
    fetch_situation_xml, fetch_situation_xml_only, fetch_source_meta, probe_datex_source,
    DatexFetchError, DatexSourceMeta,
};
pub use filter::{corridor_view, filter_near_route, split_active_inactive, DatexCorridorView};
pub use impact::{
    classify_impact, planner_impacts, DatexImpact, DatexPlannerConstraint, DATEX_IMPACT_RADIUS_M,
    DATEX_PENALIZE_MULT,
};
pub use parse::{parse_situation_publication, DatexSituation, SituationKind};
pub use session::{reset_session_for_tests, with_session, DatexSession};

use chrono::{DateTime, Utc};

use crate::pack_server::{
    self, check_connectivity_chain_blocking, data_source_tag, pack_server_discovery_bases,
    PackDataSource,
};

use session::{fingerprint_source_body, load_disk_cache, save_disk_cache};

/// Outcome of a refresh attempt suitable for HUD / overlay wiring.
#[derive(Debug, Clone)]
pub struct DatexRefreshResult {
    pub overlay_enabled: bool,
    pub situations_on_route: Vec<DatexSituation>,
    pub active: Vec<DatexSituation>,
    pub inactive: Vec<DatexSituation>,
    pub attribution: Option<String>,
    pub warning: Option<String>,
    /// `server-lan` / `server-duckdns` / `none` (never `local-bake` for DATEX).
    pub data_source: String,
}

fn empty_result(warning: impl Into<String>, data_source: &str) -> DatexRefreshResult {
    DatexRefreshResult {
        overlay_enabled: false,
        situations_on_route: Vec::new(),
        active: Vec::new(),
        inactive: Vec::new(),
        attribution: None,
        warning: Some(warning.into()),
        data_source: data_source.to_string(),
    }
}

fn view_result(
    view: DatexCorridorView,
    attribution: Option<String>,
    warning: Option<String>,
    data_source: PackDataSource,
) -> DatexRefreshResult {
    DatexRefreshResult {
        overlay_enabled: true,
        situations_on_route: {
            let mut v = view.active.clone();
            v.extend(view.inactive.iter().cloned());
            v
        },
        active: view.active,
        inactive: view.inactive,
        attribution,
        warning,
        data_source: data_source.as_str().to_string(),
    }
}

/// Resolve a DATEX host: reuse sticky hop when still reachable; otherwise run
/// the pack_server discovery chain. Returns `None` when no host is usable.
fn resolve_datex_host(
    config: &DatexConfig,
) -> Result<Option<(PackDataSource, String)>, DatexFetchError> {
    if !config.use_discovery_chain {
        let base = pack_server::base_url(&config.host, config.port);
        let tag = if base.trim_end_matches('/') == pack_server::FALLBACK_PACK_SERVER_BASE_URL {
            PackDataSource::ServerDuckdns
        } else {
            PackDataSource::ServerLan
        };
        match probe_datex_source(&base) {
            Ok(true) => {
                with_session(|s| s.remember_host(tag, base.clone()));
                return Ok(Some((tag, base)));
            }
            Ok(false) => return Ok(None),
            Err(e) => return Err(e.into()),
        }
    }

    // Sticky reuse: probe known-good host first (DATEX source.json via probe_path).
    let sticky = with_session(|s| s.sticky.clone());
    if let Some((tag, base)) = sticky {
        match probe_datex_source(&base) {
            Ok(true) => {
                with_session(|s| s.sticky_reuse_count = s.sticky_reuse_count.saturating_add(1));
                log::info!(
                    target: "NaviDatex",
                    "sticky host reuse source={} base={}",
                    tag.as_str(),
                    base
                );
                return Ok(Some((tag, base)));
            }
            Ok(false) | Err(_) => {
                log::info!(
                    target: "NaviDatex",
                    "sticky host failed source={} base={}; re-probing chain",
                    tag.as_str(),
                    base
                );
                with_session(|s| s.clear_sticky());
            }
        }
    }

    with_session(|s| s.chain_probe_count = s.chain_probe_count.saturating_add(1));
    let bases = config
        .discovery_bases_override
        .clone()
        .unwrap_or_else(pack_server_discovery_bases);
    let (conn, hop) = check_connectivity_chain_blocking(&bases);
    let Some(tag) = hop else {
        log::warn!(
            target: "NaviDatex",
            "discovery chain failed: {}",
            match &conn {
                pack_server::Connectivity::Unreachable { reason } => reason.as_str(),
                pack_server::Connectivity::Ready(_) => "unknown",
            }
        );
        return Ok(None);
    };
    let base = match &conn {
        pack_server::Connectivity::Ready(c) => c.served_from.clone(),
        pack_server::Connectivity::Unreachable { .. } => return Ok(None),
    };

    // Host serves current.json — confirm DATEX is published there.
    match probe_datex_source(&base) {
        Ok(true) => {
            with_session(|s| s.remember_host(tag, base.clone()));
            log::info!(
                target: "NaviDatex",
                "resolved source={} base={}",
                tag.as_str(),
                base
            );
            Ok(Some((tag, base)))
        }
        Ok(false) => {
            log::warn!(
                target: "NaviDatex",
                "host {} reachable but /datex/source.json missing",
                tag.as_str()
            );
            Ok(None)
        }
        Err(e) => Err(e.into()),
    }
}

fn hydrate_session_from_disk(config: &DatexConfig) {
    let Some(dir) = config.cache_dir.as_ref() else {
        return;
    };
    let Some((meta, xml, fetched, fp)) = load_disk_cache(dir) else {
        return;
    };
    with_session(|s| {
        if s.cached_xml.is_none() {
            s.cached_meta = Some(meta);
            s.cached_xml = Some(xml);
            s.last_fetch_unix = Some(fetched);
            s.source_fingerprint = Some(fp);
        }
    });
}

/// Refresh DATEX for the active route corridor.
///
/// Network economy:
/// - disabled / no route / wifi-only without Wi-Fi → no pull
/// - poll interval clamped to server TTL (300s)
/// - sticky host until it fails, then re-run chain
/// - `source.json` fingerprint unchanged → skip GetSituation.xml body
/// - both hosts down → `data_source=none`, empty overlay (no stale actives)
pub fn refresh_for_route(
    config: &DatexConfig,
    route_lat_lon: &[(f64, f64)],
    now: DateTime<Utc>,
) -> DatexRefreshResult {
    if !config.enabled {
        return empty_result("plugin_disabled", "none");
    }
    if route_lat_lon.len() < 2 {
        return empty_result("no_route", "none");
    }
    if config.wifi_only && !config.on_wifi {
        return empty_result("wifi_only", "none");
    }

    hydrate_session_from_disk(config);

    let now_unix = now.timestamp();
    let poll_secs = config.effective_poll_interval_secs() as i64;

    // Within poll window: re-filter in-memory cache only (no network).
    let within_poll = with_session(|s| {
        s.last_fetch_unix
            .is_some_and(|t| now_unix.saturating_sub(t) < poll_secs && s.cached_xml.is_some())
    });
    if within_poll {
        return with_session(|s| {
            let xml = s.cached_xml.as_deref().unwrap_or("");
            let meta = s.cached_meta.clone();
            let tag = s
                .sticky
                .as_ref()
                .map(|(t, _)| *t)
                .unwrap_or(PackDataSource::ServerLan);
            match parse_situation_publication(xml) {
                Ok(all) => {
                    let view = corridor_view(&all, route_lat_lon, config.corridor_margin_m(), now);
                    view_result(
                        view,
                        meta.as_ref()
                            .and_then(|m| m.attribution.clone().or(m.source.clone())),
                        Some("cache_poll_window".into()),
                        tag,
                    )
                }
                Err(e) => empty_result(format!("parse_failed:{e}"), data_source_tag(Some(tag))),
            }
        });
    }

    let host = match resolve_datex_host(config) {
        Ok(Some(h)) => h,
        Ok(None) => {
            // Do not surface stale cache as active when hosts are unreachable.
            with_session(|s| {
                s.cached_xml = None;
                s.cached_meta = None;
            });
            return empty_result("hosts_unreachable", "none");
        }
        Err(e) => {
            with_session(|s| {
                s.clear_sticky();
                s.cached_xml = None;
                s.cached_meta = None;
            });
            return empty_result(format!("fetch_failed:{e}"), "none");
        }
    };
    let (tag, base) = host;

    let (meta, source_body) = match fetch_source_meta(&base) {
        Ok(v) => v,
        Err(DatexFetchError::Unavailable) => {
            with_session(|s| s.clear_sticky());
            return empty_result("unavailable_404", "none");
        }
        Err(e) => {
            with_session(|s| s.clear_sticky());
            return empty_result(format!("fetch_failed:{e}"), "none");
        }
    };
    let fp = fingerprint_source_body(&source_body);

    let skip_xml = with_session(|s| {
        s.source_fingerprint.as_deref() == Some(fp.as_str()) && s.cached_xml.is_some()
    });

    let xml = if skip_xml {
        log::info!(
            target: "NaviDatex",
            "source={} source.json unchanged; skipping GetSituation.xml",
            tag.as_str()
        );
        with_session(|s| s.cached_xml.clone().unwrap_or_default())
    } else {
        match fetch_situation_xml_only(&base) {
            Ok(xml) => xml,
            Err(DatexFetchError::Unavailable) => {
                with_session(|s| s.clear_sticky());
                return empty_result("unavailable_404", "none");
            }
            Err(e) => {
                with_session(|s| s.clear_sticky());
                return empty_result(format!("fetch_failed:{e}"), "none");
            }
        }
    };

    with_session(|s| {
        s.cached_xml = Some(xml.clone());
        s.cached_meta = Some(meta.clone());
        s.last_fetch_unix = Some(now_unix);
        s.source_fingerprint = Some(fp.clone());
        s.remember_host(tag, base.clone());
    });

    if let Some(dir) = config.cache_dir.as_ref() {
        if let Err(e) = save_disk_cache(dir, &meta, &xml, now_unix, &fp, tag, &base) {
            log::warn!(target: "NaviDatex", "persist cache failed: {e}");
        }
    }

    match parse_situation_publication(&xml) {
        Ok(all) => {
            let view = corridor_view(&all, route_lat_lon, config.corridor_margin_m(), now);
            view_result(
                view,
                meta.attribution.or(meta.source),
                if skip_xml {
                    Some("source_unchanged".into())
                } else {
                    None
                },
                tag,
            )
        }
        Err(e) => {
            log::warn!(target: "NaviDatex", "parse failed ({e}); overlay disabled");
            // Malformed XML is a DATEX-layer failure — keep PackServerError clean.
            empty_result(format!("parse_failed:{e}"), tag.as_str())
        }
    }
}
