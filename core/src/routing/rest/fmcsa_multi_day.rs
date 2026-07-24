//! FMCSA multi-day segmentation (property-carrying HOS).
//!
//! Planning uses **driving hours** as the primary budget (11 h/day). The 14 h
//! on-duty window is noted informationally — full on-duty clocks need ELD /
//! host duty status beyond pure route ETA.

use crate::config::{civil_date_add_days, FmcsaHosParams, TruckDrivingHistory};
use crate::routing::rest::truck_multi_day::{
    TruckDaySegment, TruckMultiDayPlan, TruckOvernightKind, TruckOvernightRest, TruckRestCandidate,
    TruckRestFacility,
};

fn hours_on_date(history: &TruckDrivingHistory, date: &str) -> f64 {
    history
        .days
        .iter()
        .find(|d| d.date == date)
        .map(|d| d.driving_hours)
        .unwrap_or(0.0)
}

fn pick_candidate(
    candidates: &[TruckRestCandidate],
    target_km: f64,
    radius_km: f64,
) -> Option<&TruckRestCandidate> {
    let mut best: Option<&TruckRestCandidate> = None;
    let mut best_score = f64::INFINITY;
    for c in candidates {
        let along_err = (c.along_km - target_km).abs();
        if along_err > radius_km {
            continue;
        }
        let mut score = c.detour_km + along_err * 0.15;
        score -= match c.facility {
            TruckRestFacility::Services => 8.0,
            TruckRestFacility::RestArea => 8.0 * 0.35,
            TruckRestFacility::HgvParking => 0.0,
        };
        if score < best_score {
            best_score = score;
            best = Some(c);
        }
    }
    best
}

fn rolling_on_duty_hours(history: &TruckDrivingHistory, today: &str, days: u32) -> f64 {
    let mut sum = 0.0;
    for i in 0..days {
        let Some(d) = civil_date_add_days(today, -(i as i64)) else {
            break;
        };
        sum += hours_on_date(history, &d);
    }
    sum
}

/// Evaluate a single FMCSA trip against daily driving and 70 h / 8-day cycle.
pub fn evaluate_fmcsa_trip(
    params: &FmcsaHosParams,
    history: &TruckDrivingHistory,
    driving_hours: f64,
    today: &str,
) -> (bool, bool, Vec<String>) {
    let mut notes = Vec::new();
    notes.push(format!(
        "fmcsa: max_driving_h={:.0}; on_duty_window_h={:.0}; break_after_drive_h={:.0}; cycle={:.0}h/{}d; restart_h={:.0}",
        params.max_driving_hours,
        params.on_duty_window_hours,
        params.break_after_driving_hours,
        params.cycle_on_duty_hours,
        params.cycle_days,
        params.restart_hours
    ));
    notes.push(
        "fmcsa_note: 14 h on-duty window is informational in route planning (driving-hours budget used); full on-duty needs ELD/host status"
            .into(),
    );
    let already = hours_on_date(history, today);
    let within_daily = already + driving_hours <= params.max_driving_hours + 1e-6;
    if !within_daily {
        notes.push(format!(
            "fmcsa_daily: exceed — already={already:.1}h + trip={driving_hours:.1}h > {:.0}h",
            params.max_driving_hours
        ));
    }
    let cycle = rolling_on_duty_hours(history, today, params.cycle_days) + driving_hours;
    let within_cycle = cycle <= params.cycle_on_duty_hours + 1e-6;
    if !within_cycle {
        notes.push(format!(
            "fmcsa_cycle: exceed — rolling≈{cycle:.1}h > {:.0}h / {}d",
            params.cycle_on_duty_hours, params.cycle_days
        ));
    }
    (within_daily, within_cycle, notes)
}

