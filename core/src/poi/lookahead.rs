//! Route-independent POI look-ahead cone ("Nearby attractions").
//!
//! Geometry mirrors [`crate::routing::live_hazard`] (`bearing_deg` /
//! `angle_diff_deg` / haversine) with **separate** constants — do not change
//! the hazard cone. Spec: `docs/plugins/poi-lookahead-cone-spec.md`.

use chrono::NaiveDateTime;

use super::{PoiCategory, PoiIndex, PoiRecord};
use crate::routing::conditional::oh_condition_matches_at;

/// Look-ahead distance (metres). Distinct from the hazard cone's 300 m.
pub const POI_LOOKAHEAD_CONE_M: f64 = 850.0;
/// Half-angle from GPS heading: ±30° = 60° total width.
pub const POI_LOOKAHEAD_CONE_HALF_WIDTH_DEG: f64 = 30.0;

/// Product master toggle default — must stay off (opt-in discovery).
pub const POI_LOOKAHEAD_DEFAULT_ENABLED: bool = false;

/// "Hide when hours unknown" Settings default — off (show with label).
pub const POI_LOOKAHEAD_STRICT_HOURS_UNKNOWN_DEFAULT: bool = false;

/// Host-evaluated opening-hours state for one POI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenNow {
    Open,
    Closed,
    Unknown,
}

impl OpenNow {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "true",
            Self::Closed => "false",
            Self::Unknown => "unknown",
        }
    }
}

/// One discovery hit inside the cone (already category- and hours-filtered).
#[derive(Debug, Clone, PartialEq)]
pub struct PoiLookaheadHit {
    pub osm_id: i64,
    pub lat: f64,
    pub lon: f64,
    pub distance_m: f64,
    pub name: Option<String>,
    pub category: PoiCategory,
    pub icon_key: String,
    pub open_now: OpenNow,
    /// HUD label: `"650 m — Viewpoint"` or with `" (hours unknown)"`.
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

/// Distance when the target is inside the POI look-ahead cone.
///
/// When `heading_deg` is missing/non-finite, membership is distance-only
/// (isotropic), matching the hazard-cone fallback documented in the spec.
pub fn in_poi_lookahead_cone(
    lat: f64,
    lon: f64,
    heading_deg: Option<f64>,
    tlat: f64,
    tlon: f64,
) -> Option<f64> {
    let d = haversine_m(lat, lon, tlat, tlon);
    if !d.is_finite() || d > POI_LOOKAHEAD_CONE_M || d <= 0.0 {
        return None;
    }
    if let Some(heading) = heading_deg.filter(|h| h.is_finite()) {
        let br = bearing_deg(lat, lon, tlat, tlon);
        if angle_diff_deg(heading, br) > POI_LOOKAHEAD_CONE_HALF_WIDTH_DEG {
            return None;
        }
    }
    Some(d)
}

/// True when categories qualify for the discovery cone.
///
/// Included: General, Fishing, CraftBrewery. Water alone is excluded; Water +
/// General (attraction co-tag) is included via General. Cabins/huts/rest/lodging
/// never qualify on their own.
pub fn categories_qualify_for_lookahead(cats: &[PoiCategory]) -> bool {
    cats.iter().any(|c| {
        matches!(
            c,
            PoiCategory::General | PoiCategory::Fishing | PoiCategory::CraftBrewery
        )
    })
}

/// Primary display category (CraftBrewery > Fishing > General).
pub fn primary_lookahead_category(cats: &[PoiCategory]) -> Option<PoiCategory> {
    if cats.contains(&PoiCategory::CraftBrewery) {
        Some(PoiCategory::CraftBrewery)
    } else if cats.contains(&PoiCategory::Fishing) {
        Some(PoiCategory::Fishing)
    } else if cats.contains(&PoiCategory::General) {
        Some(PoiCategory::General)
    } else {
        None
    }
}

/// Evaluate OSM `opening_hours` at `dt` (device-local naive datetime).
pub fn open_now_at(tags: &std::collections::HashMap<String, String>, dt: NaiveDateTime) -> OpenNow {
    let Some(raw) = tags.get("opening_hours") else {
        return OpenNow::Unknown;
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return OpenNow::Unknown;
    }
    match oh_condition_matches_at(trimmed, dt) {
        Some(true) => OpenNow::Open,
        Some(false) => OpenNow::Closed,
        None => OpenNow::Unknown,
    }
}

fn generic_category_label(cat: PoiCategory) -> &'static str {
    match cat {
        PoiCategory::CraftBrewery => "Brewery",
        PoiCategory::Fishing => "Fishing",
        PoiCategory::General => "Attraction",
        _ => "Place",
    }
}

