//! Probe keyless public routers for long-trip preliminary corridors.
//!
//! **Test / fixture harness only.** Does not touch production long_trip code,
//! downloads, Kotlin bindings, or storage.
//!
//! Enable with `NAVI_ROUTER_PROBE=1` and run:
//! ```text
//! NAVI_ROUTER_PROBE=1 cargo test -p driver-break-core --test router_probe \
//!   probe_public_routers -- --ignored --nocapture
//! ```
//!
//! Politeness: ≤1 request/s, sequential, no retries, identifying User-Agent
//! (and Valhalla `X-Client-Id`). Never fabricates responses.

use driver_break_core::long_trip::{
    estimate_trip_disk_bytes, regions_bbox_adjacent, SpaceCheck, LONG_TRIP_CORRIDOR_BUFFER_KM,
};
use driver_break_core::pack_server::catalog_entries_from_ready_ids;
use driver_break_core::routing::basemap::region_bbox;
use driver_break_core::routing::elevation::country_iso_at;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const UA: &str =
    "NaviRouterProbe/0.1 (https://github.com/navigate-me/Navi; long-trip preliminary route probe)";
const CLIENT_ID: &str = "navi.long-trip.router-probe";
const VALHALLA_BASE: &str = "https://valhalla1.openstreetmap.de";
const BROUTER_BASE: &str = "https://brouter.de";
/// Car profiles confirmed present on brouter.de during discovery (bike/foot never used).
const BROUTER_CAR_PROFILES: &[&str] = &["car-eco", "car-fast"];
const BROUTER_PROFILE: &str = "car-eco";

const MIN_INTERVAL: Duration = Duration::from_millis(1100);

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/long_trip")
}

fn recorded_dir() -> PathBuf {
    let d = fixture_root().join("recorded");
    fs::create_dir_all(&d).unwrap();
    d
}

fn now_utc_rfc3339() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    // Enough for stamps; full chrono formatting available via chrono if needed.
    format!("{secs}Z_unix")
}

fn chrono_utc() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

struct Pace {
    last: Option<Instant>,
}

impl Pace {
    fn new() -> Self {
        Self { last: None }
    }
    fn wait(&mut self) {
        if let Some(t) = self.last {
            let elapsed = t.elapsed();
            if elapsed < MIN_INTERVAL {
                thread::sleep(MIN_INTERVAL - elapsed);
            }
        }
        self.last = Some(Instant::now());
    }
}

#[derive(Clone)]
struct Trip {
    id: &'static str,
    #[allow(dead_code)]
    label: &'static str,
    /// (lat, lon) waypoints.
    points: Vec<(f64, f64)>,
}

fn trips() -> Vec<Trip> {
    let us: Value = serde_json::from_str(
        &fs::read_to_string(fixture_root().join("us_endpoints.json")).unwrap(),
    )
    .unwrap();
    let red = (
        us["points"]["red_ball_garage"]["lat"].as_f64().unwrap(),
        us["points"]["red_ball_garage"]["lon"].as_f64().unwrap(),
    );
    let port = (
        us["points"]["portofino_hotel"]["lat"].as_f64().unwrap(),
        us["points"]["portofino_hotel"]["lon"].as_f64().unwrap(),
    );
    let north = (
        us["points"]["north_coast_inn"]["lat"].as_f64().unwrap(),
        us["points"]["north_coast_inn"]["lon"].as_f64().unwrap(),
    );
    vec![
        Trip {
            id: "klecken_innlandet",
            label: "Klecken -> Innlandet dest",
            points: vec![(53.3340, 10.0450), (61.5929077, 10.3318551)],
        },
        Trip {
            id: "kautokeino_roros",
            label: "Kautokeino -> Roros",
            points: vec![(69.01, 23.04), (62.57, 11.38)],
        },
        Trip {
            id: "us_a_redball_portofino",
            label: "US A Red Ball -> Portofino",
            points: vec![red, port],
        },
        Trip {
            id: "us_b_redball_crescent",
            label: "US B Red Ball -> North Coast Inn",
            points: vec![red, north],
        },
    ]
}

