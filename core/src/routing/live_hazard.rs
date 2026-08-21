//! Route-independent live hazard cone: compact points + heading look-ahead.
//!
//! Constraints (from the overhead investigation):
//! - Parse once into compact points (centroids / device nodes), never re-decode
//!   full region JSON on every GPS tick.
//! - Window residency to the same cell bbox as idle road-label snap
//!   (~0.05° cell + 1 cell pad).
//! - Speed-limit look-ahead queries the existing cell graph — no separate
//!   speed-limit dataset.
//!
//! Cone radius is **300 m** (distinct from the 200 m route-corridor children
//! band). Approach chrome still uses 750 / 150 / 25 m phases capped to the cone.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use osmpbf::{Element, ElementReader};

use crate::nav::{ApproachPhase, APPROACH_APPEAR_M, APPROACH_HIDE_M, APPROACH_URGENCY_M};
use crate::routing::graph::{edge_distance_m, RouteGraph};
use crate::routing::road_sign::{
    load_catalog, resolve_road_sign_jurisdiction_at, RoadSignJurisdiction, RoadSignRecord,
    RoadSignWarning,
};
use crate::routing::speed_camera::{
    applicable_limit_or_fallback_kmh, nearest_speed_camera_warning,
    resolve_speed_camera_jurisdiction_at, SpeedCameraJurisdiction, SpeedCameraKind,
    SpeedCameraRecord, SpeedCameraWarning,
};

/// Look-ahead cone radius for route-independent hazards (metres).
pub const LIVE_HAZARD_CONE_M: f64 = 300.0;
/// Half-angle of the heading cone (degrees).
pub const LIVE_HAZARD_CONE_HALF_WIDTH_DEG: f64 = 60.0;
/// Same cell size as idle GPS road-label graph caches in navi-ffi.
pub const LIVE_HAZARD_CELL_DEG: f64 = 0.05;
pub const LIVE_HAZARD_PAD_CELLS: f64 = 1.0;

#[derive(Debug, Clone)]
pub struct ChildrenCentroid {
    pub osm_id: i64,
    pub lat: f64,
    pub lon: f64,
    pub name: String,
    pub category: &'static str,
}

#[derive(Debug, Clone)]
pub struct SpeedBumpPoint {
    pub osm_id: i64,
    pub lat: f64,
    pub lon: f64,
    pub calming: String,
}

#[derive(Debug, Clone, Default)]
pub struct LiveHazardIndex {
    pub signs: Vec<RoadSignRecord>,
    pub children: Vec<ChildrenCentroid>,
    pub cameras: Vec<SpeedCameraRecord>,
    pub bumps: Vec<SpeedBumpPoint>,
}

#[derive(Debug, Clone)]
pub struct LiveHazardLoadStats {
    pub signs: usize,
    pub children: usize,
    pub cameras: usize,
    pub bumps: usize,
    pub compact_json_utf8: usize,
}

impl LiveHazardIndex {
    pub fn load_from_pbf(path: impl AsRef<Path>) -> anyhow::Result<(Self, LiveHazardLoadStats)> {
        let path = path.as_ref();
        // Order matters for peak RAM on Ostlandet-class PBFs: children/bumps first
        // (needed for the live cone), then catalogue signs, then cameras.
        let (children, bumps) = load_children_centroids_and_bumps(path)?;
        let catalog = load_catalog()?;
        let signs = crate::routing::road_sign::load_road_signs_from_pbf(path, &catalog)?;
        let cameras = crate::routing::speed_camera::load_speed_cameras_from_pbf(path)?;
        let index = Self {
            signs,
            children,
            cameras,
            bumps,
        };
        let stats = LiveHazardLoadStats {
            signs: index.signs.len(),
            children: index.children.len(),
            cameras: index.cameras.len(),
            bumps: index.bumps.len(),
            compact_json_utf8: index.estimated_compact_json_utf8(),
        };
        Ok((index, stats))
    }

    /// Approximate UTF-8 size of [`Self::compact_json`] without building it.
    pub fn estimated_compact_json_utf8(&self) -> usize {
        self.signs.len() * 200
            + self.children.len() * 140
            + self.cameras.len() * 160
            + self.bumps.len() * 100
            + 64
    }

