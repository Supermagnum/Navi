//! Multi-day overnight segmentation for car / motorcycle / cycle / mobilehome.
//!
//! Soft wellbeing guidance (not EC 561). Mirrors the hiking / truck day-budget
//! → overnight-at-boundary pattern with simpler lodging/camping/rest-area POI
//! matching than hiking hut scoring.

use crate::config::{CarRestParams, CyclingRestParams, Profile};

/// How the daily budget is measured.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MotorDailyBudget {
    /// Driving hours (car / motorcycle / mobilehome).
    Hours(f64),
    /// Distance kilometres (cycling).
    DistanceKm(f64),
}

/// Candidate overnight stop along a planned corridor (km from start).
#[derive(Debug, Clone)]
pub struct MotorOvernightCandidate {
    pub along_km: f64,
    pub lat: f64,
    pub lon: f64,
    pub name: String,
    pub kind: MotorOvernightKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotorOvernightKind {
    Lodging,
    Camping,
    RestArea,
    None,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MotorOvernightStop {
    pub kind: MotorOvernightKind,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub name: Option<String>,
    pub poi_found: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MotorDaySegment {
    pub day_index: u32,
    pub start_km: f64,
    pub end_km: f64,
    /// Driving hours for this day (hours-budget mode), or estimated from speed.
    pub driving_hours: f64,
    pub distance_km: f64,
    /// Overnight after this day (absent on last day).
    pub overnight: Option<MotorOvernightStop>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MotorMultiDayPlan {
    pub days: Vec<MotorDaySegment>,
    pub multi_day: bool,
    pub budget: MotorDailyBudget,
}

fn pick_candidate_near(
    candidates: &[MotorOvernightCandidate],
    target_km: f64,
    radius_km: f64,
) -> Option<&MotorOvernightCandidate> {
    // Preference: Lodging > Camping > RestArea (lower score wins).
    let rank = |k: MotorOvernightKind| -> f64 {
        match k {
            MotorOvernightKind::Lodging => 0.0,
            MotorOvernightKind::Camping => 1.0,
            MotorOvernightKind::RestArea => 2.0,
            MotorOvernightKind::None => 9.0,
        }
    };
    let mut best: Option<&MotorOvernightCandidate> = None;
    let mut best_score = f64::INFINITY;
    for c in candidates {
        let d = (c.along_km - target_km).abs();
        if d > radius_km {
            continue;
        }
        let score = d + rank(c.kind) * 5.0;
        if score < best_score {
            best_score = score;
            best = Some(c);
        }
    }
    best
}

fn choose_overnight(
    candidates: &[MotorOvernightCandidate],
    at_km: f64,
) -> MotorOvernightStop {
    let mut notes = Vec::new();
    let cand = pick_candidate_near(candidates, at_km, 25.0);
    if cand.is_none() {
        notes.push(
            "overnight_poi: no lodging/camping/rest stop within ~25 km of day boundary (informational)"
                .into(),
        );
    }
    MotorOvernightStop {
        kind: cand.map(|c| c.kind).unwrap_or(MotorOvernightKind::None),
        lat: cand.map(|c| c.lat),
        lon: cand.map(|c| c.lon),
        name: cand.map(|c| c.name.clone()),
        poi_found: cand.is_some(),
        notes,
    }
}

/// Soft daily budget for motor profiles that use car-style hours.
pub fn car_style_daily_hours(car: &CarRestParams) -> f64 {
    car.max_hours
        .filter(|h| *h > 0.0)
        .unwrap_or(crate::config::CAR_MAX_DAILY_HOURS)
}

/// Soft daily distance budget for cycling.
pub fn cycling_daily_km(cycling: &CyclingRestParams) -> f64 {
    if cycling.max_daily_distance_km > 0.0 {
        cycling.max_daily_distance_km
    } else {
        crate::config::CYCLING_MAX_DAILY_DISTANCE_KM
    }
}

/// Whether this profile uses soft motor multi-day overnight (not truck, not hiking).
pub fn uses_motor_multi_day(profile: Profile) -> bool {
    matches!(
        profile,
        Profile::Car
            | Profile::CarElectric
            | Profile::Motorcycle
            | Profile::MotorcycleElectric
            | Profile::MobileHome
            | Profile::Cycling
            | Profile::CyclingElectric
    )
}

/// Resolve the daily budget for a motor profile.
pub fn motor_daily_budget(
    profile: Profile,
    car: &CarRestParams,
    cycling: &CyclingRestParams,
) -> Option<MotorDailyBudget> {
    if !uses_motor_multi_day(profile) {
        return None;
    }
    if matches!(
        profile,
        Profile::Cycling | Profile::CyclingElectric
    ) {
        Some(MotorDailyBudget::DistanceKm(cycling_daily_km(cycling)))
    } else {
        Some(MotorDailyBudget::Hours(car_style_daily_hours(car)))
    }
}

/// Segment a motor trip into days with soft overnight suggestions between them.
///
/// Matching is intentionally simpler than hiking hut scoring: nearest lodging /
/// camping / rest area within a fixed radius of the day-boundary kilometre.
pub fn plan_motor_multi_day(
    budget: MotorDailyBudget,
    total_driving_hours: f64,
    total_dist_km: f64,
    candidates: &[MotorOvernightCandidate],
) -> MotorMultiDayPlan {
    let speed = if total_driving_hours > 1e-6 {
        (total_dist_km / total_driving_hours).max(1.0)
    } else {
        70.0
    };

    let (day_units_total, day_cap) = match budget {
        MotorDailyBudget::Hours(h) => (total_driving_hours.max(0.0), h.max(0.25)),
        MotorDailyBudget::DistanceKm(km) => (total_dist_km.max(0.0), km.max(1.0)),
    };

    let mut remaining = day_units_total;
    let mut km_cursor = 0.0;
    let mut days = Vec::new();
    let mut day_index = 1u32;

    for _ in 0..40 {
        if remaining <= 1e-6 {
            break;
        }
        let take = remaining.min(day_cap);
        let (drive_h, dist) = match budget {
            MotorDailyBudget::Hours(_) => {
                let d = (take * speed).min(total_dist_km - km_cursor).max(0.0);
                (take, d)
            }
            MotorDailyBudget::DistanceKm(_) => {
                let d = take.min(total_dist_km - km_cursor).max(0.0);
                let h = if speed > 1e-6 { d / speed } else { 0.0 };
                (h, d)
            }
        };
        let start_km = km_cursor;
        let end_km = (km_cursor + dist).min(total_dist_km);
        km_cursor = end_km;
        remaining -= take;

        let overnight = if remaining > 1e-6 && end_km < total_dist_km - 1e-6 {
            Some(choose_overnight(candidates, end_km))
        } else {
            None
        };

        days.push(MotorDaySegment {
            day_index,
            start_km,
            end_km,
            driving_hours: drive_h,
            distance_km: end_km - start_km,
            overnight,
        });
        day_index += 1;

        if km_cursor >= total_dist_km - 1e-6 && remaining <= 1e-6 {
            break;
        }
    }

    // Ensure we cover the full distance if floating-point left a stub.
    if !days.is_empty() {
        let last = days.len() - 1;
        if days[last].end_km < total_dist_km - 0.01 {
            days[last].end_km = total_dist_km;
            days[last].distance_km = total_dist_km - days[last].start_km;
            days[last].overnight = None;
        }
    }

    let multi_day = days.len() > 1;
    MotorMultiDayPlan {
        days,
        multi_day,
        budget,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CarRestParams, CyclingRestParams, Profile};

    #[test]
    fn short_trip_stays_single_day() {
        let plan = plan_motor_multi_day(MotorDailyBudget::Hours(8.0), 4.0, 320.0, &[]);
        assert!(!plan.multi_day);
        assert_eq!(plan.days.len(), 1);
        assert!(plan.days[0].overnight.is_none());
    }

    #[test]
    fn long_car_trip_inserts_overnight() {
        let plan = plan_motor_multi_day(MotorDailyBudget::Hours(8.0), 16.0, 1280.0, &[]);
        assert!(plan.multi_day, "expected multi-day: {:?}", plan.days);
        assert!(plan.days.len() >= 2);
        let o = plan.days[0]
            .overnight
            .as_ref()
            .expect("overnight after day 1");
        assert!(!o.poi_found);
        assert_eq!(o.kind, MotorOvernightKind::None);
        let sum_h: f64 = plan.days.iter().map(|d| d.driving_hours).sum();
        assert!((sum_h - 16.0).abs() < 1e-6, "sum_h={sum_h}");
        for d in &plan.days {
            assert!(d.driving_hours <= 8.0 + 1e-6, "day {:?}", d);
        }
    }

    #[test]
    fn cycle_trip_splits_on_distance() {
        let plan = plan_motor_multi_day(MotorDailyBudget::DistanceKm(100.0), 12.0, 220.0, &[]);
        assert!(plan.multi_day);
        assert!(plan.days.len() >= 3 || plan.days.len() == 2);
        assert!(plan.days[0].distance_km <= 100.0 + 1e-6);
        assert!(plan.days[0].overnight.is_some());
    }

    #[test]
    fn lodging_candidate_attached_near_boundary() {
        // 8 h @ 80 km/h → boundary at 640 km.
        let cands = vec![MotorOvernightCandidate {
            along_km: 635.0,
            lat: 60.5,
            lon: 11.0,
            name: "Test Hotel".into(),
            kind: MotorOvernightKind::Lodging,
        }];
        let plan = plan_motor_multi_day(MotorDailyBudget::Hours(8.0), 16.0, 1280.0, &cands);
        let o = plan.days[0].overnight.as_ref().unwrap();
        assert!(o.poi_found);
        assert_eq!(o.kind, MotorOvernightKind::Lodging);
        assert_eq!(o.name.as_deref(), Some("Test Hotel"));
    }

    #[test]
    fn prefers_lodging_over_rest_area_when_both_near() {
        let cands = vec![
            MotorOvernightCandidate {
                along_km: 640.0,
                lat: 60.0,
                lon: 10.0,
                name: "Rest".into(),
                kind: MotorOvernightKind::RestArea,
            },
            MotorOvernightCandidate {
                along_km: 642.0,
                lat: 60.1,
                lon: 10.1,
                name: "Hotel".into(),
                kind: MotorOvernightKind::Lodging,
            },
        ];
        let plan = plan_motor_multi_day(MotorDailyBudget::Hours(8.0), 16.0, 1280.0, &cands);
        let o = plan.days[0].overnight.as_ref().unwrap();
        assert_eq!(o.kind, MotorOvernightKind::Lodging);
        assert_eq!(o.name.as_deref(), Some("Hotel"));
    }

    #[test]
    fn motorcycle_uses_same_hours_budget_as_car() {
        let car = CarRestParams::default();
        let cycling = CyclingRestParams::default();
        let b_car = motor_daily_budget(Profile::Car, &car, &cycling).unwrap();
        let b_moto = motor_daily_budget(Profile::Motorcycle, &car, &cycling).unwrap();
        assert_eq!(b_car, b_moto);
        assert_eq!(b_car, MotorDailyBudget::Hours(8.0));
    }

    #[test]
    fn cycling_budget_is_distance() {
        let car = CarRestParams::default();
        let cycling = CyclingRestParams::default();
        let b = motor_daily_budget(Profile::Cycling, &car, &cycling).unwrap();
        assert_eq!(b, MotorDailyBudget::DistanceKm(100.0));
    }
}
