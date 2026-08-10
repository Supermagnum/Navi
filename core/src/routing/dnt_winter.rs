//! DNT / official-trail winter advisory (informational only).
//!
//! Ostlandet census (2026-08): 1097 DNT-related route relations had **zero**
//! `seasonal` / `opening_hours` tags. Path-level `seasonal=*` is rare and
//! unreliable. Hiking a DNT trail in winter is not a hard legal closure the way
//! `motor_vehicle:conditional=no @ …` is — treat like horse toxic-plant notes:
//! surface guidance, not enforcement.

use chrono::{Datelike, Local};

/// Shown on hiking plans during the conventional Norwegian mountain winter
/// window (November through May). Does not change routing costs or filters.
pub const DNT_WINTER_ADVISORY: &str = "Winter (Nov-May): DNT-marked trails may be unmarked under snow; huts may be unstaffed; avalanche terrain requires your own judgment. Not a legal access closure.";

/// True for calendar months November through May (1-based month).
pub fn is_dnt_winter_month(month: u32) -> bool {
    matches!(month, 11 | 12 | 1 | 2 | 3 | 4 | 5)
}

/// Advisory text when a hiking plan falls in the winter window; else empty.
///
/// `departure_month` is 1..=12; `None` uses the device local month.
pub fn dnt_winter_advisory_for_month(departure_month: Option<u32>) -> Option<&'static str> {
    let month = departure_month.unwrap_or_else(|| Local::now().month());
    if is_dnt_winter_month(month) {
        Some(DNT_WINTER_ADVISORY)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn winter_months_active() {
        assert!(is_dnt_winter_month(11));
        assert!(is_dnt_winter_month(1));
        assert!(is_dnt_winter_month(5));
        assert!(!is_dnt_winter_month(6));
        assert!(!is_dnt_winter_month(10));
    }

    #[test]
    fn advisory_text_non_blocking() {
        assert!(dnt_winter_advisory_for_month(Some(1)).is_some());
        assert!(dnt_winter_advisory_for_month(Some(7)).is_none());
        let text = dnt_winter_advisory_for_month(Some(12)).unwrap();
        assert!(text.contains("judgment"));
        assert!(text.contains("Not a legal access closure"));
    }
}
