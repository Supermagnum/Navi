//! Speed-camera display / warning support (no avoid-routing toggle).
//!
//! Jurisdiction gating mirrors EC561 / allemannsretten: opt-in where allowed,
//! decline-by-default elsewhere. Point cameras use approach-instruction distance
//! phases; average-speed (section control) uses a distinct zone enter/exit UX.

use std::collections::HashMap;
use std::path::Path;

use chrono::{Local, NaiveDateTime};
use osmpbf::{Element, ElementReader, RelMemberType};

use crate::nav::{ApproachPhase, APPROACH_APPEAR_M, APPROACH_HIDE_M, APPROACH_URGENCY_M};
use crate::routing::conditional::{
    conditional_maxspeed_kmh_at, departure_or_now, extract_oh_condition, oh_condition_matches_at,
};
use crate::routing::elevation::country_iso_at;
use crate::routing::eta::parse_maxspeed_kmh;

/// ISO codes where speed-camera display may be offered (opt-in).
/// Norway and UK allow with OSM-sourced caveats; DE/FR/CH and unknown decline.
const SPEED_CAMERA_ALLOWED_ISO: &[&str] = &["no", "gb"];

/// Explicit decline jurisdictions (documented product table).
const SPEED_CAMERA_DECLINE_ISO: &[&str] = &["de", "fr", "ch"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeedCameraJurisdiction {
    /// Allowed after first-run opt-in (NO, UK).
    AllowedOptIn,
    /// Icons and warnings must not appear.
    Declined,
}

pub fn resolve_speed_camera_jurisdiction_at(lat: f64, lon: f64) -> SpeedCameraJurisdiction {
    match country_iso_at(lat, lon) {
        Some(code) if SPEED_CAMERA_ALLOWED_ISO.contains(&code) => {
            SpeedCameraJurisdiction::AllowedOptIn
        }
        Some(code) if SPEED_CAMERA_DECLINE_ISO.contains(&code) => SpeedCameraJurisdiction::Declined,
        Some(_) | None => SpeedCameraJurisdiction::Declined,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeedCameraKind {
    Point,
    AverageSpeed,
}

#[derive(Debug, Clone)]
pub struct SpeedCameraRecord {
    pub osm_id: i64,
    pub lat: f64,
    pub lon: f64,
    pub kind: SpeedCameraKind,
    /// Relation maxspeed, else device, else nearest-way (filled at ingest when known).
    pub maxspeed_kmh: Option<f64>,
    pub maxspeed_conditional: Option<String>,
    /// For average-speed zones: paired from/to endpoints when known.
    pub zone_from_lat: Option<f64>,
    pub zone_from_lon: Option<f64>,
    pub zone_to_lat: Option<f64>,
    pub zone_to_lon: Option<f64>,
    pub zone_length_m: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct SpeedCameraWarning {
    pub kind: SpeedCameraKind,
    pub phase: ApproachPhase,
    pub distance_m: f64,
    pub applicable_limit_kmh: Option<f64>,
    /// Average-speed only: remaining distance in zone (m), when computable.
    pub zone_remaining_m: Option<f64>,
    /// Average-speed only: time budget at the required average (seconds), when computable.
    pub zone_time_budget_s: Option<f64>,
    pub label: String,
}

fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let rlat1 = lat1.to_radians();
    let rlat2 = lat2.to_radians();
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let h = (dlat / 2.0).sin().powi(2) + rlat1.cos() * rlat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * 6_378_100.0 * h.sqrt().asin()
}

fn phase_for_distance(distance_m: f64) -> ApproachPhase {
    if !distance_m.is_finite() || distance_m > APPROACH_APPEAR_M || distance_m <= APPROACH_HIDE_M {
        ApproachPhase::Hidden
    } else if distance_m <= APPROACH_URGENCY_M {
        ApproachPhase::Urgency
    } else {
        ApproachPhase::Appear
    }
}

/// Resolve currently-applicable limit: relation/device base, overlaying conditional.
pub fn applicable_limit_kmh(
    base: Option<f64>,
    conditional: Option<&str>,
    at: Option<NaiveDateTime>,
) -> Option<f64> {
    let dt = departure_or_now(at);
    if let Some(raw) = conditional {
        if let Some(v) = conditional_maxspeed_kmh_at(raw, dt) {
            return Some(v);
        }
    }
    base
}

/// Live posted/applicable speed limit (km/h) for a road edge: evaluate
/// `maxspeed:conditional` at `at` (or now), else base `maxspeed`, else the
/// pre-departure ETA highway-class fallback table.
pub fn applicable_limit_or_fallback_kmh(
    base: Option<f64>,
    conditional: Option<&str>,
    highway: Option<&str>,
    at: Option<NaiveDateTime>,
) -> f64 {
    applicable_limit_kmh(base, conditional, at)
        .filter(|v| v.is_finite() && *v > 0.0)
        .unwrap_or_else(|| crate::routing::eta::highway_fallback_kmh(highway))
}

/// Index speed cameras from a PBF: `highway=speed_camera` nodes plus
/// `type=enforcement` relations (`maxspeed` / `average_speed`).
pub fn load_speed_cameras_from_pbf(
    path: impl AsRef<Path>,
) -> anyhow::Result<Vec<SpeedCameraRecord>> {
    let path = path.as_ref();
    let mut node_coords: HashMap<i64, (f64, f64)> = HashMap::new();
    let mut node_tags: HashMap<i64, HashMap<String, String>> = HashMap::new();
    let mut way_maxspeed: HashMap<i64, (Option<f64>, Option<String>)> = HashMap::new();
    let mut way_nodes: HashMap<i64, Vec<i64>> = HashMap::new();

    // Pass 1: nodes + ways (coords / maxspeed for nearest-way fallback).
    let reader = ElementReader::from_path(path)?;
    reader.for_each(|el| match el {
        Element::Node(n) => {
            let id = n.id();
            node_coords.insert(id, (n.lat(), n.lon()));
            let tags: HashMap<String, String> =
                n.tags().map(|(k, v)| (k.into(), v.into())).collect();
            if !tags.is_empty() {
                node_tags.insert(id, tags);
            }
        }
        Element::DenseNode(n) => {
            let id = n.id;
            node_coords.insert(id, (n.lat(), n.lon()));
            let tags: HashMap<String, String> =
                n.tags().map(|(k, v)| (k.into(), v.into())).collect();
            if !tags.is_empty() {
                node_tags.insert(id, tags);
            }
        }
        Element::Way(w) => {
            let tags: HashMap<String, String> =
                w.tags().map(|(k, v)| (k.into(), v.into())).collect();
            if tags.contains_key("highway") {
                let base = tags.get("maxspeed").and_then(|v| parse_maxspeed_kmh(v));
                let cond = tags.get("maxspeed:conditional").cloned();
                way_maxspeed.insert(w.id(), (base, cond));
                way_nodes.insert(w.id(), w.refs().collect());
            }
        }
        _ => {}
    })?;

    let mut out: Vec<SpeedCameraRecord> = Vec::new();
    let mut seen_device: HashMap<i64, usize> = HashMap::new();

    for (id, tags) in &node_tags {
        if tags.get("highway").map(String::as_str) != Some("speed_camera") {
            continue;
        }
        let Some(&(lat, lon)) = node_coords.get(id) else {
            continue;
        };
        let maxspeed_kmh = tags.get("maxspeed").and_then(|v| parse_maxspeed_kmh(v));
        let maxspeed_conditional = tags.get("maxspeed:conditional").cloned();
        let rec = SpeedCameraRecord {
            osm_id: *id,
            lat,
            lon,
            kind: SpeedCameraKind::Point,
            maxspeed_kmh,
            maxspeed_conditional,
            zone_from_lat: None,
            zone_from_lon: None,
            zone_to_lat: None,
            zone_to_lon: None,
            zone_length_m: None,
        };
        seen_device.insert(*id, out.len());
        out.push(rec);
    }

    // Pass 2: enforcement relations.
    let reader = ElementReader::from_path(path)?;
    reader.for_each(|el| {
        let Element::Relation(rel) = el else {
            return;
        };
        let tags: HashMap<String, String> = rel.tags().map(|(k, v)| (k.into(), v.into())).collect();
        if tags.get("type").map(String::as_str) != Some("enforcement") {
            return;
        }
        let enf = tags.get("enforcement").map(String::as_str).unwrap_or("");
        let kind = match enf {
            "maxspeed" => SpeedCameraKind::Point,
            "average_speed" => SpeedCameraKind::AverageSpeed,
            _ => return,
        };
        let rel_max = tags.get("maxspeed").and_then(|v| parse_maxspeed_kmh(v));
        let rel_cond = tags.get("maxspeed:conditional").cloned();

        let mut device: Option<i64> = None;
        let mut from_n: Option<i64> = None;
        let mut to_n: Option<i64> = None;
        for m in rel.members() {
            if m.member_type != RelMemberType::Node {
                continue;
            }
            let Ok(role) = m.role() else {
                continue;
            };
            match role {
                "device" => device = Some(m.member_id),
                "from" => from_n = Some(m.member_id),
                "to" => to_n = Some(m.member_id),
                _ => {}
            }
        }

        let device_id = device.or(from_n);
        let Some(did) = device_id else {
            return;
        };
        let Some(&(lat, lon)) = node_coords.get(&did) else {
            return;
        };

        let device_tags = node_tags.get(&did);
        let device_max = device_tags
            .and_then(|t| t.get("maxspeed"))
            .and_then(|v| parse_maxspeed_kmh(v));
        let device_cond = device_tags
            .and_then(|t| t.get("maxspeed:conditional"))
            .cloned();

        // Nearest-way maxspeed fallback (coarse: any way containing the device node).
        let mut way_max = None;
        let mut way_cond = None;
        for (wid, nodes) in &way_nodes {
            if nodes.contains(&did) {
                if let Some((b, c)) = way_maxspeed.get(wid) {
                    way_max = *b;
                    way_cond = c.clone();
                    break;
                }
            }
        }

        let maxspeed_kmh = rel_max.or(device_max).or(way_max);
        let maxspeed_conditional = rel_cond.or(device_cond).or(way_cond);

        let (zone_from_lat, zone_from_lon, zone_to_lat, zone_to_lon, zone_length_m) =
            if kind == SpeedCameraKind::AverageSpeed {
                let f = from_n.and_then(|id| node_coords.get(&id).copied());
                let t = to_n.and_then(|id| node_coords.get(&id).copied());
                match (f, t) {
                    (Some((flat, flon)), Some((tlat, tlon))) => (
                        Some(flat),
                        Some(flon),
                        Some(tlat),
                        Some(tlon),
                        Some(haversine_m(flat, flon, tlat, tlon)),
                    ),
                    _ => (None, None, None, None, None),
                }
            } else {
                (None, None, None, None, None)
            };

        if let Some(&idx) = seen_device.get(&did) {
            let rec = &mut out[idx];
            if rec.maxspeed_kmh.is_none() {
                rec.maxspeed_kmh = maxspeed_kmh;
            }
            if rec.maxspeed_conditional.is_none() {
                rec.maxspeed_conditional = maxspeed_conditional.clone();
            }
            if kind == SpeedCameraKind::AverageSpeed {
                rec.kind = SpeedCameraKind::AverageSpeed;
                rec.zone_from_lat = zone_from_lat;
                rec.zone_from_lon = zone_from_lon;
                rec.zone_to_lat = zone_to_lat;
                rec.zone_to_lon = zone_to_lon;
                rec.zone_length_m = zone_length_m;
            }
        } else {
            let rec = SpeedCameraRecord {
                osm_id: did,
                lat,
                lon,
                kind,
                maxspeed_kmh,
                maxspeed_conditional,
                zone_from_lat,
                zone_from_lon,
                zone_to_lat,
                zone_to_lon,
                zone_length_m,
            };
            seen_device.insert(did, out.len());
            out.push(rec);
        }
    })?;

    Ok(out)
}

/// Pick the nearest relevant warning for the driver's position.
///
/// Point cameras: approach phases to the device.
/// Average-speed: zone enter (approach to `from`) or in-zone remaining budget.
pub fn nearest_speed_camera_warning(
    cameras: &[SpeedCameraRecord],
    lat: f64,
    lon: f64,
    opted_in: bool,
    at: Option<NaiveDateTime>,
) -> Option<SpeedCameraWarning> {
    if !opted_in {
        return None;
    }
    if resolve_speed_camera_jurisdiction_at(lat, lon) != SpeedCameraJurisdiction::AllowedOptIn {
        return None;
    }
    let now = at.unwrap_or_else(|| Local::now().naive_local());

    let mut best: Option<SpeedCameraWarning> = None;
    let mut best_d = f64::INFINITY;

    for cam in cameras {
        match cam.kind {
            SpeedCameraKind::Point => {
                let d = haversine_m(lat, lon, cam.lat, cam.lon);
                let phase = phase_for_distance(d);
                if phase == ApproachPhase::Hidden {
                    continue;
                }
                if d < best_d {
                    best_d = d;
                    let limit = applicable_limit_kmh(
                        cam.maxspeed_kmh,
                        cam.maxspeed_conditional.as_deref(),
                        Some(now),
                    );
                    best = Some(SpeedCameraWarning {
                        kind: SpeedCameraKind::Point,
                        phase,
                        distance_m: d,
                        applicable_limit_kmh: limit,
                        zone_remaining_m: None,
                        zone_time_budget_s: None,
                        label: match limit {
                            Some(v) => format!("Speed camera {v:.0} km/h"),
                            None => "Speed camera".into(),
                        },
                    });
                }
            }
            SpeedCameraKind::AverageSpeed => {
                let (enter_lat, enter_lon) = match (cam.zone_from_lat, cam.zone_from_lon) {
                    (Some(a), Some(b)) => (a, b),
                    _ => (cam.lat, cam.lon),
                };
                let (exit_lat, exit_lon) = match (cam.zone_to_lat, cam.zone_to_lon) {
                    (Some(a), Some(b)) => (a, b),
                    _ => continue,
                };
                let d_enter = haversine_m(lat, lon, enter_lat, enter_lon);
                let d_exit = haversine_m(lat, lon, exit_lat, exit_lon);
                let zone_len = cam
                    .zone_length_m
                    .unwrap_or_else(|| haversine_m(enter_lat, enter_lon, exit_lat, exit_lon));
                let limit = applicable_limit_kmh(
                    cam.maxspeed_kmh,
                    cam.maxspeed_conditional.as_deref(),
                    Some(now),
                );

                // Inside zone: closer to exit than (enter + small slack) and within length band.
                let inside = d_enter + d_exit <= zone_len * 1.25 && d_exit < zone_len;
                if inside {
                    let remaining = d_exit;
                    let budget_s = limit.map(|kmh| {
                        let mps = (kmh / 3.6).max(1.0);
                        remaining / mps
                    });
                    // Prefer in-zone state over distant point cameras.
                    let score = remaining;
                    if score < best_d {
                        best_d = score;
                        best = Some(SpeedCameraWarning {
                            kind: SpeedCameraKind::AverageSpeed,
                            phase: ApproachPhase::Urgency,
                            distance_m: remaining,
                            applicable_limit_kmh: limit,
                            zone_remaining_m: Some(remaining),
                            zone_time_budget_s: budget_s,
                            label: match limit {
                                Some(v) => format!("Average-speed zone {v:.0} km/h"),
                                None => "Average-speed zone".into(),
                            },
                        });
                    }
                } else {
                    let phase = phase_for_distance(d_enter);
                    if phase == ApproachPhase::Hidden {
                        continue;
                    }
                    if d_enter < best_d {
                        best_d = d_enter;
                        best = Some(SpeedCameraWarning {
                            kind: SpeedCameraKind::AverageSpeed,
                            phase,
                            distance_m: d_enter,
                            applicable_limit_kmh: limit,
                            zone_remaining_m: Some(zone_len),
                            zone_time_budget_s: limit.map(|kmh| {
                                let mps = (kmh / 3.6).max(1.0);
                                zone_len / mps
                            }),
                            label: match limit {
                                Some(v) => format!("Entering average-speed zone {v:.0} km/h"),
                                None => "Entering average-speed zone".into(),
                            },
                        });
                    }
                }
            }
        }
    }
    best
}

/// True when a maxspeed conditional window is active (for tests / diagnostics).
pub fn conditional_window_active(raw: &str, at: NaiveDateTime) -> bool {
    let Some(cond) = extract_oh_condition(raw) else {
        return false;
    };
    oh_condition_matches_at(cond, at).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn norway_allows_opt_in() {
        assert_eq!(
            resolve_speed_camera_jurisdiction_at(59.91, 10.75),
            SpeedCameraJurisdiction::AllowedOptIn
        );
    }

    #[test]
    fn uk_allows_opt_in() {
        assert_eq!(
            resolve_speed_camera_jurisdiction_at(51.5074, -0.1278),
            SpeedCameraJurisdiction::AllowedOptIn
        );
    }

    #[test]
    fn germany_france_switzerland_decline() {
        assert_eq!(
            resolve_speed_camera_jurisdiction_at(52.52, 13.405),
            SpeedCameraJurisdiction::Declined
        );
        assert_eq!(
            resolve_speed_camera_jurisdiction_at(48.8566, 2.3522),
            SpeedCameraJurisdiction::Declined
        );
        assert_eq!(
            resolve_speed_camera_jurisdiction_at(46.948, 7.4474),
            SpeedCameraJurisdiction::Declined
        );
    }

    #[test]
    fn unknown_ocean_declines() {
        assert_eq!(
            resolve_speed_camera_jurisdiction_at(35.0, -40.0),
            SpeedCameraJurisdiction::Declined
        );
    }

    #[test]
    fn warning_requires_opt_in() {
        let cams = vec![SpeedCameraRecord {
            osm_id: 1,
            lat: 59.91,
            lon: 10.75,
            kind: SpeedCameraKind::Point,
            maxspeed_kmh: Some(60.0),
            maxspeed_conditional: None,
            zone_from_lat: None,
            zone_from_lon: None,
            zone_to_lat: None,
            zone_to_lon: None,
            zone_length_m: None,
        }];
        assert!(nearest_speed_camera_warning(&cams, 59.91, 10.75, false, None).is_none());
        let w = nearest_speed_camera_warning(&cams, 59.9105, 10.75, true, None).unwrap();
        assert_eq!(w.kind, SpeedCameraKind::Point);
        assert_eq!(w.applicable_limit_kmh, Some(60.0));
    }
}