fn save_recorded(
    name: &str,
    base_url: &str,
    method: &str,
    path_or_url: &str,
    request_body: Option<&str>,
    status: u16,
    wall_ms: u128,
    response_body: &str,
    extra: Value,
) -> PathBuf {
    let stamp = chrono_utc();
    let meta = json!({
        "navi_fixture": "recorded",
        "recorded_utc": stamp,
        "provider_base_url": base_url,
        "http_method": method,
        "request_url": path_or_url,
        "request_body": request_body,
        "http_status": status,
        "wall_time_ms": wall_ms,
        "response_bytes": response_body.len(),
        "user_agent": UA,
        "extra": extra,
    });
    let out = json!({
        "navi_fixture": "recorded",
        "meta": meta,
        "response_body": response_body,
    });
    let path = recorded_dir().join(format!("{name}.json"));
    fs::write(&path, serde_json::to_string_pretty(&out).unwrap()).unwrap();
    // Also dump raw response beside it for diffing.
    let raw = recorded_dir().join(format!("{name}.raw"));
    fs::write(&raw, response_body).unwrap();
    if let Some(req) = request_body {
        fs::write(recorded_dir().join(format!("{name}.req")), req).unwrap();
    }
    eprintln!(
        "saved {path:?} status={status} wall_ms={wall_ms} bytes={}",
        response_body.len()
    );
    path
}

fn classify_limit(status: u16, body: &str) -> Option<&'static str> {
    if status == 200 {
        return None;
    }
    let lower = body.to_ascii_lowercase();
    if status == 429 || lower.contains("rate") || lower.contains("retry later") {
        return Some("rate");
    }
    if lower.contains("timeout") || status == 504 {
        return Some("timeout");
    }
    if lower.contains("max distance")
        || lower.contains("maximum distance")
        || lower.contains("exceeds the max distance")
        || (lower.contains("distance") && lower.contains("limit"))
    {
        return Some("distance");
    }
    if lower.contains("too many") && lower.contains("location") {
        return Some("locations");
    }
    if status >= 500 {
        return Some("server_error");
    }
    if status >= 400 {
        return Some("client_error");
    }
    None
}

/// Decode Valhalla/OSRM-style polyline with precision 1e-6.
fn decode_polyline6(encoded: &str) -> Vec<(f64, f64)> {
    let mut coords = Vec::new();
    let bytes = encoded.as_bytes();
    let mut index = 0usize;
    let mut lat: i32 = 0;
    let mut lon: i32 = 0;
    while index < bytes.len() {
        let mut result = 0i32;
        let mut shift = 0;
        loop {
            if index >= bytes.len() {
                return coords;
            }
            let b = bytes[index] as i32 - 63;
            index += 1;
            result |= (b & 0x1f) << shift;
            shift += 5;
            if b < 0x20 {
                break;
            }
        }
        let dlat = if (result & 1) != 0 {
            !(result >> 1)
        } else {
            result >> 1
        };
        lat += dlat;

        result = 0;
        shift = 0;
        loop {
            if index >= bytes.len() {
                return coords;
            }
            let b = bytes[index] as i32 - 63;
            index += 1;
            result |= (b & 0x1f) << shift;
            shift += 5;
            if b < 0x20 {
                break;
            }
        }
        let dlon = if (result & 1) != 0 {
            !(result >> 1)
        } else {
            result >> 1
        };
        lon += dlon;
        coords.push((lat as f64 / 1e6, lon as f64 / 1e6));
    }
    coords
}

fn count_outside_norway(lat_lon: &[(f64, f64)]) -> usize {
    lat_lon
        .iter()
        .filter(|(lat, lon)| country_iso_at(*lat, *lon) != Some("no"))
        .count()
}