    pub fn windowed(&self, lat: f64, lon: f64) -> Self {
        let bbox = live_hazard_bbox(lat, lon);
        Self {
            signs: self
                .signs
                .iter()
                .filter(|s| in_bbox(s.lat, s.lon, bbox))
                .cloned()
                .collect(),
            children: self
                .children
                .iter()
                .filter(|c| in_bbox(c.lat, c.lon, bbox))
                .cloned()
                .collect(),
            cameras: self
                .cameras
                .iter()
                .filter(|c| in_bbox(c.lat, c.lon, bbox))
                .cloned()
                .collect(),
            bumps: self
                .bumps
                .iter()
                .filter(|b| in_bbox(b.lat, b.lon, bbox))
                .cloned()
                .collect(),
        }
    }

    pub fn compact_json(&self) -> String {
        let mut rows = Vec::with_capacity(
            self.signs.len() + self.children.len() + self.cameras.len() + self.bumps.len(),
        );
        for s in &self.signs {
            rows.push(serde_json::json!({
                "kind": "road_sign",
                "osm_id": s.osm_id,
                "lat": s.lat,
                "lon": s.lon,
                "code": s.code,
                "icon_key": s.icon_key,
                "name_en": s.name_en,
            }));
        }
        for c in &self.children {
            rows.push(serde_json::json!({
                "kind": "children_zone",
                "osm_id": c.osm_id,
                "lat": c.lat,
                "lon": c.lon,
                "name": c.name,
                "category": c.category,
            }));
        }
        for c in &self.cameras {
            rows.push(serde_json::json!({
                "kind": "speed_camera",
                "osm_id": c.osm_id,
                "lat": c.lat,
                "lon": c.lon,
                "camera_kind": match c.kind {
                    SpeedCameraKind::Point => "point",
                    SpeedCameraKind::AverageSpeed => "average_speed",
                },
            }));
        }
        for b in &self.bumps {
            rows.push(serde_json::json!({
                "kind": "speed_bump",
                "osm_id": b.osm_id,
                "lat": b.lat,
                "lon": b.lon,
                "calming": b.calming,
            }));
        }
        serde_json::to_string(&rows).unwrap_or_else(|_| "[]".into())
    }

    pub fn children_json(&self) -> String {
        let rows: Vec<_> = self
            .children
            .iter()
            .map(|c| {
                serde_json::json!({
                    "osm_id": c.osm_id,
                    "lat": c.lat,
                    "lon": c.lon,
                    "name": c.name,
                    "category": c.category,
                    "kind": "centroid",
                })
            })
            .collect();
        serde_json::to_string(&rows).unwrap_or_else(|_| "[]".into())
    }

    pub fn signs_json(&self) -> String {
        let rows: Vec<_> = self
            .signs
            .iter()
            .map(|s| {
                serde_json::json!({
                    "osm_id": s.osm_id,
                    "lat": s.lat,
                    "lon": s.lon,
                    "icon_key": s.icon_key,
                    "code": s.code,
                    "name_en": s.name_en,
                    "traffic_sign_raw": s.traffic_sign_raw,
                })
            })
            .collect();
        serde_json::to_string(&rows).unwrap_or_else(|_| "[]".into())
    }

    pub fn cameras_json(&self) -> String {
        let rows: Vec<_> = self
            .cameras
            .iter()
            .map(|c| {
                serde_json::json!({
                    "osm_id": c.osm_id,
                    "lat": c.lat,
                    "lon": c.lon,
                    "kind": match c.kind {
                        SpeedCameraKind::Point => "point",
                        SpeedCameraKind::AverageSpeed => "average_speed",
                    },
                    "maxspeed_kmh": c.maxspeed_kmh,
                    "maxspeed_conditional": c.maxspeed_conditional,
                    "zone_from_lat": c.zone_from_lat,
                    "zone_from_lon": c.zone_from_lon,
                    "zone_to_lat": c.zone_to_lat,
                    "zone_to_lon": c.zone_to_lon,
                    "zone_length_m": c.zone_length_m,
                })
            })
            .collect();
        serde_json::to_string(&rows).unwrap_or_else(|_| "[]".into())
    }

