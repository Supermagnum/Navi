//! Local HTTP API + static assets + PMTiles range serving for the desktop shell.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use driver_break_core::icons::{rasterize_key, IconTheme};
use driver_break_core::nav::{format_distance_m, ManeuverKind};
use driver_break_core::routing::guidance_path::parse_maneuver_kind;
use driver_break_core::sensors::SensorBus;
use navi::{plan_car_route, search_places, FfiVehicleLimits, TravelProfile};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;

use crate::basemap::{self, ResolvedBasemap};

const ASSETS: include_dir::Dir<'_> = include_dir::include_dir!("$CARGO_MANIFEST_DIR/assets");

#[derive(Clone)]
pub struct AppState {
    pub bus: SensorBus,
    pub data_dir: PathBuf,
    pub pbf_path: PathBuf,
    pub elev_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub place_index: Option<PathBuf>,
    pub explicit_pmtiles: Option<PathBuf>,
    pub force_online: bool,
    pub icons_dir: PathBuf,
    pub pmtiles_dirs: Arc<Vec<PathBuf>>,
    pub route: Arc<Mutex<Option<ActiveRoute>>>,
    pub local_origin: Arc<Mutex<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActiveRoute {
    pub distance_km: f64,
    pub eta_minutes: f64,
    pub route_polyline: String,
    pub break_pois_json: String,
    pub maneuvers_json: String,
    pub sim_samples_json: String,
    pub report: String,
    pub use_eco: bool,
    pub profile: String,
}

pub async fn serve(state: AppState, addr: SocketAddr) -> anyhow::Result<SocketAddr> {
    let app = Router::new()
        .route("/", get(index_html))
        .route(
            "/app.js",
            get(|| async { asset("app.js", "application/javascript") }),
        )
        .route("/app.css", get(|| async { asset("app.css", "text/css") }))
        .route("/api/status", get(api_status))
        .route("/api/basemap", get(api_basemap))
        .route("/api/search", get(api_search))
        .route("/api/plan", post(api_plan))
        .route("/api/icon/{key}", get(api_icon))
        .route("/styles/offline.json", get(offline_style))
        .route("/pmtiles/{name}", get(serve_pmtiles))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            eprintln!("navi-desktop HTTP server ended: {e:#}");
        }
    });
    Ok(bound)
}

fn asset(name: &str, ctype: &'static str) -> Response {
    match ASSETS.get_file(name) {
        Some(f) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, ctype)],
            f.contents(),
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "missing asset").into_response(),
    }
}

async fn index_html() -> Html<String> {
    let body = ASSETS
        .get_file("index.html")
        .and_then(|f| std::str::from_utf8(f.contents()).ok())
        .unwrap_or("<html><body>missing index.html</body></html>");
    Html(body.to_string())
}

#[derive(Serialize)]
struct StatusResponse {
    position: Option<PosDto>,
    imu: Option<ImuDto>,
    basemap: ResolvedBasemap,
    hud: HudDto,
    route: Option<ActiveRoute>,
}

#[derive(Serialize)]
struct PosDto {
    lat: f64,
    lon: f64,
    course_deg: f64,
    speed_m_s: f64,
    altitude_m: Option<f64>,
    satellites_used: Option<u32>,
}

#[derive(Serialize)]
struct ImuDto {
    heading_deg: f64,
    pitch_deg: f64,
    roll_deg: f64,
}

#[derive(Serialize)]
struct HudDto {
    next_turn: String,
    next_turn_icon: String,
    distance_to_turn: String,
    distance_to_break: String,
    eco_active: bool,
    eta_minutes: Option<f64>,
    distance_km: Option<f64>,
}

async fn api_status(State(state): State<AppState>) -> Json<StatusResponse> {
    let pos = state.bus.latest_position();
    let imu = state.bus.latest_imu();
    let (lat, lon) = pos.map(|p| (p.lat, p.lon)).unwrap_or((60.79, 10.68)); // Gjøvik-ish default for Ostlandet
    let origin = state
        .local_origin
        .lock()
        .map(|g| g.clone())
        .unwrap_or_else(|_| "http://127.0.0.1".into());
    let basemap = basemap::resolve(
        &state.data_dir,
        lat,
        lon,
        state.explicit_pmtiles.as_deref(),
        state.force_online,
        &origin,
    );
    let route = state.route.lock().ok().and_then(|g| g.clone());
    let hud = build_hud(pos.as_ref(), route.as_ref());
    Json(StatusResponse {
        position: pos.map(|p| PosDto {
            lat: p.lat,
            lon: p.lon,
            course_deg: p.course_deg,
            speed_m_s: p.speed_m_s,
            altitude_m: p.altitude_m,
            satellites_used: p.satellites_used,
        }),
        imu: imu.map(|i| ImuDto {
            heading_deg: i.heading_deg,
            pitch_deg: i.pitch_deg,
            roll_deg: i.roll_deg,
        }),
        basemap,
        hud,
        route,
    })
}

