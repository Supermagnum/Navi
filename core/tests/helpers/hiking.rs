//! Hiking / DNT fixture helpers for ignored integration tests.
#![allow(dead_code, clippy::match_like_matches_macro)]

use std::collections::{HashMap, HashSet};
use std::path::Path;

use driver_break_core::config::{Profile, RestConfig, SafetyConfig};
use driver_break_core::poi::{PoiCategory, PoiRecord};
use driver_break_core::routing::graph::RouteGraph;
use driver_break_core::routing::rest::{max_daily_distance_km, next_break_distance_km, BreakKind};
use driver_break_core::routing::safety::check_overnight_candidate;
use osm4routing::{NodeId, Reader};
use osmpbf::{Element, ElementReader, RelMemberType};

use super::{haversine_m, CombinedPoiIndex};

const DNT_OPERATORS: &[&str] = &[
    "DNT",
    "STF",
    "DAV",
    "SAC",
    "OeAV",
    "Metsähallitus",
    "Metsahallitus",
];
const FORBIDDEN_HIGHWAYS: &[&str] = &["motorway", "trunk", "primary"];
const PRIORITY_HIGHWAYS: &[&str] = &["footway", "path", "steps", "bridleway"];
const NON_DNT_PENALTY: f64 = 2.5;
/// Hard avoidance for motorway/trunk/primary and military areas (hiking validation).
const FORBIDDEN_PENALTY: f64 = 10_000.0;
const PRIORITY_PATH_WARN_THRESHOLD: f64 = 0.50;
pub const OVERNIGHT_NEAR_HUT_MAX_M: f64 = 5_000.0;

#[derive(Debug, Clone)]
pub struct EdgeTagMap {
    pub tags: HashMap<String, HashMap<String, String>>,
    pub dnt_way_ids: HashSet<i64>,
}

fn load_dnt_relation_ways(path: &Path) -> anyhow::Result<HashSet<i64>> {
    let file = std::fs::File::open(path)?;
    let reader = ElementReader::new(file);
    let mut way_ids = HashSet::new();
    reader.for_each(|element| {
        if let Element::Relation(rel) = element {
            let tags: HashMap<String, String> = rel
                .tags()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            let is_dnt = tags.get("route") == Some(&"hiking".to_string())
                && (tags.get("network") == Some(&"lwn".to_string())
                    || tags
                        .get("operator")
                        .is_some_and(|op| DNT_OPERATORS.iter().any(|t| op.contains(t)))
                    || tags
                        .get("operator")
                        .is_some_and(|op| op.contains("Den Norske Turistforening")));
            if is_dnt {
                for member in rel.members() {
                    if member.member_type == RelMemberType::Way {
                        way_ids.insert(member.member_id);
                    }
                }
            }
        }
    })?;
    Ok(way_ids)
}

fn edge_way_id(edge_id: &str) -> Option<i64> {
    edge_id
        .strip_suffix("-rev")
        .unwrap_or(edge_id)
        .split('-')
        .next()
        .and_then(|s| s.parse().ok())
}

impl EdgeTagMap {
    pub fn load_from_pbf(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path_ref = path.as_ref();
        let path_owned = path_ref.to_path_buf();
        let path_owned2 = path_owned.clone();

        let (dnt_result, tags_result) = std::thread::scope(|s| {
            let dnt_handle = s.spawn(|| load_dnt_relation_ways(&path_owned));
            let tags_handle = s.spawn(|| {
                let (_, edges) = Reader::new()
                    .read_tag("highway")
                    .read_tag("network")
                    .read_tag("operator")
                    .read_tag("network:type")
                    .read_tag("route")
                    .read_tag("landuse")
                    .read_tag("military")
                    .read_tag("foot")
                    .read_tag("access")
                    .read(&path_owned2)
                    .map_err(|e| anyhow::anyhow!("edge tag read failed: {e}"))?;
                let mut tags = HashMap::new();
                for edge in edges {
                    let edge_tags: HashMap<String, String> = edge
                        .tags
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    tags.insert(edge.id, edge_tags);
                }
                Ok::<_, anyhow::Error>(tags)
            });
            (
                dnt_handle.join().expect("dnt thread"),
                tags_handle.join().expect("tags thread"),
            )
        });

        Ok(Self {
            tags: tags_result?,
            dnt_way_ids: dnt_result?,
        })
    }

