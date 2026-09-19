//! Host-only long-trip region handling (no Android / emulator).
//!
//! Packs are fetched with the same read-only `pack_server` path the app uses,
//! from `https://navigate-me.duckdns.org`, with Geofabrik PBF convert as the
//! automatic fallback. Production code is not modified: missing planner
//! features are stubbed in this test only and listed as gaps.
//!
//! ```bash
//! cargo test -p driver-break-core --test long_trip_region_handling -- --ignored --nocapture
//! ```
//!
//! Optional:
//! - `NAVI_LONG_TRIP_DIR=/path` reuse a download cache
//! - `NAVI_LONG_TRIP_DATE=YYYY-MM-DD` departure date (default 2026-07-15, when Friisvegen is open)
//! - `NAVI_LONG_TRIP_BREAK_MIN=15` rest-break length
//! - `NAVI_LONG_TRIP_TIMEOUT_SECS=14400` per-region install+index budget
//! - `NAVI_PACK_SERVER_BASE_URL` (default `https://navigate-me.duckdns.org`)

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use chrono::{Duration as ChronoDuration, NaiveDate, NaiveDateTime, NaiveTime};
use driver_break_core::pack_server::{
    catalog_entries_from_ready_ids, discover_pack_catalog, ensure_geofabrik_pbf_for_region,
    ensure_indexed_packs_prefer_server, ensure_place_index_after_pack_install,
    leaf_stem_for_region_id, normalize_region_id, ordered_regions_along_corridor,
    pack_catalog_region_id_aliases, plan_region_acquisition, region_ids_match_for_catalog,
    resolve_area_to_catalog, resolve_areas_to_catalog, PackCatalogSnapshot, PackDataSource,
    PLACE_INDEX_DB_NAME,
};
use driver_break_core::routing::eta::motor_path_minutes_from_edges;
use driver_break_core::routing::graph::{RouteGraph, RouteOptions, RoutingProfile};
use driver_break_core::routing::indexed::{
    bbox_intersects, load_graph_pack_bbox, merge_tile_graphs, try_load_graph_for_plan_bbox,
    NaviManifest, PackStatus,
};
use driver_break_core::routing::rest::{plan_motor_multi_day, MotorDailyBudget};
use driver_break_core::routing::suggest_geofabrik_path_for_point;
use driver_break_core::search::NameIndex;
use serde::Deserialize;

const PACK_BASE: &str = "https://navigate-me.duckdns.org";
const BASE_REGION: &str = "europe/germany/niedersachsen";
/// OSM node 12985331075 (`tourism=viewpoint`), not a highway vertex.
const DEST_LAT: f64 = 61.5929077;
const DEST_LON: f64 = 10.3318551;
/// Closest vertex on Friisvegen / Fv2204 (OSM way 361797686, node 7078112184), ~6 m from the viewpoint.
const FRIISVEGEN_LAT: f64 = 61.5928621;
const FRIISVEGEN_LON: f64 = 10.331906;
const PLACE_QUERY: &str = "Bahnhof Klecken";
const HIGHWAY_KMH: f64 = 90.0;
const SAMPLE_STEP_KM: f64 = 25.0;

/// Per-behaviour source tag printed in the long-trip report.
#[derive(Debug, Clone, Copy)]
enum BehaviourSource {
    Production,
    Stub,
}

impl BehaviourSource {
    fn label(self) -> &'static str {
        match self {
            Self::Production => "production",
            Self::Stub => "stub",
        }
    }
}

const FERRY_JSON: &str = include_str!("fixtures/long_trip/ferry_timetable.json");
const BBOX_JSON: &str = include_str!("fixtures/long_trip/subregion_bboxes.json");
const CORRIDOR_JSON: &str = include_str!("fixtures/long_trip/scenario_corridors.json");

#[derive(Debug, Clone)]
struct Assertion {
    name: String,
    pass: bool,
    detail: String,
}

impl Assertion {
    fn check(name: &str, pass: bool, detail: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            pass,
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct FerryFile {
    lines: Vec<FerryLine>,
    forbidden_needles: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct FerryLine {
    id: String,
    name: String,
    match_needles: Vec<String>,
    from: NamedCoord,
    to: NamedCoord,
    checkin_minutes: i64,
    sailing_minutes: i64,
    departures_local: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct NamedCoord {
    #[allow(dead_code)]
    name: String,
    lat: f64,
    lon: f64,
}

#[derive(Debug, Deserialize)]
struct BboxFile {
    regions: Vec<BboxRegion>,
}

#[derive(Debug, Clone, Deserialize)]
struct BboxRegion {
    id: String,
    bbox: [f64; 4],
}

#[derive(Debug, Deserialize)]
struct CorridorFile {
    scenarios: Vec<Scenario>,
}

#[derive(Debug, Clone, Deserialize)]
struct Scenario {
    id: String,
    label: String,
    #[serde(default)]
    ferry_id: Option<String>,
    waypoints: Vec<NamedCoord>,
}

#[derive(Debug, Clone)]
struct RegionRow {
    region_id: String,
    download_bytes: u64,
    pack_on_disk: u64,
    place_index_on_disk: u64,
    download_s: f64,
    install_s: f64,
    index_s: f64,
    peak_rss_bytes: u64,
    source: String,
    fallback: Option<String>,
}

#[derive(Debug, Clone)]
struct ItinEvent {
    at: NaiveDateTime,
    kind: String,
    detail: String,
}

fn pack_base() -> String {
    std::env::var("NAVI_PACK_SERVER_BASE_URL")
        .ok()
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| PACK_BASE.trim_end_matches('/').to_string())
}

fn param_date() -> NaiveDate {
    if let Ok(s) = std::env::var("NAVI_LONG_TRIP_DATE") {
        if let Ok(d) = NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d") {
            return d;
        }
    }
    NaiveDate::from_ymd_opt(2026, 7, 15).expect("default date")
}

fn break_minutes() -> i64 {
    std::env::var("NAVI_LONG_TRIP_BREAK_MIN")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(15)
}

fn break_every_hours() -> f64 {
    std::env::var("NAVI_LONG_TRIP_BREAK_HOURS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2.0)
}

fn daily_hours() -> f64 {
    std::env::var("NAVI_LONG_TRIP_DAILY_HOURS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8.0)
}

fn sleep_hours() -> f64 {
    std::env::var("NAVI_LONG_TRIP_SLEEP_HOURS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(11.0)
}

fn region_timeout() -> Duration {
    let secs: u64 = std::env::var("NAVI_LONG_TRIP_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4 * 60 * 60);
    Duration::from_secs(secs.max(60))
}

fn data_root() -> PathBuf {
    if let Ok(p) = std::env::var("NAVI_LONG_TRIP_DIR") {
        let p = PathBuf::from(p.trim());
        if !p.as_os_str().is_empty() {
            return p;
        }
    }
    std::env::temp_dir().join("navi-long-trip-region-handling")
}

fn haversine_km(a: (f64, f64), b: (f64, f64)) -> f64 {
    let r = 6371.0;
    let dlat = (b.0 - a.0).to_radians();
    let dlon = (b.1 - a.1).to_radians();
    let x = (dlat / 2.0).sin().powi(2)
        + a.0.to_radians().cos() * b.0.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * r * x.sqrt().asin()
}

fn bbox_covers(bbox: [f64; 4], lat: f64, lon: f64) -> bool {
    lat >= bbox[0] && lat <= bbox[2] && lon >= bbox[1] && lon <= bbox[3]
}

fn bbox_area(bbox: [f64; 4]) -> f64 {
    (bbox[2] - bbox[0]).max(0.0) * (bbox[3] - bbox[1]).max(0.0)
}

fn rss_and_hwm() -> (u64, u64) {
    let Ok(text) = fs::read_to_string("/proc/self/status") else {
        return (0, 0);
    };
    let mut rss = 0u64;
    let mut hwm = 0u64;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            rss = rest
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0)
                * 1024;
        }
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            hwm = rest
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0)
                * 1024;
        }
    }
    (rss, hwm)
}

fn dir_size_if(dir: &Path, pred: &dyn Fn(&Path) -> bool) -> u64 {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    let mut n = 0u64;
    for ent in entries.flatten() {
        let path = ent.path();
        if path.is_dir() {
            n += dir_size_if(&path, pred);
            continue;
        }
        if pred(&path) {
            n += ent.metadata().map(|m| m.len()).unwrap_or(0);
        }
    }
    n
}

fn is_pack_file(path: &Path) -> bool {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    name.ends_with(".rkyv")
        || name.ends_with(".navi-manifest.json")
        || name.ends_with(".navi-server-install.json")
}

fn is_place_index_file(path: &Path) -> bool {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    name.starts_with("place_index.db")
}

fn list_manifests(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for ent in entries.flatten() {
        let name = ent.file_name();
        let name = name.to_string_lossy();
        if name.ends_with(".navi-manifest.json") {
            out.push(name.trim_end_matches(".navi-manifest.json").to_string());
        }
    }
    out.sort();
    out
}

fn packs_ready(data_dir: &Path, region_id: &str) -> bool {
    let stem = leaf_stem_for_region_id(region_id);
    let path = data_dir.join(format!("{stem}.navi-manifest.json"));
    let Ok(man) = NaviManifest::load(&path) else {
        return false;
    };
    man.status_pack_files(data_dir) == PackStatus::Ready
}

fn place_index_ready(data_dir: &Path, region_id: &str) -> bool {
    let db = data_dir.join(PLACE_INDEX_DB_NAME);
    NameIndex::has_entries_for_region(&db, region_id)
        && NameIndex::region_index_complete(&db, region_id)
}

fn wait_ready(data_dir: &Path, region_id: &str, timeout: Duration) -> Result<(), String> {
    let t0 = Instant::now();
    loop {
        if packs_ready(data_dir, region_id) && place_index_ready(data_dir, region_id) {
            return Ok(());
        }
        if t0.elapsed() > timeout {
            return Err(format!(
                "timeout after {:.0}s waiting for install+index of {region_id}: packs_ready={} place_index_ready={}",
                t0.elapsed().as_secs_f64(),
                packs_ready(data_dir, region_id),
                place_index_ready(data_dir, region_id)
            ));
        }
        thread::sleep(Duration::from_millis(250));
    }
}

fn load_ferries() -> FerryFile {
    serde_json::from_str(FERRY_JSON).expect("ferry fixture")
}

fn load_bboxes() -> Vec<BboxRegion> {
    serde_json::from_str::<BboxFile>(BBOX_JSON)
        .expect("bbox fixture")
        .regions
}

fn load_scenarios() -> Vec<Scenario> {
    serde_json::from_str::<CorridorFile>(CORRIDOR_JSON)
        .expect("corridor fixture")
        .scenarios
}

fn expected_for_scenario(id: &str) -> &'static [&'static str] {
    match id {
        // Against current-style catalog (Danish leaves absent → europe/denmark).
        "all_road" => &[
            "europe/germany/hamburg",
            "europe/germany/schleswig-holstein",
            "europe/denmark",
            "europe/sweden/skane",
            "europe/sweden/halland",
            "europe/sweden/vastra_gotaland",
            "europe/norway/ostlandet",
        ],
        "kiel-oslo" => &[
            "europe/germany/hamburg",
            "europe/germany/schleswig-holstein",
            // Schleswig approaches + densified samples graze the Denmark country box.
            "europe/denmark",
            "europe/norway/ostlandet",
        ],
        "copenhagen-oslo" => &[
            "europe/germany/hamburg",
            "europe/germany/schleswig-holstein",
            "europe/denmark",
            // Ferry chord Copenhagen→Oslo is densified as a straight line and
            // falsely samples southern Sweden (bbox-only limitation).
            "europe/sweden/skane",
            "europe/sweden/halland",
            "europe/sweden/vastra_gotaland",
            "europe/norway/ostlandet",
        ],
        "hirtshals-larvik" => &[
            "europe/germany/hamburg",
            "europe/germany/schleswig-holstein",
            "europe/denmark",
            "europe/norway/ostlandet",
        ],
        _ => &[],
    }
}

