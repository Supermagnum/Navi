//! Speed-camera display / warning support (no avoid-routing toggle).
//!
//! Jurisdiction gating mirrors EC561 / allemannsretten: opt-in where allowed,
//! decline-by-default elsewhere. Point cameras use approach-instruction distance
//! phases; average-speed (section control) uses a distinct zone enter/exit UX.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use chrono::{Local, NaiveDateTime};
use log::warn;
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

/// Hard cap on node ids retained while indexing speed cameras from one extract.
/// Regional extracts have thousands of cameras at most; anything near this bound
/// indicates corrupt input or a logic bug — refuse rather than allocate multi-GB.
const SPEED_CAMERA_INDEX_CAP: usize = 100_000;

struct PendingEnforcement {
    kind: SpeedCameraKind,
    rel_max: Option<f64>,
    rel_cond: Option<String>,
    device: Option<i64>,
    from_n: Option<i64>,
    to_n: Option<i64>,
}

fn ensure_speed_camera_index_cap(label: &str, n: usize) -> anyhow::Result<()> {
    if n > SPEED_CAMERA_INDEX_CAP {
        warn!(
            target: "speed_camera",
            "{label} count {n} exceeds cap {SPEED_CAMERA_INDEX_CAP}"
        );
        anyhow::bail!(
            "speed camera index refused: {label} count {n} exceeds cap {SPEED_CAMERA_INDEX_CAP}"
        );
    }
    Ok(())
}

