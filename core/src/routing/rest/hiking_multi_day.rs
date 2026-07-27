//! Multi-day hiking segmentation (daily distance budget → overnight at boundary).
//!
//! Promotes the proven test-helper `plan_multi_day` pattern into core so
//! [`plan_hiking_route`] can emit day segments and overnight hut pins.

use geo::{Distance, Haversine, Point};

use crate::config::SafetyConfig;
use crate::poi::{PoiCategory, PoiIndex, PoiRecord};
use crate::routing::safety::{check_overnight_candidate, OvernightProximityIndex};

/// Max distance from the day-boundary sample to accept a hut as “near”.
pub const OVERNIGHT_NEAR_HUT_MAX_M: f64 = 5_000.0;

#[derive(Debug, Clone, Copy)]
pub struct HikingRouteSample {
    pub lat: f64,
    pub lon: f64,
    pub cumulative_km: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HikingOvernightStop {
    pub lat: f64,
    pub lon: f64,
    pub name: String,
    pub osm_id: i64,
    pub is_network: bool,
    pub distance_from_target_m: f64,
    pub safety_rejected: bool,
    pub icon_key: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HikingDaySegment {
    pub day_index: u32,
    pub start_km: f64,
    pub end_km: f64,
    pub distance_km: f64,
    /// Overnight after this day (None on the final day when destination is reached).
    pub overnight: Option<HikingOvernightStop>,
    /// True when no near hut was found (or only a far / safety-rejected fallback).
    pub overnight_gap: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HikingMultiDayPlan {
    pub days: Vec<HikingDaySegment>,
    pub multi_day: bool,
}

fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    Haversine::distance(Point::new(lon1, lat1), Point::new(lon2, lat2))
}

fn interpolate_at_km(samples: &[HikingRouteSample], target_km: f64) -> (f64, f64) {
    if samples.is_empty() {
        return (0.0, 0.0);
    }
    if target_km <= samples[0].cumulative_km {
        return (samples[0].lat, samples[0].lon);
    }
    for w in samples.windows(2) {
        if target_km <= w[1].cumulative_km {
            let span = (w[1].cumulative_km - w[0].cumulative_km).max(1e-9);
            let t = ((target_km - w[0].cumulative_km) / span).clamp(0.0, 1.0);
            return (
                w[0].lat + t * (w[1].lat - w[0].lat),
                w[0].lon + t * (w[1].lon - w[0].lon),
            );
        }
    }
    let last = samples.last().unwrap();
    (last.lat, last.lon)
}

fn to_stop(
    p: &PoiRecord,
    dist_m: f64,
    is_network: bool,
    safety_rejected: bool,
) -> HikingOvernightStop {
    HikingOvernightStop {
        lat: p.lat,
        lon: p.lon,
        name: p
            .name
            .clone()
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| {
                if is_network {
                    format!("Network hut {}", p.osm_id)
                } else {
                    format!("Hut {}", p.osm_id)
                }
            }),
        osm_id: p.osm_id,
        is_network,
        distance_from_target_m: dist_m,
        safety_rejected,
        icon_key: p.icon_key.clone(),
    }
}

/// Prefer network huts within preference radius, then cabins / overnight facilities.
pub fn choose_hiking_overnight(
    poi: &PoiIndex,
    safety: &SafetyConfig,
    prox: &OvernightProximityIndex,
    lat: f64,
    lon: f64,
) -> Option<HikingOvernightStop> {
    let mut candidates: Vec<(PoiRecord, f64, bool)> = Vec::new();

    for p in poi.nearest(
        PoiCategory::NetworkHut,
        lat,
        lon,
        safety.poi_radius_network_hut_m,
    ) {
        let d = haversine_m(lat, lon, p.lat, p.lon);
        candidates.push(((*p).clone(), d, true));
    }
    for p in poi.nearest(PoiCategory::Cabin, lat, lon, safety.poi_radius_cabin_m) {
        let d = haversine_m(lat, lon, p.lat, p.lon);
        let is_net = p.categories.contains(&PoiCategory::NetworkHut);
        if candidates.iter().any(|(c, _, _)| c.osm_id == p.osm_id) {
            continue;
        }
        candidates.push(((*p).clone(), d, is_net));
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
        candidates.push(((*p).clone(), d, is_net));
    }

    candidates.sort_by(|a, b| {
        let a_pref = a.2 && a.1 <= safety.network_hut_preference_radius_m;
        let b_pref = b.2 && b.1 <= safety.network_hut_preference_radius_m;
        b_pref
            .cmp(&a_pref)
            .then_with(|| a.2.cmp(&b.2))
            .then_with(|| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut fallback: Option<(PoiRecord, f64, bool)> = None;
    for (p, d, is_net) in candidates {
        let rejected =
            check_overnight_candidate(p.lat, p.lon, safety, &p, &prox.buildings, &prox.glaciers)
                .is_some();
        if !rejected {
            return Some(to_stop(&p, d, is_net, false));
        }
        if fallback.is_none() {
            fallback = Some((p, d, is_net));
        }
    }

    fallback.map(|(p, d, is_net)| to_stop(&p, d, is_net, true))
}

/// Segment a hiking corridor into days bounded by `max_daily_km`, matching overnight
/// huts near each day boundary (same scoring spirit as the DNT integration helper).
pub fn plan_hiking_multi_day(
    samples: &[HikingRouteSample],
    max_daily_km: f64,
    safety: &SafetyConfig,
    poi: &PoiIndex,
    prox: &OvernightProximityIndex,
) -> HikingMultiDayPlan {
    let max_daily = max_daily_km.max(1.0);
    let total_km = samples.last().map(|s| s.cumulative_km).unwrap_or(0.0);
    let mut days = Vec::new();
    let mut day_start_km = 0.0;
    let mut day_num = 1u32;

    while day_start_km < total_km - 0.01 {
        let remaining = total_km - day_start_km;
        let budget = max_daily.min(remaining);
        let window_end = (day_start_km + budget).min(total_km);
        let is_final = window_end >= total_km - 0.01;

        let mut best: Option<(f64, HikingOvernightStop)> = None;
        let mut probe_km = day_start_km + 8.0;
        const PROBE_STEP_KM: f64 = 0.5;
        const DETOUR_SLACK_M: f64 = 500.0;

        while probe_km <= window_end && !is_final {
            let (lat, lon) = interpolate_at_km(samples, probe_km);
            if let Some(choice) = choose_hiking_overnight(poi, safety, prox, lat, lon) {
                if choice.distance_from_target_m <= OVERNIGHT_NEAR_HUT_MAX_M {
                    let hut_km = samples
                        .iter()
                        .filter(|x| {
                            x.cumulative_km >= day_start_km && x.cumulative_km <= window_end
                        })
                        .min_by(|a, b| {
                            let da = haversine_m(a.lat, a.lon, choice.lat, choice.lon);
                            let db = haversine_m(b.lat, b.lon, choice.lat, choice.lon);
                            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                        })
                        .map(|x| x.cumulative_km)
                        .unwrap_or(probe_km);

                    let day_len = hut_km - day_start_km;
                    if day_len >= 5.0 {
                        let (elat, elon) = interpolate_at_km(samples, hut_km);
                        let mut snapped = choice.clone();
                        snapped.distance_from_target_m =
                            haversine_m(elat, elon, choice.lat, choice.lon);

                        let take = match &best {
                            None => true,
                            Some((prev_km, prev)) => match (snapped.is_network, prev.is_network) {
                                (true, false) => true,
                                (false, true) => false,
                                _ => {
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
                                            || ((hut_km - *prev_km).abs() < 1.0
                                                && snapped.distance_from_target_m
                                                    < prev.distance_from_target_m)
                                    }
                                }
                            },
                        };
                        if take {
                            best = Some((hut_km, snapped));
                        }
                    }
                }
            }
            probe_km += PROBE_STEP_KM;
        }

        let (end_km, overnight, overnight_gap) = if is_final {
            (total_km, None, false)
        } else if let Some((hut_km, choice)) = best {
            let gap =
                choice.distance_from_target_m > OVERNIGHT_NEAR_HUT_MAX_M || choice.safety_rejected;
            (hut_km, Some(choice), gap)
        } else {
            let (lat, lon) = interpolate_at_km(samples, window_end);
            let fallback = choose_hiking_overnight(poi, safety, prox, lat, lon);
            let gap = fallback
                .as_ref()
                .map(|o| o.distance_from_target_m > OVERNIGHT_NEAR_HUT_MAX_M || o.safety_rejected)
                .unwrap_or(true);
            (window_end, fallback, gap)
        };

        let end_km = end_km.max(day_start_km + 0.01).min(total_km);
        days.push(HikingDaySegment {
            day_index: day_num,
            start_km: day_start_km,
            end_km,
            distance_km: end_km - day_start_km,
            overnight,
            overnight_gap,
        });

        day_start_km = end_km;
        day_num += 1;
        if day_num > 30 {
            break;
        }
    }

    let multi_day = days.len() > 1;
    HikingMultiDayPlan { days, multi_day }
}

/// Build evenly spaced samples along a lat/lon polyline with cumulative km.
pub fn hiking_samples_from_coords(coords: &[(f64, f64)]) -> Vec<HikingRouteSample> {
    let mut out = Vec::with_capacity(coords.len());
    let mut cum = 0.0;
    for (i, &(lat, lon)) in coords.iter().enumerate() {
        if i > 0 {
            let (plat, plon) = coords[i - 1];
            cum += haversine_m(plat, plon, lat, lon) / 1000.0;
        }
        out.push(HikingRouteSample {
            lat,
            lon,
            cumulative_km: cum,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SafetyConfig;
    use crate::poi::{PoiCategory, PoiIndex, PoiRecord};
    use std::collections::HashMap;

    fn hut(id: i64, lat: f64, lon: f64, name: &str, network: bool) -> PoiRecord {
        let mut cats = vec![PoiCategory::Cabin, PoiCategory::OvernightFacility];
        if network {
            cats.push(PoiCategory::NetworkHut);
        }
        PoiRecord {
            osm_id: id,
            lat,
            lon,
            categories: cats,
            icon_key: "tourism-wilderness_hut".into(),
            tags: HashMap::new(),
            name: Some(name.into()),
        }
    }

    fn samples_along_north(total_km: f64, step_km: f64) -> Vec<HikingRouteSample> {
        // ~111 km per degree latitude.
        let mut out = Vec::new();
        let mut km = 0.0;
        while km <= total_km + 1e-9 {
            let lat = 61.0 + km / 111.0;
            out.push(HikingRouteSample {
                lat,
                lon: 10.0,
                cumulative_km: km,
            });
            km += step_km;
        }
        out
    }

    #[test]
    fn short_hike_stays_single_day() {
        let samples = samples_along_north(30.0, 2.0);
        let poi = PoiIndex::new();
        let plan = plan_hiking_multi_day(
            &samples,
            40.0,
            &SafetyConfig::default(),
            &poi,
            &OvernightProximityIndex::default(),
        );
        assert!(!plan.multi_day);
        assert_eq!(plan.days.len(), 1);
        assert!(plan.days[0].overnight.is_none());
    }

    #[test]
    fn long_hike_splits_and_attaches_hut() {
        let samples = samples_along_north(80.0, 1.0);
        // Boundary near 40 km → lat ≈ 61 + 40/111 ≈ 61.360
        let mut poi = PoiIndex::new();
        poi.insert_record(hut(1, 61.36, 10.01, "Testbu", true));
        let plan = plan_hiking_multi_day(
            &samples,
            40.0,
            &SafetyConfig::default(),
            &poi,
            &OvernightProximityIndex::default(),
        );
        assert!(plan.multi_day, "days={:?}", plan.days.len());
        assert!(plan.days.len() >= 2);
        let o = plan.days[0]
            .overnight
            .as_ref()
            .expect("overnight after day 1");
        assert_eq!(o.name, "Testbu");
        assert!(o.is_network);
        assert!(!o.safety_rejected);
        assert!(plan.days[0].distance_km <= 40.0 + 1.0);
    }
}
