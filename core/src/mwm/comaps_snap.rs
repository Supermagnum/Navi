//! Nearest-road suggestion for `StartNotOnRoad` / `EndNotOnRoad` (Phase 5, A2).
//!
//! Policy: the router never snaps start or destination by itself. When CoMaps
//! reports that an endpoint is not on a routable road the client may ask for a
//! [`SnapSuggestion`], show the snap point and its distance, and only route to
//! it after the user confirms. [`SnapRole::from_code`] is the single gate: it
//! yields a role for exactly those two codes and `None` for everything else.

use super::comaps_router::{
    ComapsLatLon, ComapsProfile, ComapsRouter, ComapsRouterCode, ComapsRouterError, ComapsSnap,
};

/// Default lookup radius for the nearest-road suggestion (metres).
pub const DEFAULT_SNAP_RADIUS_M: f64 = 3_000.0;

/// Which endpoint failed to match a road.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapRole {
    Start,
    End,
}

impl SnapRole {
    /// Only `StartNotOnRoad` / `EndNotOnRoad` may lead to a snap suggestion.
    pub fn from_code(code: ComapsRouterCode) -> Option<Self> {
        match code {
            ComapsRouterCode::StartNotOnRoad => Some(Self::Start),
            ComapsRouterCode::EndNotOnRoad => Some(Self::End),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Start => "Start",
            Self::End => "Destination",
        }
    }
}

/// A proposed replacement endpoint. Never applied without user confirmation.
#[derive(Debug, Clone, PartialEq)]
pub struct SnapSuggestion {
    pub role: SnapRole,
    pub original: ComapsLatLon,
    pub snap: ComapsSnap,
    /// Short English summary; the app localizes its own wording from the fields.
    pub message: String,
}

/// Distance formatted for the confirmation prompt (`85 m`, `1.4 km`).
pub fn format_snap_distance(distance_m: f64) -> String {
    if distance_m < 1000.0 {
        format!("{} m", distance_m.round() as i64)
    } else {
        format!("{:.1} km", distance_m / 1000.0)
    }
}

/// Pure assembly of a suggestion from a native snap result.
pub fn build_suggestion(
    role: SnapRole,
    original: ComapsLatLon,
    snap: ComapsSnap,
) -> SnapSuggestion {
    let message = format!(
        "{} is not on a drivable road. Nearest road is {} away.",
        role.label(),
        format_snap_distance(snap.distance_m)
    );
    SnapSuggestion {
        role,
        original,
        snap,
        message,
    }
}

/// Ask CoMaps for the nearest road after a route failed with `code`.
///
/// Returns `Ok(None)` when `code` is not a not-on-road code (no snap is
/// attempted) or when no road lies within `radius_m`.
pub fn suggest_snap(
    router: &ComapsRouter,
    profile: ComapsProfile,
    code: ComapsRouterCode,
    point: ComapsLatLon,
    radius_m: f64,
) -> Result<Option<SnapSuggestion>, ComapsRouterError> {
    let Some(role) = SnapRole::from_code(code) else {
        return Ok(None);
    };
    let snap = router.snap_to_road(profile, point.clone(), radius_m)?;
    Ok(snap.map(|s| build_suggestion(role, point, s)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(distance_m: f64) -> ComapsSnap {
        ComapsSnap {
            lat: 61.386,
            lon: 8.812,
            distance_m,
            feature_id: 42,
            segment_idx: 3,
            mwm_name: Some("Norway_Oppland".into()),
        }
    }

    #[test]
    fn role_only_for_not_on_road_codes() {
        assert_eq!(
            SnapRole::from_code(ComapsRouterCode::StartNotOnRoad),
            Some(SnapRole::Start)
        );
        assert_eq!(
            SnapRole::from_code(ComapsRouterCode::EndNotOnRoad),
            Some(SnapRole::End)
        );
        for code in [
            ComapsRouterCode::Ok,
            ComapsRouterCode::NotInit,
            ComapsRouterCode::RouteNotFound,
            ComapsRouterCode::NeedMoreMaps,
            ComapsRouterCode::Cancelled,
            ComapsRouterCode::Internal,
            ComapsRouterCode::Unsupported,
            ComapsRouterCode::Unavailable,
            ComapsRouterCode::RoadClosed,
        ] {
            assert_eq!(SnapRole::from_code(code), None, "{code:?}");
        }
    }

    #[test]
    fn distance_formatting() {
        assert_eq!(format_snap_distance(84.6), "85 m");
        assert_eq!(format_snap_distance(999.0), "999 m");
        assert_eq!(format_snap_distance(1_449.0), "1.4 km");
    }

    #[test]
    fn suggestion_keeps_original_and_snap_separate() {
        let original = ComapsLatLon {
            lat: 61.45,
            lon: 8.95,
        };
        let s = build_suggestion(SnapRole::End, original.clone(), snap(1_200.0));
        assert_eq!(s.original, original);
        assert_eq!(s.snap.lat, 61.386);
        assert!(s.message.contains("Destination"));
        assert!(s.message.contains("1.2 km"));
    }
}