fn build_hud(
    pos: Option<&driver_break_core::sensors::PositionSample>,
    route: Option<&ActiveRoute>,
) -> HudDto {
    let Some(route) = route else {
        return HudDto {
            next_turn: "No route".into(),
            next_turn_icon: "nav_destination".into(),
            distance_to_turn: "—".into(),
            distance_to_break: "—".into(),
            eco_active: false,
            eta_minutes: None,
            distance_km: None,
        };
    };

    let mut next_turn = "Continue".to_string();
    let mut next_icon = "nav_straight".to_string();
    let mut dist_turn = "—".to_string();

    if let (Some(p), Ok(maneuvers)) = (
        pos,
        serde_json::from_str::<Vec<ManDto>>(&route.maneuvers_json),
    ) {
        if let Some(cum) = along_route_m(p.lat, p.lon, &route.sim_samples_json) {
            if let Some(m) = maneuvers.iter().find(|m| m.cum_m > cum + 5.0) {
                let kind = parse_maneuver_kind(&m.kind);
                let dist_m = (m.cum_m - cum).max(0.0);
                let street = m.street.clone().unwrap_or_default();
                next_turn = if street.is_empty() {
                    format!("{kind:?}")
                } else {
                    format!("{kind:?} · {street}")
                };
                next_icon = match m.icon.as_deref().filter(|s| !s.is_empty()) {
                    Some(icon) => icon.to_string(),
                    None if kind == ManeuverKind::Roundabout => {
                        ManeuverKind::roundabout_icon_key(m.roundabout_exit).to_string()
                    }
                    None => kind.icon_key().to_string(),
                };
                dist_turn = format_distance_m(dist_m, true);
            }
        }
    }

    let dist_break = estimate_break_distance(route);

    HudDto {
        next_turn,
        next_turn_icon: next_icon,
        distance_to_turn: dist_turn,
        distance_to_break: dist_break,
        eco_active: route.use_eco,
        eta_minutes: Some(route.eta_minutes),
        distance_km: Some(route.distance_km),
    }
}

#[derive(Deserialize)]
struct ManDto {
    cum_m: f64,
    kind: String,
    street: Option<String>,
    roundabout_exit: Option<u8>,
    #[serde(default)]
    icon: Option<String>,
}

#[derive(Deserialize)]
struct SampleDto {
    lat: f64,
    lon: f64,
    cum_m: f64,
}

fn along_route_m(lat: f64, lon: f64, samples_json: &str) -> Option<f64> {
    let samples: Vec<SampleDto> = serde_json::from_str(samples_json).ok()?;
    if samples.is_empty() {
        return None;
    }
    let mut best = &samples[0];
    let mut best_d = f64::MAX;
    for s in &samples {
        let d = haversine_m(lat, lon, s.lat, s.lon);
        if d < best_d {
            best_d = d;
            best = s;
        }
    }
    Some(best.cum_m)
}

fn estimate_break_distance(route: &ActiveRoute) -> String {
    // Soft car default ~2 h; show remaining distance proxy from first break POI if any.
    if let Ok(pois) = serde_json::from_str::<Vec<serde_json::Value>>(&route.break_pois_json) {
        if let Some(p) = pois.first() {
            if let Some(name) = p.get("name").and_then(|v| v.as_str()) {
                return format!("next stop: {name}");
            }
        }
    }
    let hours = route.eta_minutes / 60.0;
    if hours > 2.0 {
        format!("~{:.0} min to soft break budget", (hours - 2.0) * 60.0)
    } else {
        "within break budget".into()
    }
}

fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6_378_100.0;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

async fn api_basemap(State(state): State<AppState>) -> Json<ResolvedBasemap> {
    let pos = state.bus.latest_position();
    let (lat, lon) = pos.map(|p| (p.lat, p.lon)).unwrap_or((60.79, 10.68));
    let origin = state
        .local_origin
        .lock()
        .map(|g| g.clone())
        .unwrap_or_else(|_| "http://127.0.0.1".into());
    Json(basemap::resolve(
        &state.data_dir,
        lat,
        lon,
        state.explicit_pmtiles.as_deref(),
        state.force_online,
        &origin,
    ))
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    #[serde(default = "default_limit")]
    limit: u32,
}

fn default_limit() -> u32 {
    8
}

