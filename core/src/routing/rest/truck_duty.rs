//! EC 561/2006 duty evaluation for truck profiles (daily / weekly / fortnightly).

use crate::config::{
    record_truck_driving_hours, TruckDrivingHistory, TruckRestParams,
};

#[derive(Debug, Clone, PartialEq)]
pub struct TruckDutyEvaluation {
    pub planned_driving_hours: f64,
    pub allowed_daily_hours: f64,
    pub used_daily_extension: bool,
    pub used_exceptional_extension: bool,
    pub remaining_weekly_hours: f64,
    pub remaining_fortnightly_hours: f64,
    pub within_daily: bool,
    pub within_weekly: bool,
    pub within_fortnightly: bool,
    pub weekly_rest_due: bool,
    pub notes: Vec<String>,
}

fn sum_hours_on_dates(history: &TruckDrivingHistory, dates: &[String]) -> f64 {
    history
        .days
        .iter()
        .filter(|d| dates.iter().any(|x| x == &d.date))
        .map(|d| d.driving_hours)
        .sum()
}

fn hours_today(history: &TruckDrivingHistory, today: &str) -> f64 {
    history
        .days
        .iter()
        .find(|d| d.date == today)
        .map(|d| d.driving_hours)
        .unwrap_or(0.0)
}

/// Evaluate whether a planned single-day truck trip fits EC 561 driving caps.
///
/// Daily rest / weekly rest / in-cab rules are surfaced as notes for multi-day
/// duty tracking; they are not hard-gates on single-leg route geometry.
pub fn evaluate_truck_trip(
    truck: &TruckRestParams,
    history: &TruckDrivingHistory,
    planned_driving_hours: f64,
    today: &str,
    week_id: &str,
    week_dates: &[String],
    fortnight_dates: &[String],
) -> TruckDutyEvaluation {
    let mut notes = Vec::new();
    let already_today = hours_today(history, today);
    let weekly_so_far = sum_hours_on_dates(history, week_dates);
    let fortnight_so_far = sum_hours_on_dates(history, fortnight_dates);

    let extensions_used = if history.extension_week_id == week_id {
        history.extensions_used_this_week
    } else {
        0
    };

    let mut allowed_daily = truck.max_daily_driving_hours;
    let mut used_daily_extension = false;
    let projected_today = already_today + planned_driving_hours;
    if projected_today > truck.max_daily_driving_hours + 1e-6
        && extensions_used < truck.max_daily_extensions_per_week
    {
        allowed_daily = truck.max_daily_driving_extended_hours;
        used_daily_extension = true;
        notes.push(format!(
            "daily_extension: using 10 h cap ({extensions_used}/{} used this week before commit)",
            truck.max_daily_extensions_per_week
        ));
    }

    let mut used_exceptional = false;
    if truck.exceptional_extension_armed {
        allowed_daily += truck.exceptional_extension_hours;
        used_exceptional = true;
        notes.push(format!(
            "exceptional_extension: +{:.0} h armed (explicit opt-in)",
            truck.exceptional_extension_hours
        ));
    }

    let within_daily = projected_today <= allowed_daily + 1e-6;
    if !within_daily {
        notes.push(format!(
            "daily_cap: projected {projected_today:.2} h exceeds allowed {allowed_daily:.2} h"
        ));
    }

    let remaining_weekly =
        (truck.max_weekly_driving_hours - weekly_so_far - planned_driving_hours).max(0.0);
    let within_weekly =
        weekly_so_far + planned_driving_hours <= truck.max_weekly_driving_hours + 1e-6;
    if !within_weekly {
        notes.push(format!(
            "weekly_cap: projected {:.2} h exceeds {:.2} h",
            weekly_so_far + planned_driving_hours,
            truck.max_weekly_driving_hours
        ));
    }

    let remaining_fortnightly = (truck.max_fortnightly_driving_hours
        - fortnight_so_far
        - planned_driving_hours)
        .max(0.0);
    let within_fortnightly =
        fortnight_so_far + planned_driving_hours <= truck.max_fortnightly_driving_hours + 1e-6;
    if !within_fortnightly {
        notes.push(format!(
            "fortnightly_cap: projected {:.2} h exceeds {:.2} h",
            fortnight_so_far + planned_driving_hours,
            truck.max_fortnightly_driving_hours
        ));
    }

    let weekly_rest_due =
        history.consecutive_working_days >= truck.max_consecutive_working_days;
    if weekly_rest_due {
        notes.push(format!(
            "weekly_rest_due: {} consecutive working days (max {})",
            history.consecutive_working_days, truck.max_consecutive_working_days
        ));
    }
    notes.push(format!(
        "daily_rest_policy: regular {:.0} h (reduced {:.0} h up to {}×); split {:.0}+{:.0} h — multi-day segmentation applies when trip exceeds daily driving cap",
        truck.daily_rest_hours,
        truck.daily_rest_reduced_hours,
        truck.max_reduced_daily_rests,
        truck.split_daily_rest_first_hours,
        truck.split_daily_rest_second_hours
    ));
    notes.push(format!(
        "weekly_rest_policy: regular {:.0} h (reduced {:.0} h); in-cab forbidden for regular={}; multi-day planner inserts weekly rest after {} consecutive working days",
        truck.weekly_rest_hours,
        truck.weekly_rest_reduced_hours,
        truck.regular_weekly_rest_not_in_cab,
        truck.max_consecutive_working_days
    ));

    TruckDutyEvaluation {
        planned_driving_hours,
        allowed_daily_hours: allowed_daily,
        used_daily_extension,
        used_exceptional_extension: used_exceptional,
        remaining_weekly_hours: remaining_weekly,
        remaining_fortnightly_hours: remaining_fortnightly,
        within_daily,
        within_weekly,
        within_fortnightly,
        weekly_rest_due,
        notes,
    }
}

