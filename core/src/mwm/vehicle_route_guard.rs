//! Post-route check: reject a finished car route that still uses a restricted
//! segment (×1e6 HardAvoid can leave a sole-corridor path as "OK").

use crate::config::VehicleLimits;

use super::sidecar::{vehicle_exceeds_limit, VehicleLimitRecord};

/// One segment on a finished CoMaps route.
#[derive(Debug, Clone, PartialEq)]
pub struct RouteSegmentRef {
    pub mwm_name: String,
    pub feature_id: u32,
    pub segment_index: u16,
    /// Street name when known (else empty → message uses "Road").
    pub road_name: String,
    pub lat: f64,
    pub lon: f64,
}

/// A vehicle value that strictly exceeds a posted limit on a segment.
#[derive(Debug, Clone, PartialEq)]
pub struct VehicleLimitViolation {
    pub segment: RouteSegmentRef,
    pub detail: String,
}

impl VehicleLimitViolation {
    /// Same shape as [`crate::routing::graph::clearance_restriction_user_message`].
    pub fn user_message(&self) -> String {
        let road = if self.segment.road_name.trim().is_empty() {
            "Road"
        } else {
            self.segment.road_name.trim()
        };
        format!(
            "{road} near {:.5},{:.5}: {}",
            self.segment.lat, self.segment.lon, self.detail
        )
    }

    /// Full “no route” line for the planner / UI.
    pub fn no_route_message(&self) -> String {
        format!(
            "No route without passing a restriction. {}",
            self.user_message()
        )
    }
}

fn metres_to_cm(m: f64) -> Option<u16> {
    if !m.is_finite() || m <= 0.0 {
        return None;
    }
    let cm = (m * 100.0).round();
    if cm <= 0.0 {
        return None;
    }
    Some(cm.min(65535.0) as u16)
}

fn kg_to_10kg(kg: f64) -> Option<u16> {
    if !kg.is_finite() || kg <= 0.0 {
        return None;
    }
    let v = (kg / 10.0).round();
    if v <= 0.0 {
        return None;
    }
    Some(v.min(65535.0) as u16)
}

/// Compare vehicle dims to one sidecar record. Exact equality is allowed.
pub fn vehicle_vs_record(limits: &VehicleLimits, rec: &VehicleLimitRecord) -> Option<&'static str> {
    if let Some(vh) = limits.height_m.and_then(metres_to_cm) {
        if vehicle_exceeds_limit(vh, rec.max_height_cm) {
            return Some("height");
        }
    }
    if let Some(vw) = limits.width_m.and_then(metres_to_cm) {
        if vehicle_exceeds_limit(vw, rec.max_width_cm) {
            return Some("width");
        }
    }
    if let Some(vl) = limits.length_m.and_then(metres_to_cm) {
        if vehicle_exceeds_limit(vl, rec.max_length_cm) {
            return Some("length");
        }
    }
    if let Some(tw) = limits.total_weight_kg.and_then(kg_to_10kg) {
        if vehicle_exceeds_limit(tw, rec.max_weight_10kg) {
            return Some("weight");
        }
    }
    if let Some(ax) = limits.axle_weight_kg.and_then(kg_to_10kg) {
        if vehicle_exceeds_limit(ax, rec.max_axleload_10kg) {
            return Some("axle load");
        }
    }
    if let Some(bg) = limits.bogie_weight_kg.and_then(kg_to_10kg) {
        if vehicle_exceeds_limit(bg, rec.max_bogie_10kg) {
            return Some("bogie weight");
        }
    }
    None
}

fn detail_for(kind: &str, limits: &VehicleLimits, rec: &VehicleLimitRecord) -> String {
    match kind {
        "height" => format!(
            "height limit {:.2} m, your vehicle {:.2} m",
            rec.max_height_cm as f64 / 100.0,
            limits.height_m.unwrap_or(0.0)
        ),
        "width" => format!(
            "width limit {:.2} m, your vehicle {:.2} m",
            rec.max_width_cm as f64 / 100.0,
            limits.width_m.unwrap_or(0.0)
        ),
        "length" => format!(
            "length limit {:.2} m, your vehicle {:.2} m",
            rec.max_length_cm as f64 / 100.0,
            limits.length_m.unwrap_or(0.0)
        ),
        "weight" => format!(
            "weight limit {:.2} t, your vehicle {:.2} t",
            rec.max_weight_10kg as f64 / 100.0,
            limits.total_weight_kg.unwrap_or(0.0) / 1000.0
        ),
        "axle load" => format!(
            "axle load limit {:.2} t, your vehicle {:.2} t",
            rec.max_axleload_10kg as f64 / 100.0,
            limits.axle_weight_kg.unwrap_or(0.0) / 1000.0
        ),
        "bogie weight" => format!(
            "bogie weight limit {:.2} t, your vehicle {:.2} t",
            rec.max_bogie_10kg as f64 / 100.0,
            limits.bogie_weight_kg.unwrap_or(0.0) / 1000.0
        ),
        other => format!("{other} limit exceeded"),
    }
}

/// First violating segment on the finished route, if any.
pub fn find_route_vehicle_violation(
    segments: &[RouteSegmentRef],
    limits: &VehicleLimits,
    mut lookup: impl FnMut(&str, u32, u16) -> Option<VehicleLimitRecord>,
) -> Option<VehicleLimitViolation> {
    for seg in segments {
        let Some(rec) = lookup(&seg.mwm_name, seg.feature_id, seg.segment_index) else {
            continue;
        };
        if let Some(kind) = vehicle_vs_record(limits, &rec) {
            return Some(VehicleLimitViolation {
                segment: seg.clone(),
                detail: detail_for(kind, limits, &rec),
            });
        }
    }
    None
}