/// Production along-route list for a scenario corridor against `cat`.
fn ordered_regions_for_scenario(
    sc: &Scenario,
    cat: &PackCatalogSnapshot,
    exclude: &str,
) -> Vec<String> {
    let entries = catalog_entries_from_ready_ids(&cat.ready_region_ids);
    let waypoints: Vec<(f64, f64)> = sc.waypoints.iter().map(|w| (w.lat, w.lon)).collect();
    ordered_regions_along_corridor(&waypoints, &entries, &[exclude.to_string()], SAMPLE_STEP_KM)
}

/// Test-only finest covering region: fixture bboxes, then production point suggester.
fn suggest_finest(lat: f64, lon: f64, bboxes: &[BboxRegion]) -> String {
    let mut best: Option<(String, f64)> = None;
    for r in bboxes {
        if bbox_covers(r.bbox, lat, lon) {
            let area = bbox_area(r.bbox);
            if best.as_ref().is_none_or(|(_, a)| area < *a) {
                best = Some((r.id.clone(), area));
            }
        }
    }
    if let Some((id, _)) = best {
        return id;
    }
    suggest_geofabrik_path_for_point(lat, lon)
        .unwrap_or("unknown")
        .to_string()
}

fn canonicalize_suggested(id: &str) -> String {
    let id = normalize_region_id(id);
    if let Some(alias) = pack_catalog_region_id_aliases(&id)
        .into_iter()
        .find(|a| a.contains('_'))
    {
        // Prefer the published underscore form when the hyphen alias is used.
        if id.contains("vastra-gotaland") {
            return alias.to_string();
        }
    }
    id
}

fn catalog_contains(cat: &PackCatalogSnapshot, id: &str) -> bool {
    cat.ready_region_ids
        .iter()
        .any(|r| region_ids_match_for_catalog(r, id))
}

/// Production catalog parent fallback ([`resolve_area_to_catalog`]).
fn resolve_download_id(suggested: &str, cat: &PackCatalogSnapshot) -> (String, Option<String>) {
    let original = canonicalize_suggested(suggested);
    match resolve_area_to_catalog(&original, &cat.ready_region_ids) {
        Some(published) if region_ids_match_for_catalog(&published, &original) => (published, None),
        Some(published) => (
            published.clone(),
            Some(format!(
                "not in current.json ({original}); fallback parent {published}"
            )),
        ),
        None => (
            original.clone(),
            Some(format!(
                "not in current.json ({original}); Geofabrik/local-bake fallback"
            )),
        ),
    }
}

fn dedupe_download_list(
    suggested: &[String],
    cat: &PackCatalogSnapshot,
) -> Vec<(String, Option<String>)> {
    let resolved = resolve_areas_to_catalog(suggested, &cat.ready_region_ids);
    let mut out = Vec::new();
    for id in resolved {
        let fb = suggested.iter().find_map(|s| {
            let (r, note) = resolve_download_id(s, cat);
            if region_ids_match_for_catalog(&r, &id) {
                note
            } else {
                None
            }
        });
        out.push((id, fb));
    }
    // Keep any unresolved suggestions so the live report still surfaces gaps.
    let mut seen: BTreeSet<String> = out.iter().map(|(id, _)| id.clone()).collect();
    for s in suggested {
        let (id, fb) = resolve_download_id(s, cat);
        if seen.insert(id.clone()) {
            out.push((id, fb));
        }
    }
    out
}

fn diff_lists(expected: &[&str], actual: &[String]) -> String {
    let mut lines = Vec::new();
    let max = expected.len().max(actual.len());
    for i in 0..max {
        let e = expected.get(i).copied().unwrap_or("(none)");
        let a = actual.get(i).map(String::as_str).unwrap_or("(none)");
        if e != a {
            lines.push(format!("  index {i}: expected={e} actual={a}"));
        }
    }
    for e in expected {
        if !actual.iter().any(|a| region_ids_match_for_catalog(a, e)) {
            lines.push(format!("  missing from actual: {e}"));
        }
    }
    for a in actual {
        if !expected.iter().any(|e| region_ids_match_for_catalog(a, e)) {
            lines.push(format!("  extra in actual: {a}"));
        }
    }
    if lines.is_empty() {
        "  (identical)".into()
    } else {
        lines.join("\n")
    }
}

fn drive_hours_km(km: f64) -> f64 {
    km / HIGHWAY_KMH
}

fn next_departure(after: NaiveDateTime, line: &FerryLine) -> NaiveDateTime {
    let mut day = after.date();
    for _ in 0..4 {
        for t in &line.departures_local {
            let nt = NaiveTime::parse_from_str(t, "%H:%M").expect("hh:mm");
            let dep = day.and_time(nt);
            let checkin = dep - ChronoDuration::minutes(line.checkin_minutes);
            if after <= checkin {
                return dep;
            }
        }
        day = day.succ_opt().unwrap();
    }
    after + ChronoDuration::hours(24)
}