    pub fn bumps_json(&self) -> String {
        let rows: Vec<_> = self
            .bumps
            .iter()
            .map(|b| {
                serde_json::json!({
                    "osm_id": b.osm_id,
                    "lat": b.lat,
                    "lon": b.lon,
                    "calming": b.calming,
                })
            })
            .collect();
        serde_json::to_string(&rows).unwrap_or_else(|_| "[]".into())
    }
}

pub fn live_hazard_bbox(lat: f64, lon: f64) -> [f64; 4] {
    let cell = LIVE_HAZARD_CELL_DEG;
    let pad = LIVE_HAZARD_PAD_CELLS * cell;
    let i = (lat / cell).floor();
    let j = (lon / cell).floor();
    [
        i * cell - pad,
        j * cell - pad,
        (i + 1.0) * cell + pad,
        (j + 1.0) * cell + pad,
    ]
}

pub fn live_hazard_cell_key(lat: f64, lon: f64) -> String {
    let b = live_hazard_bbox(lat, lon);
    format!("{:.2}_{:.2}_{:.2}_{:.2}", b[0], b[1], b[2], b[3])
}

fn in_bbox(lat: f64, lon: f64, bbox: [f64; 4]) -> bool {
    lat >= bbox[0] && lon >= bbox[1] && lat <= bbox[2] && lon <= bbox[3]
}

fn children_cat(k: &str, v: &str) -> Option<&'static str> {
    match (k, v) {
        ("amenity", "school") => Some("school"),
        ("amenity", "kindergarten") => Some("kindergarten"),
        ("leisure", "playground") => Some("playground"),
        _ => None,
    }
}

pub fn load_children_centroids_json(path: impl AsRef<Path>) -> anyhow::Result<String> {
    let (children, _) = load_children_centroids_and_bumps(path.as_ref())?;
    let rows: Vec<_> = children
        .iter()
        .map(|c| {
            serde_json::json!({
                "osm_id": c.osm_id,
                "lat": c.lat,
                "lon": c.lon,
                "name": c.name,
                "category": c.category,
                "kind": "centroid",
            })
        })
        .collect();
    Ok(serde_json::to_string(&rows).unwrap_or_else(|_| "[]".into()))
}

pub fn load_speed_bumps_json(path: impl AsRef<Path>) -> anyhow::Result<String> {
    let mut bumps: Vec<SpeedBumpPoint> = Vec::new();
    let reader = ElementReader::from_path(path.as_ref())?;
    reader.for_each(|el| match el {
        Element::Node(n) => {
            for (k, v) in n.tags() {
                if k == "traffic_calming" && matches!(v, "hump" | "bump" | "table") {
                    bumps.push(SpeedBumpPoint {
                        osm_id: n.id(),
                        lat: n.lat(),
                        lon: n.lon(),
                        calming: v.to_string(),
                    });
                }
            }
        }
        Element::DenseNode(n) => {
            for (k, v) in n.tags() {
                if k == "traffic_calming" && matches!(v, "hump" | "bump" | "table") {
                    bumps.push(SpeedBumpPoint {
                        osm_id: n.id,
                        lat: n.lat(),
                        lon: n.lon(),
                        calming: v.to_string(),
                    });
                }
            }
        }
        _ => {}
    })?;
    let rows: Vec<_> = bumps
        .iter()
        .map(|b| {
            serde_json::json!({
                "osm_id": b.osm_id,
                "lat": b.lat,
                "lon": b.lon,
                "calming": b.calming,
            })
        })
        .collect();
    Ok(serde_json::to_string(&rows).unwrap_or_else(|_| "[]".into()))
}