fn valhalla_ferry_maneuvers(trip: &Value) -> usize {
    let mut n = 0usize;
    if let Some(legs) = trip.pointer("/trip/legs").and_then(|v| v.as_array()) {
        for leg in legs {
            if let Some(maneuvers) = leg.get("maneuvers").and_then(|m| m.as_array()) {
                for m in maneuvers {
                    let tt = m.get("travel_type").and_then(|t| t.as_str()).unwrap_or("");
                    let instruction = m
                        .get("instruction")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    if tt.eq_ignore_ascii_case("ferry")
                        || instruction.contains("ferry")
                        || m.get("ferry").and_then(|f| f.as_bool()) == Some(true)
                    {
                        n += 1;
                    }
                }
            }
        }
    }
    n
}

fn brouter_ferry_segments(fc: &Value) -> usize {
    let mut n = 0usize;
    // messages table: [Longitude, Latitude, Elevation, Distance, Cost, Ascend, WayTags, ...]
    if let Some(msgs) = fc
        .pointer("/features/0/properties/messages")
        .and_then(|m| m.as_array())
    {
        for row in msgs.iter().skip(1) {
            if let Some(arr) = row.as_array() {
                let tags = arr
                    .get(9)
                    .or_else(|| arr.last())
                    .and_then(|t| t.as_str())
                    .unwrap_or("");
                if tags.to_ascii_lowercase().contains("ferry") {
                    n += 1;
                }
            }
        }
    }
    n
}

struct ProbeResult {
    provider: String,
    trip_id: String,
    status: u16,
    wall_ms: u128,
    distance_m: Option<f64>,
    duration_s: Option<f64>,
    geom_points: Option<usize>,
    response_bytes: usize,
    limit_hit: Option<&'static str>,
    ferry_segments: Option<usize>,
    outside_norway: Option<usize>,
    error_text: Option<String>,
    lat_lon: Vec<(f64, f64)>,
    warnings: Vec<String>,
}

fn http_post_json(
    pace: &mut Pace,
    url: &str,
    body: &str,
    extra_headers: &[(&str, &str)],
) -> (u16, u128, String) {
    pace.wait();
    let url = url.to_string();
    let body = body.to_string();
    let headers: Vec<(String, String)> = extra_headers
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async move {
        let client = reqwest::Client::builder()
            .user_agent(UA)
            .timeout(Duration::from_secs(180))
            .build()
            .unwrap();
        let mut req = client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json");
        for (k, v) in &headers {
            req = req.header(k.as_str(), v.as_str());
        }
        let t0 = Instant::now();
        match req.body(body).send().await {
            Ok(r) => {
                let status = r.status().as_u16();
                let text = r.text().await.unwrap_or_default();
                (status, t0.elapsed().as_millis(), text)
            }
            Err(e) => (0, t0.elapsed().as_millis(), format!("transport_error: {e}")),
        }
    })
}

fn http_get(pace: &mut Pace, url: &str) -> (u16, u128, String) {
    pace.wait();
    let url = url.to_string();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async move {
        let client = reqwest::Client::builder()
            .user_agent(UA)
            .timeout(Duration::from_secs(180))
            .build()
            .unwrap();
        let t0 = Instant::now();
        match client
            .get(&url)
            .header("Accept", "application/geo+json")
            .send()
            .await
        {
            Ok(r) => {
                let status = r.status().as_u16();
                let text = r.text().await.unwrap_or_default();
                (status, t0.elapsed().as_millis(), text)
            }
            Err(e) => (0, t0.elapsed().as_millis(), format!("transport_error: {e}")),
        }
    })
}

