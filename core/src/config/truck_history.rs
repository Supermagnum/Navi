//! Persisted truck duty history for EC 561/2006 weekly / fortnightly tracking.

use serde::{Deserialize, Serialize};

/// One calendar day of recorded driving time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TruckDrivingDay {
    /// Civil date `YYYY-MM-DD` (local device date as recorded by the host).
    pub date: String,
    pub driving_hours: f64,
}

/// Outstanding (or repaid) compensation after a reduced weekly rest (Art. 8).
///
/// A reduced weekly rest (24 h instead of 45 h) creates a shortfall that must be
/// taken en bloc and attached to another rest of at least 9 h before the end of
/// the third week following the week of the reduction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WeeklyRestCompensationDebt {
    /// Civil date when the reduced weekly rest was taken.
    pub reduced_on_date: String,
    /// Hours owed (typically 45 − 24 = 21).
    pub shortfall_hours: f64,
    /// Inclusive deadline: last day of the third week following the week of reduction.
    pub compensate_by_date: String,
    #[serde(default)]
    pub repaid: bool,
    #[serde(default)]
    pub repaid_on_date: Option<String>,
}

/// Persisted truck duty history (ConfigStore key `truck_driving_history`).
///
/// Stored in the existing `app_config` JSON blob store (same mechanism as
/// `RestConfig`), not a separate table.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TruckDrivingHistory {
    pub days: Vec<TruckDrivingDay>,
    /// ISO week id (`YYYY-Www`) for which [`extensions_used_this_week`] applies.
    #[serde(default)]
    pub extension_week_id: String,
    #[serde(default)]
    pub extensions_used_this_week: u32,
    #[serde(default)]
    pub reduced_daily_rests_since_weekly: u32,
    #[serde(default)]
    pub consecutive_working_days: u32,
    /// Last weekly rest taken (informational / multi-day).
    #[serde(default)]
    pub last_weekly_rest: TruckRestKind,
    /// Reduced weekly-rest compensation ledger (Art. 8).
    #[serde(default)]
    pub weekly_rest_compensations: Vec<WeeklyRestCompensationDebt>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TruckRestKind {
    #[default]
    None,
    Regular45,
    Reduced24,
}

/// Record driving hours for `date` (adds to an existing day row when present).
pub fn record_truck_driving_hours(
    history: &mut TruckDrivingHistory,
    date: &str,
    hours: f64,
    week_id: &str,
) {
    if let Some(day) = history.days.iter_mut().find(|d| d.date == date) {
        day.driving_hours += hours;
    } else {
        history.days.push(TruckDrivingDay {
            date: date.to_string(),
            driving_hours: hours,
        });
        history.consecutive_working_days = history.consecutive_working_days.saturating_add(1);
    }
    if history.extension_week_id != week_id {
        history.extension_week_id = week_id.to_string();
        history.extensions_used_this_week = 0;
    }
    prune_truck_driving_history(history, date, 21);
}

/// Drop day rows older than `keep_days` before `today` (`YYYY-MM-DD`).
///
/// The fortnightly 90 h cap only sums the last ~14 days passed into
/// [`crate::routing::rest::evaluate_truck_trip`]; this prune keeps the
/// persisted blob from growing without bound.
pub fn prune_truck_driving_history(
    history: &mut TruckDrivingHistory,
    today: &str,
    keep_days: i64,
) {
    let Some(cutoff) = civil_date_add_days(today, -keep_days) else {
        return;
    };
    history.days.retain(|d| d.date.as_str() >= cutoff.as_str());
}

/// Monday of the ISO week containing `ymd` (`YYYY-MM-DD`).
pub fn iso_week_monday(ymd: &str) -> Option<String> {
    let parts: Vec<&str> = ymd.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let y: i32 = parts[0].parse().ok()?;
    let m: u32 = parts[1].parse().ok()?;
    let d: u32 = parts[2].parse().ok()?;
    let mon0 = weekday_iso_mon0(y, m, d)?;
    civil_date_add_days(ymd, -(mon0 as i64))
}

/// Last day (Sunday) of the third ISO week following the week that contains `ymd`.
///
/// Used as the Art. 8 compensation deadline after a reduced weekly rest taken on
/// (or in the week of) `ymd`.
pub fn weekly_rest_compensation_deadline(reduced_on: &str) -> Option<String> {
    let monday = iso_week_monday(reduced_on)?;
    // Monday of week W → Sunday of week W+3 = monday + 6 + 21.
    civil_date_add_days(&monday, 6 + 21)
}

/// Record a reduced weekly rest on the compensation ledger.
pub fn record_reduced_weekly_compensation(
    history: &mut TruckDrivingHistory,
    reduced_on_date: &str,
    shortfall_hours: f64,
) {
    let Some(compensate_by_date) = weekly_rest_compensation_deadline(reduced_on_date) else {
        return;
    };
    history
        .weekly_rest_compensations
        .push(WeeklyRestCompensationDebt {
            reduced_on_date: reduced_on_date.to_string(),
            shortfall_hours: shortfall_hours.max(0.0),
            compensate_by_date,
            repaid: false,
            repaid_on_date: None,
        });
}