    pub fn get(&self, edge_id: &str) -> Option<&HashMap<String, String>> {
        let base_id = edge_id.strip_suffix("-rev").unwrap_or(edge_id);
        self.tags.get(base_id)
    }

    pub fn is_dnt_edge(&self, edge_id: &str, tags: &HashMap<String, String>) -> bool {
        if is_dnt_tagged(tags) {
            return true;
        }
        edge_way_id(edge_id).is_some_and(|id| self.dnt_way_ids.contains(&id))
    }
}

#[derive(Debug, Clone, Default)]
pub struct RouteValidation {
    pub total_m: f64,
    pub dnt_m: f64,
    pub priority_m: f64,
    pub other_foot_m: f64,
    pub forbidden_segments: Vec<ForbiddenSegment>,
    pub low_priority_warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ForbiddenSegment {
    pub edge_id: String,
    pub highway: Option<String>,
    pub length_m: f64,
}

impl RouteValidation {
    pub fn dnt_pct(&self) -> f64 {
        if self.total_m <= 0.0 {
            0.0
        } else {
            self.dnt_m / self.total_m * 100.0
        }
    }

    pub fn priority_pct(&self) -> f64 {
        if self.total_m <= 0.0 {
            0.0
        } else {
            self.priority_m / self.total_m * 100.0
        }
    }