async fn api_search(
    State(state): State<AppState>,
    Query(q): Query<SearchQuery>,
) -> impl IntoResponse {
    let Some(db) = state.place_index.clone() else {
        return Json(serde_json::json!({
            "hits": [],
            "note": "No place index configured (--place-index). Use lat/lon fields."
        }))
        .into_response();
    };
    let query = q.q.clone();
    let limit = q.limit;
    let hits = tokio::task::spawn_blocking(move || {
        search_places(db.display().to_string(), query, limit)
            .into_iter()
            .map(|h| {
                serde_json::json!({
                    "name": h.name,
                    "kind": h.kind,
                    "lat": h.lat,
                    "lon": h.lon,
                    "sub_area": h.sub_area,
                    "municipality": h.municipality,
                })
            })
            .collect::<Vec<_>>()
    })
    .await
    .unwrap_or_default();
    Json(serde_json::json!({ "hits": hits })).into_response()
}

#[derive(Deserialize)]
struct PlanBody {
    start_lat: f64,
    start_lon: f64,
    end_lat: f64,
    end_lon: f64,
    #[serde(default)]
    use_eco: bool,
    #[serde(default = "default_profile")]
    profile: String,
}

fn default_profile() -> String {
    "car".into()
}

fn parse_profile(s: &str) -> TravelProfile {
    match s.trim().to_ascii_lowercase().as_str() {
        "car_electric" | "ev" => TravelProfile::CarElectric,
        "truck" => TravelProfile::Truck,
        "truck_electric" => TravelProfile::TruckElectric,
        "mobile_home" | "motorhome" => TravelProfile::MobileHome,
        "bicycle" | "bike" | "cycling" => TravelProfile::Bicycle,
        "bicycle_electric" | "ebike" | "e_bike" | "cycling_electric" => {
            TravelProfile::BicycleElectric
        }
        "motorcycle" => TravelProfile::Motorcycle,
        "motorcycle_electric" => TravelProfile::MotorcycleElectric,
        _ => TravelProfile::Car,
    }
}

