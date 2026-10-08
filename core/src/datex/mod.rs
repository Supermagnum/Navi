//! Host-owned DATEX II client for cached NPRA snapshots on navi-server.
//!
//! Host discovery reuses [`crate::pack_server::check_connectivity_chain`]
//! (public pack host, 3s timeout, `server-duckdns` tag). There is
//! **no** local-bake equivalent for live traffic — host failing yields
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
    classify_impact, constraint_from_situation, default_penalty_minutes, delay_penalize_mult,
    is_full_closure_management, is_known_situation_xsi_type, is_never_block_management,
    is_structurally_ignore, planner_impacts, text_indicates_closure, wind_penalize_mult,
    DatexClassification, DatexClassifyFields, DatexImpact, DatexPlannerConstraint, CLOSURE_PHRASES,
    CLOSURE_RISK_EXCLUSIONS, DATEX_IMPACT_RADIUS_M, DATEX_PENALIZE_MULT, DATEX_PENALIZE_MULT_MAX,
    DATEX_PENALIZE_MULT_MIN, NPRA_LIVE_XSI_TYPES, SCHEMA_VALID_UNUSED_XSI_TYPES,
};
pub use parse::{parse_situation_publication, DatexSituation, DatexValidPeriod, SituationKind};
pub use session::{reset_session_for_tests, with_session, DatexSession};

use chrono::{DateTime, Utc};

use crate::pack_server::{
    self, check_connectivity_chain_blocking, data_source_tag, pack_server_discovery_bases,
    PackDataSource,
};

use session::{fingerprint_source_body, load_disk_cache, save_disk_cache};

use std::path::Path;

/// Stamp file under `datex_cache/`: when present, initial plan applies cached
/// DATEX impacts. Written when the plugin is enabled; removed when disabled.
pub const DATEX_APPLY_TO_ROUTING_STAMP: &str = "apply_to_routing";

/// Max age of a disk DATEX snapshot for plan-time impacts (seconds).
///
/// Matches three server poll intervals ([`DATEX_SERVER_SITUATION_POLL_SECS`]).
/// Older caches are ignored (empty impacts) so a week-old closure snapshot
/// cannot silently block a cold-start plan after the incident has cleared.
pub const DATEX_PLAN_CACHE_MAX_AGE_SECS: i64 = (DATEX_SERVER_SITUATION_POLL_SECS as i64) * 3;

/// Plan-time corridor band (metres) for a **single hop** (bbox + small margin).
/// Long-trip densify must not load DATEX for the whole Bevensen→Dalsøren chord.
pub const DATEX_HOP_MARGIN_M: f64 = 5_000.0;

/// Debug plan mode: `none` (no DATEX), `saved` (use cache, ignore TTL), `live` (TTL).
pub const DATEX_PLAN_MODE_FILE: &str = "datex_plan_mode";

/// On-disk GetSituation body (under `datex_cache/`). Copied next to routing-plan.log.
pub const DATEX_CACHE_XML_FILE: &str = "datex-GetSituation.xml";

/// Load active DATEX constraints for the **current hop** (bbox + [`DATEX_HOP_MARGIN_M`]).
///
/// Soft / no-op when the plugin stamp is missing, the cache is empty/stale, or
/// parse fails — routing must not fail open-blocked because DATEX is unavailable.
pub fn planner_impacts_from_data_dir(
    data_dir: &Path,
    route_lat_lon: &[(f64, f64)],
    now: DateTime<Utc>,
) -> Vec<DatexPlannerConstraint> {
    let Some(all) = load_plan_datex_situations(data_dir, now) else {
        return Vec::new();
    };
    impacts_near_route(&all, route_lat_lon, DATEX_HOP_MARGIN_M, now)
}

/// Parse the on-disk snapshot when the apply stamp is present.
///
/// `datex_plan_mode` / `NAVI_DATEX_MODE`: `none` skips, `saved` ignores TTL,
/// `live` (default) drops caches older than [`DATEX_PLAN_CACHE_MAX_AGE_SECS`].
pub fn load_plan_datex_situations(
    data_dir: &Path,
    now: DateTime<Utc>,
) -> Option<Vec<DatexSituation>> {
    if datex_plan_mode(data_dir) == DatexPlanMode::None {
        return Some(Vec::new());
    }
    let cache_dir = data_dir.join("datex_cache");
    if !cache_dir.join(DATEX_APPLY_TO_ROUTING_STAMP).is_file() {
        return None;
    }
    let (_meta, xml, fetched_unix, _fp) = load_disk_cache(&cache_dir)?;
    let ignore_ttl = datex_plan_mode(data_dir) == DatexPlanMode::Saved;
    if !ignore_ttl {
        let age_secs = now.timestamp().saturating_sub(fetched_unix);
        if age_secs > DATEX_PLAN_CACHE_MAX_AGE_SECS {
            log::info!(
                target: "NaviDatex",
                "plan-time DATEX cache stale age_secs={age_secs} max={DATEX_PLAN_CACHE_MAX_AGE_SECS}; skipping"
            );
            return Some(Vec::new());
        }
    }
    match parse_situation_publication(&xml) {
        Ok(v) => Some(v),
        Err(e) => {
            log::warn!(target: "NaviDatex", "plan-time DATEX parse failed: {e}");
            None
        }
    }
}

