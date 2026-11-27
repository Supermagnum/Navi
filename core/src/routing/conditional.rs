//! OSM conditional restriction evaluation via `opening-hours` 2.x.
//!
//! Used for seasonal road closures (`motor_vehicle:conditional` /
//! `access:conditional`) and live maxspeed conditionals on speed cameras.
//!
//! Unparseable clauses (e.g. `snow`, full month names like `March`) are
//! declined — they never invent a restriction.

use chrono::{Local, NaiveDateTime};
use opening_hours::OpeningHours;
use std::str::FromStr;

/// Extract the opening-hours condition from one OSM conditional clause.
///
/// Prefers parenthesized form `… @ ( … )`, else bare `… @ Nov-Jun`.
pub fn extract_oh_condition(clause: &str) -> Option<&str> {
    let s = clause.trim();
    let at = s.find('@')?;
    let rest = s[at + 1..].trim();
    if rest.is_empty() {
        return None;
    }
    if let Some(start) = rest.find('(') {
        if let Some(end) = rest.rfind(')') {
            if end > start {
                let inner = rest[start + 1..end].trim();
                if !inner.is_empty() {
                    return Some(inner);
                }
            }
        }
    }
    Some(rest)
}

/// Split a multi-clause conditional value on `;` (OSM convention).
pub fn split_conditional_clauses(raw: &str) -> Vec<&str> {
    raw.split(';')
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .collect()
}

/// Leading access token before `@` (`no`, `yes`, `private`, numeric maxspeed, …).
pub fn clause_access_token(clause: &str) -> Option<&str> {
    let s = clause.trim();
    let at = s.find('@')?;
    let token = s[..at].trim();
    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

/// Conservative character set for OSM access/maxspeed OH conditions.
///
/// Deliberately excludes `/` (interval/period syntax). `opening-hours-syntax`
/// 2.1.6 can `debug_assert!` / abort on inputs like `6:00-06:40/24:00`, and
/// libFuzzer builds use `panic=abort` so `catch_unwind` cannot paper over it.
fn oh_condition_is_safe(condition: &str) -> bool {
    if condition.len() > 256 {
        return false;
    }
    condition.bytes().all(|b| {
        matches!(
            b,
            b'A'..=b'Z'
                | b'a'..=b'z'
                | b'0'..=b'9'
                | b' '
                | b'-'
                | b':'
                | b','
                | b'('
                | b')'
                | b'+'
                | b'.'
        )
    })
}

/// Whether an OH condition string matches (is "open") at `dt`.
///
/// `None` = unparseable — caller must decline the clause.
///
/// Unsafe / exotic syntax is declined before calling `opening-hours`. A
/// `catch_unwind` remains for unwind panic builds (unit tests / some hosts).
pub fn oh_condition_matches_at(condition: &str, dt: NaiveDateTime) -> Option<bool> {
    if !oh_condition_is_safe(condition) {
        return None;
    }
    let parsed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        OpeningHours::from_str(condition)
    }));
    match parsed {
        Ok(Ok(oh)) => Some(oh.is_open(dt)),
        Ok(Err(_)) | Err(_) => None,
    }
}

fn token_is_no_like(token: &str) -> bool {
    matches!(
        token.trim().to_ascii_lowercase().as_str(),
        "no" | "private" | "destination" | "permit" | "agricultural" | "forestry" | "delivery"
    )
}

fn token_is_yes_like(token: &str) -> bool {
    matches!(
        token.trim().to_ascii_lowercase().as_str(),
        "yes" | "permissive" | "designated" | "official"
    )
}

/// True when a conditional access expression forbids travel at `dt`.
///
/// v1 rules (hard filters for road closures):
/// - Any parseable `no`/`private`/… `@ CONDITION` whose condition matches → forbid.
/// - If the value has one or more parseable `yes @ …` clauses and **no** bare
///   always-on baseline, and none of those yes-windows match → forbid.
/// - Unparseable clauses are ignored (do not guess).
pub fn access_conditional_forbids_at(raw: &str, dt: NaiveDateTime) -> bool {
    let clauses = split_conditional_clauses(raw);
    if clauses.is_empty() {
        return false;
    }

    let mut saw_yes_window = false;
    let mut yes_window_open = false;

    for clause in clauses {
        let Some(token) = clause_access_token(clause) else {
            continue;
        };
        let Some(cond) = extract_oh_condition(clause) else {
            continue;
        };
        let Some(matches) = oh_condition_matches_at(cond, dt) else {
            continue;
        };
        if token_is_no_like(token) && matches {
            return true;
        }
        if token_is_yes_like(token) {
            saw_yes_window = true;
            if matches {
                yes_window_open = true;
            }
        }
    }

    saw_yes_window && !yes_window_open
}