fn probe_valhalla_route(
    pace: &mut Pace,
    trip: &Trip,
    suffix: &str,
    costing_auto: Value,
) -> ProbeResult {
    let locations: Vec<Value> = trip
        .points
        .iter()
        .map(|(lat, lon)| json!({"lat": lat, "lon": lon, "type": "break"}))
        .collect();
    let body = json!({
        "locations": locations,
        "costing": "auto",
        "costing_options": { "auto": costing_auto },
        "units": "kilometers",
        "id": format!("navi-probe-{}", trip.id),
    });
    let body_s = body.to_string();
    let url = format!("{VALHALLA_BASE}/route");
    let (status, wall, text) = http_post_json(
        pace,
        &url,
        &body_s,
        &[
            ("X-Client-Id", CLIENT_ID),
            ("Referer", "https://github.com/navigate-me/Navi"),
        ],
    );
    let name = format!("valhalla_{}_{}", trip.id, suffix);
    let mut warnings = Vec::new();
    if let Ok(v) = serde_json::from_str::<Value>(&text) {
        if let Some(w) = v.get("warnings").and_then(|w| w.as_array()) {
            for item in w {
                warnings.push(item.to_string());
            }
        }
        // Some builds put ignored options under error/notice fields.
        if let Some(e) = v.get("error").and_then(|e| e.as_str()) {
            if e.to_ascii_lowercase().contains("ignor") {
                warnings.push(e.to_string());
            }
        }
    }
    save_recorded(
        &name,
        VALHALLA_BASE,
        "POST",
        &url,
        Some(&body_s),
        status,
        wall,
        &text,
        json!({
            "trip_id": trip.id,
            "suffix": suffix,
            "costing_auto": costing_auto,
            "warnings": warnings,
        }),
    );

    let mut out = ProbeResult {
        provider: "valhalla".into(),
        trip_id: format!("{}_{suffix}", trip.id),
        status,
        wall_ms: wall,
        distance_m: None,
        duration_s: None,
        geom_points: None,
        response_bytes: text.len(),
        limit_hit: classify_limit(status, &text),
        ferry_segments: None,
        outside_norway: None,
        error_text: None,
        lat_lon: Vec::new(),
        warnings: warnings.clone(),
    };

    if status != 200 {
        out.error_text = Some(text.chars().take(400).collect());
        return out;
    }
    let v: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            out.error_text = Some(format!("json parse: {e}"));
            return out;
        }
    };
    if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
        out.error_text = Some(err.to_string());
        out.limit_hit = classify_limit(status, err).or(out.limit_hit);
    }
    let length_km = v
        .pointer("/trip/summary/length")
        .and_then(|x| x.as_f64())
        .unwrap_or(0.0);
    let time_s = v
        .pointer("/trip/summary/time")
        .and_then(|x| x.as_f64())
        .unwrap_or(0.0);
    out.distance_m = Some(length_km * 1000.0);
    out.duration_s = Some(time_s);
    out.ferry_segments = Some(valhalla_ferry_maneuvers(&v));

    let mut lat_lon = Vec::new();
    if let Some(legs) = v.pointer("/trip/legs").and_then(|l| l.as_array()) {
        for leg in legs {
            if let Some(shape) = leg.get("shape").and_then(|s| s.as_str()) {
                lat_lon.extend(decode_polyline6(shape));
            }
        }
    }
    out.geom_points = Some(lat_lon.len());
    if trip.id.contains("kautokeino") {
        out.outside_norway = Some(count_outside_norway(&lat_lon));
    }
    out.lat_lon = lat_lon;
    out
}