fn load_children_centroids_and_bumps(
    path: &Path,
) -> anyhow::Result<(Vec<ChildrenCentroid>, Vec<SpeedBumpPoint>)> {
    let mut children_nodes: Vec<ChildrenCentroid> = Vec::new();
    let mut way_refs: Vec<(i64, Vec<i64>, String, &'static str)> = Vec::new();
    let mut needed: HashSet<i64> = HashSet::new();
    let mut bumps: Vec<SpeedBumpPoint> = Vec::new();

    let reader = ElementReader::from_path(path)?;
    reader.for_each(|el| match el {
        Element::Node(n) => {
            let mut name = String::new();
            let mut category = None;
            let mut calming = None;
            for (k, v) in n.tags() {
                if k == "name" {
                    name = v.to_string();
                }
                if category.is_none() {
                    category = children_cat(k, v);
                }
                if k == "traffic_calming" && matches!(v, "hump" | "bump" | "table") {
                    calming = Some(v.to_string());
                }
            }
            if let Some(category) = category {
                children_nodes.push(ChildrenCentroid {
                    osm_id: n.id(),
                    lat: n.lat(),
                    lon: n.lon(),
                    name,
                    category,
                });
            }
            if let Some(calming) = calming {
                bumps.push(SpeedBumpPoint {
                    osm_id: n.id(),
                    lat: n.lat(),
                    lon: n.lon(),
                    calming,
                });
            }
        }
        Element::DenseNode(n) => {
            let mut name = String::new();
            let mut category = None;
            let mut calming = None;
            for (k, v) in n.tags() {
                if k == "name" {
                    name = v.to_string();
                }
                if category.is_none() {
                    category = children_cat(k, v);
                }
                if k == "traffic_calming" && matches!(v, "hump" | "bump" | "table") {
                    calming = Some(v.to_string());
                }
            }
            if let Some(category) = category {
                children_nodes.push(ChildrenCentroid {
                    osm_id: n.id,
                    lat: n.lat(),
                    lon: n.lon(),
                    name,
                    category,
                });
            }
            if let Some(calming) = calming {
                bumps.push(SpeedBumpPoint {
                    osm_id: n.id,
                    lat: n.lat(),
                    lon: n.lon(),
                    calming,
                });
            }
        }
        Element::Way(w) => {
            let mut name = String::new();
            let mut category = None;
            for (k, v) in w.tags() {
                if k == "name" {
                    name = v.to_string();
                }
                if category.is_none() {
                    category = children_cat(k, v);
                }
            }
            if let Some(category) = category {
                let refs: Vec<i64> = w.refs().collect();
                if refs.len() >= 2 {
                    for id in &refs {
                        needed.insert(*id);
                    }
                    way_refs.push((w.id(), refs, name, category));
                }
            }
        }
        _ => {}
    })?;

    let mut coords: HashMap<i64, (f64, f64)> = HashMap::with_capacity(needed.len().max(1024));
    let reader = ElementReader::from_path(path)?;
    reader.for_each(|el| match el {
        Element::Node(n) if needed.contains(&n.id()) => {
            coords.insert(n.id(), (n.lat(), n.lon()));
        }
        Element::DenseNode(n) if needed.contains(&n.id) => {
            coords.insert(n.id, (n.lat(), n.lon()));
        }
        _ => {}
    })?;

    let mut children = children_nodes;
    for (way_id, refs, name, category) in way_refs {
        let mut sum_lat = 0.0;
        let mut sum_lon = 0.0;
        let mut n = 0usize;
        for id in refs {
            if let Some((lat, lon)) = coords.get(&id) {
                sum_lat += *lat;
                sum_lon += *lon;
                n += 1;
            }
        }
        if n == 0 {
            continue;
        }
        children.push(ChildrenCentroid {
            osm_id: way_id,
            lat: sum_lat / n as f64,
            lon: sum_lon / n as f64,
            name,
            category,
        });
    }
    Ok((children, bumps))
}

fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let rlat1 = lat1.to_radians();
    let rlat2 = lat2.to_radians();
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let h = (dlat / 2.0).sin().powi(2) + rlat1.cos() * rlat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * 6_378_100.0 * h.sqrt().asin()
}

fn bearing_deg(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let rlat1 = lat1.to_radians();
    let rlat2 = lat2.to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let y = dlon.sin() * rlat2.cos();
    let x = rlat1.cos() * rlat2.sin() - rlat1.sin() * rlat2.cos() * dlon.cos();
    (y.atan2(x).to_degrees() + 360.0) % 360.0
}

