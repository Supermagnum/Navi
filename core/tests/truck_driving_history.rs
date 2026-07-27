//! TruckDrivingHistory accumulation / rolling-window tests (EC 561 duty state).

use driver_break_core::config::{
    civil_date_add_days, prune_truck_driving_history, record_truck_driving_hours,
    rolling_date_window, TruckDrivingHistory, TruckRestParams,
};
use driver_break_core::routing::{commit_truck_trip, evaluate_truck_trip};

#[test]
fn empty_history_defaults_under_all_caps() {
    let truck = TruckRestParams::default();
    let history = TruckDrivingHistory::default();
    assert!(history.days.is_empty());
    assert_eq!(history.extensions_used_this_week, 0);

    let today = "2026-07-24";
    let week = rolling_date_window(today, 7);
    let fortnight = rolling_date_window(today, 14);
    let eval = evaluate_truck_trip(&truck, &history, 8.0, today, "2026-W30", &week, &fortnight);
    assert!(eval.within_daily, "fresh history: 8 h under 9 h daily");
    assert!(eval.within_weekly, "fresh history: under 56 h weekly");
    assert!(
        eval.within_fortnightly,
        "fresh history: under 90 h fortnightly"
    );
    assert!(!eval.used_daily_extension);
    assert!(!eval.weekly_rest_due);
}

#[test]
fn accumulates_across_simulated_days_and_tracks_extensions() {
    let mut truck = TruckRestParams::default();
    let mut history = TruckDrivingHistory::default();
    let week_id = "2026-W28";

    // Day 1: 9.5 h → uses one 10 h extension, then commit.
    let d1 = "2026-07-06";
    let week = rolling_date_window(d1, 7);
    let fortnight = rolling_date_window(d1, 14);
    let e1 = evaluate_truck_trip(&truck, &history, 9.5, d1, week_id, &week, &fortnight);
    assert!(e1.within_daily && e1.used_daily_extension);
    commit_truck_trip(&mut truck, &mut history, &e1, d1, week_id);
    assert_eq!(history.extensions_used_this_week, 1);
    assert!(
        (history
            .days
            .iter()
            .find(|d| d.date == d1)
            .unwrap()
            .driving_hours
            - 9.5)
            .abs()
            < 1e-9
    );

    // Day 2: another 9.5 h → second extension.
    let d2 = "2026-07-07";
    let week = rolling_date_window(d2, 7);
    let fortnight = rolling_date_window(d2, 14);
    let e2 = evaluate_truck_trip(&truck, &history, 9.5, d2, week_id, &week, &fortnight);
    assert!(e2.within_daily && e2.used_daily_extension);
    commit_truck_trip(&mut truck, &mut history, &e2, d2, week_id);
    assert_eq!(history.extensions_used_this_week, 2);

    // Day 3: third 9.5 h — extensions exhausted → not within daily.
    let d3 = "2026-07-08";
    let week = rolling_date_window(d3, 7);
    let fortnight = rolling_date_window(d3, 14);
    let e3 = evaluate_truck_trip(&truck, &history, 9.5, d3, week_id, &week, &fortnight);
    assert!(
        !e3.within_daily,
        "third 10 h attempt in same week must fail: {:?}",
        e3.notes
    );

    // Accumulate toward fortnightly 90 h: add several full days at 8 h.
    for i in 9..=20 {
        let d = format!("2026-07-{i:02}");
        record_truck_driving_hours(&mut history, &d, 8.0, week_id);
    }
    let today = "2026-07-20";
    let fortnight = rolling_date_window(today, 14);
    let so_far: f64 = history
        .days
        .iter()
        .filter(|d| fortnight.iter().any(|x| x == &d.date))
        .map(|d| d.driving_hours)
        .sum();
    assert!(
        so_far > 80.0,
        "expected heavy fortnight so far, got {so_far}"
    );
    let week = rolling_date_window(today, 7);
    let over = evaluate_truck_trip(&truck, &history, 12.0, today, week_id, &week, &fortnight);
    assert!(
        !over.within_fortnightly,
        "12 h more near 90 h must breach fortnightly: so_far={so_far} notes={:?}",
        over.notes
    );
}

#[test]
fn rolling_window_prunes_old_days_from_storage_and_counting() {
    let truck = TruckRestParams::default();
    let mut history = TruckDrivingHistory::default();
    let today = "2026-07-24";

    // Old day (25 days ago): 50 h alone would blow a naive all-time sum.
    let old = civil_date_add_days(today, -25).expect("old date");
    record_truck_driving_hours(&mut history, &old, 50.0, "2026-W26");
    // Recent days: 5 × 8 h = 40 h inside the fortnight.
    for i in 0..5 {
        let d = civil_date_add_days(today, -i).unwrap();
        record_truck_driving_hours(&mut history, &d, 8.0, "2026-W30");
    }

    // Explicit prune to 14-day keep (record already prunes at 21).
    prune_truck_driving_history(&mut history, today, 14);
    assert!(
        history
            .days
            .iter()
            .all(|d| d.date.as_str() >= civil_date_add_days(today, -14).unwrap().as_str()),
        "after prune, no day older than 14 days: {:?}",
        history.days
    );
    assert!(
        !history.days.iter().any(|d| d.date == old),
        "25-day-old entry must be pruned from storage"
    );

    let fortnight = rolling_date_window(today, 14);
    let week = rolling_date_window(today, 7);
    let eval = evaluate_truck_trip(&truck, &history, 5.0, today, "2026-W30", &week, &fortnight);
    assert!(
        eval.within_fortnightly,
        "old 50 h must not count once pruned; notes={:?}",
        eval.notes
    );

    // Without prune, a 50 h entry inside the caller's window would still count —
    // re-seed old + recent without prune and pass only last 14 dates to evaluate.
    let mut hist2 = TruckDrivingHistory::default();
    record_truck_driving_hours(&mut hist2, &old, 50.0, "2026-W26");
    for i in 0..5 {
        let d = civil_date_add_days(today, -i).unwrap();
        // Bypass age prune for this counter-example by pushing directly after.
        if let Some(day) = hist2.days.iter_mut().find(|x| x.date == d) {
            day.driving_hours = 8.0;
        } else {
            hist2.days.push(driver_break_core::config::TruckDrivingDay {
                date: d,
                driving_hours: 8.0,
            });
        }
    }
    // Count only rolling fortnight dates — old date excluded → under 90.
    let only_fortnight = rolling_date_window(today, 14);
    let eval2 = evaluate_truck_trip(
        &truck,
        &hist2,
        5.0,
        today,
        "2026-W30",
        &week,
        &only_fortnight,
    );
    assert!(
        eval2.within_fortnightly,
        "evaluation window (not raw table length) defines the rolling fortnight"
    );
}