/// Copy the XML the planner actually opened, plus a JSON index of parsed sits.
pub fn copy_plan_datex_xml(data_dir: &Path) {
    let cache_xml = data_dir.join("datex_cache").join(DATEX_CACHE_XML_FILE);
    if let Ok(xml) = std::fs::read(&cache_xml) {
        crate::routing::plan_file_log::write_file(
            crate::routing::plan_file_log::DATEX_SNAPSHOT_XML,
            xml,
        );
    }
}

/// Copy the XML the planner actually opened, plus a JSON index of parsed sits.
pub fn write_plan_datex_snapshot(data_dir: &Path, sits: &[DatexSituation]) {
    copy_plan_datex_xml(data_dir);
    let mode = match datex_plan_mode(data_dir) {
        DatexPlanMode::Live => "live",
        DatexPlanMode::Saved => "saved",
        DatexPlanMode::None => "none",
    };
    let mut body = format!("{{\n  \"mode\": \"{mode}\",\n  \"situations\": [\n");
    for (i, s) in sits.iter().enumerate() {
        if i > 0 {
            body.push_str(",\n");
        }
        let (lat, lon) = s.primary_lat_lon().unwrap_or((0.0, 0.0));
        let vf = s.valid_from.map(|t| t.to_rfc3339()).unwrap_or_default();
        let vt = s.valid_to.map(|t| t.to_rfc3339()).unwrap_or_default();
        body.push_str(&format!(
            "    {{\"id\":\"{}\",\"xsi\":\"{}\",\"kind\":\"{}\",\"impact\":\"{}\",\"lat\":{:.5},\"lon\":{:.5},\"road\":\"{}\",\"valid_from\":\"{}\",\"valid_to\":\"{}\"}}",
            s.id.replace('"', ""),
            s.xsi_type.replace('"', ""),
            s.kind.as_str(),
            s.impact.as_str(),
            lat,
            lon,
            s.road_number.clone().unwrap_or_default().replace('"', ""),
            vf.replace('"', ""),
            vt.replace('"', ""),
        ));
    }
    body.push_str("\n  ]\n}\n");
    crate::routing::plan_file_log::write_file(
        crate::routing::plan_file_log::DATEX_SNAPSHOT_JSON,
        body,
    );
}

/// Filter parsed situations to a hop corridor and classify planner impacts.
pub fn impacts_near_route(
    all: &[DatexSituation],
    route_lat_lon: &[(f64, f64)],
    margin_m: f64,
    now: DateTime<Utc>,
) -> Vec<DatexPlannerConstraint> {
    impacts_near_route_ctx(
        all,
        route_lat_lon,
        margin_m,
        DatexHopContext {
            arrival: now,
            trip_uncertain: false,
            hop_bearing_deg: None,
            truck: true,
        },
    )
}

/// Arrival-aware DATEX apply for one hop.
#[derive(Debug, Clone, Copy)]
pub struct DatexHopContext {
    pub arrival: DateTime<Utc>,
    /// Multi-day / long remaining ETA: windowed closures warn instead of block.
    pub trip_uncertain: bool,
    pub hop_bearing_deg: Option<f64>,
    pub truck: bool,
}

pub fn impacts_near_route_ctx(
    all: &[DatexSituation],
    route_lat_lon: &[(f64, f64)],
    margin_m: f64,
    ctx: DatexHopContext,
) -> Vec<DatexPlannerConstraint> {
    if route_lat_lon.len() < 2 {
        return Vec::new();
    }
    let near = filter_near_route(all, route_lat_lon, margin_m);
    let mut out = Vec::new();
    for mut s in near {
        if !vehicle_applies(&s, ctx.truck) {
            continue;
        }
        if !direction_applies(&s, ctx.hop_bearing_deg) {
            continue;
        }
        let active = s.is_active_at_arrival(ctx.arrival);
        if active {
            if ctx.trip_uncertain && s.has_recurring_windows() && s.impact == DatexImpact::Block {
                s.impact = DatexImpact::Warn;
            }
            out.push(constraint_from_situation(&s));
            continue;
        }
        // Not active at arrival: no block/penalty. Windowed records still warn.
        if s.has_recurring_windows() && s.impact != DatexImpact::Ignore {
            s.impact = DatexImpact::Warn;
            s.penalty_minutes = 0.0;
            out.push(constraint_from_situation(&s));
        }
    }
    out
}

fn vehicle_applies(s: &DatexSituation, truck: bool) -> bool {
    if s.vehicle_types.is_empty() {
        return true;
    }
    let types: Vec<String> = s
        .vehicle_types
        .iter()
        .map(|t| t.to_ascii_lowercase())
        .collect();
    if types.iter().any(|t| t == "all" || t == "any") {
        return true;
    }
    if truck {
        types.iter().any(|t| {
            t.contains("lorry")
                || t.contains("hgv")
                || t.contains("truck")
                || t.contains("heavy")
                || t == "car"
                || t.contains("vehicle")
        })
    } else {
        types
            .iter()
            .any(|t| t.contains("car") || t.contains("vehicle") || t.contains("motor"))
    }
}