fn angle_diff_deg(a: f64, b: f64) -> f64 {
    let mut d = (a - b).abs() % 360.0;
    if d > 180.0 {
        d = 360.0 - d;
    }
    d
}

/// Distance when target is within the cone (and heading fan when heading is set).
pub fn in_live_cone(
    lat: f64,
    lon: f64,
    heading_deg: Option<f64>,
    tlat: f64,
    tlon: f64,
) -> Option<f64> {
    let d = haversine_m(lat, lon, tlat, tlon);
    if !d.is_finite() || d > LIVE_HAZARD_CONE_M || d <= APPROACH_HIDE_M {
        return None;
    }
    if let Some(heading) = heading_deg.filter(|h| h.is_finite()) {
        let br = bearing_deg(lat, lon, tlat, tlon);
        if angle_diff_deg(heading, br) > LIVE_HAZARD_CONE_HALF_WIDTH_DEG {
            return None;
        }
    }
    Some(d)
}

fn phase_for_distance(distance_m: f64) -> ApproachPhase {
    let appear = APPROACH_APPEAR_M.min(LIVE_HAZARD_CONE_M);
    if !distance_m.is_finite() || distance_m > appear || distance_m <= APPROACH_HIDE_M {
        ApproachPhase::Hidden
    } else if distance_m <= APPROACH_URGENCY_M {
        ApproachPhase::Urgency
    } else {
        ApproachPhase::Appear
    }
}

fn approach_priority(code: &str, phase: ApproachPhase) -> (u8, u8) {
    let phase_rank = match phase {
        ApproachPhase::Urgency => 0,
        ApproachPhase::Appear => 1,
        ApproachPhase::Hidden => 2,
    };
    let kind_rank = if code.starts_with("362") || code.starts_with("364") || code.starts_with("366")
    {
        2
    } else if code.starts_with('1') {
        0
    } else {
        1
    };
    (phase_rank, kind_rank)
}

fn phase_str(phase: ApproachPhase) -> &'static str {
    match phase {
        ApproachPhase::Hidden => "hidden",
        ApproachPhase::Appear => "appear",
        ApproachPhase::Urgency => "urgency",
    }
}

pub fn road_sign_warning_json(w: &RoadSignWarning) -> String {
    serde_json::json!({
        "phase": phase_str(w.phase),
        "distance_m": w.distance_m,
        "icon_key": w.icon_key,
        "code": w.code,
        "name_en": w.name_en,
        "label": w.label,
    })
    .to_string()
}

/// Merge order: tagged `142` > children proximity > other signs / bumps.
pub fn nearest_live_sign_style_warning(
    index: &LiveHazardIndex,
    lat: f64,
    lon: f64,
    heading_deg: Option<f64>,
) -> Option<RoadSignWarning> {
    let (sign, children) = live_sign_and_children(index, lat, lon, heading_deg);
    match (&sign, &children) {
        (Some(s), _) if s.code == "142" => sign,
        (_, Some((c, _))) => Some(c.clone()),
        (Some(_), None) => sign,
        (None, None) => None,
    }
}