fn probe_brouter(pace: &mut Pace, trip: &Trip) -> ProbeResult {
    let lonlats: String = trip
        .points
        .iter()
        .map(|(lat, lon)| format!("{lon},{lat}"))
        .collect::<Vec<_>>()
        .join("|");
    let url = format!(
        "{BROUTER_BASE}/brouter?lonlats={lonlats}&profile={BROUTER_PROFILE}&alternativeidx=0&format=geojson"
    );
    let (status, wall, text) = http_get(pace, &url);
    let name = format!("brouter_{}_{}", trip.id, BROUTER_PROFILE);
    save_recorded(
        &name,
        BROUTER_BASE,
        "GET",
        &url,
        None,
        status,
        wall,
        &text,
        json!({
            "trip_id": trip.id,
            "profile": BROUTER_PROFILE,
            "car_profiles_offered": BROUTER_CAR_PROFILES,
        }),
    );
    let mut out = ProbeResult {
        provider: "brouter".into(),
        trip_id: trip.id.to_string(),
        status,
        wall_ms: wall,
        distance_m: None,
        duration_s: None,
        geom_points: None,
        response_bytes: text.len(),
        limit_hit: classify_limit(status, &text),
        ferry_segments: None,
        outside_norway: None,
        error_text: None,
        lat_lon: Vec::new(),
        warnings: Vec::new(),
    };
    if status != 200 {
        out.error_text = Some(text.chars().take(400).collect());
        return out;
    }
    let v: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            out.error_text = Some(format!("json parse: {e}"));
            return out;
        }
    };
    let props = v.pointer("/features/0/properties");
    let track_m = props.and_then(|p| p.get("track-length")).and_then(|x| {
        x.as_f64()
            .or_else(|| x.as_str().and_then(|s| s.parse().ok()))
    });
    let time_s = props.and_then(|p| p.get("total-time")).and_then(|x| {
        x.as_f64()
            .or_else(|| x.as_str().and_then(|s| s.parse().ok()))
    });
    out.distance_m = track_m;
    out.duration_s = time_s;
    out.ferry_segments = Some(brouter_ferry_segments(&v));

    let mut lat_lon = Vec::new();
    if let Some(coords) = v
        .pointer("/features/0/geometry/coordinates")
        .and_then(|c| c.as_array())
    {
        for c in coords {
            if let Some(a) = c.as_array() {
                if a.len() >= 2 {
                    let lon = a[0].as_f64().unwrap_or(0.0);
                    let lat = a[1].as_f64().unwrap_or(0.0);
                    lat_lon.push((lat, lon));
                }
            }
        }
    }
    out.geom_points = Some(lat_lon.len());
    if trip.id.contains("kautokeino") {
        out.outside_norway = Some(count_outside_norway(&lat_lon));
    }
    out.lat_lon = lat_lon;
    out
}

fn load_catalog() -> (Vec<String>, Vec<(String, u64)>) {
    #[derive(Deserialize)]
    struct Cat {
        regions: Vec<Reg>,
    }
    #[derive(Deserialize)]
    struct Reg {
        region_id: String,
        bytes: Option<u64>,
    }
    let cat: Cat =
        serde_json::from_str(&fs::read_to_string(fixture_root().join("current.json")).unwrap())
            .unwrap();
    let ids: Vec<_> = cat.regions.iter().map(|r| r.region_id.clone()).collect();
    let sizes: Vec<_> = cat
        .regions
        .iter()
        .map(|r| (r.region_id.clone(), r.bytes.unwrap_or(0)))
        .collect();
    (ids, sizes)
}

fn regions_along(
    lat_lon: &[(f64, f64)],
    catalog_ids: &[String],
    country_iso: Option<&str>,
) -> Vec<String> {
    let entries = catalog_entries_from_ready_ids(catalog_ids);
    let installed: Vec<String> = Vec::new();
    driver_break_core::long_trip::ordered_needed_regions_along_route_filtered(
        lat_lon,
        &entries,
        &installed,
        LONG_TRIP_CORRIDOR_BUFFER_KM,
        country_iso,
    )
}

fn check_region_list_properties(needed: &[String], label: &str) {
    let mut seen = BTreeSet::new();
    for r in needed {
        assert!(seen.insert(r.clone()), "{label}: duplicate {r}");
    }
    for w in needed.windows(2) {
        let Some(ba) = region_bbox(&w[0]) else {
            eprintln!("{label}: no bbox for {}", w[0]);
            continue;
        };
        let Some(bb) = region_bbox(&w[1]) else {
            eprintln!("{label}: no bbox for {}", w[1]);
            continue;
        };
        if !regions_bbox_adjacent(&ba, &bb, 1.75) {
            eprintln!(
                "{label}: NON-ADJACENT {} -> {} (property check, not a hard fail)",
                w[0], w[1]
            );
        }
    }
}