/// Index speed cameras from a PBF: `highway=speed_camera` nodes plus
/// `type=enforcement` relations (`maxspeed` / `average_speed`).
///
/// Two-pass: collect only camera / enforcement member ids first, then resolve
/// coordinates for that set. Does **not** materialize every node or highway way
/// in the extract (that previously drove multi-GB HashMap growth).
///
/// Nearest-way maxspeed fallback is omitted for v1: limits come from the
/// relation and/or the device node tags only. HUD still works without a limit
/// (`"Speed camera"` label); restoring way-based fill needs a camera-local
/// index, not a full-extract `way_nodes` map.
pub fn load_speed_cameras_from_pbf(
    path: impl AsRef<Path>,
) -> anyhow::Result<Vec<SpeedCameraRecord>> {
    let path = path.as_ref();
    let mut out: Vec<SpeedCameraRecord> = Vec::new();
    let mut seen_device: HashMap<i64, usize> = HashMap::new();
    let mut needed: HashSet<i64> = HashSet::new();
    let mut pending: Vec<PendingEnforcement> = Vec::new();
    // Device-node maxspeed tags from pass-1 cameras; pass 2 fills relation-only devices.
    let mut node_tags: HashMap<i64, HashMap<String, String>> = HashMap::new();
    let mut node_coords: HashMap<i64, (f64, f64)> = HashMap::new();

    // Pass 1: speed_camera nodes + enforcement relations (member ids only).
    let reader = ElementReader::from_path(path)?;
    reader.for_each(|el| match el {
        Element::Node(n) => {
            let mut is_cam = false;
            let mut tags: HashMap<String, String> = HashMap::new();
            for (k, v) in n.tags() {
                if k == "highway" && v == "speed_camera" {
                    is_cam = true;
                }
                if k == "highway" || k == "maxspeed" || k == "maxspeed:conditional" {
                    tags.insert(k.into(), v.into());
                }
            }
            if !is_cam {
                return;
            }
            let id = n.id();
            let lat = n.lat();
            let lon = n.lon();
            node_coords.insert(id, (lat, lon));
            if !tags.is_empty() {
                node_tags.insert(id, tags.clone());
            }
            let maxspeed_kmh = tags.get("maxspeed").and_then(|v| parse_maxspeed_kmh(v));
            let maxspeed_conditional = tags.get("maxspeed:conditional").cloned();
            seen_device.insert(id, out.len());
            out.push(SpeedCameraRecord {
                osm_id: id,
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
            });
            needed.insert(id);
        }
        Element::DenseNode(n) => {
            let mut is_cam = false;
            let mut tags: HashMap<String, String> = HashMap::new();
            for (k, v) in n.tags() {
                if k == "highway" && v == "speed_camera" {
                    is_cam = true;
                }
                if k == "highway" || k == "maxspeed" || k == "maxspeed:conditional" {
                    tags.insert(k.into(), v.into());
                }
            }
            if !is_cam {
                return;
            }
            let id = n.id;
            let lat = n.lat();
            let lon = n.lon();
            node_coords.insert(id, (lat, lon));
            if !tags.is_empty() {
                node_tags.insert(id, tags.clone());
            }
            let maxspeed_kmh = tags.get("maxspeed").and_then(|v| parse_maxspeed_kmh(v));
            let maxspeed_conditional = tags.get("maxspeed:conditional").cloned();
            seen_device.insert(id, out.len());
            out.push(SpeedCameraRecord {
                osm_id: id,
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
            });
            needed.insert(id);
        }
        Element::Relation(rel) => {
            let tags: HashMap<String, String> =
                rel.tags().map(|(k, v)| (k.into(), v.into())).collect();
            if tags.get("type").map(String::as_str) != Some("enforcement") {
                return;
            }
            let enf = tags.get("enforcement").map(String::as_str).unwrap_or("");
            let kind = match enf {
                "maxspeed" => SpeedCameraKind::Point,
                "average_speed" => SpeedCameraKind::AverageSpeed,
                _ => return,
            };
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
            for id in [device, from_n, to_n].into_iter().flatten() {
                needed.insert(id);
            }
            pending.push(PendingEnforcement {
                kind,
                rel_max: tags.get("maxspeed").and_then(|v| parse_maxspeed_kmh(v)),
                rel_cond: tags.get("maxspeed:conditional").cloned(),
                device,
                from_n,
                to_n,
            });
        }
        _ => {}
    })?;

    ensure_speed_camera_index_cap("output cameras", out.len())?;
    ensure_speed_camera_index_cap("needed node ids", needed.len())?;
    ensure_speed_camera_index_cap("pending enforcement relations", pending.len())?;

    // Pass 2: coordinates (+ sparse tags) only for ids referenced above.
    let mut coords: HashMap<i64, (f64, f64)> = HashMap::with_capacity(needed.len().max(16));
    coords.extend(node_coords.drain());
    let reader = ElementReader::from_path(path)?;
    reader.for_each(|el| match el {
        Element::Node(n) if needed.contains(&n.id()) => {
            let id = n.id();
            coords.entry(id).or_insert_with(|| (n.lat(), n.lon()));
            if let std::collections::hash_map::Entry::Vacant(e) = node_tags.entry(id) {
                let tags: HashMap<String, String> = n
                    .tags()
                    .filter(|(k, _)| {
                        *k == "maxspeed" || *k == "maxspeed:conditional" || *k == "highway"
                    })
                    .map(|(k, v)| (k.into(), v.into()))
                    .collect();
                if !tags.is_empty() {
                    e.insert(tags);
                }
            }
        }
        Element::DenseNode(n) if needed.contains(&n.id) => {
            let id = n.id;
            coords.entry(id).or_insert_with(|| (n.lat(), n.lon()));
            if let std::collections::hash_map::Entry::Vacant(e) = node_tags.entry(id) {
                let tags: HashMap<String, String> = n
                    .tags()
                    .filter(|(k, _)| {
                        *k == "maxspeed" || *k == "maxspeed:conditional" || *k == "highway"
                    })
                    .map(|(k, v)| (k.into(), v.into()))
                    .collect();
                if !tags.is_empty() {
                    e.insert(tags);
                }
            }
        }
        _ => {}
    })?;

    ensure_speed_camera_index_cap("resolved coords", coords.len())?;

    for pend in pending {
        let device_id = pend.device.or(pend.from_n);
        let Some(did) = device_id else {
            continue;
        };
        let Some(&(lat, lon)) = coords.get(&did) else {
            continue;
        };

        let device_tags = node_tags.get(&did);
        let device_max = device_tags
            .and_then(|t| t.get("maxspeed"))
            .and_then(|v| parse_maxspeed_kmh(v));
        let device_cond = device_tags
            .and_then(|t| t.get("maxspeed:conditional"))
            .cloned();

        // Relation then device tags only (no full-extract nearest-way scan).
        let maxspeed_kmh = pend.rel_max.or(device_max);
        let maxspeed_conditional = pend.rel_cond.or(device_cond);

        let (zone_from_lat, zone_from_lon, zone_to_lat, zone_to_lon, zone_length_m) =
            if pend.kind == SpeedCameraKind::AverageSpeed {
                let f = pend.from_n.and_then(|id| coords.get(&id).copied());
                let t = pend.to_n.and_then(|id| coords.get(&id).copied());
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
            if pend.kind == SpeedCameraKind::AverageSpeed {
                rec.kind = SpeedCameraKind::AverageSpeed;
                rec.zone_from_lat = zone_from_lat;
                rec.zone_from_lon = zone_from_lon;
                rec.zone_to_lat = zone_to_lat;
                rec.zone_to_lon = zone_to_lon;
                rec.zone_length_m = zone_length_m;
            }
        } else {
            seen_device.insert(did, out.len());
            out.push(SpeedCameraRecord {
                osm_id: did,
                lat,
                lon,
                kind: pend.kind,
                maxspeed_kmh,
                maxspeed_conditional,
                zone_from_lat,
                zone_from_lon,
                zone_to_lat,
                zone_to_lon,
                zone_length_m,
            });
        }
    }

    ensure_speed_camera_index_cap("final cameras", out.len())?;
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
