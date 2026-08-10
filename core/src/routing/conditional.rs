//! OSM conditional restriction evaluation via `opening-hours` 1.4.x.
//!
//! Used for seasonal road closures (`motor_vehicle:conditional` /
//! `access:conditional`) and live maxspeed conditionals on speed cameras.
//!
//! Unparseable clauses (e.g. `snow`, full month names like `March`) are
//! declined — they never invent a restriction.

use chrono::{Local, NaiveDateTime};
use opening_hours::OpeningHours;

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

/// Whether an OH condition string matches (is "open") at `dt`.
///
/// `None` = unparseable — caller must decline the clause.
pub fn oh_condition_matches_at(condition: &str, dt: NaiveDateTime) -> Option<bool> {
    match OpeningHours::parse(condition) {
        Ok(oh) => Some(oh.is_open(dt)),
        Err(_) => None,
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
}