fn list_diff(a: &[String], b: &[String]) -> (Vec<String>, Vec<String>) {
    let sa: BTreeSet<_> = a.iter().cloned().collect();
    let sb: BTreeSet<_> = b.iter().cloned().collect();
    (
        sa.difference(&sb).cloned().collect(),
        sb.difference(&sa).cloned().collect(),
    )
}

fn synthetic_lat_lon(name: &str) -> Vec<(f64, f64)> {
    let raw = fs::read_to_string(fixture_root().join(name)).unwrap();
    let v: Value = serde_json::from_str(&raw).unwrap();
    let mut out = Vec::new();
    if let Some(coords) = v
        .pointer("/features/0/geometry/coordinates")
        .and_then(|c| c.as_array())
    {
        for c in coords {
            if let Some(a) = c.as_array() {
                out.push((a[1].as_f64().unwrap(), a[0].as_f64().unwrap()));
            }
        }
    }
    out
}

fn print_result(r: &ProbeResult) {
    eprintln!(
        "RESULT provider={} trip={} status={} wall_ms={} dist_m={:?} dur_s={:?} pts={:?} bytes={} limit={:?} ferry={:?} outside_no={:?} err={:?} warnings={:?}",
        r.provider,
        r.trip_id,
        r.status,
        r.wall_ms,
        r.distance_m,
        r.duration_s,
        r.geom_points,
        r.response_bytes,
        r.limit_hit,
        r.ferry_segments,
        r.outside_norway,
        r.error_text.as_ref().map(|s| s.chars().take(120).collect::<String>()),
        r.warnings
    );
}