/// Parse `[{"mwm":"…","fid":N,"seg":N,"name":"…","lat":…,"lon":…},…]`.
pub fn parse_vehicle_segments_json(json: &str) -> Result<Vec<RouteSegmentRef>, String> {
    let v: serde_json::Value =
        serde_json::from_str(json.trim()).map_err(|e| format!("vehicle_segments json: {e}"))?;
    let arr = v
        .as_array()
        .ok_or_else(|| "vehicle_segments: expected array".to_string())?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let mwm = item
            .get("mwm")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let feature_id =
            item.get("fid")
                .and_then(|x| x.as_u64())
                .ok_or_else(|| "vehicle_segments: missing fid".to_string())? as u32;
        let segment_index =
            item.get("seg")
                .and_then(|x| x.as_u64())
                .ok_or_else(|| "vehicle_segments: missing seg".to_string())? as u16;
        let road_name = item
            .get("name")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let lat = item.get("lat").and_then(|x| x.as_f64()).unwrap_or(0.0);
        let lon = item.get("lon").and_then(|x| x.as_f64()).unwrap_or(0.0);
        out.push(RouteSegmentRef {
            mwm_name: mwm,
            feature_id,
            segment_index,
            road_name,
            lat,
            lon,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(height_cm: u16) -> VehicleLimitRecord {
        VehicleLimitRecord {
            feature_id: 1,
            segment_index: 0,
            max_height_cm: height_cm,
            max_width_cm: 0,
            max_length_cm: 0,
            max_weight_10kg: 0,
            max_axleload_10kg: 0,
            max_bogie_10kg: 0,
        }
    }

    fn seg(name: &str) -> RouteSegmentRef {
        RouteSegmentRef {
            mwm_name: "Norway_Hedmark".into(),
            feature_id: 1,
            segment_index: 0,
            road_name: name.into(),
            lat: 60.73,
            lon: 11.18,
        }
    }

    #[test]
    fn only_connection_restricted_route_is_rejected() {
        let limits = VehicleLimits {
            height_m: Some(2.8),
            ..Default::default()
        };
        let segments = vec![seg("Fokholgutua")];
        let v = find_route_vehicle_violation(&segments, &limits, |_, _, _| Some(rec(270)));
        let v = v.expect("must reject");
        assert!(v.no_route_message().contains("Fokholgutua"));
        assert!(v.no_route_message().contains("height limit 2.70 m"));
        assert!(v.no_route_message().contains("2.80 m"));
        assert!(v
            .no_route_message()
            .starts_with("No route without passing a restriction."));
    }

    #[test]
    fn alternative_exists_unrestricted_segments_ok() {
        let limits = VehicleLimits {
            height_m: Some(2.8),
            ..Default::default()
        };
        // Detour: only unrestricted segments on the finished route.
        let segments = vec![RouteSegmentRef {
            mwm_name: "Norway_Hedmark".into(),
            feature_id: 99,
            segment_index: 1,
            road_name: "Detour".into(),
            lat: 60.74,
            lon: 11.19,
        }];
        let v = find_route_vehicle_violation(&segments, &limits, |_, fid, _| {
            if fid == 1 {
                Some(rec(270))
            } else {
                None
            }
        });
        assert!(v.is_none());
    }

    #[test]
    fn exact_equal_height_is_allowed() {
        let limits = VehicleLimits {
            height_m: Some(2.7),
            ..Default::default()
        };
        let segments = vec![seg("Fokholgutua")];
        let v = find_route_vehicle_violation(&segments, &limits, |_, _, _| Some(rec(270)));
        assert!(v.is_none());
    }

    #[test]
    fn weight_over_limit_is_rejected() {
        let limits = VehicleLimits {
            total_weight_kg: Some(12_000.0),
            ..Default::default()
        };
        let rec = VehicleLimitRecord {
            feature_id: 1,
            segment_index: 0,
            max_height_cm: 0,
            max_width_cm: 0,
            max_length_cm: 0,
            max_weight_10kg: 1_000, // 10 t
            max_axleload_10kg: 0,
            max_bogie_10kg: 0,
        };
        let v = find_route_vehicle_violation(&[seg("Bridge")], &limits, |_, _, _| Some(rec));
        let v = v.expect("must reject overweight");
        assert!(v.detail.contains("weight"));
        assert!(v.detail.contains("10.00 t"));
    }

    #[test]
    fn zero_posted_limit_never_blocks() {
        let limits = VehicleLimits {
            height_m: Some(4.0),
            width_m: Some(3.0),
            total_weight_kg: Some(40_000.0),
            ..Default::default()
        };
        let v = find_route_vehicle_violation(&[seg("Open")], &limits, |_, _, _| Some(rec(0)));
        assert!(v.is_none());
    }

    #[test]
    fn parse_segments_json_roundtrip() {
        let j =
            r#"[{"mwm":"Norway_Hedmark","fid":42,"seg":3,"name":"Atna","lat":61.5,"lon":10.8}]"#;
        let s = parse_vehicle_segments_json(j).unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].feature_id, 42);
        assert_eq!(s[0].segment_index, 3);
        assert_eq!(s[0].road_name, "Atna");
    }
}
