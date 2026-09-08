//! Corridor proximity + active/inactive split for DATEX situations.

use chrono::{DateTime, Utc};

use crate::poi::CorridorBand;

use super::parse::DatexSituation;

/// Situations near the route, split by validity at `now`.
#[derive(Debug, Clone, Default)]
pub struct DatexCorridorView {
    /// `is_active_at(now)` — intended for map overlay rendering.
    pub active: Vec<DatexSituation>,
    /// Parsed and on-corridor, but outside the validity window (upcoming / expired).
    pub inactive: Vec<DatexSituation>,
}

/// Keep situations whose primary (or any) geometry point lies within `margin_m`
/// of `route_lat_lon`, using the same [`CorridorBand`] approach as overnight
/// building pre-filter.
pub fn filter_near_route(
    situations: &[DatexSituation],
    route_lat_lon: &[(f64, f64)],
    margin_m: f64,
) -> Vec<DatexSituation> {
    if route_lat_lon.is_empty() {
        return Vec::new();
    }
    let band = CorridorBand::from_lat_lon(route_lat_lon, margin_m);
    situations
        .iter()
        .filter(|s| s.geometry.iter().any(|&(lat, lon)| band.contains(lat, lon)))
        .cloned()
        .collect()
}

/// Split corridor-relevant situations into active (overlay) vs inactive (lists).
pub fn split_active_inactive(
    situations: &[DatexSituation],
    now: DateTime<Utc>,
) -> DatexCorridorView {
    let mut view = DatexCorridorView::default();
    for s in situations {
        if s.is_active_at(now) {
            view.active.push(s.clone());
        } else {
            view.inactive.push(s.clone());
        }
    }
    view
}

/// Filter to corridor, then split by validity.
pub fn corridor_view(
    situations: &[DatexSituation],
    route_lat_lon: &[(f64, f64)],
    margin_m: f64,
    now: DateTime<Utc>,
) -> DatexCorridorView {
    let near = filter_near_route(situations, route_lat_lon, margin_m);
    split_active_inactive(&near, now)
}