pub fn live_sign_and_children(
    index: &LiveHazardIndex,
    lat: f64,
    lon: f64,
    heading_deg: Option<f64>,
) -> (
    Option<RoadSignWarning>,
    Option<(RoadSignWarning, &'static str)>,
) {
    let norway = resolve_road_sign_jurisdiction_at(lat, lon) == RoadSignJurisdiction::Norway;
    let mut best: Option<RoadSignWarning> = None;
    let mut best_key = (u8::MAX, u8::MAX, f64::INFINITY);

    if norway {
        for sign in &index.signs {
            let Some(d) = in_live_cone(lat, lon, heading_deg, sign.lat, sign.lon) else {
                continue;
            };
            let phase = phase_for_distance(d);
            if phase == ApproachPhase::Hidden {
                continue;
            }
            let (phase_rank, kind_rank) = approach_priority(&sign.code, phase);
            let key = (phase_rank, kind_rank, d);
            if key < best_key {
                best_key = key;
                best = Some(RoadSignWarning {
                    phase,
                    distance_m: d,
                    icon_key: sign.icon_key.clone(),
                    code: sign.code.clone(),
                    name_en: sign.name_en.clone(),
                    label: sign.name_en.clone(),
                });
            }
        }
        for bump in &index.bumps {
            let Some(d) = in_live_cone(lat, lon, heading_deg, bump.lat, bump.lon) else {
                continue;
            };
            let phase = phase_for_distance(d);
            if phase == ApproachPhase::Hidden {
                continue;
            }
            let code = "109";
            let (phase_rank, kind_rank) = approach_priority(code, phase);
            let key = (phase_rank, kind_rank, d);
            if key < best_key {
                best_key = key;
                best = Some(RoadSignWarning {
                    phase,
                    distance_m: d,
                    icon_key: "no_sign_109".into(),
                    code: code.into(),
                    name_en: "Speed hump".into(),
                    label: "Speed hump".into(),
                });
            }
        }
    }

    (
        best,
        nearest_live_children_warning(index, lat, lon, heading_deg),
    )
}

pub fn nearest_live_children_warning(
    index: &LiveHazardIndex,
    lat: f64,
    lon: f64,
    heading_deg: Option<f64>,
) -> Option<(RoadSignWarning, &'static str)> {
    let mut best: Option<(&ChildrenCentroid, f64)> = None;
    for c in &index.children {
        let Some(d) = in_live_cone(lat, lon, heading_deg, c.lat, c.lon) else {
            continue;
        };
        match &best {
            Some((_, bd)) if d >= *bd => {}
            _ => best = Some((c, d)),
        }
    }
    let (c, d) = best?;
    let phase = phase_for_distance(d);
    if phase == ApproachPhase::Hidden {
        return None;
    }
    let label = if c.name.trim().is_empty() {
        "Children ahead".into()
    } else {
        format!("Children zone: {}", c.name.trim())
    };
    Some((
        RoadSignWarning {
            phase,
            distance_m: d,
            icon_key: "no_sign_142".into(),
            code: "142".into(),
            name_en: "Children".into(),
            label,
        },
        c.category,
    ))
}

pub fn nearest_live_speed_camera_warning(
    index: &LiveHazardIndex,
    lat: f64,
    lon: f64,
    heading_deg: Option<f64>,
    opted_in: bool,
) -> Option<SpeedCameraWarning> {
    if !opted_in {
        return None;
    }
    if resolve_speed_camera_jurisdiction_at(lat, lon) != SpeedCameraJurisdiction::AllowedOptIn {
        return None;
    }
    let in_cone: Vec<SpeedCameraRecord> = index
        .cameras
        .iter()
        .filter(|c| in_live_cone(lat, lon, heading_deg, c.lat, c.lon).is_some())
        .cloned()
        .collect();
    if in_cone.is_empty() {
        return None;
    }
    nearest_speed_camera_warning(&in_cone, lat, lon, opted_in, None)
}

#[derive(Debug, Clone)]
pub struct LiveSpeedLimitCone {
    pub distance_m: f64,
    pub speed_limit_kmh: f64,
    pub highway: Option<String>,
    pub maxspeed_posted: bool,
}

/// Forward-looking limit on the idle cell graph within the 300 m cone.
pub fn live_speed_limit_in_cone(
    graph: &RouteGraph,
    lat: f64,
    lon: f64,
    heading_deg: Option<f64>,
) -> Option<LiveSpeedLimitCone> {
    let mut best: Option<LiveSpeedLimitCone> = None;
    for e in &graph.edges {
        let d = edge_distance_m(e, lat, lon);
        if d > LIVE_HAZARD_CONE_M || d <= APPROACH_HIDE_M {
            continue;
        }
        let end_ok = in_live_cone(lat, lon, heading_deg, e.end_lat, e.end_lon).is_some();
        let start_ok = in_live_cone(lat, lon, heading_deg, e.start_lat, e.start_lon).is_some();
        if heading_deg.is_some() && !end_ok && !start_ok {
            continue;
        }
        let limit = applicable_limit_or_fallback_kmh(
            e.maxspeed_kmh,
            e.maxspeed_conditional.as_deref(),
            e.highway.as_deref(),
            None,
        );
        if !limit.is_finite() || limit <= 0.0 {
            continue;
        }
        let posted = e
            .maxspeed_kmh
            .filter(|v| v.is_finite() && *v > 0.0)
            .is_some();
        let cand = LiveSpeedLimitCone {
            distance_m: d,
            speed_limit_kmh: limit,
            highway: e.highway.clone(),
            maxspeed_posted: posted,
        };
        match &best {
            None => best = Some(cand),
            Some(b) if d < b.distance_m => best = Some(cand),
            _ => {}
        }
    }
    best
}

pub fn speed_limit_cone_as_sign_warning(
    hit: &LiveSpeedLimitCone,
    current_limit_kmh: Option<f64>,
) -> Option<RoadSignWarning> {
    if let Some(cur) = current_limit_kmh {
        if (cur - hit.speed_limit_kmh).abs() < 0.5 {
            return None;
        }
    }
    let phase = phase_for_distance(hit.distance_m);
    if phase == ApproachPhase::Hidden {
        return None;
    }
    let rounded = hit.speed_limit_kmh.round() as i32;
    let (code, icon_key) = match rounded {
        20 => ("362.20", "no_sign_362_20"),
        30 => ("362.30", "no_sign_362_30"),
        40 => ("362.40", "no_sign_362_40"),
        50 => ("362.50", "no_sign_362_50"),
        60 => ("362.60", "no_sign_362_60"),
        70 => ("362.70", "no_sign_362_70"),
        80 => ("362.80", "no_sign_362_80"),
        90 => ("362.90", "no_sign_362_90"),
        100 => ("362.100", "no_sign_362_100"),
        110 => ("362.110", "no_sign_362_110"),
        other => {
            // Snap to the nearest plate we ship an SVG for (never emit a
            // missing `no_sign_362` key — that falls through to unknown.svg).
            const PLATES: &[(i32, &str, &str)] = &[
                (20, "362.20", "no_sign_362_20"),
                (30, "362.30", "no_sign_362_30"),
                (40, "362.40", "no_sign_362_40"),
                (50, "362.50", "no_sign_362_50"),
                (60, "362.60", "no_sign_362_60"),
                (70, "362.70", "no_sign_362_70"),
                (80, "362.80", "no_sign_362_80"),
                (90, "362.90", "no_sign_362_90"),
                (100, "362.100", "no_sign_362_100"),
                (110, "362.110", "no_sign_362_110"),
            ];
            let (_, code, key) = PLATES
                .iter()
                .min_by_key(|(kmh, _, _)| (kmh - other).unsigned_abs())
                .copied()
                .unwrap_or((50, "362.50", "no_sign_362_50"));
            (code, key)
        }
    };
    Some(RoadSignWarning {
        phase,
        distance_m: hit.distance_m,
        icon_key: icon_key.into(),
        code: code.into(),
        name_en: format!("Speed limit {rounded}"),
        label: format!("Speed limit {rounded} km/h"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cone_rejects_behind_and_beyond_radius() {
        let lat = 60.68;
        let lon = 11.34;
        let north = in_live_cone(lat, lon, Some(0.0), lat + 0.0018, lon);
        assert!(north.is_some());
        let behind = in_live_cone(lat, lon, Some(180.0), lat + 0.0018, lon);
        assert!(behind.is_none());
        let far = in_live_cone(lat, lon, Some(0.0), lat + 0.0036, lon);
        assert!(far.is_none());
    }

    #[test]
    fn cell_key_stable_inside_cell() {
        let a = live_hazard_cell_key(60.6808, 11.3454);
        let b = live_hazard_cell_key(60.6810, 11.3456);
        assert_eq!(a, b);
    }

    #[test]
    fn speed_limit_20_maps_to_standin_icon() {
        let hit = LiveSpeedLimitCone {
            distance_m: 58.0,
            speed_limit_kmh: 20.0,
            highway: Some("residential".into()),
            maxspeed_posted: true,
        };
        let w = speed_limit_cone_as_sign_warning(&hit, Some(50.0)).expect("warning");
        assert_eq!(w.icon_key, "no_sign_362_20");
        assert_eq!(w.code, "362.20");
        assert!(w.label.contains("20"));
    }
}
