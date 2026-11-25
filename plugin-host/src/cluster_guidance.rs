//! Assemble `navi.guidance.v1` snapshots and merge approach warnings.
//!
//! Warning merge order matches the Compose HUD: tagged road sign (including
//! explicit `142`) outranks children proximity, which outranks speed cameras.
//! Opted-out cameras never become the primary warning.

use serde_json::{json, Value};

/// Idle / no-route guidance document (same shape as [`crate::HostApi`] default).
pub fn idle_nav_guidance_v1() -> String {
    r#"{"schema":"navi.guidance.v1","route_active":false,"position":null,"speed_limit_kmh":null,"overspeed":false,"maneuver":null,"warning":null,"trip":null,"rest":null,"eco":{"enabled":false,"profile":null},"attitude":null,"fuel":{"l_per_scandinavian_mil":null,"l_per_us_mile":null},"route":{"polyline":""}}"#.into()
}

/// Inputs the Android / Linux host already has when building the Drive HUD.
#[derive(Debug, Clone, Default)]
pub struct GuidanceAssemblyInput {
    pub ts: Option<String>,
    pub route_active: bool,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub heading_deg: Option<f64>,
    pub speed_kmh: Option<f64>,
    pub h_acc_m: Option<f64>,
    pub speed_limit_kmh: Option<f64>,
    pub overspeed: bool,
    /// Maneuver type wire id (`turn_right`, `straight`, …). None = hide.
    pub maneuver_type: Option<String>,
    pub maneuver_icon_key: Option<String>,
    pub maneuver_distance_m: Option<f64>,
    pub maneuver_street_name: Option<String>,
    pub eco_enabled: bool,
    pub eco_profile: Option<String>,
    pub trip_eta: Option<String>,
    pub trip_remaining_duration_s: Option<f64>,
    pub trip_remaining_distance_m: Option<f64>,
    pub rest_active: bool,
    pub rest_minutes_to_break: Option<f64>,
    pub rest_distance_to_break_m: Option<f64>,
    pub attitude_yaw_deg: Option<f64>,
    pub attitude_roll_deg: Option<f64>,
    pub attitude_pitch_deg: Option<f64>,
    pub attitude_heading_deg: Option<f64>,
    pub imu_calibrated: Option<bool>,
    pub route_polyline: String,
    /// Pre-merged primary warning object, or null.
    pub warning: Option<Value>,
}

/// Build a `navi.guidance.v1` JSON string from host HUD inputs.
pub fn assemble_nav_guidance_v1(input: &GuidanceAssemblyInput) -> String {
    let position = match (input.lat, input.lon) {
        (Some(lat), Some(lon)) => Some(json!({
            "lat": lat,
            "lon": lon,
            "heading_deg": input.heading_deg,
            "speed_kmh": input.speed_kmh,
            "h_acc_m": input.h_acc_m,
        })),
        _ => None,
    };

    let maneuver = if input.route_active {
        match (
            input.maneuver_type.as_ref(),
            input.maneuver_distance_m,
            input.maneuver_icon_key.as_ref(),
        ) {
            (Some(ty), Some(dist), Some(icon)) if dist.is_finite() => Some(json!({
                "type": ty,
                "icon_key": icon,
                "distance_m": dist,
                "street_name": input.maneuver_street_name,
            })),
            _ => None,
        }
    } else {
        None
    };

    let (trip, rest) = if input.route_active {
        let trip = if input.trip_eta.is_some()
            || input.trip_remaining_duration_s.is_some()
            || input.trip_remaining_distance_m.is_some()
        {
            Some(json!({
                "eta": input.trip_eta,
                "remaining_duration_s": input.trip_remaining_duration_s,
                "remaining_distance_m": input.trip_remaining_distance_m,
            }))
        } else {
            None
        };
        let rest = if input.rest_active {
            Some(json!({
                "active": true,
                "minutes_to_break": input.rest_minutes_to_break,
                "distance_to_break_m": input.rest_distance_to_break_m,
            }))
        } else {
            Some(json!({
                "active": false,
                "minutes_to_break": null,
                "distance_to_break_m": null,
            }))
        };
        (trip, rest)
    } else {
        (None, None)
    };

    let attitude = if input.attitude_yaw_deg.is_some()
        || input.attitude_roll_deg.is_some()
        || input.attitude_heading_deg.is_some()
    {
        Some(json!({
            "yaw_deg": input.attitude_yaw_deg,
            "roll_deg": input.attitude_roll_deg,
            "pitch_deg": input.attitude_pitch_deg,
            "heading_deg": input.attitude_heading_deg,
            "imu_calibrated": input.imu_calibrated,
        }))
    } else {
        None
    };

    let doc = json!({
        "schema": "navi.guidance.v1",
        "ts": input.ts,
        "route_active": input.route_active,
        "position": position,
        "speed_limit_kmh": input.speed_limit_kmh,
        "overspeed": input.overspeed,
        "maneuver": maneuver,
        "warning": input.warning.clone(),
        "trip": trip,
        "rest": rest,
        "eco": {
            "enabled": input.eco_enabled,
            "profile": input.eco_profile,
        },
        "attitude": attitude,
        "fuel": {
            "l_per_scandinavian_mil": null,
            "l_per_us_mile": null,
        },
        "route": {
            "polyline": if input.route_active {
                input.route_polyline.as_str()
            } else {
                ""
            },
        },
    });
    doc.to_string()
}

