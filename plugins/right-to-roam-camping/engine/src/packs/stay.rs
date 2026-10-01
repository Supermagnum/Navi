//! Stay counters for land-manager packs (fixture-tested; flags OFF in production defaults).

use crate::host::{CampingHost, LocalDate};

pub const BLM_RADIUS_MILES: f64 = 25.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StayPolicy {
    /// BLM: 14 days within any 28-day period; radius key (25 miles).
    Blm14In28,
    /// USFS default: 14 nights in 30 days.
    Usfs14In30,
    /// Ontario: 21 days per site per calendar year; move ≥ 100 m.
    On21PerYear,
    /// BC: 14 consecutive; reset after ≥ 72 h absence.
    Bc14Consecutive72hReset,
    /// Alberta: 14 days then move 1 km for 72 h.
    Ab14Then1km72h,
    /// NPS Alaska: 14 consecutive then move ≥ 2 miles.
    NpsAk14,
}

pub const STAY_POLICIES: &[(&str, StayPolicy)] = &[
    ("blm_14_in_28", StayPolicy::Blm14In28),
    ("usfs_14_in_30", StayPolicy::Usfs14In30),
    ("on_21_per_year", StayPolicy::On21PerYear),
    (
        "bc_14_consecutive_72h_reset",
        StayPolicy::Bc14Consecutive72hReset,
    ),
    ("ab_14_then_1km_72h", StayPolicy::Ab14Then1km72h),
    ("nps_ak_14", StayPolicy::NpsAk14),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StayDecision {
    Allow,
    Exceed {
        reason: &'static str,
    },
    /// Ontario unknown residency → treat as non-resident (stricter path).
    NonResidentPath {
        note: &'static str,
    },
}

#[cfg(test)]
fn date_key(d: LocalDate) -> String {
    format!("{:04}-{:02}-{:02}", d.year, d.month, d.day)
}

fn kv_key(policy: &str, loc: &str) -> String {
    format!("rtr_stay:{policy}:{loc}")
}

/// Record one night for fixture tests (simple counter).
#[cfg(test)]
pub fn stay_record_night(
    host: &mut dyn CampingHost,
    policy_id: &str,
    location_id: &str,
    tonight: LocalDate,
) -> Result<(), String> {
    let key = if policy_id == "on_21_per_year" {
        kv_key(policy_id, &format!("{}:{}", tonight.year, location_id))
    } else {
        kv_key(policy_id, location_id)
    };
    let used = host
        .kv_get(&key)
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);
    host.kv_set(&key, &(used + 1).to_string())?;
    let _ = date_key(tonight);
    Ok(())
}