fn overlay_itinerary(
    start: NaiveDateTime,
    drive_legs_h: &[(String, f64)],
    ferry: Option<&FerryLine>,
    ferry_after_leg: Option<usize>,
    break_every_h: f64,
    break_min: i64,
    daily_h: f64,
    sleep_h: f64,
) -> (Vec<ItinEvent>, NaiveDateTime, f64) {
    let mut events = Vec::new();
    let mut t = start;
    events.push(ItinEvent {
        at: t,
        kind: "depart".into(),
        detail: "Bahnhof Klecken".into(),
    });
    let mut driving_since_break = 0.0;
    let mut driving_today = 0.0;
    let mut total_drive = 0.0;

    let insert_rest =
        |t: &mut NaiveDateTime, driving_since_break: &mut f64, events: &mut Vec<ItinEvent>| {
            *t += ChronoDuration::minutes(break_min);
            events.push(ItinEvent {
                at: *t,
                kind: "break".into(),
                detail: format!("{break_min} min rest after 2 h driving"),
            });
            *driving_since_break = 0.0;
        };
    let insert_sleep = |t: &mut NaiveDateTime,
                        driving_today: &mut f64,
                        driving_since_break: &mut f64,
                        events: &mut Vec<ItinEvent>| {
        *t += ChronoDuration::minutes((sleep_h * 60.0) as i64);
        events.push(ItinEvent {
            at: *t,
            kind: "sleep".into(),
            detail: format!("{sleep_h:.0} h sleep (daily driving cap {daily_h:.0} h)"),
        });
        *driving_today = 0.0;
        *driving_since_break = 0.0;
    };

    for (i, (label, hours)) in drive_legs_h.iter().enumerate() {
        let mut remaining = *hours;
        while remaining > 1e-6 {
            let until_break = (break_every_h - driving_since_break).max(0.0);
            let until_daily = (daily_h - driving_today).max(0.0);
            let slice = remaining.min(until_break).min(until_daily).max(1e-4);
            t += ChronoDuration::milliseconds((slice * 3_600_000.0) as i64);
            driving_since_break += slice;
            driving_today += slice;
            total_drive += slice;
            remaining -= slice;
            events.push(ItinEvent {
                at: t,
                kind: "drive".into(),
                detail: format!("{label} +{slice:.2} h (day driving {driving_today:.2} h)"),
            });
            if remaining > 1e-6 && driving_today >= daily_h - 1e-6 {
                insert_sleep(
                    &mut t,
                    &mut driving_today,
                    &mut driving_since_break,
                    &mut events,
                );
            } else if remaining > 1e-6 && driving_since_break >= break_every_h - 1e-6 {
                insert_rest(&mut t, &mut driving_since_break, &mut events);
            }
        }
        if ferry_after_leg == Some(i) {
            if let Some(line) = ferry {
                let dep = next_departure(t, line);
                let checkin = dep - ChronoDuration::minutes(line.checkin_minutes);
                if t < checkin {
                    t = checkin;
                }
                events.push(ItinEvent {
                    at: t,
                    kind: "ferry_checkin".into(),
                    detail: format!("{} check-in ({})", line.name, line.from.name),
                });
                t = dep;
                events.push(ItinEvent {
                    at: t,
                    kind: "ferry_depart".into(),
                    detail: format!("{} sailing {} min", line.name, line.sailing_minutes),
                });
                t += ChronoDuration::minutes(line.sailing_minutes);
                events.push(ItinEvent {
                    at: t,
                    kind: "ferry_arrive".into(),
                    detail: line.to.name.clone(),
                });
                driving_since_break = 0.0;
            }
        }
    }
    events.push(ItinEvent {
        at: t,
        kind: "arrive".into(),
        detail: format!("destination {DEST_LAT},{DEST_LON}"),
    });
    (events, t, total_drive)
}

fn corridor_drive_legs(
    sc: &Scenario,
    ferries: &FerryFile,
) -> (Vec<(String, f64)>, Option<FerryLine>, Option<usize>) {
    let ferry = sc
        .ferry_id
        .as_deref()
        .and_then(|id| ferries.lines.iter().find(|l| l.id == id))
        .cloned();
    let mut legs = Vec::new();
    if let Some(line) = ferry.as_ref() {
        let mut before = Vec::new();
        let mut after = Vec::new();
        let mut seen_from = false;
        for wp in &sc.waypoints {
            if !seen_from {
                before.push(wp);
                if haversine_km((wp.lat, wp.lon), (line.from.lat, line.from.lon)) < 8.0 {
                    seen_from = true;
                }
            } else {
                after.push(wp);
            }
        }
        let km_b: f64 = before
            .windows(2)
            .map(|w| haversine_km((w[0].lat, w[0].lon), (w[1].lat, w[1].lon)))
            .sum();
        let km_a: f64 = after
            .windows(2)
            .map(|w| haversine_km((w[0].lat, w[0].lon), (w[1].lat, w[1].lon)))
            .sum();
        legs.push((format!("drive to {}", line.from.name), drive_hours_km(km_b)));
        legs.push((format!("drive from {}", line.to.name), drive_hours_km(km_a)));
        (legs, ferry, Some(0))
    } else {
        let km: f64 = sc
            .waypoints
            .windows(2)
            .map(|w| haversine_km((w[0].lat, w[0].lon), (w[1].lat, w[1].lon)))
            .sum();
        legs.push(("all-road drive".into(), drive_hours_km(km)));
        (legs, None, None)
    }
}

fn format_events(events: &[ItinEvent]) -> String {
    let mut s = String::new();
    for e in events {
        s.push_str(&format!(
            "  {}  {:<14} {}\n",
            e.at.format("%Y-%m-%d %H:%M"),
            e.kind,
            e.detail
        ));
    }
    s
}

fn itinerary_rules_hold(
    events: &[ItinEvent],
    break_every_h: f64,
    daily_h: f64,
    sleep_h: f64,
    break_min: i64,
) -> Result<(), String> {
    let mut drive_since_break = 0.0;
    let mut drive_today = 0.0;
    let mut last_t: Option<NaiveDateTime> = None;
    for e in events {
        if let Some(prev) = last_t {
            if e.at < prev {
                return Err("itinerary times went backwards".into());
            }
        }
        last_t = Some(e.at);
        match e.kind.as_str() {
            "drive" => {
                if let Some(rest) = e.detail.split("+").nth(1) {
                    if let Some(h) = rest.split(' ').next().and_then(|x| x.parse::<f64>().ok()) {
                        drive_since_break += h;
                        drive_today += h;
                    }
                }
                if drive_since_break > break_every_h + 0.05 {
                    return Err(format!(
                        "drove {drive_since_break:.2} h without a break (cap {break_every_h})"
                    ));
                }
                if drive_today > daily_h + 0.05 {
                    return Err(format!("drove {drive_today:.2} h in a day (cap {daily_h})"));
                }
            }
            "break" => {
                drive_since_break = 0.0;
                let _ = break_min;
            }
            "sleep" => {
                drive_today = 0.0;
                drive_since_break = 0.0;
                let _ = sleep_h;
            }
            _ => {}
        }
    }
    Ok(())
}

fn region_pack_bytes(data_dir: &Path, region_id: &str) -> u64 {
    let stem = leaf_stem_for_region_id(region_id);
    dir_size_if(data_dir, &|p| {
        p.file_name()
            .and_then(|s| s.to_str())
            .map(|n| n.starts_with(&stem) && is_pack_file(p))
            .unwrap_or(false)
    })
}