#[test]
#[ignore = "live probe: set NAVI_ROUTER_PROBE=1 (uses public Valhalla/BRouter; ≤1 req/s)"]
fn probe_public_routers() {
    assert_eq!(
        std::env::var("NAVI_ROUTER_PROBE").ok().as_deref(),
        Some("1"),
        "refusing network probe without NAVI_ROUTER_PROBE=1"
    );

    let mut pace = Pace::new();
    let all_trips = trips();
    let mut results: Vec<ProbeResult> = Vec::new();
    let mut request_count = 0usize;

    // --- Valhalla ferry option behaviour (short Kiel -> Oslo ferry corridor) ---
    let ferry_trip = Trip {
        id: "ferry_kiel_oslo",
        label: "ferry probe Kiel->Oslo",
        points: vec![(54.3233, 10.1394), (59.9044, 10.7410)],
    };
    eprintln!("=== Valhalla use_ferry=0 (Kiel->Oslo) ===");
    let r = probe_valhalla_route(
        &mut pace,
        &ferry_trip,
        "use_ferry_0",
        json!({"use_ferry": 0}),
    );
    print_result(&r);
    results.push(r);
    request_count += 1;

    eprintln!("=== Valhalla exclude_ferry=true (Kiel->Oslo) ===");
    let r = probe_valhalla_route(
        &mut pace,
        &ferry_trip,
        "exclude_ferry",
        json!({"exclude_ferry": true}),
    );
    print_result(&r);
    // Also try plural spelling the user mentioned.
    results.push(r);
    request_count += 1;

    eprintln!("=== Valhalla exclude_ferries=true (plural; Kiel->Oslo) ===");
    let r = probe_valhalla_route(
        &mut pace,
        &ferry_trip,
        "exclude_ferries_plural",
        json!({"exclude_ferries": true}),
    );
    print_result(&r);
    results.push(r);
    request_count += 1;

    // --- country_crossing_penalty accepted range (small SE border hop) ---
    // Oslo -> Karlstad (crosses NO->SE). Probe a few values; no retry loops.
    let border_trip = Trip {
        id: "penalty_oslo_karlstad",
        label: "penalty probe Oslo->Karlstad",
        points: vec![(59.9139, 10.7522), (59.3793, 13.5036)],
    };
    for (suffix, penalty) in [
        ("ccp_2e3", 2_000.0),
        ("ccp_1e6", 1_000_000.0),
        ("ccp_1e9", 1_000_000_000.0),
    ] {
        eprintln!("=== Valhalla country_crossing_penalty={penalty} ===");
        let r = probe_valhalla_route(
            &mut pace,
            &border_trip,
            suffix,
            json!({"use_ferry": 0, "country_crossing_penalty": penalty}),
        );
        print_result(&r);
        results.push(r);
        request_count += 1;
    }

    // --- Main trips: Valhalla ---
    for trip in &all_trips {
        eprintln!("=== Valhalla use_ferry=0 trip={} ===", trip.id);
        let r = probe_valhalla_route(&mut pace, trip, "use_ferry_0", json!({"use_ferry": 0}));
        print_result(&r);
        results.push(r);
        request_count += 1;

        if trip.id == "kautokeino_roros" {
            eprintln!("=== Valhalla kautokeino country_crossing_penalty=1e9 ===");
            let r = probe_valhalla_route(
                &mut pace,
                trip,
                "ccp_1e9",
                json!({"use_ferry": 0, "country_crossing_penalty": 1_000_000_000.0}),
            );
            print_result(&r);
            results.push(r);
            request_count += 1;
        }
    }

    // --- BRouter car profiles note + main trips ---
    eprintln!(
        "BRouter car profiles confirmed on {BROUTER_BASE}: {:?}",
        BROUTER_CAR_PROFILES
    );
    eprintln!("BRouter /brouter/profilelist returned nested 500 during discovery; using confirmed car-eco/car-fast only.");

    for trip in &all_trips {
        eprintln!("=== BRouter {BROUTER_PROFILE} trip={} ===", trip.id);
        let r = probe_brouter(&mut pace, trip);
        print_result(&r);
        results.push(r);
        request_count += 1;
    }

    eprintln!("TOTAL_REQUESTS={request_count}");
    assert!(
        request_count <= 20,
        "request budget exceeded: {request_count}"
    );

    // --- Region lists for successful routes ---
    let (catalog_ids, sizes) = load_catalog();
    let mut region_lists: Vec<(String, Vec<String>)> = Vec::new();

    for r in &results {
        if r.status != 200 || r.lat_lon.len() < 2 {
            continue;
        }
        // Skip auxiliary ferry/penalty probes for region-list comparison except kautokeino ccp.
        let is_main = all_trips.iter().any(|t| r.trip_id.starts_with(t.id))
            || r.trip_id.contains("kautokeino");
        if !is_main && (r.trip_id.contains("ferry_") || r.trip_id.contains("penalty_")) {
            continue;
        }
        let country = if r.trip_id.contains("us_") {
            Some("us")
        } else if r.trip_id.contains("kautokeino") {
            Some("no")
        } else {
            None
        };
        let needed = regions_along(&r.lat_lon, &catalog_ids, country);
        let label = format!("{}:{}", r.provider, r.trip_id);
        eprintln!("REGIONS {label} ({}):", needed.len());
        for id in &needed {
            eprintln!("  {id}");
        }
        check_region_list_properties(&needed, &label);
        region_lists.push((label, needed));
    }

    // Synthetic comparisons
    let synth_pairs = [
        (
            "klecken_innlandet",
            "ors_klecken_innlandet.geojson",
            None::<&str>,
        ),
        (
            "kautokeino_roros",
            "ors_kautokeino_roros.geojson",
            Some("no"),
        ),
        (
            "us_a_redball_portofino",
            "ors_us_a_redball_portofino.geojson",
            Some("us"),
        ),
        (
            "us_b_redball_crescent",
            "ors_us_b_redball_crescent.geojson",
            Some("us"),
        ),
    ];
    for (trip_id, synth_file, country) in synth_pairs {
        let ll = synthetic_lat_lon(synth_file);
        let synth = regions_along(&ll, &catalog_ids, country);
        eprintln!("REGIONS synthetic:{trip_id} ({}):", synth.len());
        for id in &synth {
            eprintln!("  {id}");
        }
        for (label, list) in &region_lists {
            if !label.contains(trip_id) {
                continue;
            }
            let (only_rec, only_synth) = list_diff(list, &synth);
            eprintln!("DIFF {label} vs synthetic:{trip_id}");
            eprintln!("  only in recorded: {only_rec:?}");
            eprintln!("  only in synthetic: {only_synth:?}");
        }
        // Provider vs provider for same trip
        let peers: Vec<_> = region_lists
            .iter()
            .filter(|(l, _)| l.contains(trip_id))
            .collect();
        for i in 0..peers.len() {
            for j in (i + 1)..peers.len() {
                let (only_a, only_b) = list_diff(&peers[i].1, &peers[j].1);
                eprintln!("DIFF {} vs {}", peers[i].0, peers[j].0);
                eprintln!("  only in {}: {only_a:?}", peers[i].0);
                eprintln!("  only in {}: {only_b:?}", peers[j].0);
            }
        }
        region_lists.push((format!("synthetic:{trip_id}"), synth));
    }

    // US storage estimates from recorded lists
    for (label, list) in &region_lists {
        if !label.contains("us_a") && !label.contains("us_b") {
            continue;
        }
        if label.starts_with("synthetic:") {
            continue;
        }
        let check = estimate_trip_disk_bytes(list, &sizes, 512u64 << 30);
        match check {
            SpaceCheck::Ok(r) => eprintln!(
                "STORAGE {label}: packs={} needed={} (512GiB OK)",
                r.pack_bytes, r.needed_bytes
            ),
            SpaceCheck::InsufficientSpace { report, .. } => eprintln!(
                "STORAGE {label}: packs={} needed={} (insufficient vs 512GiB!?)",
                report.pack_bytes, report.needed_bytes
            ),
        }
        let check64 = estimate_trip_disk_bytes(list, &sizes, 64u64 << 30);
        eprintln!("STORAGE64 {label}: {check64:?}");
    }

    // Summary table
    eprintln!("\n=== SUMMARY TABLE ===");
    eprintln!("provider|trip|status|wall_ms|distance_m|limit|ferry|outside_no");
    for r in &results {
        eprintln!(
            "{}|{}|{}|{}|{:?}|{:?}|{:?}|{:?}",
            r.provider,
            r.trip_id,
            r.status,
            r.wall_ms,
            r.distance_m,
            r.limit_hit,
            r.ferry_segments,
            r.outside_norway
        );
    }

    // Write machine-readable summary
    let summary_path = recorded_dir().join("probe_summary.json");
    let summary = json!({
        "navi_fixture": "recorded",
        "recorded_utc": chrono_utc(),
        "request_count": request_count,
        "results": results.iter().map(|r| json!({
            "provider": r.provider,
            "trip_id": r.trip_id,
            "status": r.status,
            "wall_ms": r.wall_ms,
            "distance_m": r.distance_m,
            "duration_s": r.duration_s,
            "geom_points": r.geom_points,
            "response_bytes": r.response_bytes,
            "limit_hit": r.limit_hit,
            "ferry_segments": r.ferry_segments,
            "outside_norway": r.outside_norway,
            "error_text": r.error_text,
            "warnings": r.warnings,
        })).collect::<Vec<_>>(),
    });
    fs::write(
        &summary_path,
        serde_json::to_string_pretty(&summary).unwrap(),
    )
    .unwrap();
    eprintln!("wrote {summary_path:?}");
}

/// Offline: ensure the probe binary compiles and env gate works without network.
#[test]
fn probe_gate_offline() {
    assert!(Path::new(&fixture_root().join("us_endpoints.json")).exists());
    assert!(BROUTER_CAR_PROFILES.contains(&"car-eco"));
    let _ = now_utc_rfc3339();
}