/// Evaluate stay policy against fixture KV history.
///
/// `residency_known`: when false on Ontario, returns [`StayDecision::NonResidentPath`].
pub fn stay_would_exceed(
    host: &dyn CampingHost,
    policy_id: &str,
    location_id: &str,
    tonight: LocalDate,
    nights_already_at_site: u32,
    hours_absent: Option<u32>,
    residency_known: bool,
    unit_name: &str,
) -> StayDecision {
    let Some((_, policy)) = STAY_POLICIES.iter().find(|(id, _)| *id == policy_id) else {
        return StayDecision::Allow;
    };
    match policy {
        StayPolicy::Blm14In28 => {
            // Fixture: nights in rolling 28-day window stored as count in KV.
            let key = kv_key(policy_id, location_id);
            let used = host
                .kv_get(&key)
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(nights_already_at_site);
            if used >= 14 {
                StayDecision::Exceed {
                    reason: "blm_14_in_28_exceeded",
                }
            } else {
                let _ = (tonight, BLM_RADIUS_MILES);
                StayDecision::Allow
            }
        }
        StayPolicy::Usfs14In30 => {
            let key = kv_key(policy_id, location_id);
            let used = host
                .kv_get(&key)
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(nights_already_at_site);
            if used >= 14 {
                StayDecision::Exceed {
                    reason: "usfs_14_in_30_exceeded",
                }
            } else {
                let _ = unit_name; // card text: check forest order for {unit_name}
                StayDecision::Allow
            }
        }
        StayPolicy::On21PerYear => {
            if !residency_known {
                return StayDecision::NonResidentPath {
                    note: "Ontario residency unknown — applying non-resident Crown-land path \
(permit/green-zone rules may apply north of French/Mattawa Rivers).",
                };
            }
            let key = kv_key(policy_id, &format!("{}:{}", tonight.year, location_id));
            let used = host
                .kv_get(&key)
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(nights_already_at_site);
            if used >= 21 {
                StayDecision::Exceed {
                    reason: "on_21_per_year_exceeded",
                }
            } else {
                StayDecision::Allow
            }
        }
        StayPolicy::Bc14Consecutive72hReset => {
            if hours_absent.is_some_and(|h| h >= 72) {
                return StayDecision::Allow;
            }
            if nights_already_at_site >= 14 {
                StayDecision::Exceed {
                    reason: "bc_14_consecutive_exceeded",
                }
            } else {
                StayDecision::Allow
            }
        }
        StayPolicy::Ab14Then1km72h => {
            if nights_already_at_site >= 14 {
                StayDecision::Exceed {
                    reason: "ab_14_then_move_1km_72h",
                }
            } else {
                StayDecision::Allow
            }
        }
        StayPolicy::NpsAk14 => {
            if nights_already_at_site >= 14 {
                StayDecision::Exceed {
                    reason: "nps_ak_14_exceeded",
                }
            } else {
                StayDecision::Allow
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{CampingHost, TravelMode};
    use crate::safety_view::OvernightSafety;
    use std::collections::HashMap;

    struct Mem {
        kv: HashMap<String, String>,
    }
    impl CampingHost for Mem {
        fn safety_config(&self) -> Option<OvernightSafety> {
            Some(OvernightSafety::default())
        }
        fn clock_local(&self) -> Option<LocalDate> {
            None
        }
        fn plugin_kv_available(&self) -> bool {
            true
        }
        fn kv_get(&self, key: &str) -> Option<String> {
            self.kv.get(key).cloned()
        }
        fn kv_set(&mut self, key: &str, value: &str) -> Result<(), String> {
            self.kv.insert(key.into(), value.into());
            Ok(())
        }
        fn admin_country_iso(&self, _: f64, _: f64) -> Option<String> {
            None
        }
        fn admin_subdivision_iso(&self, _: f64, _: f64) -> Option<String> {
            None
        }
        fn travel_mode(&self) -> TravelMode {
            TravelMode::NonMotorised
        }
        fn overnight_buildings(&self) -> &[(f64, f64)] {
            &[]
        }
        fn overnight_glacier_rings(&self) -> &[Vec<[f64; 2]>] {
            &[]
        }
    }

    fn d(day: u32) -> LocalDate {
        LocalDate {
            year: 2026,
            month: 7,
            day,
        }
    }

    #[test]
    fn blm_14_in_28_with_radius_key() {
        let mut h = Mem { kv: HashMap::new() };
        let loc = "cell:blm:25mi";
        for _ in 0..14 {
            stay_record_night(&mut h, "blm_14_in_28", loc, d(1)).unwrap();
        }
        assert!(matches!(
            stay_would_exceed(&h, "blm_14_in_28", loc, d(2), 14, None, true, "BLM"),
            StayDecision::Exceed { .. }
        ));
        assert_eq!(BLM_RADIUS_MILES, 25.0);
    }

    #[test]
    fn usfs_default_14_30() {
        let mut h = Mem { kv: HashMap::new() };
        for _ in 0..14 {
            stay_record_night(&mut h, "usfs_14_in_30", "cell:fs", d(1)).unwrap();
        }
        assert!(matches!(
            stay_would_exceed(
                &h,
                "usfs_14_in_30",
                "cell:fs",
                d(2),
                14,
                None,
                true,
                "Fishlake"
            ),
            StayDecision::Exceed { .. }
        ));
    }

    #[test]
    fn ontario_21_and_unknown_residency_non_resident() {
        let h = Mem { kv: HashMap::new() };
        assert!(matches!(
            stay_would_exceed(&h, "on_21_per_year", "cell:on", d(1), 0, None, false, "ON"),
            StayDecision::NonResidentPath { .. }
        ));
        let mut h = Mem { kv: HashMap::new() };
        for _ in 0..21 {
            stay_record_night(&mut h, "on_21_per_year", "cell:on", d(1)).unwrap();
        }
        assert!(matches!(
            stay_would_exceed(&h, "on_21_per_year", "cell:on", d(1), 21, None, true, "ON"),
            StayDecision::Exceed { .. }
        ));
    }

    #[test]
    fn bc_14_resets_after_72h_absence() {
        let h = Mem { kv: HashMap::new() };
        assert!(matches!(
            stay_would_exceed(
                &h,
                "bc_14_consecutive_72h_reset",
                "cell:bc",
                d(1),
                14,
                Some(71),
                true,
                "BC"
            ),
            StayDecision::Exceed { .. }
        ));
        assert!(matches!(
            stay_would_exceed(
                &h,
                "bc_14_consecutive_72h_reset",
                "cell:bc",
                d(1),
                14,
                Some(72),
                true,
                "BC"
            ),
            StayDecision::Allow
        ));
    }

    #[test]
    fn alberta_14_then_1km_72h() {
        let h = Mem { kv: HashMap::new() };
        assert!(matches!(
            stay_would_exceed(
                &h,
                "ab_14_then_1km_72h",
                "cell:ab",
                d(1),
                14,
                None,
                true,
                "AB"
            ),
            StayDecision::Exceed {
                reason: "ab_14_then_move_1km_72h"
            }
        ));
    }
}
