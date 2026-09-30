//! Plugin-local consecutive-night store. Keys are lat/lon grid cells — never
//! graph node ids (navi-server rebakes weekly; OSM ids are not stable).

use serde::{Deserialize, Serialize};

use crate::host::{CampingHost, LocalDate};

/// ~111 m at equator; stable across pack rebakes.
pub const LOCATION_GRID_DEG: f64 = 0.001;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NightRecord {
    pub location_id: String,
    pub first_night: String, // YYYY-MM-DD
    pub last_night: String,
    pub nights_used: u32,
}

pub fn location_id_from_lat_lon(lat: f64, lon: f64) -> String {
    let glat = (lat / LOCATION_GRID_DEG).round() as i64;
    let glon = (lon / LOCATION_GRID_DEG).round() as i64;
    format!("cell:{glat}:{glon}")
}

fn date_key(d: LocalDate) -> String {
    format!("{:04}-{:02}-{:02}", d.year, d.month, d.day)
}

fn parse_date(s: &str) -> Option<chrono::NaiveDate> {
    let parts: Vec<_> = s.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    chrono::NaiveDate::from_ymd_opt(
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
    )
}

fn kv_key(pack: &str, location_id: &str) -> String {
    format!("rtr_night:{pack}:{location_id}")
}

pub struct NightStore;

impl NightStore {
    /// Returns true if suggesting another night at this spot would exceed
    /// `max_nights` consecutive. Applies reset after a gap ≥ 1 unused night
    /// or when camping elsewhere (different location_id).
    pub fn would_exceed(
        host: &dyn CampingHost,
        pack: &str,
        location_id: &str,
        tonight: LocalDate,
        max_nights: u32,
    ) -> bool {
        if !host.plugin_kv_available() {
            // Caller must decline before calling when KV unavailable.
            return true;
        }
        let key = kv_key(pack, location_id);
        let Some(raw) = host.kv_get(&key) else {
            return false;
        };
        let Ok(rec) = serde_json::from_str::<NightRecord>(&raw) else {
            return false;
        };
        let Some(last) = parse_date(&rec.last_night) else {
            return false;
        };
        let Some(today) = tonight.to_naive() else {
            return true;
        };
        let gap = (today - last).num_days();
        if gap > 1 {
            // Reset — gap night(s).
            return false;
        }
        if gap < 0 {
            return true;
        }
        // gap == 0 (same night re-suggest) or gap == 1 (next consecutive).
        let next_count = if gap == 0 {
            rec.nights_used
        } else {
            rec.nights_used + 1
        };
        next_count > max_nights
    }