fn direction_applies(s: &DatexSituation, hop_bearing_deg: Option<f64>) -> bool {
    let Some(dir) = s.direction.as_deref() else {
        return true;
    };
    let d = dir.to_ascii_lowercase();
    if d.contains("both") || d.contains("unknown") {
        return true;
    }
    let Some(hop) = hop_bearing_deg else {
        return true;
    };
    if s.geometry.len() < 2 {
        return true;
    }
    let (a_lat, a_lon) = s.geometry[0];
    let (b_lat, b_lon) = s.geometry[s.geometry.len() - 1];
    let sit = bearing_deg(a_lat, a_lon, b_lat, b_lon);
    let diff = angle_diff_deg(hop, sit);
    if d.contains("neg") {
        diff > 60.0
    } else if d.contains("pos") {
        diff < 120.0
    } else {
        true
    }
}

fn bearing_deg(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let p1 = lat1.to_radians();
    let p2 = lat2.to_radians();
    let dl = (lon2 - lon1).to_radians();
    let y = dl.sin() * p2.cos();
    let x = p1.cos() * p2.sin() - p1.sin() * p2.cos() * dl.cos();
    (y.atan2(x).to_degrees() + 360.0) % 360.0
}

fn angle_diff_deg(a: f64, b: f64) -> f64 {
    let d = (a - b).abs() % 360.0;
    d.min(360.0 - d)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatexPlanMode {
    Live,
    Saved,
    None,
}

pub fn datex_plan_mode(data_dir: &Path) -> DatexPlanMode {
    let env = std::env::var("NAVI_DATEX_MODE").unwrap_or_default();
    parse_datex_plan_mode(&env)
        .or_else(|| {
            std::fs::read_to_string(data_dir.join(DATEX_PLAN_MODE_FILE))
                .ok()
                .and_then(|s| parse_datex_plan_mode(s.trim()))
        })
        .unwrap_or(DatexPlanMode::Live)
}

pub fn set_datex_plan_mode(data_dir: &Path, mode: DatexPlanMode) {
    let path = data_dir.join(DATEX_PLAN_MODE_FILE);
    let body = match mode {
        DatexPlanMode::Live => "live",
        DatexPlanMode::Saved => "saved",
        DatexPlanMode::None => "none",
    };
    let _ = std::fs::write(path, body);
}

fn parse_datex_plan_mode(s: &str) -> Option<DatexPlanMode> {
    match s.trim().to_ascii_lowercase().as_str() {
        "none" | "off" | "0" => Some(DatexPlanMode::None),
        "saved" | "snapshot" | "cache" => Some(DatexPlanMode::Saved),
        "live" | "ttl" => Some(DatexPlanMode::Live),
        _ => None,
    }
}

/// Create or remove the plan-time DATEX apply stamp under `datex_cache`.
pub fn set_apply_to_routing(cache_dir: &Path, enabled: bool) {
    let stamp = cache_dir.join(DATEX_APPLY_TO_ROUTING_STAMP);
    if enabled {
        let _ = std::fs::create_dir_all(cache_dir);
        let _ = std::fs::write(&stamp, b"1");
    } else {
        let _ = std::fs::remove_file(&stamp);
    }
}

/// Outcome of a refresh attempt suitable for HUD / overlay wiring.
#[derive(Debug, Clone)]
pub struct DatexRefreshResult {
    pub overlay_enabled: bool,
    pub situations_on_route: Vec<DatexSituation>,
    pub active: Vec<DatexSituation>,
    pub inactive: Vec<DatexSituation>,
    pub attribution: Option<String>,
    pub warning: Option<String>,
    /// `server-duckdns` / `none` (never `local-bake` for DATEX).
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
        let tag = PackDataSource::ServerDuckdns;
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
                pack_server::Connectivity::Unreachable { reason, .. } => reason.as_str(),
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
                "host {} reachable but /datex/npra/source.json missing",
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
    if let Some(dir) = config.cache_dir.as_ref() {
        if let Some((meta, xml, fetched, fp)) = load_disk_cache(dir) {
            if fp.starts_with("navi-synth")
                && now_unix.saturating_sub(fetched) <= DATEX_PLAN_CACHE_MAX_AGE_SECS
            {
                log::info!(
                    target: "NaviDatex",
                    "keeping navi-synth cache age_secs={}; skip NPRA pull",
                    now_unix.saturating_sub(fetched)
                );
                return match parse_situation_publication(&xml) {
                    Ok(all) => {
                        let view =
                            corridor_view(&all, route_lat_lon, config.corridor_margin_m(), now);
                        view_result(
                            view,
                            meta.attribution.or(meta.source),
                            Some("navi_synth_keep".into()),
                            PackDataSource::ServerDuckdns,
                        )
                    }
                    Err(e) => empty_result(format!("parse_failed:{e}"), "server-duckdns"),
                };
            }
        }
    }

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
                .unwrap_or(PackDataSource::ServerDuckdns);
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
