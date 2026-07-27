//! Multi-day truck trip segmentation (EC 561 daily / weekly rest).
//!
//! Mirrors the hiking `plan_multi_day` pattern (day budget → overnight at
//! boundary → next day) but budgets in **driving hours** instead of kilometres.

use crate::config::{
    civil_date_add_days, outstanding_weekly_rest_compensations, record_reduced_weekly_compensation,
    try_repay_weekly_rest_compensation, TruckDrivingHistory, TruckRestKind, TruckRestParams,
};

/// Facility tier for truck overnight preference (higher is better).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TruckRestFacility {
    /// HGV-allowed parking without highway rest/services tags.
    HgvParking = 0,
    /// `highway=rest_area`.
    RestArea = 1,
    /// `highway=services` (fuller facilities).
    Services = 2,
}

/// Candidate rest / services stop along a planned corridor (km from start).
#[derive(Debug, Clone)]
pub struct TruckRestCandidate {
    pub along_km: f64,
    pub lat: f64,
    pub lon: f64,
    pub name: String,
    /// Lateral / off-route distance from the corridor sample used to discover this POI (km).
    pub detour_km: f64,
    pub facility: TruckRestFacility,
    /// True when OSM suggests facilities suitable for a full 45 h weekly rest
    /// (e.g. `highway=services`); bare `rest_area` / HGV parking may be false.
    pub suitable_for_weekly: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TruckOvernightKind {
    DailyRegular,
    DailyReduced,
    DailySplit,
    WeeklyRegular,
    WeeklyReduced,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TruckOvernightRest {
    pub kind: TruckOvernightKind,
    pub hours: f64,
    pub split_parts: Option<(f64, f64)>,
    /// Regular 45 h weekly rest must not be taken in the cab.
    pub not_in_cab: bool,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub name: Option<String>,
    pub poi_found: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TruckDaySegment {
    pub day_index: u32,
    pub date: String,
    pub start_km: f64,
    pub end_km: f64,
    pub driving_hours: f64,
    pub used_daily_extension: bool,
    /// Rest taken **after** this day before the next driving day (absent on last day).
    pub overnight: Option<TruckOvernightRest>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TruckMultiDayPlan {
    pub days: Vec<TruckDaySegment>,
    pub multi_day: bool,
}

fn hours_on_date(history: &TruckDrivingHistory, date: &str) -> f64 {
    history
        .days
        .iter()
        .find(|d| d.date == date)
        .map(|d| d.driving_hours)
        .unwrap_or(0.0)
}

fn extensions_used(history: &TruckDrivingHistory, week_id: &str) -> u32 {
    if history.extension_week_id == week_id {
        history.extensions_used_this_week
    } else {
        0
    }
}

/// Allowed driving hours for `date` given history (9 h, or 10 h if extension available).
pub fn truck_day_cap_hours(
    truck: &TruckRestParams,
    history: &TruckDrivingHistory,
    date: &str,
    week_id: &str,
    need_hours: f64,
) -> (f64, bool) {
    let already = hours_on_date(history, date);
    let base = truck.max_daily_driving_hours;
    let ext = truck.max_daily_driving_extended_hours;
    let used = extensions_used(history, week_id);
    let mut allowed = base;
    let mut used_ext = false;
    if already + need_hours > base + 1e-6 && used < truck.max_daily_extensions_per_week {
        allowed = ext;
        used_ext = true;
    }
    if truck.exceptional_extension_armed {
        allowed += truck.exceptional_extension_hours;
    }
    (allowed, used_ext && (already + need_hours > base + 1e-6))
}

/// How much farther a higher-tier facility may be (detour km) and still win.
const FACILITY_DETOUR_SLACK_KM: f64 = 8.0;

fn candidate_score(c: &TruckRestCandidate, target_km: f64, radius_km: f64) -> Option<f64> {
    let along_err = (c.along_km - target_km).abs();
    if along_err > radius_km {
        return None;
    }
    // Detour-weighted: lateral off-route distance dominates; along-route miss is soft.
    let mut score = c.detour_km + along_err * 0.15;
    // Facility preference within a similar detour band (services > rest_area > parking).
    let facility_bonus = match c.facility {
        TruckRestFacility::Services => FACILITY_DETOUR_SLACK_KM,
        TruckRestFacility::RestArea => FACILITY_DETOUR_SLACK_KM * 0.35,
        TruckRestFacility::HgvParking => 0.0,
    };
    score -= facility_bonus;
    Some(score)
}

fn pick_candidate_near(
    candidates: &[TruckRestCandidate],
    target_km: f64,
    radius_km: f64,
    prefer_weekly_suitable: bool,
) -> Option<&TruckRestCandidate> {
    let mut best: Option<&TruckRestCandidate> = None;
    let mut best_score = f64::INFINITY;
    for c in candidates {
        let Some(mut score) = candidate_score(c, target_km, radius_km) else {
            continue;
        };
        if prefer_weekly_suitable && c.suitable_for_weekly {
            score -= 1.5;
        }
        if score < best_score {
            best_score = score;
            best = Some(c);
        }
    }
    best
}

/// Build a daily overnight, optionally preferring reduced 9 h when slots remain.
pub fn choose_daily_overnight_rest(
    truck: &TruckRestParams,
    history: &TruckDrivingHistory,
    candidates: &[TruckRestCandidate],
    at_km: f64,
    prefer_reduced: bool,
) -> TruckOvernightRest {
    let (kind, hours, split, mut notes) = if truck.prefer_split_daily_rest {
        (
            TruckOvernightKind::DailySplit,
            truck.split_daily_rest_first_hours + truck.split_daily_rest_second_hours,
            Some((
                truck.split_daily_rest_first_hours,
                truck.split_daily_rest_second_hours,
            )),
            vec!["daily_rest: split 3+9 h selected".into()],
        )
    } else if prefer_reduced
        && history.reduced_daily_rests_since_weekly < truck.max_reduced_daily_rests
    {
        (
            TruckOvernightKind::DailyReduced,
            truck.daily_rest_reduced_hours,
            None,
            vec![format!(
                "daily_rest: reduced {:.0} h ({}/{} used this weekly cycle before)",
                truck.daily_rest_reduced_hours,
                history.reduced_daily_rests_since_weekly,
                truck.max_reduced_daily_rests
            )],
        )
    } else {
        (
            TruckOvernightKind::DailyRegular,
            truck.daily_rest_hours,
            None,
            vec![format!(
                "daily_rest: regular {:.0} h (reduced slots remaining {}/{})",
                truck.daily_rest_hours,
                truck
                    .max_reduced_daily_rests
                    .saturating_sub(history.reduced_daily_rests_since_weekly),
                truck.max_reduced_daily_rests
            )],
        )
    };

    let cand = pick_candidate_near(candidates, at_km, 25.0, false);
    if cand.is_none() {
        notes.push(
            "daily_rest_poi: no RestArea/services within ~25 km of day boundary (informational)"
                .into(),
        );
    }
    TruckOvernightRest {
        kind,
        hours,
        split_parts: split,
        not_in_cab: false,
        lat: cand.map(|c| c.lat),
        lon: cand.map(|c| c.lon),
        name: cand.map(|c| c.name.clone()),
        poi_found: cand.is_some(),
        notes,
    }
}

fn choose_weekly_overnight(
    truck: &TruckRestParams,
    history: &TruckDrivingHistory,
    candidates: &[TruckRestCandidate],
    at_km: f64,
) -> TruckOvernightRest {
    let mut notes = Vec::new();
    let pending = outstanding_weekly_rest_compensations(history);
    for d in &pending {
        notes.push(format!(
            "compensation_pending: reduced_weekly on {}; shortfall_h={:.0}; compensate_by={}",
            d.reduced_on_date, d.shortfall_hours, d.compensate_by_date
        ));
    }
    // Reduced 24 h every second week: if last weekly was Regular45, allow Reduced24 —
    // unless an unpaid compensation debt already exists (prefer regular to repay / avoid stacking).
    let use_reduced =
        matches!(history.last_weekly_rest, TruckRestKind::Regular45) && pending.is_empty();
    let (kind, hours, not_in_cab) = if use_reduced {
        let shortfall = (truck.weekly_rest_hours - truck.weekly_rest_reduced_hours).max(0.0);
        notes.push(format!(
            "weekly_rest: reduced {:.0} h (last weekly was regular {:.0} h; shortfall {:.0} h recorded on compensation ledger)",
            truck.weekly_rest_reduced_hours, truck.weekly_rest_hours, shortfall
        ));
        (
            TruckOvernightKind::WeeklyReduced,
            truck.weekly_rest_reduced_hours,
            false, // reduced weekly may remain in-vehicle if stationary
        )
    } else {
        if !pending.is_empty() && matches!(history.last_weekly_rest, TruckRestKind::Regular45) {
            notes.push(
                "weekly_rest: preferring regular 45 h because compensation debt is still outstanding"
                    .into(),
            );
        }
        notes.push(format!(
            "weekly_rest: regular {:.0} h; not_in_cab={} (do not treat bare roadside as sufficient)",
            truck.weekly_rest_hours, truck.regular_weekly_rest_not_in_cab
        ));
        (
            TruckOvernightKind::WeeklyRegular,
            truck.weekly_rest_hours,
            truck.regular_weekly_rest_not_in_cab,
        )
    };

    let cand = pick_candidate_near(candidates, at_km, 40.0, true);
    if cand.is_none() {
        notes.push(
            "weekly_rest_poi: no services/rest stop near boundary — plan continues; seek suitable accommodation (informational)".into(),
        );
    } else if not_in_cab && cand.is_some_and(|c| !c.suitable_for_weekly) {
        notes.push(
            "weekly_rest_poi: nearest stop may lack facilities for a full 45 h away-from-cab rest"
                .into(),
        );
    } else if let Some(c) = cand {
        notes.push(format!(
            "weekly_rest_poi: facility={:?}; detour_km={:.1}",
            c.facility, c.detour_km
        ));
    }

    TruckOvernightRest {
        kind,
        hours,
        split_parts: None,
        not_in_cab,
        lat: cand.map(|c| c.lat),
        lon: cand.map(|c| c.lon),
        name: cand.map(|c| c.name.clone()),
        poi_found: cand.is_some(),
        notes,
    }
}

/// Segment a truck trip into driving days with daily / weekly rests between them.
///
/// When total driving fits in the remaining capacity of `start_date`, returns a
/// single day with no overnight. Overnight stops are scored by detour distance
/// from the corridor plus facility tier (`services` preferred over bare
/// `rest_area` / HGV parking within a similar detour band).
pub fn plan_truck_multi_day(
    truck: &TruckRestParams,
    history: &TruckDrivingHistory,
    total_driving_hours: f64,
    total_dist_km: f64,
    start_date: &str,
    week_id: &str,
    candidates: &[TruckRestCandidate],
    prefer_reduced_daily_rest: bool,
) -> TruckMultiDayPlan {
    let speed = if total_driving_hours > 1e-6 {
        (total_dist_km / total_driving_hours).max(1.0)
    } else {
        70.0
    };

    let mut sim = history.clone();
    let mut date = start_date.to_string();
    let mut remaining = total_driving_hours.max(0.0);
    let mut km_cursor = 0.0;
    let mut days = Vec::new();
    let mut day_index = 1u32;

    // Cap pathological loops.
    for _ in 0..40 {
        if remaining <= 1e-6 {
            break;
        }

        let already = hours_on_date(&sim, &date);
        let base_room = (truck.max_daily_driving_hours - already).max(0.0);
        // Cap lookup: fill the ordinary 9 h day before extension applies to the remainder.
        let need_for_cap = if remaining > base_room + 1e-6 && base_room > 0.25 {
            base_room
        } else {
            remaining
        };
        let (cap, _) = truck_day_cap_hours(truck, &sim, &date, week_id, need_for_cap.max(1e-9));
        let room = (cap - already).max(0.0);

        if room < 0.25 {
            // No room left today — advance calendar day with a rest if we already drove.
            date = civil_date_add_days(&date, 1).unwrap_or(date);
            continue;
        }

        let drive = remaining.min(room);
        let used_ext = drive + already > truck.max_daily_driving_hours + 1e-6
            && extensions_used(&sim, week_id) < truck.max_daily_extensions_per_week;
        let start_km = km_cursor;
        let end_km = (km_cursor + drive * speed).min(total_dist_km);
        km_cursor = end_km;
        remaining -= drive;
        let driving_date = date.clone();

        // Simulate recording this day's driving for subsequent day caps / consecutive days.
        crate::config::record_truck_driving_hours(&mut sim, &driving_date, drive, week_id);
        if used_ext {
            if sim.extension_week_id != week_id {
                sim.extension_week_id = week_id.to_string();
                sim.extensions_used_this_week = 0;
            }
            sim.extensions_used_this_week = sim
                .extensions_used_this_week
                .saturating_add(1)
                .min(truck.max_daily_extensions_per_week);
        }

        let overnight = if remaining > 1e-6 {
            let weekly_due = sim.consecutive_working_days >= truck.max_consecutive_working_days;
            let rest = if weekly_due {
                let w = choose_weekly_overnight(truck, &sim, candidates, end_km);
                match w.kind {
                    TruckOvernightKind::WeeklyReduced => {
                        let shortfall =
                            (truck.weekly_rest_hours - truck.weekly_rest_reduced_hours).max(0.0);
                        record_reduced_weekly_compensation(&mut sim, &driving_date, shortfall);
                        sim.last_weekly_rest = TruckRestKind::Reduced24;
                    }
                    _ => {
                        let _ =
                            try_repay_weekly_rest_compensation(&mut sim, &driving_date, w.hours);
                        sim.last_weekly_rest = TruckRestKind::Regular45;
                    }
                }
                sim.consecutive_working_days = 0;
                sim.reduced_daily_rests_since_weekly = 0;
                w
            } else {
                let d = choose_daily_overnight_rest(
                    truck,
                    &sim,
                    candidates,
                    end_km,
                    prefer_reduced_daily_rest,
                );
                if d.kind == TruckOvernightKind::DailyReduced {
                    sim.reduced_daily_rests_since_weekly =
                        sim.reduced_daily_rests_since_weekly.saturating_add(1);
                }
                d
            };
            date = civil_date_add_days(&driving_date, 1).unwrap_or(driving_date.clone());
            Some(rest)
        } else {
            None
        };

        days.push(TruckDaySegment {
            day_index,
            date: driving_date,
            start_km,
            end_km,
            driving_hours: drive,
            used_daily_extension: used_ext,
            overnight,
        });
        day_index += 1;
    }

    let multi_day = days.len() > 1;
    TruckMultiDayPlan { days, multi_day }
}

/// Apply a multi-day plan to persisted history (one row per driving day).
pub fn commit_truck_multi_day_plan(
    truck: &mut TruckRestParams,
    history: &mut TruckDrivingHistory,
    plan: &TruckMultiDayPlan,
    week_id: &str,
) {
    for day in &plan.days {
        crate::config::record_truck_driving_hours(history, &day.date, day.driving_hours, week_id);
        if day.used_daily_extension {
            if history.extension_week_id != week_id {
                history.extension_week_id = week_id.to_string();
                history.extensions_used_this_week = 0;
            }
            history.extensions_used_this_week = history
                .extensions_used_this_week
                .saturating_add(1)
                .min(truck.max_daily_extensions_per_week);
        }
        if let Some(rest) = &day.overnight {
            match rest.kind {
                TruckOvernightKind::DailyReduced => {
                    history.reduced_daily_rests_since_weekly =
                        history.reduced_daily_rests_since_weekly.saturating_add(1);
                }
                TruckOvernightKind::WeeklyRegular => {
                    history.last_weekly_rest = TruckRestKind::Regular45;
                    history.consecutive_working_days = 0;
                    history.reduced_daily_rests_since_weekly = 0;
                    let _ = try_repay_weekly_rest_compensation(history, &day.date, rest.hours);
                }
                TruckOvernightKind::WeeklyReduced => {
                    history.last_weekly_rest = TruckRestKind::Reduced24;
                    history.consecutive_working_days = 0;
                    history.reduced_daily_rests_since_weekly = 0;
                    let shortfall =
                        (truck.weekly_rest_hours - truck.weekly_rest_reduced_hours).max(0.0);
                    record_reduced_weekly_compensation(history, &day.date, shortfall);
                }
                TruckOvernightKind::DailyRegular | TruckOvernightKind::DailySplit => {
                    // A long enough attached rest can repay (9 h + shortfall en bloc).
                    let _ = try_repay_weekly_rest_compensation(history, &day.date, rest.hours);
                }
            }
        }
    }
    truck.exceptional_extension_armed = false;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TruckDrivingHistory;

    #[test]
    fn short_trip_stays_single_day() {
        let truck = TruckRestParams::default();
        let history = TruckDrivingHistory::default();
        let plan = plan_truck_multi_day(
            &truck,
            &history,
            4.0,
            320.0,
            "2026-07-24",
            "2026-W30",
            &[],
            false,
        );
        assert!(!plan.multi_day);
        assert_eq!(plan.days.len(), 1);
        assert!(plan.days[0].overnight.is_none());
        assert!((plan.days[0].driving_hours - 4.0).abs() < 1e-9);
    }

    #[test]
    fn long_trip_inserts_daily_rest_between_days() {
        let truck = TruckRestParams::default();
        let history = TruckDrivingHistory::default();
        // 16 h @ 80 km/h → two 8 h days with 11 h rest between.
        let plan = plan_truck_multi_day(
            &truck,
            &history,
            16.0,
            1280.0,
            "2026-07-24",
            "2026-W30",
            &[],
            false,
        );
        assert!(plan.multi_day, "expected multi-day: {:?}", plan.days);
        assert!(plan.days.len() >= 2);
        let o = plan.days[0]
            .overnight
            .as_ref()
            .expect("daily rest after day 1");
        assert_eq!(o.kind, TruckOvernightKind::DailyRegular);
        assert!((o.hours - 11.0).abs() < 1e-9);
        assert!(!o.not_in_cab);
        let sum: f64 = plan.days.iter().map(|d| d.driving_hours).sum();
        assert!((sum - 16.0).abs() < 1e-6, "sum={sum}");
        for d in &plan.days {
            assert!(
                d.driving_hours <= truck.max_daily_driving_hours + 1e-6 || d.used_daily_extension,
                "day {d:?} exceeds 9 h without extension"
            );
        }
    }

    #[test]
    fn reduced_daily_rest_when_preferred() {
        let truck = TruckRestParams::default();
        let history = TruckDrivingHistory::default();
        let plan = plan_truck_multi_day(
            &truck,
            &history,
            16.0,
            1280.0,
            "2026-07-24",
            "2026-W30",
            &[],
            true,
        );
        let o = plan.days[0].overnight.as_ref().unwrap();
        assert_eq!(o.kind, TruckOvernightKind::DailyReduced);
        assert!((o.hours - 9.0).abs() < 1e-9);
    }

    #[test]
    fn split_daily_rest_when_configured() {
        let truck = TruckRestParams {
            prefer_split_daily_rest: true,
            ..Default::default()
        };
        let history = TruckDrivingHistory::default();
        let plan = plan_truck_multi_day(
            &truck,
            &history,
            16.0,
            1280.0,
            "2026-07-24",
            "2026-W30",
            &[],
            false,
        );
        let o = plan.days[0].overnight.as_ref().unwrap();
        assert_eq!(o.kind, TruckOvernightKind::DailySplit);
        assert_eq!(o.split_parts, Some((3.0, 9.0)));
    }

    #[test]
    fn weekly_rest_after_six_consecutive_working_days() {
        let truck = TruckRestParams::default();
        let mut history = TruckDrivingHistory {
            consecutive_working_days: 5,
            ..Default::default()
        };
        // First day of this plan bumps consecutive to 6 → weekly rest before day 2.
        let plan = plan_truck_multi_day(
            &truck,
            &history,
            16.0,
            1280.0,
            "2026-07-24",
            "2026-W30",
            &[],
            false,
        );
        assert!(plan.multi_day);
        let o = plan.days[0].overnight.as_ref().expect("rest after day 1");
        assert!(
            matches!(
                o.kind,
                TruckOvernightKind::WeeklyRegular | TruckOvernightKind::WeeklyReduced
            ),
            "expected weekly rest, got {:?}",
            o.kind
        );
        assert!(o.not_in_cab || o.kind == TruckOvernightKind::WeeklyReduced);
        assert!(
            (o.hours - 45.0).abs() < 1e-9 || (o.hours - 24.0).abs() < 1e-9,
            "hours={}",
            o.hours
        );

        // Commit and ensure weekly rest reset the streak (day 2 is one new working day).
        let mut truck2 = truck.clone();
        commit_truck_multi_day_plan(&mut truck2, &mut history, &plan, "2026-W30");
        assert_eq!(history.consecutive_working_days, 1);
    }

    #[test]
    fn rest_candidate_attached_when_near_boundary() {
        let truck = TruckRestParams::default();
        let history = TruckDrivingHistory::default();
        // Day1 = 9 h → 9*80 = 720 km boundary.
        let cands = vec![TruckRestCandidate {
            along_km: 715.0,
            lat: 61.0,
            lon: 10.0,
            name: "Test Services".into(),
            detour_km: 0.5,
            facility: TruckRestFacility::Services,
            suitable_for_weekly: true,
        }];
        let plan = plan_truck_multi_day(
            &truck,
            &history,
            16.0,
            1280.0,
            "2026-07-24",
            "2026-W30",
            &cands,
            false,
        );
        let o = plan.days[0].overnight.as_ref().unwrap();
        assert!(o.poi_found);
        assert_eq!(o.name.as_deref(), Some("Test Services"));
        assert!((o.lat.unwrap() - 61.0).abs() < 1e-9);
    }

    #[test]
    fn overnight_prefers_services_over_closer_bare_rest_area() {
        let truck = TruckRestParams::default();
        let history = TruckDrivingHistory::default();
        // Boundary ~720 km. Bare rest_area is closer (detour 1 km) but services at
        // 5 km detour should win within FACILITY_DETOUR_SLACK_KM.
        let cands = vec![
            TruckRestCandidate {
                along_km: 720.0,
                lat: 61.0,
                lon: 10.0,
                name: "Bare Rest".into(),
                detour_km: 1.0,
                facility: TruckRestFacility::RestArea,
                suitable_for_weekly: false,
            },
            TruckRestCandidate {
                along_km: 722.0,
                lat: 61.05,
                lon: 10.05,
                name: "Full Services".into(),
                detour_km: 5.0,
                facility: TruckRestFacility::Services,
                suitable_for_weekly: true,
            },
        ];
        let plan = plan_truck_multi_day(
            &truck,
            &history,
            16.0,
            1280.0,
            "2026-07-24",
            "2026-W30",
            &cands,
            false,
        );
        let o = plan.days[0].overnight.as_ref().unwrap();
        assert_eq!(o.name.as_deref(), Some("Full Services"));
    }

    #[test]
    fn overnight_keeps_near_rest_area_when_services_detour_is_large() {
        let truck = TruckRestParams::default();
        let history = TruckDrivingHistory::default();
        let cands = vec![
            TruckRestCandidate {
                along_km: 720.0,
                lat: 61.0,
                lon: 10.0,
                name: "Near Rest".into(),
                detour_km: 0.5,
                facility: TruckRestFacility::RestArea,
                suitable_for_weekly: false,
            },
            TruckRestCandidate {
                along_km: 725.0,
                lat: 61.2,
                lon: 10.2,
                name: "Far Services".into(),
                detour_km: 20.0,
                facility: TruckRestFacility::Services,
                suitable_for_weekly: true,
            },
        ];
        let plan = plan_truck_multi_day(
            &truck,
            &history,
            16.0,
            1280.0,
            "2026-07-24",
            "2026-W30",
            &cands,
            false,
        );
        let o = plan.days[0].overnight.as_ref().unwrap();
        assert_eq!(o.name.as_deref(), Some("Near Rest"));
    }

    #[test]
    fn commit_records_and_repays_reduced_weekly_compensation() {
        let truck = TruckRestParams::default();
        let mut history = TruckDrivingHistory {
            consecutive_working_days: 5,
            last_weekly_rest: TruckRestKind::Regular45,
            ..Default::default()
        };
        let plan = plan_truck_multi_day(
            &truck,
            &history,
            16.0,
            1280.0,
            "2026-07-24",
            "2026-W30",
            &[],
            false,
        );
        let o = plan.days[0].overnight.as_ref().unwrap();
        assert_eq!(o.kind, TruckOvernightKind::WeeklyReduced);

        let mut truck2 = truck.clone();
        commit_truck_multi_day_plan(&mut truck2, &mut history, &plan, "2026-W30");
        assert_eq!(history.weekly_rest_compensations.len(), 1);
        assert!(!history.weekly_rest_compensations[0].repaid);
        assert_eq!(
            history.weekly_rest_compensations[0].compensate_by_date,
            "2026-08-16"
        );

        // Later: another multi-day after consecutive days, with last weekly Reduced → Regular.
        history.consecutive_working_days = 5;
        let plan2 = plan_truck_multi_day(
            &truck2,
            &history,
            16.0,
            1280.0,
            "2026-08-02",
            "2026-W31",
            &[],
            false,
        );
        let o2 = plan2.days[0].overnight.as_ref().unwrap();
        assert_eq!(o2.kind, TruckOvernightKind::WeeklyRegular);
        commit_truck_multi_day_plan(&mut truck2, &mut history, &plan2, "2026-W31");
        assert!(
            history.weekly_rest_compensations[0].repaid,
            "regular weekly should repay compensation debt"
        );
    }

    #[test]
    fn history_room_already_used_shortens_first_day() {
        let truck = TruckRestParams::default();
        let mut history = TruckDrivingHistory::default();
        crate::config::record_truck_driving_hours(&mut history, "2026-07-24", 6.0, "2026-W30");
        // 6 h already today → only 3 h room (9 h cap) on first day before overnight.
        let plan = plan_truck_multi_day(
            &truck,
            &history,
            8.0,
            640.0,
            "2026-07-24",
            "2026-W30",
            &[],
            false,
        );
        assert!(plan.multi_day);
        assert!((plan.days[0].driving_hours - 3.0).abs() < 0.1);
        assert!(plan.days[0].overnight.is_some());
    }
}