    /// Record that the user is attributed a night at this spot (call when a
    /// suggestion is accepted / shown as planned overnight).
    pub fn record_night(
        host: &mut dyn CampingHost,
        pack: &str,
        location_id: &str,
        tonight: LocalDate,
    ) -> Result<(), String> {
        if !host.plugin_kv_available() {
            return Err("plugin_kv unavailable".into());
        }
        let key = kv_key(pack, location_id);
        let today_s = date_key(tonight);
        let Some(today) = tonight.to_naive() else {
            return Err("bad date".into());
        };

        let rec = if let Some(raw) = host.kv_get(&key) {
            if let Ok(mut existing) = serde_json::from_str::<NightRecord>(&raw) {
                if let Some(last) = parse_date(&existing.last_night) {
                    let gap = (today - last).num_days();
                    if gap > 1 || gap < 0 {
                        NightRecord {
                            location_id: location_id.into(),
                            first_night: today_s.clone(),
                            last_night: today_s.clone(),
                            nights_used: 1,
                        }
                    } else if gap == 0 {
                        existing
                    } else {
                        existing.last_night = today_s;
                        existing.nights_used += 1;
                        existing
                    }
                } else {
                    NightRecord {
                        location_id: location_id.into(),
                        first_night: today_s.clone(),
                        last_night: today_s.clone(),
                        nights_used: 1,
                    }
                }
            } else {
                NightRecord {
                    location_id: location_id.into(),
                    first_night: today_s.clone(),
                    last_night: today_s.clone(),
                    nights_used: 1,
                }
            }
        } else {
            NightRecord {
                location_id: location_id.into(),
                first_night: today_s.clone(),
                last_night: today_s.clone(),
                nights_used: 1,
            }
        };

        // Clear other location keys for this pack (camping elsewhere resets).
        // Spec: reset when user camps elsewhere. We only store one active key
        // per pack via a pointer.
        let active_key = format!("rtr_night_active:{pack}");
        if let Some(prev) = host.kv_get(&active_key) {
            if prev != location_id {
                let _ = host.kv_set(&kv_key(pack, &prev), "");
            }
        }
        host.kv_set(&active_key, location_id)?;
        host.kv_set(&key, &serde_json::to_string(&rec).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{CampingHost, TravelMode};
    use driver_break_core::config::SafetyConfig;
    use std::collections::HashMap;

    struct MemHost {
        kv: HashMap<String, String>,
        available: bool,
        safety: Option<SafetyConfig>,
        date: Option<LocalDate>,
        buildings: Vec<(f64, f64)>,
    }

    impl CampingHost for MemHost {
        fn safety_config(&self) -> Option<SafetyConfig> {
            self.safety.clone()
        }
        fn clock_local(&self) -> Option<LocalDate> {
            self.date
        }
        fn plugin_kv_available(&self) -> bool {
            self.available
        }
        fn kv_get(&self, key: &str) -> Option<String> {
            self.kv.get(key).cloned().filter(|s| !s.is_empty())
        }
        fn kv_set(&mut self, key: &str, value: &str) -> Result<(), String> {
            self.kv.insert(key.into(), value.into());
            Ok(())
        }
        fn admin_country_iso(&self, _: f64, _: f64) -> Option<String> {
            Some("no".into())
        }
        fn admin_subdivision_iso(&self, _: f64, _: f64) -> Option<String> {
            None
        }
        fn travel_mode(&self) -> TravelMode {
            TravelMode::NonMotorised
        }
        fn overnight_buildings(&self) -> &[(f64, f64)] {
            &self.buildings
        }
        fn overnight_glacier_rings(&self) -> &[Vec<[f64; 2]>] {
            &[]
        }
    }

    fn host() -> MemHost {
        MemHost {
            kv: HashMap::new(),
            available: true,
            safety: Some(SafetyConfig::default()),
            date: Some(LocalDate {
                year: 2026,
                month: 7,
                day: 1,
            }),
            buildings: vec![],
        }
    }

    #[test]
    fn third_consecutive_night_exceeds() {
        let mut h = host();
        let loc = location_id_from_lat_lon(61.1, 10.5);
        NightStore::record_night(
            &mut h,
            "no",
            &loc,
            LocalDate {
                year: 2026,
                month: 7,
                day: 1,
            },
        )
        .unwrap();
        NightStore::record_night(
            &mut h,
            "no",
            &loc,
            LocalDate {
                year: 2026,
                month: 7,
                day: 2,
            },
        )
        .unwrap();
        assert!(NightStore::would_exceed(
            &h,
            "no",
            &loc,
            LocalDate {
                year: 2026,
                month: 7,
                day: 3,
            },
            2
        ));
    }

    #[test]
    fn gap_night_resets() {
        let mut h = host();
        let loc = location_id_from_lat_lon(61.1, 10.5);
        NightStore::record_night(
            &mut h,
            "no",
            &loc,
            LocalDate {
                year: 2026,
                month: 7,
                day: 1,
            },
        )
        .unwrap();
        NightStore::record_night(
            &mut h,
            "no",
            &loc,
            LocalDate {
                year: 2026,
                month: 7,
                day: 2,
            },
        )
        .unwrap();
        // Gap on day 3 → day 5 is a fresh stay.
        assert!(!NightStore::would_exceed(
            &h,
            "no",
            &loc,
            LocalDate {
                year: 2026,
                month: 7,
                day: 5,
            },
            2
        ));
    }

    #[test]
    fn move_elsewhere_resets_active() {
        let mut h = host();
        let a = location_id_from_lat_lon(61.1, 10.5);
        let b = location_id_from_lat_lon(61.2, 10.6);
        NightStore::record_night(
            &mut h,
            "no",
            &a,
            LocalDate {
                year: 2026,
                month: 7,
                day: 1,
            },
        )
        .unwrap();
        NightStore::record_night(
            &mut h,
            "no",
            &a,
            LocalDate {
                year: 2026,
                month: 7,
                day: 2,
            },
        )
        .unwrap();
        NightStore::record_night(
            &mut h,
            "no",
            &b,
            LocalDate {
                year: 2026,
                month: 7,
                day: 3,
            },
        )
        .unwrap();
        // A was cleared when moving to B.
        assert!(!NightStore::would_exceed(
            &h,
            "no",
            &a,
            LocalDate {
                year: 2026,
                month: 7,
                day: 4,
            },
            2
        ));
    }
}