/// Evaluate maxspeed:conditional; returns the applicable limit (km/h) when a
/// numeric `@ CONDITION` window matches, else `None` (use base maxspeed).
pub fn conditional_maxspeed_kmh_at(raw: &str, dt: NaiveDateTime) -> Option<f64> {
    for clause in split_conditional_clauses(raw) {
        let Some(token) = clause_access_token(clause) else {
            continue;
        };
        let Some(cond) = extract_oh_condition(clause) else {
            continue;
        };
        let Some(true) = oh_condition_matches_at(cond, dt) else {
            continue;
        };
        if let Some(kmh) = crate::routing::eta::parse_maxspeed_kmh(token) {
            return Some(kmh);
        }
    }
    None
}

/// Departure instant for plan-time evaluation (`None` → local now).
pub fn departure_or_now(departure: Option<NaiveDateTime>) -> NaiveDateTime {
    departure.unwrap_or_else(|| Local::now().naive_local())
}

/// Whether motor / general access conditionals forbid an edge at departure.
pub fn edge_seasonally_closed(
    motor_vehicle_conditional: Option<&str>,
    access_conditional: Option<&str>,
    apply_motor_vehicle: bool,
    departure: Option<NaiveDateTime>,
) -> bool {
    let dt = departure_or_now(departure);
    if apply_motor_vehicle {
        if let Some(raw) = motor_vehicle_conditional {
            if !raw.is_empty() && access_conditional_forbids_at(raw, dt) {
                return true;
            }
        }
    }
    if let Some(raw) = access_conditional {
        if !raw.is_empty() && access_conditional_forbids_at(raw, dt) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn noon(y: i32, m: u32, d: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap()
    }

    #[test]
    fn friisvegen_nov_jun_closed_in_january() {
        let raw = "no @ Nov-Jun";
        assert!(access_conditional_forbids_at(raw, noon(2026, 1, 15)));
        assert!(access_conditional_forbids_at(raw, noon(2026, 4, 20)));
        assert!(!access_conditional_forbids_at(raw, noon(2026, 7, 15)));
        assert!(!access_conditional_forbids_at(raw, noon(2026, 10, 1)));
    }

    #[test]
    fn parenthesized_and_bare_forms() {
        assert!(access_conditional_forbids_at(
            "no @ (Dec-Apr)",
            noon(2026, 1, 15)
        ));
        assert!(access_conditional_forbids_at(
            "no@Apr 15-Jul 15",
            noon(2026, 5, 1)
        ));
        assert!(!access_conditional_forbids_at(
            "no@Apr 15-Jul 15",
            noon(2026, 1, 15)
        ));
    }

    #[test]
    fn unparseable_snow_declined() {
        assert!(!access_conditional_forbids_at("no@snow", noon(2026, 1, 15)));
    }

    #[test]
    fn unparseable_march_full_name_declined() {
        // OH month tokens are Mar, not March — decline rather than guess.
        assert!(!access_conditional_forbids_at(
            "no @ (Oct 14-March 16)",
            noon(2026, 1, 15)
        ));
    }

    #[test]
    fn conditional_maxspeed_picks_matching_window() {
        let raw = "50 @ (Mo-Fr 00:00-06:00)";
        // 2026-01-15 is Thursday
        let early = NaiveDate::from_ymd_opt(2026, 1, 15)
            .unwrap()
            .and_hms_opt(3, 0, 0)
            .unwrap();
        let midday = noon(2026, 1, 15);
        assert_eq!(conditional_maxspeed_kmh_at(raw, early), Some(50.0));
        assert_eq!(conditional_maxspeed_kmh_at(raw, midday), None);
    }

    /// Regression: libFuzzer found a panic in opening-hours-syntax 2.1.6
    /// (`GrammarLogic { rule: hour_minutes, invariant: "cannot be empty" }`).
    #[test]
    fn oh_malformed_hour_minutes_does_not_panic() {
        let crash = b"6\x00'@2:00-06-40;Mo-;,@6:00-06:40/24:00;";
        let s = String::from_utf8_lossy(crash);
        let dt = NaiveDate::from_ymd_opt(2026, 6, 15)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap();
        assert_eq!(oh_condition_matches_at(&s, dt), None);
        assert_eq!(conditional_maxspeed_kmh_at(&s, dt), None);
        // Inner condition extracted from the third clause — the actual panic site.
        assert_eq!(oh_condition_matches_at("6:00-06:40/24:00", dt), None);
        assert!(!oh_condition_is_safe("6:00-06:40/24:00"));
        assert!(oh_condition_is_safe("Mo-Fr 00:00-06:00"));
        assert!(oh_condition_is_safe("Nov-Jun"));
    }
}