/// Try to repay the oldest unpaid compensation debt by attaching it to a rest
/// period of at least 9 h that is long enough to include the shortfall en bloc
/// (`attached_rest_hours >= 9 + shortfall`), or a full regular weekly rest (45 h).
///
/// Returns true when a debt was marked repaid.
pub fn try_repay_weekly_rest_compensation(
    history: &mut TruckDrivingHistory,
    rest_date: &str,
    attached_rest_hours: f64,
) -> bool {
    if attached_rest_hours < 9.0 - 1e-6 {
        return false;
    }
    let Some(idx) = history
        .weekly_rest_compensations
        .iter()
        .position(|d| !d.repaid)
    else {
        return false;
    };
    let shortfall = history.weekly_rest_compensations[idx].shortfall_hours;
    let can_repay = attached_rest_hours + 1e-6 >= 9.0 + shortfall
        || attached_rest_hours + 1e-6 >= 45.0;
    if !can_repay {
        return false;
    }
    let debt = &mut history.weekly_rest_compensations[idx];
    debt.repaid = true;
    debt.repaid_on_date = Some(rest_date.to_string());
    true
}

/// Unpaid compensation debts (oldest first).
pub fn outstanding_weekly_rest_compensations(
    history: &TruckDrivingHistory,
) -> Vec<&WeeklyRestCompensationDebt> {
    history
        .weekly_rest_compensations
        .iter()
        .filter(|d| !d.repaid)
        .collect()
}

/// Sakamoto weekday with Monday = 0 … Sunday = 6 (ISO).
fn weekday_iso_mon0(y: i32, m: u32, d: u32) -> Option<u32> {
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // Tomohiko Sakamoto: 0 = Sunday.
    let t = [0u32, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let mut yy = y;
    if m < 3 {
        yy -= 1;
    }
    let y = yy as u32;
    let sun0 = (y + y / 4 - y / 100 + y / 400 + t[(m - 1) as usize] + d) % 7;
    Some((sun0 + 6) % 7)
}

/// Simple `YYYY-MM-DD` ± days (proleptic Gregorian; enough for duty windows).
pub fn civil_date_add_days(ymd: &str, delta_days: i64) -> Option<String> {
    let parts: Vec<&str> = ymd.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let y: i32 = parts[0].parse().ok()?;
    let m: u32 = parts[1].parse().ok()?;
    let d: u32 = parts[2].parse().ok()?;
    let day_num = civil_to_day_number(y, m, d)? as i64 + delta_days;
    if day_num < 0 {
        return None;
    }
    let (ny, nm, nd) = day_number_to_civil(day_num as u32)?;
    Some(format!("{ny:04}-{nm:02}-{nd:02}"))
}

fn civil_to_day_number(y: i32, m: u32, d: u32) -> Option<u32> {
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // Howard Hinnant days_from_civil
    let y = if m <= 2 { y - 1 } else { y } as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let m = m as u64;
    let d = d as u64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146_097 + doe as i64 - 719_468) as u32)
}

fn day_number_to_civil(z: u32) -> Option<(i32, u32, u32)> {
    let z = z as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    Some((y as i32, m as u32, d as u32))
}

/// Dates from `today` back `n` inclusive calendar days (`YYYY-MM-DD`).
pub fn rolling_date_window(today: &str, n: usize) -> Vec<String> {
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        if let Some(d) = civil_date_add_days(today, -(i as i64)) {
            out.push(d);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_week_monday_known_friday() {
        // 2026-07-24 is a Friday; ISO week Monday is 2026-07-20.
        assert_eq!(
            iso_week_monday("2026-07-24").as_deref(),
            Some("2026-07-20")
        );
    }

    #[test]
    fn compensation_deadline_is_sunday_of_week_plus_three() {
        // Reduced on Friday 2026-07-24 (week of Mon 20 Jul).
        // End of third following week = Sunday 2026-08-16.
        assert_eq!(
            weekly_rest_compensation_deadline("2026-07-24").as_deref(),
            Some("2026-08-16")
        );
    }

    #[test]
    fn reduced_weekly_records_debt_and_regular_repay() {
        let mut history = TruckDrivingHistory::default();
        record_reduced_weekly_compensation(&mut history, "2026-07-24", 21.0);
        assert_eq!(history.weekly_rest_compensations.len(), 1);
        let d = &history.weekly_rest_compensations[0];
        assert!(!d.repaid);
        assert!((d.shortfall_hours - 21.0).abs() < 1e-9);
        assert_eq!(d.compensate_by_date, "2026-08-16");

        // 11 h daily rest cannot carry 21 h compensation en bloc with 9 h base.
        assert!(!try_repay_weekly_rest_compensation(
            &mut history,
            "2026-07-31",
            11.0
        ));
        assert!(!history.weekly_rest_compensations[0].repaid);

        // Full regular 45 h weekly rest can repay.
        assert!(try_repay_weekly_rest_compensation(
            &mut history,
            "2026-08-02",
            45.0
        ));
        assert!(history.weekly_rest_compensations[0].repaid);
        assert_eq!(
            history.weekly_rest_compensations[0]
                .repaid_on_date
                .as_deref(),
            Some("2026-08-02")
        );
        assert!(outstanding_weekly_rest_compensations(&history).is_empty());
    }

    #[test]
    fn thirty_hour_rest_repays_twenty_one_shortfall() {
        let mut history = TruckDrivingHistory::default();
        record_reduced_weekly_compensation(&mut history, "2026-07-24", 21.0);
        assert!(try_repay_weekly_rest_compensation(
            &mut history,
            "2026-08-01",
            30.0
        ));
        assert!(history.weekly_rest_compensations[0].repaid);
    }
}
