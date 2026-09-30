//! Norway fire-ban date window and related guidance text.
//!
//! The ban window is a **local calendar** rule for Europe/Oslo. Embedders must
//! supply the device-local Y-M-D via `clock_read` (never raw UTC Y-M-D).

use chrono::{Datelike, TimeZone, Utc};
use chrono_tz::Europe::Oslo;

use crate::host::LocalDate;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FireGuidance {
    pub text: String,
    pub bare_rock_note: &'static str,
    pub in_ban_window: Option<bool>,
}

pub const BARE_ROCK_NOTE: &str =
    "Do not light a fire on bare rock — rock can crack from heat.";

pub const PROTECTED_SPECIES_NOTE: &str =
    "Some rare berry, mushroom, and flower species are protected from picking.";

pub const LEAVE_NO_TRACE_NOTE: &str = "Clean up after yourself (leave no trace).";

pub const CAUTIOUS_FIRE_UNKNOWN_DATE: &str =
    "date unknown — from 15 April to 15 September open fire near forest is generally prohibited without permission";

pub const FIRE_IN_BAN: &str =
    "Open fire is generally prohibited without municipal permission (15 April – 15 September). \
Exception where fire clearly cannot spread is user judgment only — the plugin cannot verify that from map data.";

pub const FIRE_OUTSIDE_BAN: &str =
    "Outside the 15 April – 15 September window, open fire is generally permitted with normal caution.";

/// Inclusive 15 Apr – 15 Sep **local** calendar (Norway pack).
pub fn in_norway_fire_ban_window(d: LocalDate) -> bool {
    match (d.month, d.day) {
        (4, day) if day >= 15 => true,
        (5..=8, _) => true,
        (9, day) if day <= 15 => true,
        _ => false,
    }
}

pub fn fire_guidance_norway(date: Option<LocalDate>) -> FireGuidance {
    match date {
        None => FireGuidance {
            text: CAUTIOUS_FIRE_UNKNOWN_DATE.into(),
            bare_rock_note: BARE_ROCK_NOTE,
            in_ban_window: None,
        },
        Some(d) => {
            let in_ban = in_norway_fire_ban_window(d);
            FireGuidance {
                text: if in_ban {
                    FIRE_IN_BAN.into()
                } else {
                    FIRE_OUTSIDE_BAN.into()
                },
                bare_rock_note: BARE_ROCK_NOTE,
                in_ban_window: Some(in_ban),
            }
        }
    }
}

/// Convert a UTC instant to the Europe/Oslo local calendar date (fire + night rules).
pub fn local_date_europe_oslo_from_utc(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    min: u32,
    sec: u32,
) -> Option<LocalDate> {
    let utc = Utc
        .with_ymd_and_hms(year, month, day, hour, min, sec)
        .single()?;
    let local = utc.with_timezone(&Oslo);
    Some(LocalDate {
        year: local.year(),
        month: local.month(),
        day: local.day(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(year: i32, month: u32, day: u32) -> LocalDate {
        LocalDate { year, month, day }
    }

    #[test]
    fn fire_window_boundaries() {
        assert!(!in_norway_fire_ban_window(d(2026, 4, 14)));
        assert!(in_norway_fire_ban_window(d(2026, 4, 15)));
        assert!(in_norway_fire_ban_window(d(2026, 9, 15)));
        assert!(!in_norway_fire_ban_window(d(2026, 9, 16)));
    }

    #[test]
    fn unknown_date_is_cautious_not_permissive() {
        let g = fire_guidance_norway(None);
        assert!(g.text.contains("date unknown"));
        assert!(g.text.contains("prohibited"));
        assert!(!g.text.contains("generally permitted"));
    }

    #[test]
    fn utc_late_apr14_is_already_apr15_in_oslo() {
        // 2026-04-14T23:30Z → 2026-04-15 01:30 CEST
        let local = local_date_europe_oslo_from_utc(2026, 4, 14, 23, 30, 0).unwrap();
        assert_eq!(local, d(2026, 4, 15));
        assert!(in_norway_fire_ban_window(local));
        assert!(fire_guidance_norway(Some(local)).in_ban_window.unwrap());
    }

    #[test]
    fn utc_late_sep15_is_already_sep16_in_oslo() {
        // 2026-09-15T22:30Z → 2026-09-16 00:30 CEST
        let local = local_date_europe_oslo_from_utc(2026, 9, 15, 22, 30, 0).unwrap();
        assert_eq!(local, d(2026, 9, 16));
        assert!(!in_norway_fire_ban_window(local));
        assert!(!fire_guidance_norway(Some(local)).in_ban_window.unwrap());
    }

    #[test]
    fn dst_spring_forward_2026() {
        // Europe/Oslo springs forward 2026-03-29 02:00 → 03:00 local.
        // 2026-03-29T00:30Z = 01:30 CET (still 29 Mar).
        let before = local_date_europe_oslo_from_utc(2026, 3, 29, 0, 30, 0).unwrap();
        assert_eq!(before, d(2026, 3, 29));
        // 2026-03-29T01:30Z = 03:30 CEST (after gap).
        let after = local_date_europe_oslo_from_utc(2026, 3, 29, 1, 30, 0).unwrap();
        assert_eq!(after, d(2026, 3, 29));
        assert!(!in_norway_fire_ban_window(after));
    }

    #[test]
    fn dst_fall_back_2026() {
        // Europe/Oslo falls back 2026-10-25 03:00 → 02:00 local.
        let local = local_date_europe_oslo_from_utc(2026, 10, 25, 0, 30, 0).unwrap();
        assert_eq!(local, d(2026, 10, 25));
        assert!(!in_norway_fire_ban_window(local));
    }
}