fn generic_label_from_icon(icon_key: &str, cat: PoiCategory) -> String {
    match icon_key {
        "tourism-viewpoint" => "Viewpoint".into(),
        "tourism-attraction" => "Attraction".into(),
        "tourism-museum" | "amenity-museum" => "Museum".into(),
        "tourism-artwork" => "Artwork".into(),
        "amenity-gallery" => "Gallery".into(),
        "amenity-zoo" => "Zoo".into(),
        "amenity-aquarium" => "Aquarium".into(),
        "amenity-picnic_site" => "Picnic site".into(),
        "amenity-cafe" => "Cafe".into(),
        "amenity-restaurant" => "Restaurant".into(),
        "amenity-fast_food" => "Fast food".into(),
        "leisure-fishing" | "leisure-fishing_pier" | "sport-fishing" | "shop-fishing" => {
            "Fishing".into()
        }
        "shop-alcohol" => "Brewery".into(),
        _ => generic_category_label(cat).into(),
    }
}

/// Format HUD label: `"650 m — Name"` or generic category; append hours unknown.
pub fn format_lookahead_label(
    distance_m: f64,
    name: Option<&str>,
    icon_key: &str,
    category: PoiCategory,
    open_now: OpenNow,
) -> String {
    let title = name
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| generic_label_from_icon(icon_key, category));
    let mut label = format!("{:.0} m — {title}", distance_m);
    if open_now == OpenNow::Unknown {
        label.push_str(" (hours unknown)");
    }
    label
}