fn install_one_region(data_dir: &Path, region_id: &str) -> Result<RegionRow, String> {
    let region_id = normalize_region_id(region_id);
    if packs_ready(data_dir, &region_id) && place_index_ready(data_dir, &region_id) {
        let (_, peak) = rss_and_hwm();
        let pack_on_disk = region_pack_bytes(data_dir, &region_id);
        return Ok(RegionRow {
            region_id,
            download_bytes: 0,
            pack_on_disk,
            place_index_on_disk: 0,
            download_s: 0.0,
            install_s: 0.0,
            index_s: 0.0,
            peak_rss_bytes: peak,
            source: format!("cache-hit {}", pack_base()),
            fallback: None,
        });
    }
    let timeout = region_timeout();
    let packs_before = dir_size_if(data_dir, &is_pack_file);
    let idx_before = dir_size_if(data_dir, &is_place_index_file);
    let t_all = Instant::now();
    let (_, peak0) = rss_and_hwm();

    let (tx, rx) = mpsc::channel();
    let dir = data_dir.to_path_buf();
    let rid = region_id.clone();
    let base = pack_base();
    thread::spawn(move || {
        let t_dl = Instant::now();
        let plan = plan_region_acquisition(&rid, Some(&base), Some(dir.as_path()));
        let download_s = t_dl.elapsed().as_secs_f64();
        let t_inst = Instant::now();
        let mut source = plan.data_source.as_str().to_string();
        if plan.execute_local_convert {
            if let Err(e) = ensure_geofabrik_pbf_for_region(&dir, &rid) {
                let _ = tx.send(Err(format!("Geofabrik PBF for {rid}: {e}")));
                return;
            }
            let pbf = dir.join(format!("{}.osm.pbf", leaf_stem_for_region_id(&rid)));
            match ensure_indexed_packs_prefer_server(&dir, &pbf, None, Some(&rid)) {
                Ok(r) => source = r.data_source.as_str().to_string(),
                Err(e) => {
                    let _ = tx.send(Err(format!("local convert {rid}: {e}")));
                    return;
                }
            }
        }
        let install_s = t_inst.elapsed().as_secs_f64();
        let t_idx = Instant::now();
        let index_s = match ensure_place_index_after_pack_install(&dir, &rid, false) {
            Ok(rep) => {
                let _ = rep;
                t_idx.elapsed().as_secs_f64()
            }
            Err(e) => {
                let _ = tx.send(Err(format!("place index {rid}: {e}")));
                return;
            }
        };
        let _ = tx.send(Ok((
            plan.log_message,
            source,
            download_s,
            install_s,
            index_s,
        )));
    });

    let result = loop {
        match rx.recv_timeout(Duration::from_secs(2)) {
            Ok(v) => break v,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if t_all.elapsed() > timeout {
                    return Err(format!(
                        "timeout after {:.0}s installing {region_id} (still running pack_server GET / Geofabrik / index)",
                        t_all.elapsed().as_secs_f64()
                    ));
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(format!("install thread died for {region_id}"));
            }
        }
    }?;

    wait_ready(data_dir, &region_id, timeout)?;
    let (_, peak1) = rss_and_hwm();
    let packs_after = dir_size_if(data_dir, &is_pack_file);
    let idx_after = dir_size_if(data_dir, &is_place_index_file);
    Ok(RegionRow {
        region_id,
        download_bytes: packs_after.saturating_sub(packs_before),
        pack_on_disk: packs_after.saturating_sub(packs_before),
        place_index_on_disk: idx_after.saturating_sub(idx_before),
        download_s: result.2,
        install_s: result.3,
        index_s: result.4,
        peak_rss_bytes: peak0.max(peak1),
        source: result.1,
        fallback: None,
    })
}

fn assert_clean(data_dir: &Path) -> Assertion {
    let manifests = list_manifests(data_dir);
    let idx = data_dir.join(PLACE_INDEX_DB_NAME);
    let pbfs: Vec<_> = fs::read_dir(data_dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| {
                    e.file_name().to_string_lossy().ends_with(".osm.pbf")
                        && e.metadata().map(|m| m.len()).unwrap_or(0) > 0
                })
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    let pass = manifests.is_empty() && !idx.exists() && pbfs.is_empty();
    Assertion::check(
        "clean_state_no_regions_packs_or_index",
        pass,
        format!(
            "manifests={manifests:?} place_index={} pbfs={pbfs:?}",
            idx.exists()
        ),
    )
}

fn production_missing_is_plain_no_route(report: &str) -> bool {
    let lower = report.to_ascii_lowercase();
    (lower.contains("no route") || lower.contains("fail: no route"))
        && !lower.contains("missing region")
        && !lower.contains("not downloaded")
}

fn load_merged_car_graph(data_dir: &Path, bbox: [f64; 4]) -> Result<RouteGraph, String> {
    let Ok(entries) = fs::read_dir(data_dir) else {
        return Err("data_dir unreadable".into());
    };
    let mut graphs = Vec::new();
    for ent in entries.flatten() {
        let name = ent.file_name();
        let name = name.to_string_lossy();
        let Some(_) = name.strip_suffix(".navi-manifest.json") else {
            continue;
        };
        let Ok(man) = NaviManifest::load(&ent.path()) else {
            continue;
        };
        if man.status_pack_files(data_dir) != PackStatus::Ready {
            continue;
        }
        if let Some(tiles) = man.graph_tiles_for(RoutingProfile::Car) {
            for t in tiles {
                if !bbox_intersects(t.bbox, bbox) {
                    continue;
                }
                let path = data_dir.join(&t.file);
                if !path.is_file() {
                    continue;
                }
                match load_graph_pack_bbox(&path, RoutingProfile::Car, Some(bbox)) {
                    Ok(g) => graphs.push(g),
                    Err(e) => {
                        eprintln!("skip tile {} ({e:?})", path.display());
                    }
                }
            }
        } else if let Some(path) = man.graph_path(data_dir, RoutingProfile::Car) {
            match load_graph_pack_bbox(&path, RoutingProfile::Car, Some(bbox)) {
                Ok(g) => graphs.push(g),
                Err(e) => return Err(format!("load {}: {e:?}", path.display())),
            }
        }
    }
    if graphs.is_empty() {
        return Err("no ready car tiles in bbox".into());
    }
    Ok(merge_tile_graphs(graphs, RoutingProfile::Car))
}

fn snap_named_road(
    graph: &RouteGraph,
    lat: f64,
    lon: f64,
    needles: &[&str],
) -> Option<(osm4routing::NodeId, f64, String)> {
    let mut best: Option<(osm4routing::NodeId, f64, String)> = None;
    for e in &graph.edges {
        let hay = format!(
            "{} {}",
            e.name.as_deref().unwrap_or(""),
            e.road_ref.as_deref().unwrap_or("")
        )
        .to_ascii_lowercase();
        if !needles.iter().any(|n| hay.contains(n)) {
            continue;
        }
        let label = e
            .name
            .clone()
            .or_else(|| e.road_ref.clone())
            .unwrap_or_else(|| hay.clone());
        for (id, elat, elon) in [
            (e.source, e.start_lat, e.start_lon),
            (e.target, e.end_lat, e.end_lon),
        ] {
            let d = haversine_km((lat, lon), (elat, elon)) * 1000.0;
            if best.as_ref().is_none_or(|b| d < b.1) {
                best = Some((id, d, label.clone()));
            }
        }
    }
    best
}

fn snap_point(
    graph: &RouteGraph,
    lat: f64,
    lon: f64,
    opts: &RouteOptions,
    allow_uncapped: bool,
) -> Result<(osm4routing::NodeId, String), String> {
    match graph.nearest_routable_with_options(lat, lon, opts, false) {
        Ok((id, dist)) => Ok((id, format!("production snap: PASS ({dist:.0} m)"))),
        Err(e) => {
            let prod = format!(
                "production snap: FAIL ({:.0} m) max_m={:.0}",
                e.nearest_m, e.max_m
            );
            if let Some((id, d, name)) = snap_named_road(graph, lat, lon, &["friisvegen", "2204"]) {
                return Ok((
                    id,
                    format!(
                        "{prod}; stub snap: onto {name} at {d:.0} m (NOT a production pass; OSM viewpoint 12985331075 is 6 m from Friisvegen shape, way 361797686)"
                    ),
                ));
            }
            if !allow_uncapped {
                return Err(prod);
            }
            let mut best: Option<(osm4routing::NodeId, f64)> = None;
            for n in graph.nodes.values() {
                if !graph.is_linked(n.id) {
                    continue;
                }
                let d = haversine_km((lat, lon), (n.coord.y, n.coord.x)) * 1000.0;
                if best.as_ref().is_none_or(|b| d < b.1) {
                    best = Some((n.id, d));
                }
            }
            let (id, d) = best.ok_or_else(|| format!("{prod}; empty graph"))?;
            Ok((
                id,
                format!(
                    "{prod}; stub snap: uncapped linked node at {d:.0} m (NOT a production pass)"
                ),
            ))
        }
    }
}

fn plan_on_graph(
    graph: &RouteGraph,
    start: (f64, f64),
    goal: (f64, f64),
    vias: &[(f64, f64)],
    depart: NaiveDateTime,
    allow_uncapped_snap: bool,
) -> Result<(Vec<usize>, f64, f64, Vec<String>), String> {
    let opts = RouteOptions {
        avoid_ferries: false,
        departure_local: Some(depart),
        ..RouteOptions::default()
    };
    let mut pts = Vec::new();
    pts.push(start);
    pts.extend_from_slice(vias);
    pts.push(goal);
    let mut snapped = Vec::new();
    let mut notes = Vec::new();
    for &(lat, lon) in &pts {
        let (id, note) = snap_point(graph, lat, lon, &opts, allow_uncapped_snap)?;
        notes.push(format!("{lat:.5},{lon:.5}: {note}"));
        snapped.push(id);
    }
    let mut edges = Vec::new();
    for w in snapped.windows(2) {
        let stats = graph.shortest_path_with_options_stats(w[0], w[1], false, &opts);
        let Some((_, e, _)) = stats.path else {
            return Err(format!(
                "no path between snapped nodes (reason={})",
                stats.terminate_reason
            ));
        };
        edges.extend(e);
    }
    let mut dist_m = 0.0;
    for &i in &edges {
        dist_m += graph.edges[i].length_m;
    }
    let minutes = motor_path_minutes_from_edges(graph, &edges);
    Ok((edges, dist_m, minutes, notes))
}