    pub fn other_foot_pct(&self) -> f64 {
        if self.total_m <= 0.0 {
            0.0
        } else {
            self.other_foot_m / self.total_m * 100.0
        }
    }
}

pub fn is_dnt_tagged(tags: &HashMap<String, String>) -> bool {
    if tags
        .get("network")
        .is_some_and(|n| n.eq_ignore_ascii_case("lwn"))
    {
        return true;
    }
    if tags
        .get("network:type")
        .is_some_and(|n| n.eq_ignore_ascii_case("dnt"))
    {
        return true;
    }
    if tags
        .get("operator")
        .is_some_and(|op| DNT_OPERATORS.iter().any(|t| op.contains(t)))
    {
        return true;
    }
    tags.get("route") == Some(&"hiking".to_string())
        && tags
            .get("network")
            .is_some_and(|n| n.eq_ignore_ascii_case("lwn"))
}

pub fn is_priority_path(tags: &HashMap<String, String>) -> bool {
    let highway = tags.get("highway").map(String::as_str);
    if highway.is_some_and(|h| PRIORITY_HIGHWAYS.contains(&h)) {
        return true;
    }
    if highway == Some("track") && foot_access_allowed(tags) {
        return true;
    }
    is_dnt_tagged(tags)
}

pub fn is_forbidden_segment(tags: &HashMap<String, String>) -> bool {
    if tags
        .get("highway")
        .is_some_and(|h| FORBIDDEN_HIGHWAYS.contains(&h.as_str()))
    {
        return true;
    }
    tags.get("landuse") == Some(&"military".to_string())
        || tags.get("military") == Some(&"danger_area".to_string())
}

fn foot_access_allowed(tags: &HashMap<String, String>) -> bool {
    match tags.get("foot").map(String::as_str) {
        Some("no") | Some("private") => false,
        Some("yes") | Some("designated") => true,
        _ => match tags.get("access").map(String::as_str) {
            Some("no") | Some("private") => false,
            _ => true,
        },
    }
}

pub fn apply_dnt_preference(graph: &mut RouteGraph, tag_map: &EdgeTagMap) {
    use rayon::prelude::*;
    let empty = HashMap::new();
    graph.edges.par_iter_mut().for_each(|edge| {
        let tags = tag_map.get(&edge.id).unwrap_or(&empty);
        let mut factor = 1.0;
        if is_forbidden_segment(tags) {
            factor *= FORBIDDEN_PENALTY;
        } else if !tag_map.is_dnt_edge(&edge.id, tags) {
            factor *= NON_DNT_PENALTY;
        }
        if factor != 1.0 {
            edge.base_weight *= factor;
            if let Some(ref mut eco) = edge.eco_weight {
                *eco *= factor;
            }
        }
    });
}

pub fn validate_route(
    graph: &RouteGraph,
    edge_indices: &[usize],
    tag_map: &EdgeTagMap,
) -> RouteValidation {
    let mut out = RouteValidation::default();
    let mut leg_m = 0.0;
    let mut leg_priority_m = 0.0;
    let mut leg_start_idx = 0usize;

    for (i, &idx) in edge_indices.iter().enumerate() {
        let edge = &graph.edges[idx];
        let tags = tag_map.get(&edge.id).cloned().unwrap_or_default();
        out.total_m += edge.length_m;

        if is_forbidden_segment(&tags) {
            out.forbidden_segments.push(ForbiddenSegment {
                edge_id: edge.id.clone(),
                highway: tags.get("highway").cloned(),
                length_m: edge.length_m,
            });
        }

        let dnt = tag_map.is_dnt_edge(&edge.id, &tags);
        if dnt {
            out.dnt_m += edge.length_m;
            out.priority_m += edge.length_m;
        } else if is_priority_path(&tags) {
            out.priority_m += edge.length_m;
            out.other_foot_m += edge.length_m;
        }

        leg_m += edge.length_m;
        if is_priority_path(&tags) || dnt {
            leg_priority_m += edge.length_m;
        }

        let leg_end = i == edge_indices.len() - 1
            || edge_indices.get(i + 1).map(|&n| {
                graph.edges[n].source != edge.target || graph.edges[n].target != edge.target
            }) == Some(false);

        if leg_end || i == edge_indices.len() - 1 {
            if leg_m > 1000.0 {
                let share = leg_priority_m / leg_m;
                if share < PRIORITY_PATH_WARN_THRESHOLD {
                    out.low_priority_warnings.push(format!(
                        "leg edges {leg_start_idx}-{i}: priority-path share {:.1}% below {:.0}%",
                        share * 100.0,
                        PRIORITY_PATH_WARN_THRESHOLD * 100.0
                    ));
                }
            }
            leg_m = 0.0;
            leg_priority_m = 0.0;
            leg_start_idx = i + 1;
        }
    }

    out
}

pub fn chain_paths(mut a: Vec<NodeId>, b: &[NodeId]) -> Vec<NodeId> {
    if a.is_empty() {
        return b.to_vec();
    }
    a.extend(b.iter().skip(1));
    a
}

#[derive(Debug, Clone)]
pub struct RouteSample {
    pub lat: f64,
    pub lon: f64,
    pub cumulative_km: f64,
    pub node_id: Option<NodeId>,
}

pub fn build_route_samples(graph: &RouteGraph, path: &[NodeId]) -> Vec<RouteSample> {
    let mut out = Vec::new();
    let mut cumulative_m = 0.0;

    if let Some(&first) = path.first() {
        if let Some(n) = graph.nodes.get(&first) {
            out.push(RouteSample {
                lat: n.coord.y,
                lon: n.coord.x,
                cumulative_km: 0.0,
                node_id: Some(first),
            });
        }
    }

    // Use graph edge.length_m (same measure as route_metrics), not node-node haversine.
    // Haversine between consecutive path nodes underestimates bent trail geometry.
    for w in path.windows(2) {
        let n1 = &graph.nodes[&w[1]];
        let seg_m = graph
            .edge_index(w[0], w[1])
            .map(|i| graph.edges[i].length_m)
            .unwrap_or_else(|| {
                let n0 = &graph.nodes[&w[0]];
                haversine_m(n0.coord.y, n0.coord.x, n1.coord.y, n1.coord.x)
            });
        cumulative_m += seg_m;
        out.push(RouteSample {
            lat: n1.coord.y,
            lon: n1.coord.x,
            cumulative_km: cumulative_m / 1000.0,
            node_id: Some(w[1]),
        });
    }
    out
}

pub fn interpolate_at_km(samples: &[RouteSample], target_km: f64) -> (f64, f64) {
    for w in samples.windows(2) {
        if w[0].cumulative_km <= target_km && w[1].cumulative_km >= target_km {
            let span = w[1].cumulative_km - w[0].cumulative_km;
            let frac = if span <= 0.0 {
                0.0
            } else {
                (target_km - w[0].cumulative_km) / span
            };
            let lat = w[0].lat + (w[1].lat - w[0].lat) * frac;
            let lon = w[0].lon + (w[1].lon - w[0].lon) * frac;
            return (lat, lon);
        }
    }
    samples.last().map(|s| (s.lat, s.lon)).unwrap_or((0.0, 0.0))
}

#[derive(Debug, Clone)]
pub struct OvernightChoice {
    pub poi: PoiRecord,
    pub distance_from_target_m: f64,
    pub is_network: bool,
    pub safety_rejected: bool,
}

#[derive(Debug, Clone)]
pub struct RestStop {
    pub cumulative_km: f64,
    pub lat: f64,
    pub lon: f64,
    pub kind: RestKind,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestKind {
    Main,
    Alternative,
}

#[derive(Debug, Clone)]
pub struct DaySegment {
    pub day: u32,
    pub start_km: f64,
    pub end_km: f64,
    pub start_lat: f64,
    pub start_lon: f64,
    pub end_lat: f64,
    pub end_lon: f64,
    pub distance_km: f64,
    pub overnight: Option<OvernightChoice>,
    pub overnight_gap: bool,
    pub rest_stops: Vec<RestStop>,
}

pub fn overnight_display_name(choice: &OvernightChoice) -> String {
    if let Some(name) = choice.poi.name.as_deref().filter(|n| !n.trim().is_empty()) {
        return name.to_string();
    }
    let kind = if choice.is_network {
        "network hut"
    } else if choice.poi.categories.contains(&PoiCategory::Cabin) {
        "shelter"
    } else {
        "overnight facility"
    };
    format!(
        "Unnamed {kind}, {:.1} km",
        choice.distance_from_target_m / 1000.0
    )
}

pub fn find_poi_by_name(
    poi: &CombinedPoiIndex,
    name_substr: &str,
    lat: f64,
    lon: f64,
    radius_m: f64,
) -> Vec<PoiRecord> {
    let mut hits = Vec::new();
    for cat in [
        PoiCategory::Cabin,
        PoiCategory::NetworkHut,
        PoiCategory::OvernightFacility,
    ] {
        for p in poi.nearest(cat, lat, lon, radius_m) {
            if p.name
                .as_ref()
                .is_some_and(|n| n.to_lowercase().contains(&name_substr.to_lowercase()))
                && !hits.iter().any(|h: &PoiRecord| h.osm_id == p.osm_id)
            {
                hits.push(p);
            }
        }
    }
    hits
}

pub fn choose_overnight(
    poi: &CombinedPoiIndex,
    safety: &SafetyConfig,
    lat: f64,
    lon: f64,
) -> Option<OvernightChoice> {
    let mut candidates: Vec<(PoiRecord, f64, bool)> = Vec::new();

    for p in poi.nearest(
        PoiCategory::NetworkHut,
        lat,
        lon,
        safety.poi_radius_network_hut_m,
    ) {
        let d = haversine_m(lat, lon, p.lat, p.lon);
        candidates.push((p, d, true));
    }
    for p in poi.nearest(PoiCategory::Cabin, lat, lon, safety.poi_radius_cabin_m) {
        let d = haversine_m(lat, lon, p.lat, p.lon);
        let is_net = p.categories.contains(&PoiCategory::NetworkHut);
        if candidates.iter().any(|(c, _, _)| c.osm_id == p.osm_id) {
            continue;
        }
        candidates.push((p, d, is_net));
    }
    for p in poi.nearest(
        PoiCategory::OvernightFacility,
        lat,
        lon,
        safety.poi_radius_cabin_m,
    ) {
        let d = haversine_m(lat, lon, p.lat, p.lon);
        let is_net = p.categories.contains(&PoiCategory::NetworkHut);
        if candidates.iter().any(|(c, _, _)| c.osm_id == p.osm_id) {
            continue;
        }
        candidates.push((p, d, is_net));
    }

    candidates.sort_by(|a, b| {
        let a_pref = a.2 && a.1 <= safety.network_hut_preference_radius_m;
        let b_pref = b.2 && b.1 <= safety.network_hut_preference_radius_m;
        b_pref
            .cmp(&a_pref)
            .then_with(|| a.2.cmp(&b.2))
            .then_with(|| a.1.partial_cmp(&b.1).unwrap())
    });

    let mut fallback: Option<(PoiRecord, f64, bool)> = None;
    for (p, d, is_net) in candidates {
        let rejected = check_overnight_candidate(p.lat, p.lon, safety, &p, &[], &[]).is_some();
        if !rejected {
            return Some(OvernightChoice {
                poi: p,
                distance_from_target_m: d,
                is_network: is_net,
                safety_rejected: false,
            });
        }
        if fallback.is_none() {
            fallback = Some((p, d, is_net));
        }
    }

    fallback.map(|(p, d, is_net)| OvernightChoice {
        poi: p,
        distance_from_target_m: d,
        is_network: is_net,
        safety_rejected: true,
    })
}

pub fn plan_rest_stops(
    samples: &[RouteSample],
    day_start_km: f64,
    day_end_km: f64,
    rest: &RestConfig,
    poi: &CombinedPoiIndex,
    safety: &SafetyConfig,
) -> Vec<RestStop> {
    let main_km = next_break_distance_km(rest, Profile::Hiking, BreakKind::Main).unwrap();
    let alt_km = next_break_distance_km(rest, Profile::Hiking, BreakKind::Alternative).unwrap();
    let mut stops = Vec::new();
    let mut next_main = day_start_km + main_km;
    let day_len = day_end_km - day_start_km;

    while next_main < day_end_km - 0.01 {
        let (lat, lon) = interpolate_at_km(samples, next_main);
        let water_near = !poi
            .nearest(PoiCategory::Water, lat, lon, safety.poi_radius_water_m)
            .is_empty();
        let general_near = !poi
            .nearest(
                PoiCategory::General,
                lat,
                lon,
                safety.poi_radius_general_m.min(3_000.0),
            )
            .is_empty();

        if water_near || general_near {
            stops.push(RestStop {
                cumulative_km: next_main,
                lat,
                lon,
                kind: RestKind::Main,
                reason: Some("water or amenity POI within search radius".into()),
            });
            next_main += main_km;
        } else {
            let mut placed = false;
            let mut alt_target = next_main - main_km + alt_km;
            while alt_target < next_main && alt_target < day_end_km {
                let (alat, alon) = interpolate_at_km(samples, alt_target);
                let near = !poi
                    .nearest(PoiCategory::Water, alat, alon, safety.poi_radius_water_m)
                    .is_empty()
                    || !poi
                        .nearest(
                            PoiCategory::General,
                            alat,
                            alon,
                            safety.poi_radius_general_m.min(3_000.0),
                        )
                        .is_empty();
                if near {
                    stops.push(RestStop {
                        cumulative_km: alt_target,
                        lat: alat,
                        lon: alon,
                        kind: RestKind::Alternative,
                        reason: Some(format!(
                            "reject_main: no water/general POI within radius at {next_main:.2} km; alt POI found at {alt_target:.2} km"
                        )),
                    });
                    placed = true;
                    break;
                }
                alt_target += alt_km;
            }
            if !placed {
                stops.push(RestStop {
                    cumulative_km: next_main,
                    lat,
                    lon,
                    kind: RestKind::Alternative,
                    reason: Some(format!(
                        "reject_main: no water/general POI within radius near {next_main:.2} km (or along {alt_km:.3} km probes); forced main mark"
                    )),
                });
            }
            next_main += main_km;
        }

        if day_end_km - day_start_km > 0.0 && stops.len() > 500 {
            break;
        }
        let _ = day_len;
    }

    stops
}

pub fn plan_multi_day(
    samples: &[RouteSample],
    rest: &RestConfig,
    safety: &SafetyConfig,
    poi: &CombinedPoiIndex,
) -> Vec<DaySegment> {
    let max_daily = max_daily_distance_km(rest, Profile::Hiking).unwrap();
    let total_km = samples.last().map(|s| s.cumulative_km).unwrap_or(0.0);
    let mut days = Vec::new();
    let mut day_start_km = 0.0;
    let mut day_num = 1u32;

    while day_start_km < total_km - 0.01 {
        let remaining = total_km - day_start_km;
        let budget = max_daily.min(remaining);
        let window_end = (day_start_km + budget).min(total_km);
        let is_final = window_end >= total_km - 0.01;

        // Probe along the day's corridor. Use 500 m steps so hut approaches are not
        // missed between coarse 2 km samples (Day 2 Vetåbua regression after length fix).
        let mut best: Option<(f64, OvernightChoice, f64)> = None;
        let mut probe_km = day_start_km + 8.0;
        const PROBE_STEP_KM: f64 = 0.5;
        // Allow a longer day only if overnight detour does not get much worse.
        const DETOUR_SLACK_M: f64 = 500.0;
        while probe_km <= window_end {
            let (lat, lon) = interpolate_at_km(samples, probe_km);
            if let Some(choice) = choose_overnight(poi, safety, lat, lon) {
                if choice.distance_from_target_m <= OVERNIGHT_NEAR_HUT_MAX_M {
                    let hut_km = samples
                        .iter()
                        .filter(|x| {
                            x.cumulative_km >= day_start_km && x.cumulative_km <= window_end
                        })
                        .min_by(|a, b| {
                            let da = haversine_m(a.lat, a.lon, choice.poi.lat, choice.poi.lon);
                            let db = haversine_m(b.lat, b.lon, choice.poi.lat, choice.poi.lon);
                            da.partial_cmp(&db).unwrap()
                        })
                        .map(|x| x.cumulative_km)
                        .unwrap_or(probe_km);

                    let day_len = hut_km - day_start_km;
                    if day_len >= 5.0 {
                        let mut snapped = choice.clone();
                        let (elat, elon) = interpolate_at_km(samples, hut_km);
                        snapped.distance_from_target_m =
                            haversine_m(elat, elon, choice.poi.lat, choice.poi.lon);

                        let take = match &best {
                            None => true,
                            Some((prev_km, prev, _)) => {
                                let cur_net = snapped.is_network;
                                let prev_net = prev.is_network;
                                match (cur_net, prev_net) {
                                    (true, false) => true,
                                    (false, true) => false,
                                    _ => {
                                        // Same class: prefer longer days toward the budget, but
                                        // do not abandon a clearly closer hut for a few more km
                                        // (visible on Day 2 after switching to true edge lengths).
                                        let much_closer = snapped.distance_from_target_m
                                            + DETOUR_SLACK_M
                                            < prev.distance_from_target_m;
                                        let much_worse = snapped.distance_from_target_m
                                            > prev.distance_from_target_m + DETOUR_SLACK_M;
                                        if much_closer {
                                            true
                                        } else if much_worse {
                                            false
                                        } else {
                                            hut_km > *prev_km
                                                || (hut_km - *prev_km).abs() < 1.0
                                                    && snapped.distance_from_target_m
                                                        < prev.distance_from_target_m
                                        }
                                    }
                                }
                            }
                        };
                        if take {
                            best = Some((hut_km, snapped, day_len));
                        }
                    }
                }
            }
            probe_km += PROBE_STEP_KM;
        }

        let (end_km, overnight, overnight_gap) = if is_final {
            let (tlat, tlon) = interpolate_at_km(samples, total_km);
            let overnight =
                choose_overnight(poi, safety, tlat, tlon).or_else(|| best.map(|(_, c, _)| c));
            let gap = overnight
                .as_ref()
                .map(|o| o.distance_from_target_m > OVERNIGHT_NEAR_HUT_MAX_M)
                .unwrap_or(true);
            (total_km, overnight, gap)
        } else if let Some((hut_km, choice, _)) = best {
            let gap = choice.distance_from_target_m > OVERNIGHT_NEAR_HUT_MAX_M;
            (hut_km, Some(choice), gap)
        } else {
            (window_end, None, true)
        };

        let end_km = end_km.max(day_start_km + 0.01).min(total_km);
        let (start_lat, start_lon) = interpolate_at_km(samples, day_start_km);
        let (end_lat, end_lon) = interpolate_at_km(samples, end_km);
        let rest_stops = plan_rest_stops(samples, day_start_km, end_km, rest, poi, safety);

        days.push(DaySegment {
            day: day_num,
            start_km: day_start_km,
            end_km,
            start_lat,
            start_lon,
            end_lat,
            end_lon,
            distance_km: end_km - day_start_km,
            overnight,
            overnight_gap,
            rest_stops,
        });

        day_start_km = end_km;
        day_num += 1;
        if day_num > 30 {
            break;
        }
    }

    days
}

pub fn hiking_eco_config() -> driver_break_core::config::EcoConfig {
    driver_break_core::config::EcoConfig {
        mass_kg: 85.0,
        drag_coefficient: 0.9,
        frontal_area_m2: 0.5,
        rolling_resistance: 0.02,
        cruise_speed_m_s: 1.4,
        ..Default::default()
    }
}