/// Segment a truck trip under FMCSA: 11 h driving days, 10 h off-duty overnight,
/// 34 h restart when the rolling cycle would otherwise be exceeded.
pub fn plan_fmcsa_multi_day(
    params: &FmcsaHosParams,
    history: &TruckDrivingHistory,
    total_driving_hours: f64,
    total_dist_km: f64,
    start_date: &str,
    candidates: &[TruckRestCandidate],
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
    let mut days: Vec<TruckDaySegment> = Vec::new();
    let mut day_index = 1u32;

    for _ in 0..40 {
        if remaining <= 1e-6 {
            break;
        }
        let already = hours_on_date(&sim, &date);
        let room = (params.max_driving_hours - already).max(0.0);
        if room < 0.25 {
            date = civil_date_add_days(&date, 1).unwrap_or(date);
            continue;
        }
        let drive = remaining.min(room);
        // If this drive would blow the rolling cycle, insert restart first.
        let cycle_after =
            rolling_on_duty_hours(&sim, &date, params.cycle_days) + drive;
        if cycle_after > params.cycle_on_duty_hours + 1e-6 && !days.is_empty() {
            // Attach restart to previous day's overnight if missing; otherwise advance.
            if let Some(prev) = days.last_mut() {
                if prev.overnight.is_none() {
                    let cand = pick_candidate(candidates, prev.end_km, 40.0);
                    prev.overnight = Some(TruckOvernightRest {
                        kind: TruckOvernightKind::WeeklyRegular,
                        hours: params.restart_hours,
                        split_parts: None,
                        not_in_cab: false,
                        lat: cand.map(|c| c.lat),
                        lon: cand.map(|c| c.lon),
                        name: cand.map(|c| c.name.clone()),
                        poi_found: cand.is_some(),
                        notes: vec![format!(
                            "fmcsa_restart: {:.0} h off duty to reset {}h/{}d cycle",
                            params.restart_hours, params.cycle_on_duty_hours, params.cycle_days
                        )],
                    });
                }
            }
            date = civil_date_add_days(&date, 1).unwrap_or(date);
            // Clear simulated cycle by not counting old days — approximate via wiping day rows older than restart.
            sim.days.clear();
            continue;
        }

        let start_km = km_cursor;
        let end_km = (km_cursor + drive * speed).min(total_dist_km);
        km_cursor = end_km;
        remaining -= drive;
        let driving_date = date.clone();
        crate::config::record_truck_driving_hours(&mut sim, &driving_date, drive, "fmcsa");

        let overnight = if remaining > 1e-6 {
            let cand = pick_candidate(candidates, end_km, 25.0);
            let mut notes = vec![format!(
                "fmcsa_off_duty: {:.0} h consecutive (after up to {:.0} h driving within {:.0} h window)",
                params.min_off_duty_hours, params.max_driving_hours, params.on_duty_window_hours
            )];
            if cand.is_none() {
                notes.push(
                    "fmcsa_rest_poi: no RestArea/services within ~25 km (informational)".into(),
                );
            }
            date = civil_date_add_days(&driving_date, 1).unwrap_or(driving_date.clone());
            Some(TruckOvernightRest {
                kind: TruckOvernightKind::DailyRegular,
                hours: params.min_off_duty_hours,
                split_parts: None,
                not_in_cab: false,
                lat: cand.map(|c| c.lat),
                lon: cand.map(|c| c.lon),
                name: cand.map(|c| c.name.clone()),
                poi_found: cand.is_some(),
                notes,
            })
        } else {
            None
        };

        days.push(TruckDaySegment {
            day_index,
            date: driving_date,
            start_km,
            end_km,
            driving_hours: drive,
            used_daily_extension: false,
            overnight,
        });
        day_index += 1;
    }

    TruckMultiDayPlan {
        multi_day: days.len() > 1,
        days,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{TruckDrivingHistory, TruckRestParams};
    use crate::routing::rest::truck_multi_day::plan_truck_multi_day;

    #[test]
    fn fmcsa_day_cap_is_eleven_hours() {
        let params = FmcsaHosParams::default();
        let history = TruckDrivingHistory::default();
        let plan = plan_fmcsa_multi_day(&params, &history, 20.0, 1600.0, "2026-07-24", &[]);
        assert!(plan.multi_day);
        assert!(
            plan.days[0].driving_hours <= 11.0 + 1e-6,
            "day1={}",
            plan.days[0].driving_hours
        );
        let o = plan.days[0].overnight.as_ref().unwrap();
        assert!((o.hours - 10.0).abs() < 1e-9);
    }

    #[test]
    fn fmcsa_allows_more_first_day_than_ec561_nine() {
        let params = FmcsaHosParams::default();
        let history = TruckDrivingHistory::default();
        let plan = plan_fmcsa_multi_day(&params, &history, 20.0, 1600.0, "2026-07-24", &[]);
        assert!(
            plan.days[0].driving_hours > 9.0 + 0.1,
            "FMCSA should use >9 h first day when 11 h available; got {}",
            plan.days[0].driving_hours
        );
    }

    #[test]
    fn same_20h_trip_ec_vs_fmcsa_first_day_hours_differ() {
        let history = TruckDrivingHistory::default();
        let total_h = 20.0;
        let total_km = 1600.0;
        let start = "2026-07-24";

        let ec = plan_truck_multi_day(
            &TruckRestParams::default(),
            &history,
            total_h,
            total_km,
            start,
            "2026-W30",
            &[],
            false,
        );
        let fmcsa = plan_fmcsa_multi_day(
            &FmcsaHosParams::default(),
            &history,
            total_h,
            total_km,
            start,
            &[],
        );

        let ec_d1 = ec.days[0].driving_hours;
        let fmcsa_d1 = fmcsa.days[0].driving_hours;
        assert!(
            ec_d1 <= 10.0 + 1e-6,
            "EC first day should be <=9 (or 10 with extension); got {ec_d1}"
        );
        assert!(
            fmcsa_d1 > 9.0 + 0.1 && fmcsa_d1 <= 11.0 + 1e-6,
            "FMCSA first day should be >9 and <=11; got {fmcsa_d1}"
        );
        assert!(
            (fmcsa_d1 - ec_d1).abs() > 0.5,
            "first-day hours should differ meaningfully: EC={ec_d1} FMCSA={fmcsa_d1}"
        );
    }
}