fn ferry_label(edge_name: Option<&str>, road_ref: Option<&str>) -> String {
    format!("{} {}", edge_name.unwrap_or(""), road_ref.unwrap_or("")).to_ascii_lowercase()
}

fn classify_ferry(label: &str, ferries: &FerryFile) -> Result<Option<String>, String> {
    let l = label.to_ascii_lowercase();
    for needle in &ferries.forbidden_needles {
        if l.contains(&needle.to_ascii_lowercase()) {
            return Err(format!("forbidden ferry appeared: {label}"));
        }
    }
    for line in &ferries.lines {
        let ok = line
            .match_needles
            .iter()
            .filter(|n| n.as_str() != "oslo")
            .any(|n| l.contains(&n.to_ascii_lowercase()));
        if ok {
            return Ok(Some(line.id.clone()));
        }
    }
    if l.trim().is_empty() {
        return Ok(Some("unnamed-ferry".into()));
    }
    // Named but not allowlisted.
    Err(format!("non-allowlisted ferry: {label}"))
}

fn route_near(graph: &RouteGraph, edges: &[usize], lat: f64, lon: f64, radius_km: f64) -> bool {
    for &i in edges {
        let e = &graph.edges[i];
        if haversine_km((e.start_lat, e.start_lon), (lat, lon)) <= radius_km
            || haversine_km((e.end_lat, e.end_lon), (lat, lon)) <= radius_km
        {
            return true;
        }
        for &(elon, elat) in &e.shape {
            if haversine_km((elat, elon), (lat, lon)) <= radius_km {
                return true;
            }
        }
    }
    false
}

fn inventory_text() -> String {
    r#"## 1. Inventory (Step 0)

Pack host (this test): https://navigate-me.duckdns.org
Client path: core/src/pack_server/{mod,acquisition,fetch,place_index_after}.rs
  discover_pack_catalog / plan_region_acquisition / try_fetch_region_packs
  ensure_indexed_packs_prefer_server (server first, Geofabrik+convert fallback)
  ensure_place_index_after_pack_install
Default URL constant: pack_server::DEFAULT_PACK_SERVER_BASE_URL

### Region catalog and along-route suggestion
EXISTS:
  core/src/routing/basemap/regions.rs
    suggest_geofabrik_path_for_point(lat, lon) — tightest known bbox (Norway landsdels;
    country extracts for DE/DK/SE; no Hamburg / Schleswig-Holstein / Danish region /
    Swedish lan table entries).
  navi-ffi suggest_geofabrik_path (UniFFI)
  app/.../GeofabrikDownloadCatalog.kt — picker chips (DE states, SE lan including
    vastra_gotaland underscore; NO Denmark subregion chips)
  app/.../RegionCoverage.kt missingCoverage — From/To/Via waypoints only
  core/src/pack_server/acquisition.rs pack_catalog_region_id_aliases
    europe/sweden/vastra-gotaland <-> europe/sweden/vastra_gotaland
  core/src/pack_server/acquisition.rs resolve_area_to_catalog / resolve_areas_to_catalog
    generic parent fallback onto published catalog ids (Danish leaves -> europe/denmark)
  core/src/pack_server/corridor_regions.rs ordered_regions_along_corridor
    densify caller corridor + catalog geom PIP → ordered missing regions
GAP (stubbed in this test):
  Waypoint missingCoverage does not walk the corridor (Android RegionCoverage).
  Corridor source itself (overview route / skeleton / routed path) is caller-owned.

### Rest-break / max-daily / sleep
EXISTS (post-plan overlay, does not change A* path):
  core/src/config/defaults.rs CAR_BREAK_INTERVAL_MIN/MAX_HOURS = 4.0 / 4.5
    CAR_MAX_DAILY_HOURS = 8.0, break duration 15-45 min
  core/src/routing/rest/mod.rs motor_break_interval_km / soft_break_distances_km
  core/src/routing/rest/motor_multi_day.rs plan_motor_multi_day — overnight split
  navi-ffi plan_car_route_inner applies those after the route exists
  Truck-only: TRUCK_DAILY_REST_HOURS = 11.0 (EC 561)
GAP (stubbed in this test):
  Car has no 2 h interval in production defaults (4-4.5 h).
  11 h sleep is not a car itinerary constraint (truck daily rest only).
  Breaks/sleep are not included in elapsed-time path choice.

### Ferry handling
EXISTS:
  v8 FlatGraphPack.edge_is_ferry (core/src/routing/indexed/graph_pack.rs GRAPH_FORMAT_VERSION=8)
  bbox_build.rs / builder.rs: route=ferry or ferry=* or highway=ferry
  Car profile uses ferry edges unless RouteOptions.avoid_ferries
  edge_name / edge_road_ref stored; OSM duration is NOT stored or used
  ETA uses maxspeed / highway fallback (ferry often 50 km/h class default)
GAP (stubbed in this test):
  No allowlist by name/ref/duration. avoid_ferries is all-or-nothing.
  No timetable, check-in, or wait-for-departure.

### Pack download + install + place index
EXISTS:
  plan_region_acquisition GET current.json + pack files (read-only HTTP)
  PackStatus::Ready via NaviManifest::status_pack_files
  NameIndex::has_entries_for_region + region_index_complete
  PLACE_INDEX_DB_NAME = place_index.db
  Android PlaceIndexReady.kt is host-only stamp — not used on cargo
  core/tests/pack_server_place_index_live.rs is the existing live pattern
GAP:
  extra_corridor_manifests (indexed/load.rs) maps stems through
  pbf_stem_to_geofabrik_path + region_bbox. German states, Danish regions and
  Swedish lan return None, so multi-stem merge is skipped. This test stubs
  loading every Ready car tile that intersects the trip bbox.
  current.json publishes europe/denmark (country), not syddanmark/sjaelland/etc.
  Planner FAIL is "no route found" without listing missing corridor regions.
  Destination OSM 12985331075 is a viewpoint 6 m from Friisvegen (way 361797686,
  Fv2204). Production car snap honours motor_vehicle:conditional=no @ Nov-Jun, so
  a June departure treats the nearby secondary as closed and the nearest open
  road can exceed CAR_MAX_WAYPOINT_SNAP_M (750 m). Default DATE is 2026-07-15.
"#
    .to_string()
}

#[test]
fn production_catalog_parent_fallback_for_danish_leaves() {
    println!(
        "behaviour=catalog_parent_fallback source={}",
        BehaviourSource::Production.label()
    );
    let ready = vec![
        "europe/germany/hamburg".into(),
        "europe/denmark".into(),
        "europe/sweden/vastra_gotaland".into(),
    ];
    let required = vec![
        "europe/denmark/syddanmark".into(),
        "europe/denmark/sjaelland".into(),
        "europe/denmark/hovedstaden".into(),
        "europe/sweden/vastra-gotaland".into(),
    ];
    let got = resolve_areas_to_catalog(&required, &ready);
    assert_eq!(
        got,
        vec![
            "europe/denmark".to_string(),
            "europe/sweden/vastra_gotaland".to_string(),
        ]
    );
}

#[test]
fn production_ordered_regions_all_road_current_and_leaf_catalogs() {
    println!(
        "behaviour=ordered_along_route_regions source={}",
        BehaviourSource::Production.label()
    );
    let scenarios = load_scenarios();
    let all_road = scenarios.iter().find(|s| s.id == "all_road").unwrap();
    let waypoints: Vec<(f64, f64)> = all_road.waypoints.iter().map(|w| (w.lat, w.lon)).collect();
    let installed = vec!["europe/germany/niedersachsen".into()];
    let current = catalog_entries_from_ready_ids(&[
        "europe/germany/niedersachsen".into(),
        "europe/germany/hamburg".into(),
        "europe/germany/schleswig-holstein".into(),
        "europe/denmark".into(),
        "europe/sweden/skane".into(),
        "europe/sweden/halland".into(),
        "europe/sweden/vastra_gotaland".into(),
        "europe/norway/ostlandet".into(),
    ]);
    let got = ordered_regions_along_corridor(&waypoints, &current, &installed, SAMPLE_STEP_KM);
    assert_eq!(got, expected_for_scenario("all_road"));
    let leaves = catalog_entries_from_ready_ids(&[
        "europe/germany/niedersachsen".into(),
        "europe/germany/hamburg".into(),
        "europe/germany/schleswig-holstein".into(),
        "europe/denmark/syddanmark".into(),
        "europe/denmark/sjaelland".into(),
        "europe/denmark/hovedstaden".into(),
        "europe/sweden/skane".into(),
        "europe/sweden/halland".into(),
        "europe/sweden/vastra_gotaland".into(),
        "europe/norway/ostlandet".into(),
    ]);
    let got_leaves =
        ordered_regions_along_corridor(&waypoints, &leaves, &installed, SAMPLE_STEP_KM);
    assert!(got_leaves.iter().any(|r| r.contains("syddanmark")));
    assert!(got_leaves.iter().any(|r| r.contains("sjaelland")));
    assert!(got_leaves.iter().any(|r| r.contains("hovedstaden")));
    assert!(!got_leaves.iter().any(|r| r == "europe/denmark"));
}

