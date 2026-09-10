//! Trip-bbox pad schedule for adaptive plan widen-retry.
//!
//! Initial pad matches historical `plan_car_route_inner` behaviour; widen doubles
//! until [`PLAN_BBOX_PAD_CAP_DEG`] so RAM stays bounded on Automotive devices.

/// Initial pad: `span * 0.35` clamped to this band (degrees).
pub const PLAN_BBOX_PAD_MIN_DEG: f64 = 0.35;
pub const PLAN_BBOX_PAD_INITIAL_MAX_DEG: f64 = 2.5;
/// Hard cap for widen-retry (degrees). Not unbounded.
pub const PLAN_BBOX_PAD_CAP_DEG: f64 = 5.0;

/// Build the pad attempt list for an OD pair (degrees of lat/lon pad).
///
/// Always includes the initial pad; then doubles until the cap (inclusive once).
pub fn plan_bbox_pad_schedule(
    start_lat: f64,
    start_lon: f64,
    end_lat: f64,
    end_lon: f64,
) -> Vec<f64> {
    plan_bbox_pad_schedule_points(&[(start_lat, start_lon), (end_lat, end_lon)])
}

/// Pad schedule from the axis-aligned span of all route points (start, vias, end).
pub fn plan_bbox_pad_schedule_points(points: &[(f64, f64)]) -> Vec<f64> {
    let (min_lat, min_lon, max_lat, max_lon) = points_bounds(points);
    let lat_span = (max_lat - min_lat).abs();
    let lon_span = (max_lon - min_lon).abs();
    let mut pad =
        (lat_span.max(lon_span) * 0.35).clamp(PLAN_BBOX_PAD_MIN_DEG, PLAN_BBOX_PAD_INITIAL_MAX_DEG);
    let mut out = Vec::with_capacity(4);
    loop {
        out.push(pad);
        if pad >= PLAN_BBOX_PAD_CAP_DEG - 1e-9 {
            break;
        }
        let next = (pad * 2.0).min(PLAN_BBOX_PAD_CAP_DEG);
        if (next - pad).abs() < 1e-9 {
            break;
        }
        pad = next;
    }
    out
}

pub fn trip_bbox(start_lat: f64, start_lon: f64, end_lat: f64, end_lon: f64, pad: f64) -> [f64; 4] {
    trip_bbox_points(&[(start_lat, start_lon), (end_lat, end_lon)], pad)
}

/// Axis-aligned bbox covering all points, expanded by `pad` degrees.
pub fn trip_bbox_points(points: &[(f64, f64)], pad: f64) -> [f64; 4] {
    let (min_lat, min_lon, max_lat, max_lon) = points_bounds(points);
    [min_lat - pad, min_lon - pad, max_lat + pad, max_lon + pad]
}

fn points_bounds(points: &[(f64, f64)]) -> (f64, f64, f64, f64) {
    let mut min_lat = f64::INFINITY;
    let mut min_lon = f64::INFINITY;
    let mut max_lat = f64::NEG_INFINITY;
    let mut max_lon = f64::NEG_INFINITY;
    for &(lat, lon) in points {
        min_lat = min_lat.min(lat);
        min_lon = min_lon.min(lon);
        max_lat = max_lat.max(lat);
        max_lon = max_lon.max(lon);
    }
    if !min_lat.is_finite() {
        (0.0, 0.0, 0.0, 0.0)
    } else {
        (min_lat, min_lon, max_lat, max_lon)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_starts_clamped_and_caps() {
        let pads = plan_bbox_pad_schedule(60.0, 10.0, 60.01, 10.01);
        assert!((pads[0] - PLAN_BBOX_PAD_MIN_DEG).abs() < 1e-9);
        assert!(*pads.last().unwrap() <= PLAN_BBOX_PAD_CAP_DEG + 1e-9);
        assert!(pads.windows(2).all(|w| w[1] >= w[0]));
    }

    #[test]
    fn schedule_is_finite() {
        let pads = plan_bbox_pad_schedule(50.0, 5.0, 70.0, 25.0);
        assert!(pads.len() <= 8);
        assert!((pads[0] - PLAN_BBOX_PAD_INITIAL_MAX_DEG).abs() < 1e-9);
    }

    #[test]
    fn points_bbox_includes_via() {
        let pts = [(60.0, 10.0), (61.0, 11.0), (60.5, 12.0)];
        let b = trip_bbox_points(&pts, 0.1);
        assert!((b[0] - 59.9).abs() < 1e-9);
        assert!((b[1] - 9.9).abs() < 1e-9);
        assert!((b[2] - 61.1).abs() < 1e-9);
        assert!((b[3] - 12.1).abs() < 1e-9);
    }
}