/// Commit a completed trip: updates history and consumes daily-extension / exceptional flags.
pub fn commit_truck_trip(
    truck: &mut TruckRestParams,
    history: &mut TruckDrivingHistory,
    eval: &TruckDutyEvaluation,
    today: &str,
    week_id: &str,
) {
    record_truck_driving_hours(history, today, eval.planned_driving_hours, week_id);
    if history.extension_week_id != week_id {
        history.extension_week_id = week_id.to_string();
        history.extensions_used_this_week = 0;
    }
    if eval.used_daily_extension {
        history.extensions_used_this_week = history
            .extensions_used_this_week
            .saturating_add(1)
            .min(truck.max_daily_extensions_per_week);
    }
    if eval.used_exceptional_extension {
        truck.exceptional_extension_armed = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{record_truck_driving_hours, TruckRestParams};

    #[test]
    fn fortnightly_cap_uses_rolling_history() {
        let truck = TruckRestParams::default();
        let mut history = TruckDrivingHistory::default();
        let mut dates = Vec::new();
        for i in 1..=14 {
            let d = format!("2026-07-{i:02}");
            dates.push(d.clone());
            record_truck_driving_hours(&mut history, &d, 6.5, "2026-W28");
        }
        let today = dates.last().unwrap().clone();
        let eval = evaluate_truck_trip(
            &truck,
            &history,
            5.0,
            &today,
            "2026-W28",
            &dates[7..],
            &dates,
        );
        assert!(
            !eval.within_fortnightly,
            "5 h more after 91 h must breach 90 h fortnightly: {:?}",
            eval.notes
        );
    }

    #[test]
    fn daily_extension_only_twice_per_week() {
        let truck = TruckRestParams::default();
        let mut history = TruckDrivingHistory {
            extension_week_id: "2026-W30".into(),
            extensions_used_this_week: 2,
            ..Default::default()
        };
        let week = vec!["2026-07-20".into()];
        let eval = evaluate_truck_trip(
            &truck,
            &history,
            9.5,
            "2026-07-20",
            "2026-W30",
            &week,
            &week,
        );
        assert!(
            !eval.within_daily,
            "with 2 extensions already used, 9.5 h must fail: {:?}",
            eval.notes
        );
        history.extensions_used_this_week = 0;
        let ok = evaluate_truck_trip(
            &truck,
            &history,
            9.5,
            "2026-07-20",
            "2026-W30",
            &week,
            &week,
        );
        assert!(ok.within_daily && ok.used_daily_extension);
    }

    #[test]
    fn exceptional_extension_requires_arming() {
        let mut truck = TruckRestParams::default();
        let history = TruckDrivingHistory::default();
        let week = vec!["2026-07-20".into()];
        let blocked = evaluate_truck_trip(
            &truck,
            &history,
            10.5,
            "2026-07-20",
            "2026-W30",
            &week,
            &week,
        );
        assert!(!blocked.within_daily);
        truck.exceptional_extension_armed = true;
        let allowed = evaluate_truck_trip(
            &truck,
            &history,
            10.5,
            "2026-07-20",
            "2026-W30",
            &week,
            &week,
        );
        assert!(allowed.within_daily && allowed.used_exceptional_extension);
    }
}