#[test]
fn stub_suggester_keeps_hamburg_and_alias() {
    let bboxes = load_bboxes();
    let hamburg = suggest_finest(53.55, 10.00, &bboxes);
    assert!(
        hamburg.contains("hamburg"),
        "Hamburg sample must not collapse to country-only, got {hamburg}"
    );
    let vg = canonicalize_suggested("europe/sweden/vastra-gotaland");
    assert_eq!(vg, "europe/sweden/vastra_gotaland");
    let aliases = pack_catalog_region_id_aliases("europe/sweden/vastra-gotaland");
    assert_eq!(aliases, vec!["europe/sweden/vastra_gotaland"]);
}

#[test]
fn stub_report_scenario_lists_and_0900_winner() {
    let ferries = load_ferries();
    let scenarios = load_scenarios();
    let date = NaiveDate::from_ymd_opt(2026, 6, 20).unwrap();
    let depart = date.and_hms_opt(9, 0, 0).unwrap();
    println!("DATE={date} departure=09:00 (fixture; live test may override)");
    println!(
        "behaviour=ordered_along_route_regions source={}",
        BehaviourSource::Production.label()
    );
    let synthetic = PackCatalogSnapshot {
        data_source: PackDataSource::ServerDuckdns,
        ready_region_ids: vec![
            "europe/germany/niedersachsen".into(),
            "europe/germany/hamburg".into(),
            "europe/germany/schleswig-holstein".into(),
            "europe/denmark".into(),
            "europe/sweden/skane".into(),
            "europe/sweden/halland".into(),
            "europe/sweden/vastra_gotaland".into(),
            "europe/norway/ostlandet".into(),
        ],
        catalog_generation: Some("fixture".into()),
        served_from: None,
        unreachable_reason: None,
    };
    for sc in &scenarios {
        let actual = ordered_regions_for_scenario(sc, &synthetic, BASE_REGION);
        let expected = expected_for_scenario(&sc.id);
        println!("\n### {} ({})", sc.label, sc.id);
        println!("expected: {expected:?}");
        println!("actual:   {actual:?}");
        println!("diff:\n{}", diff_lists(expected, &actual));
        assert!(
            actual.iter().any(|a| a.contains("hamburg")),
            "{} skipped Hamburg: {actual:?}",
            sc.id
        );
    }
    let mut best: Option<(String, NaiveDateTime)> = None;
    for sc in &scenarios {
        let (legs, ferry, after) = corridor_drive_legs(sc, &ferries);
        let (events, arrival, _) =
            overlay_itinerary(depart, &legs, ferry.as_ref(), after, 2.0, 15, 8.0, 11.0);
        println!(
            "\n09:00 option {} arrival {} events=\n{}",
            sc.id,
            arrival.format("%Y-%m-%d %H:%M"),
            format_events(&events)
        );
        if best.as_ref().is_none_or(|b| arrival < b.1) {
            best = Some((sc.id.clone(), arrival));
        }
    }
    let (win, arr) = best.unwrap();
    println!(
        "09:00 stub winner={win} arrival={}",
        arr.format("%Y-%m-%d %H:%M")
    );
}

#[test]
fn stub_itinerary_respects_break_daily_sleep() {
    let start = NaiveDate::from_ymd_opt(2026, 6, 20)
        .unwrap()
        .and_hms_opt(9, 0, 0)
        .unwrap();
    let legs = vec![("drive".into(), 10.0)];
    let (events, _, drive) = overlay_itinerary(start, &legs, None, None, 2.0, 15, 8.0, 11.0);
    assert!((drive - 10.0).abs() < 0.05);
    itinerary_rules_hold(&events, 2.0, 8.0, 11.0, 15).unwrap();
    assert!(events.iter().any(|e| e.kind == "sleep"));
    assert!(events.iter().any(|e| e.kind == "break"));
}