/// Merge UniFFI warning JSON blobs (`{}` when none) into one primary warning.
///
/// Priority: tagged road sign > children proximity > speed camera.
/// When `cameras_opted_in` is false, camera JSON is ignored even if non-empty.
pub fn merge_primary_warning(
    road_sign_json: &str,
    children_json: &str,
    camera_json: &str,
    cameras_opted_in: bool,
) -> Option<Value> {
    if let Some(mut w) = parse_warning_obj(road_sign_json) {
        ensure_category(&mut w, "road_sign");
        if w.get("source").is_none() {
            w.as_object_mut()
                .map(|o| o.insert("source".into(), json!("traffic_sign")));
        }
        return Some(w);
    }
    if let Some(mut w) = parse_warning_obj(children_json) {
        ensure_category(&mut w, "children_proximity");
        if w.get("source").is_none() {
            w.as_object_mut()
                .map(|o| o.insert("source".into(), json!("children_proximity")));
        }
        return Some(w);
    }
    if cameras_opted_in {
        if let Some(mut w) = parse_warning_obj(camera_json) {
            let kind = w.get("kind").and_then(|k| k.as_str()).unwrap_or("point");
            let cat = if kind.contains("section") {
                "speed_camera_section"
            } else {
                "speed_camera_point"
            };
            ensure_category(&mut w, cat);
            return Some(w);
        }
    }
    None
}

fn parse_warning_obj(raw: &str) -> Option<Value> {
    let v: Value = serde_json::from_str(raw.trim()).ok()?;
    let obj = v.as_object()?;
    if obj.is_empty() {
        return None;
    }
    // Hidden / empty phase means chrome is off.
    if let Some("hidden") = obj.get("phase").and_then(|p| p.as_str()) {
        return None;
    }
    Some(v)
}

fn ensure_category(w: &mut Value, category: &str) {
    if let Some(obj) = w.as_object_mut() {
        obj.entry("category".to_string())
            .or_insert_with(|| json!(category));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tagged_109_outranks_children_and_camera() {
        let sign = r#"{"phase":"appear","distance_m":410,"code":"109","icon_key":"no_sign_109","name_en":"School","label":"School"}"#;
        let children = r#"{"phase":"urgency","distance_m":67,"code":"142","icon_key":"no_sign_142","source":"children_proximity","label":"Children zone"}"#;
        let camera = r#"{"kind":"point","phase":"appear","distance_m":200,"label":"Camera"}"#;
        let w = merge_primary_warning(sign, children, camera, true).unwrap();
        assert_eq!(w.get("code").and_then(|c| c.as_str()), Some("109"));
        assert_eq!(
            w.get("category").and_then(|c| c.as_str()),
            Some("road_sign")
        );
    }

    #[test]
    fn children_used_when_no_sign() {
        let children = r#"{"phase":"urgency","distance_m":67,"code":"142","icon_key":"no_sign_142","source":"children_proximity","label":"Children zone: Vallset"}"#;
        let w = merge_primary_warning("{}", children, "{}", true).unwrap();
        assert_eq!(w.get("code").and_then(|c| c.as_str()), Some("142"));
        assert_eq!(
            w.get("category").and_then(|c| c.as_str()),
            Some("children_proximity")
        );
    }

    #[test]
    fn opted_out_cameras_never_appear() {
        let camera = r#"{"kind":"point","phase":"appear","distance_m":50,"label":"Camera"}"#;
        assert!(merge_primary_warning("{}", "{}", camera, false).is_none());
        let w = merge_primary_warning("{}", "{}", camera, true).unwrap();
        assert_eq!(
            w.get("category").and_then(|c| c.as_str()),
            Some("speed_camera_point")
        );
    }

    #[test]
    fn hidden_phase_clears_warning() {
        let sign = r#"{"phase":"hidden","distance_m":10,"code":"109","icon_key":"no_sign_109"}"#;
        assert!(merge_primary_warning(sign, "{}", "{}", true).is_none());
    }

    #[test]
    fn no_route_clears_trip_and_rest() {
        let mut input = GuidanceAssemblyInput {
            route_active: false,
            trip_eta: Some("2026-01-01T00:00:00Z".into()),
            rest_active: true,
            rest_minutes_to_break: Some(12.0),
            warning: Some(json!({"code":"109","phase":"appear","category":"road_sign"})),
            ..GuidanceAssemblyInput::default()
        };
        let json = assemble_nav_guidance_v1(&input);
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["route_active"], false);
        assert!(v["trip"].is_null());
        assert!(v["rest"].is_null());
        // Warnings may remain on idle GPS (match map behaviour).
        assert_eq!(v["warning"]["code"], "109");

        input.route_active = true;
        input.rest_active = false;
        let json = assemble_nav_guidance_v1(&input);
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["rest"]["active"], false);
        assert!(v["rest"]["minutes_to_break"].is_null());
    }
}
