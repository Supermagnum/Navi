//! Persisted truck duty history for EC 561/2006 weekly / fortnightly tracking.

use serde::{Deserialize, Serialize};

/// One calendar day of recorded driving time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TruckDrivingDay {
    /// Civil date `YYYY-MM-DD` (local device date as recorded by the host).
    pub date: String,
    pub driving_hours: f64,
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