/// Query the look-ahead cone over an existing [`PoiIndex`].
///
/// Closed-now hits are dropped here (host-side). When
/// `strict_hours_unknown` is true, Unknown hours are also dropped.
pub fn query_poi_lookahead(
    index: &PoiIndex,
    lat: f64,
    lon: f64,
    heading_deg: Option<f64>,
    now: NaiveDateTime,
    strict_hours_unknown: bool,
) -> Vec<PoiLookaheadHit> {
    let mut hits = Vec::new();
    for cat in [
        PoiCategory::General,
        PoiCategory::Fishing,
        PoiCategory::CraftBrewery,
    ] {
        for rec in index.nearest(cat, lat, lon, POI_LOOKAHEAD_CONE_M) {
            if !categories_qualify_for_lookahead(&rec.categories) {
                continue;
            }
            let Some(primary) = primary_lookahead_category(&rec.categories) else {
                continue;
            };
            // Dedup by osm_id across category queries.
            if hits
                .iter()
                .any(|h: &PoiLookaheadHit| h.osm_id == rec.osm_id)
            {
                continue;
            }
            let Some(distance_m) = in_poi_lookahead_cone(lat, lon, heading_deg, rec.lat, rec.lon)
            else {
                continue;
            };
            let open_now = open_now_at(&rec.tags, now);
            if open_now == OpenNow::Closed {
                continue;
            }
            if strict_hours_unknown && open_now == OpenNow::Unknown {
                continue;
            }
            let label = format_lookahead_label(
                distance_m,
                rec.name.as_deref(),
                &rec.icon_key,
                primary,
                open_now,
            );
            hits.push(PoiLookaheadHit {
                osm_id: rec.osm_id,
                lat: rec.lat,
                lon: rec.lon,
                distance_m,
                name: rec.name.clone(),
                category: primary,
                icon_key: rec.icon_key.clone(),
                open_now,
                label,
            });
        }
    }
    hits.sort_by(|a, b| {
        a.distance_m
            .partial_cmp(&b.distance_m)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits
}

/// Guest-side helper: keep a radius hit if it lies in the cone (bearing filter).
pub fn guest_cone_filter(
    origin_lat: f64,
    origin_lon: f64,
    heading_deg: Option<f64>,
    poi_lat: f64,
    poi_lon: f64,
) -> Option<f64> {
    in_poi_lookahead_cone(origin_lat, origin_lon, heading_deg, poi_lat, poi_lon)
}

/// Wire category name for JSON / HUD (`snake_case`).
pub fn category_wire_name(cat: PoiCategory) -> &'static str {
    match cat {
        PoiCategory::General => "general",
        PoiCategory::Fishing => "fishing",
        PoiCategory::CraftBrewery => "craft_brewery",
        PoiCategory::Water => "water",
        PoiCategory::Cabin => "cabin",
        PoiCategory::NetworkHut => "network_hut",
        PoiCategory::Restroom => "restroom",
        PoiCategory::OvernightFacility => "overnight_facility",
        PoiCategory::TentSite => "tent_site",
        PoiCategory::RestArea => "rest_area",
        PoiCategory::Lodging => "lodging",
    }
}

/// Build a [`PoiIndex`] from tagged OSM-like JSON objects (tests / FFI ingest).
///
/// Each element: `{ "osm_id", "lat", "lon", "tags": { ... } }`. Classification
/// and icons reuse [`crate::poi::classify_tags`] / [`crate::poi::osm_icon_key`].
pub fn poi_index_from_tagged_json(raw: &str) -> Result<PoiIndex, String> {
    use super::{classify_tags, osm_icon_key};
    use std::collections::HashMap;

    let arr: Vec<serde_json::Value> =
        serde_json::from_str(raw).map_err(|e| format!("poi_lookahead json: {e}"))?;
    let mut index = PoiIndex::new();
    for v in arr {
        let osm_id = v
            .get("osm_id")
            .and_then(|x| x.as_i64())
            .ok_or_else(|| "missing osm_id".to_string())?;
        let lat = v
            .get("lat")
            .and_then(|x| x.as_f64())
            .ok_or_else(|| "missing lat".to_string())?;
        let lon = v
            .get("lon")
            .and_then(|x| x.as_f64())
            .ok_or_else(|| "missing lon".to_string())?;
        let mut tags: HashMap<String, String> = HashMap::new();
        if let Some(obj) = v.get("tags").and_then(|t| t.as_object()) {
            for (k, val) in obj {
                if let Some(s) = val.as_str() {
                    tags.insert(k.clone(), s.to_string());
                }
            }
        }
        let categories = classify_tags(&tags);
        if categories.is_empty() {
            continue;
        }
        let icon_key = osm_icon_key(&tags);
        let name = tags
            .get("name")
            .cloned()
            .or_else(|| v.get("name").and_then(|n| n.as_str()).map(str::to_string));
        index.insert_record(PoiRecord {
            osm_id,
            lat,
            lon,
            categories,
            icon_key,
            tags,
            name,
        });
    }
    Ok(index)
}

/// Build a synthetic [`PoiRecord`] for unit tests.
#[cfg(test)]
pub fn test_record(
    osm_id: i64,
    lat: f64,
    lon: f64,
    cats: &[PoiCategory],
    name: Option<&str>,
    icon_key: &str,
    opening_hours: Option<&str>,
) -> PoiRecord {
    use std::collections::HashMap;
    let mut tags = HashMap::new();
    if let Some(oh) = opening_hours {
        tags.insert("opening_hours".into(), oh.into());
    }
    PoiRecord {
        osm_id,
        lat,
        lon,
        categories: cats.to_vec(),
        icon_key: icon_key.into(),
        tags,
        name: name.map(str::to_string),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn dt(h: u32, m: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 6, 15)
            .unwrap()
            .and_hms_opt(h, m, 0)
            .unwrap()
    }

    #[test]
    fn defaults_are_opt_in_off() {
        let enabled_default = POI_LOOKAHEAD_DEFAULT_ENABLED;
        let strict_default = POI_LOOKAHEAD_STRICT_HOURS_UNKNOWN_DEFAULT;
        if enabled_default || strict_default {
            panic!("Nearby attractions toggles must ship off (opt-in)");
        }
    }

    #[test]
    fn cone_rejects_behind_and_beyond_radius() {
        // Origin; heading due north.
        let lat = 60.0;
        let lon = 10.0;
        let heading = Some(0.0_f64);
        // ~500 m north — inside.
        let north = in_poi_lookahead_cone(lat, lon, heading, 60.0045, lon);
        assert!(north.is_some());
        assert!(north.unwrap() < POI_LOOKAHEAD_CONE_M);
        // ~500 m south — behind, outside ±30°.
        assert!(in_poi_lookahead_cone(lat, lon, heading, 59.9955, lon).is_none());
        // ~1200 m north — beyond radius.
        assert!(in_poi_lookahead_cone(lat, lon, heading, 60.011, lon).is_none());
        // ~500 m east — outside ±30° when heading north.
        assert!(in_poi_lookahead_cone(lat, lon, heading, lat, 10.009).is_none());
    }

    #[test]
    fn cone_isotropic_without_heading() {
        let lat = 60.0;
        let lon = 10.0;
        // South of origin, within radius — allowed when heading unknown.
        let d = in_poi_lookahead_cone(lat, lon, None, 59.9955, lon);
        assert!(d.is_some());
    }

    #[test]
    fn category_filter_excludes_cabin_and_plain_water() {
        assert!(!categories_qualify_for_lookahead(&[PoiCategory::Cabin]));
        assert!(!categories_qualify_for_lookahead(&[PoiCategory::Water]));
        assert!(!categories_qualify_for_lookahead(&[
            PoiCategory::RestArea,
            PoiCategory::Lodging
        ]));
        assert!(categories_qualify_for_lookahead(&[PoiCategory::General]));
        assert!(categories_qualify_for_lookahead(&[PoiCategory::Fishing]));
        assert!(categories_qualify_for_lookahead(&[
            PoiCategory::CraftBrewery
        ]));
        // Water + General (attraction co-tag) qualifies via General.
        assert!(categories_qualify_for_lookahead(&[
            PoiCategory::Water,
            PoiCategory::General
        ]));
    }

    #[test]
    fn brewery_cider_is_craft_brewery() {
        use crate::poi::classify_tags;
        use std::collections::HashMap;
        let mut tags = HashMap::new();
        tags.insert("brewery".into(), "cider".into());
        let cats = classify_tags(&tags);
        assert!(cats.contains(&PoiCategory::CraftBrewery));
        assert!(categories_qualify_for_lookahead(&cats));
    }

    #[test]
    fn opening_hours_open_closed_unknown() {
        use std::collections::HashMap;
        let mut tags = HashMap::new();
        tags.insert("opening_hours".into(), "Mo-Su 10:00-18:00".into());
        assert_eq!(open_now_at(&tags, dt(12, 0)), OpenNow::Open);
        assert_eq!(open_now_at(&tags, dt(20, 0)), OpenNow::Closed);
        let empty = HashMap::new();
        assert_eq!(open_now_at(&empty, dt(12, 0)), OpenNow::Unknown);
        let mut bad = HashMap::new();
        bad.insert("opening_hours".into(), "not a real schedule!!!".into());
        assert_eq!(open_now_at(&bad, dt(12, 0)), OpenNow::Unknown);
    }

    #[test]
    fn query_suppresses_closed_and_orders_nearest() {
        let mut idx = PoiIndex::new();
        // Origin 60,10 heading north. Two attractions north; one closed.
        idx.insert_record(test_record(
            1,
            60.003,
            10.0,
            &[PoiCategory::General],
            Some("Near VP"),
            "tourism-viewpoint",
            Some("Mo-Su 10:00-18:00"),
        ));
        idx.insert_record(test_record(
            2,
            60.005,
            10.0,
            &[PoiCategory::General],
            Some("Far VP"),
            "tourism-viewpoint",
            Some("Mo-Su 10:00-18:00"),
        ));
        idx.insert_record(test_record(
            3,
            60.002,
            10.0,
            &[PoiCategory::General],
            Some("Closed Cafe"),
            "amenity-cafe",
            Some("Mo-Su 10:00-11:00"),
        ));
        idx.insert_record(test_record(
            4,
            60.0025,
            10.0,
            &[PoiCategory::Cabin],
            Some("Hut"),
            "tourism-wilderness_hut",
            None,
        ));

        let noon = query_poi_lookahead(&idx, 60.0, 10.0, Some(0.0), dt(12, 0), false);
        assert_eq!(noon.len(), 2);
        assert_eq!(noon[0].name.as_deref(), Some("Near VP"));
        assert_eq!(noon[1].name.as_deref(), Some("Far VP"));
        assert!(!noon
            .iter()
            .any(|h| h.name.as_deref() == Some("Closed Cafe")));
        assert!(!noon.iter().any(|h| h.name.as_deref() == Some("Hut")));

        let evening = query_poi_lookahead(&idx, 60.0, 10.0, Some(0.0), dt(20, 0), false);
        assert!(evening.is_empty());
    }

    #[test]
    fn hours_unknown_label_and_strict_filter() {
        let mut idx = PoiIndex::new();
        idx.insert_record(test_record(
            10,
            60.003,
            10.0,
            &[PoiCategory::General],
            None,
            "tourism-viewpoint",
            None,
        ));
        let show = query_poi_lookahead(&idx, 60.0, 10.0, Some(0.0), dt(12, 0), false);
        assert_eq!(show.len(), 1);
        assert!(show[0].label.contains("Viewpoint"));
        assert!(show[0].label.contains("hours unknown"));
        let hide = query_poi_lookahead(&idx, 60.0, 10.0, Some(0.0), dt(12, 0), true);
        assert!(hide.is_empty());
    }

    /// Hardanger (Ulvik) geometry: cidery ahead, viewpoint off-cone, closed venue suppressed.
    #[test]
    fn hardanger_ulvik_cone_categories_and_hours() {
        use crate::poi::classify_tags;
        use std::collections::HashMap;

        // Ulvik frukt & cideri (docs/cider-route.md stop 19).
        let cider_lat = 60.57525;
        let cider_lon = 6.93919;
        // ~200 m south of cidery, heading north toward it.
        let origin_lat = 60.57345;
        let origin_lon = 6.93919;
        let heading = Some(0.0_f64);

        let mut cider_tags = HashMap::new();
        cider_tags.insert("brewery".into(), "cider".into());
        cider_tags.insert("name".into(), "Ulvik frukt & cideri".into());
        cider_tags.insert("opening_hours".into(), "Mo-Su 10:00-18:00".into());
        assert!(classify_tags(&cider_tags).contains(&PoiCategory::CraftBrewery));

        let mut idx = PoiIndex::new();
        idx.insert_record(test_record(
            2412997030,
            cider_lat,
            cider_lon,
            &[PoiCategory::CraftBrewery],
            Some("Ulvik frukt & cideri"),
            "shop-alcohol",
            Some("Mo-Su 10:00-18:00"),
        ));
        // Viewpoint ~400 m east — inside 850 m, outside ±30° when heading north.
        idx.insert_record(test_record(
            99,
            origin_lat,
            origin_lon + 0.007,
            &[PoiCategory::General],
            Some("Fjordsicht"),
            "tourism-viewpoint",
            None,
        ));
        // Closed cafe ahead — must never surface.
        idx.insert_record(test_record(
            100,
            60.5745,
            6.93919,
            &[PoiCategory::General],
            Some("Closed cafe"),
            "amenity-cafe",
            Some("Mo-Su 10:00-11:00"),
        ));
        // Hours-unknown fishing spot ahead.
        idx.insert_record(test_record(
            101,
            60.5748,
            6.93919,
            &[PoiCategory::Fishing],
            Some("Ulvik fishing"),
            "leisure-fishing",
            None,
        ));

        assert!(
            in_poi_lookahead_cone(origin_lat, origin_lon, heading, cider_lat, cider_lon).is_some()
        );
        assert!(in_poi_lookahead_cone(
            origin_lat,
            origin_lon,
            heading,
            origin_lat,
            origin_lon + 0.007
        )
        .is_none());

        let noon = query_poi_lookahead(&idx, origin_lat, origin_lon, heading, dt(12, 0), false);
        assert!(noon.iter().any(|h| h.icon_key == "shop-alcohol"));
        assert!(noon.iter().any(|h| h.label.contains("hours unknown")));
        assert!(!noon.iter().any(|h| h.name.as_deref() == Some("Fjordsicht")));
        assert!(!noon
            .iter()
            .any(|h| h.name.as_deref() == Some("Closed cafe")));

        let evening = query_poi_lookahead(&idx, origin_lat, origin_lon, heading, dt(20, 0), false);
        assert!(!evening.iter().any(|h| h.icon_key == "shop-alcohol"));
    }
}
