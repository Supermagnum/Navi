//! Item 5: night-store against file-backed plugin_kv (survives reopen).

use navi_plugin_host::FilePluginKv;
use navi_right_to_roam_camping::{
    location_id_from_lat_lon, CampingHost, LocalDate, NightStore, OvernightSafety, TravelMode,
};
use std::path::PathBuf;

struct FileCampingHost {
    kv: FilePluginKv,
    available: bool,
    safety: Option<OvernightSafety>,
    date: Option<LocalDate>,
}

impl CampingHost for FileCampingHost {
    fn safety_config(&self) -> Option<OvernightSafety> {
        self.safety
    }
    fn clock_local(&self) -> Option<LocalDate> {
        self.date
    }
    fn plugin_kv_available(&self) -> bool {
        self.available
    }
    fn kv_get(&self, key: &str) -> Option<String> {
        self.kv.get(key)
    }
    fn kv_set(&mut self, key: &str, value: &str) -> Result<(), String> {
        self.kv.set(key, value).map_err(|e| e.to_string())
    }
    fn admin_country_iso(&self, _: f64, _: f64) -> Option<String> {
        Some("no".into())
    }
    fn admin_subdivision_iso(&self, _: f64, _: f64) -> Option<String> {
        Some("no-34".into())
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

fn open_host(path: PathBuf, available: bool) -> FileCampingHost {
    FileCampingHost {
        kv: FilePluginKv::open(path).expect("open kv"),
        available,
        safety: Some(OvernightSafety::default()),
        date: Some(LocalDate {
            year: 2026,
            month: 7,
            day: 1,
        }),
    }
}

#[test]
fn file_kv_third_night_gap_move_and_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plugin_kv.json");
    let loc = location_id_from_lat_lon(61.14, 10.60);
    let other = location_id_from_lat_lon(61.20, 10.70);

    {
        let mut h = open_host(path.clone(), true);
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

    // Re-open same file — persistence.
    {
        let h = open_host(path.clone(), true);
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

    // Move elsewhere resets.
    {
        let mut h = open_host(path.clone(), true);
        NightStore::record_night(
            &mut h,
            "no",
            &other,
            LocalDate {
                year: 2026,
                month: 7,
                day: 3,
            },
        )
        .unwrap();
        assert!(!NightStore::would_exceed(
            &h,
            "no",
            &loc,
            LocalDate {
                year: 2026,
                month: 7,
                day: 4,
            },
            2
        ));
    }

    // KV unavailable → hard decline for 2-night rule.
    {
        let h = open_host(path, false);
        assert!(NightStore::would_exceed(
            &h,
            "no",
            &loc,
            LocalDate {
                year: 2026,
                month: 7,
                day: 1,
            },
            2
        ));
    }
}

#[test]
fn prune_uses_pack_max_not_magic_number() {
    use navi_right_to_roam_camping::{
        default_night_store_retention_days, on_camping_plugin_enable_changed,
    };
    assert_eq!(default_night_store_retention_days(), 2);

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("camping_night.json");
    let loc = location_id_from_lat_lon(61.14, 10.60);
    {
        let mut h = open_host(path.clone(), true);
        h.date = Some(LocalDate {
            year: 2026,
            month: 6,
            day: 1,
        });
        NightStore::record_night(
            &mut h,
            "no",
            &loc,
            LocalDate {
                year: 2026,
                month: 6,
                day: 1,
            },
        )
        .unwrap();
        let keys = h.kv.keys();
        let cleared = NightStore::prune_older_than(
            &mut h,
            LocalDate {
                year: 2026,
                month: 7,
                day: 1,
            },
            default_night_store_retention_days(),
            &keys,
        );
        assert!(cleared >= 1);
        assert!(h.kv.get(&format!("rtr_night:no:{loc}")).is_none());
    }

    std::fs::write(&path, r#"{"x":"1"}"#).unwrap();
    assert!(on_camping_plugin_enable_changed("right_to_roam_camping", false, &path).unwrap());
    assert!(!path.exists());
}