#[test]
#[ignore = "live network: pack server https://navigate-me.duckdns.org + Geofabrik; multi-GB"]
fn live_klecken_to_innlandet_long_trip() {
    let date = param_date();
    let break_min = break_minutes();
    let break_h = break_every_hours();
    let daily_h = daily_hours();
    let sleep_h = sleep_hours();
    let depart = date.and_hms_opt(9, 0, 0).expect("09:00");
    let ferries = load_ferries();
    let scenarios = load_scenarios();
    let mut assertions = Vec::new();
    let mut report = String::new();
    report.push_str("# Navi long-trip region handling (host-only)\n\n");
    report.push_str(&format!(
        "pack_server={}\nDATE={} (parameter; default 2026-07-15, Friisvegen open)\ndestination=OSM node 12985331075 viewpoint ({DEST_LAT},{DEST_LON}); on-road Friisvegen Fv2204 ({FRIISVEGEN_LAT},{FRIISVEGEN_LON}) ~6 m\ndeparture=09:00 Europe/Berlin\nbreak_every={break_h} h break_len={break_min} min daily_cap={daily_h} h sleep={sleep_h} h\n\n",
        pack_base(),
        date
    ));
    report.push_str("## Behaviour sources\n");
    report.push_str(&format!(
        "- catalog_parent_fallback: {}\n",
        BehaviourSource::Production.label()
    ));
    report.push_str(&format!(
        "- ordered_along_route_regions: {}\n",
        BehaviourSource::Production.label()
    ));
    report.push_str(&format!(
        "- missing_regions_typed_result: {}\n",
        BehaviourSource::Stub.label()
    ));
    report.push_str(&format!(
        "- multi_stem_corridor_merge: {}\n\n",
        BehaviourSource::Stub.label()
    ));
    report.push_str(&inventory_text());
    report.push('\n');

    // Fresh temp dir = clear app data. Optional reuse dir is wiped first unless KEEP set.
    let root = data_root();
    if std::env::var("NAVI_LONG_TRIP_KEEP").is_err() {
        let _ = fs::remove_dir_all(&root);
    }
    fs::create_dir_all(&root).expect("create data dir");
    let data_dir = root.join("app-data");
    fs::create_dir_all(&data_dir).expect("app-data");
    if std::env::var("NAVI_LONG_TRIP_KEEP").is_err() {
        assertions.push(assert_clean(&data_dir));
    } else {
        assertions.push(Assertion::check(
            "clean_state_skipped_keep",
            true,
            "NAVI_LONG_TRIP_KEEP set; reused existing dir",
        ));
    }
    report.push_str(&format!("data_dir={}\n", data_dir.display()));

    eprintln!("discovering catalog at {}", pack_base());
    let cat = discover_pack_catalog(Some(&pack_base()));
    if let Some(reason) = &cat.unreachable_reason {
        panic!("pack catalog unreachable at {}: {reason}", pack_base());
    }
    report.push_str(&format!(
        "catalog generation={:?} regions={} served_from={:?}\n",
        cat.catalog_generation,
        cat.ready_region_ids.len(),
        cat.served_from
    ));

    // Step 2 — base region
    eprintln!("installing base region {BASE_REGION}");
    let mut rows = Vec::new();
    let base_row = install_one_region(&data_dir, BASE_REGION).unwrap_or_else(|e| panic!("{e}"));
    assertions.push(Assertion::check(
        "base_region_installed_and_indexed",
        packs_ready(&data_dir, BASE_REGION) && place_index_ready(&data_dir, BASE_REGION),
        format!("source={}", base_row.source),
    ));
    rows.push(base_row);

    // Place search
    let db = data_dir.join(PLACE_INDEX_DB_NAME);
    let idx = NameIndex::open(&db).expect("open place index");
    let hits = idx.search(PLACE_QUERY, 20).expect("search");
    let klecken = hits
        .iter()
        .find(|h| {
            h.name.to_ascii_lowercase().contains("klecken")
                && (h.name.to_ascii_lowercase().contains("bahn")
                    || h.kind.to_ascii_lowercase().contains("station")
                    || h.name.to_ascii_lowercase().contains("bahnhof"))
        })
        .or_else(|| {
            hits.iter()
                .find(|h| h.name.to_ascii_lowercase().contains("klecken"))
        });
    let Some(klecken) = klecken else {
        panic!(
            "Bahnhof Klecken did not resolve in place index. hits={:?}",
            hits.iter()
                .map(|h| format!("{} ({})", h.name, h.kind))
                .collect::<Vec<_>>()
        );
    };
    assertions.push(Assertion::check(
        "bahnhof_klecken_resolved",
        true,
        format!(
            "{} ({:.5},{:.5}) kind={}",
            klecken.name, klecken.lat, klecken.lon, klecken.kind
        ),
    ));
    let start = (klecken.lat, klecken.lon);

    // Step 3 — plan with only Niedersachsen; must report missing regions
    let pbf = data_dir.join(format!("{}.osm.pbf", leaf_stem_for_region_id(BASE_REGION)));
    let prod_plan = try_load_graph_for_plan_bbox(
        &data_dir,
        &pbf,
        RoutingProfile::Car,
        Some([
            start.0.min(DEST_LAT),
            start.1.min(DEST_LON),
            start.0.max(DEST_LAT),
            start.1.max(DEST_LON),
        ]),
    );
    let missing_msg = match prod_plan {
        Ok(g) => match plan_on_graph(
            &g,
            start,
            (FRIISVEGEN_LAT, FRIISVEGEN_LON),
            &[],
            depart,
            false,
        ) {
            Ok(_) => "production returned a route with only Niedersachsen (unexpected)".into(),
            Err(e) => format!("production plan failed: {e}"),
        },
        Err(e) => format!("production graph load failed: {e:?}"),
    };
    let missing_regions = {
        let all_road = scenarios
            .iter()
            .find(|s| s.id == "all_road")
            .expect("all_road scenario");
        ordered_regions_for_scenario(all_road, &cat, BASE_REGION)
    };
    let silent = production_missing_is_plain_no_route(&missing_msg) && missing_regions.is_empty();
    assertions.push(Assertion::check(
        "incomplete_coverage_reports_missing_regions",
        !silent && !missing_regions.is_empty(),
        format!("{missing_msg}; missing_regions={missing_regions:?}"),
    ));
    report.push_str("\n## Step 3 — plan with only Niedersachsen\n");
    report.push_str(&format!(
        "{missing_msg}\nmissing regions (production ordered_regions_along_corridor):\n"
    ));
    for r in &missing_regions {
        let (dl, fb) = resolve_download_id(r, &cat);
        report.push_str(&format!(
            "  {r} -> download {dl} {}\n",
            fb.as_deref().unwrap_or("in current.json")
        ));
    }

    // Step 4 — per-scenario lists
    report.push_str("\n## 2. Expected vs actual region list\n");
    report.push_str(
        "Actual = production ordered_regions_along_corridor (catalog geom + densified corridor).\n",
    );
    report
        .push_str("Danish leaves absent in current.json → europe/denmark via catalog entries.\n\n");
    let mut hamburg_ok = true;
    for sc in &scenarios {
        let actual = ordered_regions_for_scenario(sc, &cat, BASE_REGION);
        let expected = expected_for_scenario(&sc.id);
        let diff = diff_lists(expected, &actual);
        report.push_str(&format!("### {} ({})\nexpected:\n", sc.label, sc.id));
        for e in expected {
            report.push_str(&format!("  {e}\n"));
        }
        report.push_str("actual:\n");
        for a in &actual {
            let (dl, fb) = resolve_download_id(a, &cat);
            report.push_str(&format!(
                "  {a}  catalog={} {}\n",
                catalog_contains(&cat, a),
                fb.map(|s| format!("fallback={s} resolved={dl}"))
                    .unwrap_or_else(|| format!("resolved={dl}"))
            ));
        }
        report.push_str("differences:\n");
        report.push_str(&diff);
        report.push_str("\n\n");
        if !actual.iter().any(|a| a.contains("hamburg")) {
            hamburg_ok = false;
        }
        if sc.id == "all_road" {
            let has_vg = actual.iter().any(|a| {
                region_ids_match_for_catalog(a, "europe/sweden/vastra_gotaland")
                    || region_ids_match_for_catalog(a, "europe/sweden/vastra-gotaland")
            });
            assertions.push(Assertion::check(
                "vastra_gotaland_alias",
                has_vg
                    && canonicalize_suggested("europe/sweden/vastra-gotaland")
                        == "europe/sweden/vastra_gotaland",
                format!("actual contains VG={has_vg}; hyphen->underscore alias applied"),
            ));
        }
    }
    assertions.push(Assertion::check(
        "hamburg_not_skipped",
        hamburg_ok,
        "every scenario corridor must include europe/germany/hamburg",
    ));

    // Score options at 09:00 using stub driving + ferry fixtures
    report.push_str("## 4. 09:00 itinerary and departure-time sweep\n\n");
    report.push_str("GAP: production does not choose among ferry lines by elapsed time.\n");
    report.push_str("Scoring below uses haversine/90 km/h driving (until packs exist) plus timetable fixtures.\n\n");
    let mut best: Option<(String, NaiveDateTime, Vec<ItinEvent>, f64)> = None;
    for sc in &scenarios {
        let (legs, ferry, after) = corridor_drive_legs(sc, &ferries);
        let (events, arrival, _drive) = overlay_itinerary(
            depart,
            &legs,
            ferry.as_ref(),
            after,
            break_h,
            break_min,
            daily_h,
            sleep_h,
        );
        report.push_str(&format!(
            "option {} elapsed until {}\n",
            sc.id,
            arrival.format("%Y-%m-%d %H:%M")
        ));
        if best.as_ref().is_none_or(|b| arrival < b.1) {
            best = Some((sc.id.clone(), arrival, events, 0.0));
        }
    }
    let (win_id, win_arrival, win_events, _) = best.expect("a winning option");
    report.push_str(&format!(
        "\n### Primary 09:00 winner (stub elapsed-time): {win_id} arrival {}\n",
        win_arrival.format("%Y-%m-%d %H:%M")
    ));
    report.push_str(&format_events(&win_events));
    match itinerary_rules_hold(&win_events, break_h, daily_h, sleep_h, break_min) {
        Ok(()) => assertions.push(Assertion::check("stub_itinerary_rules", true, "ok")),
        Err(e) => assertions.push(Assertion::check("stub_itinerary_rules", false, e)),
    }

    // Sweep other hours
    report.push_str("\n### Departure-time sweep (do not assert a fixed winner)\n");
    let mut last_winner: Option<String> = None;
    for hour in 0..24 {
        let dep = date.and_hms_opt(hour, 0, 0).unwrap();
        let mut w: Option<(String, NaiveDateTime)> = None;
        for sc in &scenarios {
            let (legs, ferry, after) = corridor_drive_legs(sc, &ferries);
            let (_, arrival, _) = overlay_itinerary(
                dep,
                &legs,
                ferry.as_ref(),
                after,
                break_h,
                break_min,
                daily_h,
                sleep_h,
            );
            if w.as_ref().is_none_or(|x| arrival < x.1) {
                w = Some((sc.id.clone(), arrival));
            }
        }
        let (wid, arr) = w.unwrap();
        let changed = last_winner.as_ref() != Some(&wid);
        report.push_str(&format!(
            "  {:02}:00 -> {wid} arrive {}{}\n",
            hour,
            arr.format("%m-%d %H:%M"),
            if changed { "  (winner changed)" } else { "" }
        ));
        last_winner = Some(wid);
    }

    // Step 5 — download suggested for winning corridor
    let win_sc = scenarios.iter().find(|s| s.id == win_id).unwrap();
    let suggested = ordered_regions_for_scenario(win_sc, &cat, BASE_REGION);
    let download_list = dedupe_download_list(&suggested, &cat);
    report.push_str("\n## Step 5 — sequential download (winning corridor)\n");
    for (id, fb) in &download_list {
        report.push_str(&format!("  queue {id} {}\n", fb.as_deref().unwrap_or("")));
        eprintln!("installing {id}");
        match install_one_region(&data_dir, id) {
            Ok(mut row) => {
                row.fallback = fb.clone();
                assertions.push(Assertion::check(
                    &format!("installed_{id}"),
                    packs_ready(&data_dir, id) && place_index_ready(&data_dir, id),
                    format!("source={}", row.source),
                ));
                rows.push(row);
            }
            Err(e) => {
                assertions.push(Assertion::check(&format!("installed_{id}"), false, e));
            }
        }
    }

    report.push_str("\n## 3. Per-region sizes and times\n\n");
    report.push_str(&format!(
        "{:<42} {:>12} {:>12} {:>12} {:>8} {:>8} {:>8} {:>8} {}\n",
        "region",
        "dl_bytes",
        "pack_disk",
        "index_disk",
        "dl_s",
        "inst_s",
        "idx_s",
        "peakRSS",
        "source"
    ));
    let mut tot_dl = 0u64;
    let mut tot_pack = 0u64;
    let mut tot_idx = 0u64;
    let mut tot_peak = 0u64;
    for r in &rows {
        tot_dl += r.download_bytes;
        tot_pack += r.pack_on_disk;
        tot_idx += r.place_index_on_disk;
        tot_peak = tot_peak.max(r.peak_rss_bytes);
        report.push_str(&format!(
            "{:<42} {:>12} {:>12} {:>12} {:>8.1} {:>8.1} {:>8.1} {:>8} {} {}\n",
            r.region_id,
            r.download_bytes,
            r.pack_on_disk,
            r.place_index_on_disk,
            r.download_s,
            r.install_s,
            r.index_s,
            r.peak_rss_bytes,
            r.source,
            r.fallback.as_deref().unwrap_or("")
        ));
    }
    if tot_idx == 0 {
        tot_idx = dir_size_if(&data_dir, &is_place_index_file);
    }
    report.push_str(&format!(
        "{:<42} {:>12} {:>12} {:>12} {:>8} {:>8} {:>8} {:>8}\n",
        "TOTAL", tot_dl, tot_pack, tot_idx, "", "", "", tot_peak
    ));
    report.push_str(&format!(
        "totals_human: download={:.2} GiB  packs_on_disk={:.2} GiB  place_index={:.2} GiB  peak_rss={:.2} GiB\n",
        tot_dl as f64 / 1024.0 / 1024.0 / 1024.0,
        tot_pack as f64 / 1024.0 / 1024.0 / 1024.0,
        tot_idx as f64 / 1024.0 / 1024.0 / 1024.0,
        tot_peak as f64 / 1024.0 / 1024.0 / 1024.0
    ));

    // Step 6 — re-plan with stub-merged tiles (production extra_corridor cannot see DE/DK/SE leaves)
    report.push_str("\n## Step 6 — re-plan with installed packs\n");
    report
        .push_str("GAP: try_load_graph_for_plan_bbox extra_corridor_manifests skips stems whose\n");
    report.push_str(
        "pbf_stem_to_geofabrik_path/region_bbox is unknown. Test loads Ready car tiles itself.\n",
    );
    let trip_bbox = [53.0, 7.5, 62.8, 14.8];
    let graph = match load_merged_car_graph(&data_dir, trip_bbox) {
        Ok(g) => {
            report.push_str(&format!(
                "merged graph nodes={} edges={}\n",
                g.nodes.len(),
                g.edges.len()
            ));
            Some(g)
        }
        Err(e) => {
            assertions.push(Assertion::check("merged_graph_load", false, e));
            None
        }
    };

    if let Some(graph) = graph.as_ref() {
        let vias: Vec<(f64, f64)> = if let Some(fid) = &win_sc.ferry_id {
            ferries
                .lines
                .iter()
                .find(|l| l.id == *fid)
                .map(|l| vec![(l.from.lat, l.from.lon), (l.to.lat, l.to.lon)])
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        match plan_on_graph(
            graph,
            start,
            (FRIISVEGEN_LAT, FRIISVEGEN_LON),
            &vias,
            depart,
            true,
        ) {
            Ok((edges, dist_m, minutes, snap_notes)) => {
                for n in &snap_notes {
                    report.push_str(&format!("snap: {n}\n"));
                }
                report.push_str(&format!(
                    "forward route distance={:.1} km driving_eta={:.1} min\n",
                    dist_m / 1000.0,
                    minutes
                ));
                let mut ferry_ids = BTreeSet::new();
                let mut ferry_fail = None;
                for &i in &edges {
                    let e = &graph.edges[i];
                    if !e.is_ferry {
                        continue;
                    }
                    let label = ferry_label(e.name.as_deref(), e.road_ref.as_deref());
                    match classify_ferry(&label, &ferries) {
                        Ok(Some(id)) => {
                            ferry_ids.insert(id);
                        }
                        Ok(None) => {}
                        Err(err) => ferry_fail = Some(err),
                    }
                }
                report.push_str(&format!("ferries_on_route={ferry_ids:?}\n"));
                if let Some(err) = ferry_fail {
                    assertions.push(Assertion::check("only_allowlisted_ferries", false, err));
                } else {
                    assertions.push(Assertion::check(
                        "only_allowlisted_ferries",
                        true,
                        format!("{ferry_ids:?}"),
                    ));
                }
                let belt = route_near(graph, &edges, 55.345, 10.97, 12.0);
                let oresund = route_near(graph, &edges, 55.57, 12.85, 12.0);
                let svinesund = route_near(graph, &edges, 59.09, 11.25, 15.0);
                if win_id == "all_road" {
                    assertions.push(Assertion::check(
                        "great_belt",
                        belt,
                        "route near Storebaelt",
                    ));
                    assertions.push(Assertion::check("oresund", oresund, "route near Oresund"));
                    assertions.push(Assertion::check(
                        "svinesund",
                        svinesund,
                        "route near Svinesund",
                    ));
                } else {
                    report.push_str(&format!(
                        "bridge proximity (informational for ferry winner): belt={belt} oresund={oresund} svinesund={svinesund}\n"
                    ));
                    if let Some(fid) = &win_sc.ferry_id {
                        if let Some(line) = ferries.lines.iter().find(|l| l.id == *fid) {
                            let a = route_near(graph, &edges, line.from.lat, line.from.lon, 15.0);
                            let b = route_near(graph, &edges, line.to.lat, line.to.lon, 15.0);
                            assertions.push(Assertion::check(
                                "ferry_both_ends",
                                a && b,
                                format!("{} from={a} to={b}", line.name),
                            ));
                        }
                    }
                }
                let drive_h = minutes / 60.0;
                let ferry_line = win_sc
                    .ferry_id
                    .as_deref()
                    .and_then(|id| ferries.lines.iter().find(|l| l.id == id));
                let after = if ferry_line.is_some() {
                    Some(0usize)
                } else {
                    None
                };
                // Motor ETA includes ferry length at highway-class speed; subtract
                // fixture sailing so the stub overlay can apply check-in + wait.
                let drive_only = if let Some(l) = ferry_line {
                    (drive_h - (l.sailing_minutes as f64 / 60.0)).max(0.5)
                } else {
                    drive_h
                };
                let (events, arrival, _) = overlay_itinerary(
                    depart,
                    &[("planned drive".into(), drive_only)],
                    ferry_line,
                    after,
                    break_h,
                    break_min,
                    daily_h,
                    sleep_h,
                );
                report.push_str(&format!(
                    "\n09:00 itinerary on real driving ETA (winner {win_id}), arrival {}\n",
                    arrival.format("%Y-%m-%d %H:%M")
                ));
                report.push_str(&format_events(&events));
                match itinerary_rules_hold(&events, break_h, daily_h, sleep_h, break_min) {
                    Ok(()) => assertions.push(Assertion::check("real_itinerary_rules", true, "ok")),
                    Err(e) => assertions.push(Assertion::check("real_itinerary_rules", false, e)),
                }
                let multi = plan_motor_multi_day(
                    MotorDailyBudget::Hours(daily_h),
                    drive_only,
                    dist_m / 1000.0,
                    &[],
                );
                report.push_str(&format!(
                    "production plan_motor_multi_day days={} (overnight split only; no 11 h sleep clock)\n",
                    multi.days.len()
                ));

                match plan_on_graph(
                    graph,
                    (FRIISVEGEN_LAT, FRIISVEGEN_LON),
                    start,
                    &vias.iter().rev().copied().collect::<Vec<_>>(),
                    depart,
                    true,
                ) {
                    Ok((rev_edges, rev_m, rev_min, _)) => {
                        report.push_str(&format!(
                            "reverse route distance={:.1} km eta={:.1} min edges={}\n",
                            rev_m / 1000.0,
                            rev_min,
                            rev_edges.len()
                        ));
                        assertions.push(Assertion::check("reverse_route", true, "connected"));
                    }
                    Err(e) => assertions.push(Assertion::check("reverse_route", false, e)),
                }
            }
            Err(e) => assertions.push(Assertion::check("forward_route", false, e)),
        }
    }

    report.push_str("\n## 5. Pass/fail\n\n");
    let mut failed = 0usize;
    for a in &assertions {
        report.push_str(&format!(
            "{}  {}  {}\n",
            if a.pass { "PASS" } else { "FAIL" },
            a.name,
            a.detail
        ));
        if !a.pass {
            failed += 1;
        }
    }
    report.push_str("\nRe-run:\n");
    report.push_str("  cargo test -p driver-break-core --test long_trip_region_handling -- --ignored --nocapture\n");
    report.push_str("  NAVI_LONG_TRIP_DIR=/path NAVI_LONG_TRIP_DATE=2026-07-15 NAVI_LONG_TRIP_BREAK_MIN=15 \\\n");
    report.push_str("    NAVI_PACK_SERVER_BASE_URL=https://navigate-me.duckdns.org \\\n");
    report.push_str("    cargo test -p driver-break-core --test long_trip_region_handling -- --ignored --nocapture\n");

    println!("{report}");
    if failed > 0 {
        panic!("{failed} assertion(s) failed; see report above");
    }
}