async fn api_plan(State(state): State<AppState>, Json(body): Json<PlanBody>) -> impl IntoResponse {
    let pbf = state.pbf_path.clone();
    let elev = state.elev_dir.clone();
    let cache = state.cache_dir.clone();
    if !pbf.is_file() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "ok": false,
                "error": format!("PBF missing: {}", pbf.display()),
            })),
        )
            .into_response();
    }
    let profile = parse_profile(&body.profile);
    let use_eco = body.use_eco;
    let vehicle = FfiVehicleLimits {
        axle_weight_kg: None,
        bogie_weight_kg: None,
        height_m: None,
        width_m: None,
        length_m: None,
        total_weight_kg: None,
    };
    let result = tokio::task::spawn_blocking(move || {
        plan_car_route(
            pbf.display().to_string(),
            elev.display().to_string(),
            cache.display().to_string(),
            body.start_lat,
            body.start_lon,
            body.end_lat,
            body.end_lon,
            use_eco,
            profile,
            false,
            false,
            false,
            vehicle,
            false,
            String::new(),
        )
    })
    .await;

    match result {
        Ok(r) if r.route_polyline.is_empty() => (
            StatusCode::OK,
            Json(serde_json::json!({
                "ok": false,
                "error": r.report,
            })),
        )
            .into_response(),
        Ok(r) => {
            let active = ActiveRoute {
                distance_km: r.distance_km,
                eta_minutes: r.eta_minutes,
                route_polyline: r.route_polyline.clone(),
                break_pois_json: r.break_pois_json.clone(),
                maneuvers_json: r.maneuvers_json.clone(),
                sim_samples_json: r.sim_samples_json.clone(),
                report: r.report.clone(),
                use_eco,
                profile: body.profile.clone(),
            };
            if let Ok(mut g) = state.route.lock() {
                *g = Some(active.clone());
            }
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "ok": true,
                    "distance_km": active.distance_km,
                    "eta_minutes": active.eta_minutes,
                    "route_polyline": active.route_polyline,
                    "break_pois_json": active.break_pois_json,
                    "maneuvers_json": active.maneuvers_json,
                    "report": active.report,
                })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "ok": false, "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn api_icon(
    State(state): State<AppState>,
    AxumPath(key): AxumPath<String>,
) -> impl IntoResponse {
    let key = key.trim_end_matches(".png").to_string();
    let icons = state.icons_dir.clone();
    let png = tokio::task::spawn_blocking(move || {
        rasterize_key(&key, IconTheme::Day, 64, 64, None, &icons)
    })
    .await;
    match png {
        Ok(Ok(rgba)) => {
            // Encode raw RGBA via png crate already in workspace through core — use simple PNG encoder
            match rgba8_to_png(&rgba, 64, 64) {
                Ok(bytes) => {
                    (StatusCode::OK, [(header::CONTENT_TYPE, "image/png")], bytes).into_response()
                }
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
            }
        }
        Ok(Err(e)) => (StatusCode::NOT_FOUND, e.to_string()).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

fn rgba8_to_png(rgba: &[u8], w: u32, h: u32) -> anyhow::Result<Vec<u8>> {
    let mut buf = Vec::new();
    let mut encoder = png::Encoder::new(&mut buf, w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(rgba)?;
    drop(writer);
    Ok(buf)
}

#[derive(Deserialize)]
struct OfflineStyleQuery {
    file: String,
}

async fn offline_style(
    State(state): State<AppState>,
    Query(q): Query<OfflineStyleQuery>,
) -> impl IntoResponse {
    let origin = state
        .local_origin
        .lock()
        .map(|g| g.clone())
        .unwrap_or_else(|_| "http://127.0.0.1".into());
    let template = ASSETS
        .get_file("protomaps-style.template.json")
        .and_then(|f| std::str::from_utf8(f.contents()).ok())
        .unwrap_or("{}");
    let file = q.file.replace(['/', '\\'], "");
    let pmtiles_url = format!("pmtiles://{origin}/pmtiles/{file}");
    // Glyphs/sprites from Protomaps CDN (vector tiles stay local).
    let rewritten = template
        .replace("__PMTILES_URL__", &pmtiles_url)
        .replace(
            "__SPRITE__",
            "https://protomaps.github.io/basemaps-assets/sprites/v4/light",
        )
        .replace(
            "__GLYPHS__",
            "https://protomaps.github.io/basemaps-assets/fonts",
        );
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        rewritten,
    )
        .into_response()
}

async fn serve_pmtiles(
    State(state): State<AppState>,
    AxumPath(name): AxumPath<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let name = name.replace(['/', '\\'], "");
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(p) = &state.explicit_pmtiles {
        if p.file_name().and_then(|s| s.to_str()) == Some(name.as_str()) {
            candidates.push(p.clone());
        }
    }
    for dir in state.pmtiles_dirs.iter() {
        candidates.push(dir.join(&name));
    }
    candidates.push(state.data_dir.join("pmtiles").join(&name));
    candidates.push(state.data_dir.join(&name));

    let path = candidates.into_iter().find(|p| p.is_file());
    let Some(path) = path else {
        return (StatusCode::NOT_FOUND, "pmtiles not found").into_response();
    };

    let meta = match std::fs::metadata(&path) {
        Ok(m) => m,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };
    let len = meta.len();
    let range = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_byte_range);

    match range {
        Some((start, end)) => {
            let end = end.min(len.saturating_sub(1));
            if start > end || start >= len {
                return StatusCode::RANGE_NOT_SATISFIABLE.into_response();
            }
            let take = (end - start + 1) as usize;
            match read_range(&path, start, take) {
                Ok(bytes) => Response::builder()
                    .status(StatusCode::PARTIAL_CONTENT)
                    .header(header::CONTENT_TYPE, "application/octet-stream")
                    .header(header::ACCEPT_RANGES, "bytes")
                    .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{len}"))
                    .header(header::CONTENT_LENGTH, bytes.len().to_string())
                    .body(Body::from(bytes))
                    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
            }
        }
        None => match std::fs::read(&path) {
            Ok(bytes) => (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "application/octet-stream"),
                    (header::ACCEPT_RANGES, "bytes"),
                ],
                bytes,
            )
                .into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        },
    }
}

fn parse_byte_range(h: &str) -> Option<(u64, u64)> {
    let h = h.strip_prefix("bytes=")?;
    let (a, b) = h.split_once('-')?;
    let start: u64 = a.parse().ok()?;
    let end: u64 = if b.is_empty() {
        u64::MAX
    } else {
        b.parse().ok()?
    };
    Some((start, end))
}

fn read_range(path: &Path, start: u64, take: usize) -> std::io::Result<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path)?;
    f.seek(SeekFrom::Start(start))?;
    let mut buf = vec![0u8; take];
    let mut read = 0;
    while read < take {
        match f.read(&mut buf[read..])? {
            0 => break,
            n => read += n,
        }
    }
    buf.truncate(read);
    Ok(buf)
}

/// Collect directories that may hold `.pmtiles` files for the range server.
pub fn default_pmtiles_dirs(data_dir: &Path) -> Vec<PathBuf> {
    vec![
        data_dir.join("pmtiles"),
        data_dir.to_path_buf(),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../core/target/integration-fixtures"),
    ]
}
