//! Card-completeness checks for every Phase 3a pack.

use navi_right_to_roam_camping::{
    builtin_enabled_packs, suggest_overnight_fixed_probes, CampingHost, LocalDate, OvernightSafety,
    ProtectedAreaStatus, Tier, TravelMode, DISCLAIMER,
};
use std::collections::HashMap;

struct FixtureHost {
    country: String,
    subdivision: Option<String>,
    protected: ProtectedAreaStatus,
    cmz_ready: bool,
    cmz_inside: bool,
    safety: OvernightSafety,
    date: LocalDate,
    kv: HashMap<String, String>,
}

impl CampingHost for FixtureHost {
    fn safety_config(&self) -> Option<OvernightSafety> {
        Some(self.safety)
    }
    fn clock_local(&self) -> Option<LocalDate> {
        Some(self.date)
    }
    fn plugin_kv_available(&self) -> bool {
        true
    }
    fn kv_get(&self, key: &str) -> Option<String> {
        self.kv.get(key).cloned().filter(|s| !s.is_empty())
    }
    fn kv_set(&mut self, key: &str, value: &str) -> Result<(), String> {
        if value.is_empty() {
            self.kv.remove(key);
        } else {
            self.kv.insert(key.into(), value.into());
        }
        Ok(())
    }
    fn admin_country_iso(&self, _: f64, _: f64) -> Option<String> {
        Some(self.country.clone())
    }
    fn admin_subdivision_iso(&self, _: f64, _: f64) -> Option<String> {
        self.subdivision.clone()
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
    fn protected_area_layer_ready(&self) -> bool {
        !matches!(self.protected, ProtectedAreaStatus::Unknown)
    }
    fn protected_area_status(&self, _: f64, _: f64) -> ProtectedAreaStatus {
        self.protected
    }
    fn cmz_layer_ready(&self) -> bool {
        self.cmz_ready
    }
    fn cmz_contains(&self, _: f64, _: f64) -> bool {
        self.cmz_inside
    }
}

#[test]
fn every_builtin_pack_declares_completeness_fields() {
    for pack in builtin_enabled_packs() {
        assert!(!pack.legal_basis.is_empty(), "{:?}", pack.id);
        assert!(
            matches!(pack.tier, Tier::A | Tier::B | Tier::C | Tier::D),
            "{:?} unexpected tier {:?}",
            pack.id,
            pack.tier
        );
        if pack.tier == Tier::A {
            assert!(
                !pack.guidance_notes.is_empty(),
                "{:?} missing guidance",
                pack.id
            );
            assert!(!pack.sources.is_empty(), "{:?} missing sources", pack.id);
        }
        let _ = (
            &pack.distance,
            &pack.duration,
            &pack.fire,
            pack.farmland_not_checked_when_landcover_unknown,
            pack.decline_when_protected_unknown,
        );
    }
}

#[test]
fn finland_mainland_accepts_aland_is_tier_d() {
    let mut fi = FixtureHost {
        country: "fi".into(),
        subdivision: None,
        protected: ProtectedAreaStatus::Unknown,
        cmz_ready: false,
        cmz_inside: false,
        safety: OvernightSafety::default(),
        date: LocalDate {
            year: 2026,
            month: 7,
            day: 1,
        },
        kv: HashMap::new(),
    };
    let out = suggest_overnight_fixed_probes(&mut fi, &[(62.0, 25.0)], Some(1));
    assert!(out
        .list
        .cards
        .iter()
        .any(|c| c.accepted && c.country_iso == "fi"));
    assert!(out.list.cards.iter().any(|c| c
        .fire_text
        .as_deref()
        .is_some_and(|t| t.contains("landowner permission"))));

    let mut ax = FixtureHost {
        country: "ax".into(),
        subdivision: None,
        protected: ProtectedAreaStatus::Unknown,
        cmz_ready: false,
        cmz_inside: false,
        safety: OvernightSafety::default(),
        date: LocalDate {
            year: 2026,
            month: 7,
            day: 1,
        },
        kv: HashMap::new(),
    };
    let out = suggest_overnight_fixed_probes(&mut ax, &[(60.1, 19.9)], Some(1));
    assert!(out.probe_log.iter().any(|e| e.reason.contains("tier_d")));
    assert!(out
        .list
        .cards
        .iter()
        .any(|c| c.tier == Tier::D && c.country_iso == "ax"));

    // FI country with Åland subdivision must also be Tier D (never mainland pack).
    let mut fi_ax = FixtureHost {
        country: "fi".into(),
        subdivision: Some("AX".into()),
        protected: ProtectedAreaStatus::Unknown,
        cmz_ready: false,
        cmz_inside: false,
        safety: OvernightSafety::default(),
        date: LocalDate {
            year: 2026,
            month: 7,
            day: 1,
        },
        kv: HashMap::new(),
    };
    let out = suggest_overnight_fixed_probes(&mut fi_ax, &[(60.1, 19.9)], Some(1));
    assert!(out.probe_log.iter().any(|e| e.reason.contains("tier_d")));
}

#[test]
fn scotland_unknown_subdivision_is_tier_d_fixture_sct_is_tier_a() {
    let mut gb = FixtureHost {
        country: "gb".into(),
        subdivision: None,
        protected: ProtectedAreaStatus::Unknown,
        cmz_ready: false,
        cmz_inside: false,
        safety: OvernightSafety::default(),
        date: LocalDate {
            year: 2026,
            month: 1,
            day: 15,
        },
        kv: HashMap::new(),
    };
    let out = suggest_overnight_fixed_probes(&mut gb, &[(56.8, -5.1)], Some(1));
    assert!(
        out.list
            .cards
            .iter()
            .any(|c| c.tier == Tier::D && !c.accepted),
        "unknown GB subdivision must be Tier D; log={:?}",
        out.probe_log
    );
    let card = out
        .list
        .cards
        .iter()
        .find(|c| c.tier == Tier::D)
        .expect("D card");
    assert!(
        !card.legal_basis.to_ascii_lowercase().contains("england")
            && !card.legal_basis.to_ascii_lowercase().contains("wales")
            && !card.legal_basis.to_ascii_lowercase().contains("darwall"),
        "must not claim England/Wales law; got {}",
        card.legal_basis
    );

    gb.subdivision = Some("GB-SCT".into());
    let out = suggest_overnight_fixed_probes(&mut gb, &[(56.8, -5.1)], Some(1));
    assert!(
        out.list
            .cards
            .iter()
            .any(|c| c.accepted && c.tier == Tier::A),
        "injected GB-SCT outside CMZ season should accept; log={:?}",
        out.probe_log
    );
}

#[test]
fn belfast_and_scottish_point_unknown_subdivision_are_tier_d() {
    // Belfast approx 54.597, -5.930
    let mut belfast = FixtureHost {
        country: "gb".into(),
        subdivision: None,
        protected: ProtectedAreaStatus::Unknown,
        cmz_ready: false,
        cmz_inside: false,
        safety: OvernightSafety::default(),
        date: LocalDate {
            year: 2026,
            month: 7,
            day: 1,
        },
        kv: HashMap::new(),
    };
    let out = suggest_overnight_fixed_probes(&mut belfast, &[(54.597, -5.930)], Some(1));
    assert!(out.list.cards.iter().any(|c| c.tier == Tier::D));
    let card = out.list.cards.iter().find(|c| c.tier == Tier::D).unwrap();
    let blob = format!("{} {}", card.legal_basis, card.notes.join(" "));
    assert!(!blob.to_ascii_lowercase().contains("england/wales"));
    assert!(!blob.to_ascii_lowercase().contains("darwall"));
    assert!(!blob
        .to_ascii_lowercase()
        .contains("scottish outdoor access"));

    // Scottish Highlands point with unknown subdivision — still D, not SCT pack.
    let out = suggest_overnight_fixed_probes(&mut belfast, &[(56.8, -5.1)], Some(1));
    assert!(out.list.cards.iter().any(|c| c.tier == Tier::D));
    assert!(!out
        .list
        .cards
        .iter()
        .any(|c| c.accepted && c.tier == Tier::A));
}

#[test]
fn scotland_cmz_season_without_layer_declines() {
    let mut gb = FixtureHost {
        country: "gb".into(),
        subdivision: Some("GB-SCT".into()),
        protected: ProtectedAreaStatus::Unknown,
        cmz_ready: false,
        cmz_inside: false,
        safety: OvernightSafety::default(),
        date: LocalDate {
            year: 2026,
            month: 6,
            day: 15,
        },
        kv: HashMap::new(),
    };
    let out = suggest_overnight_fixed_probes(&mut gb, &[(56.2, -4.6)], Some(1));
    assert!(out
        .probe_log
        .iter()
        .any(|e| e.reason == "scotland_cmz_unproven_outside"));
}

#[test]
fn iceland_declines_when_protected_unknown_accepts_when_clear() {
    let mut is = FixtureHost {
        country: "is".into(),
        subdivision: None,
        protected: ProtectedAreaStatus::Unknown,
        cmz_ready: false,
        cmz_inside: false,
        safety: OvernightSafety::default(),
        date: LocalDate {
            year: 2026,
            month: 7,
            day: 1,
        },
        kv: HashMap::new(),
    };
    let out = suggest_overnight_fixed_probes(&mut is, &[(64.1, -21.9)], Some(1));
    assert!(out
        .probe_log
        .iter()
        .any(|e| e.reason == "iceland_protected_area_unknown"));

    is.protected = ProtectedAreaStatus::Clear;
    let out = suggest_overnight_fixed_probes(&mut is, &[(64.1, -21.9)], Some(1));
    assert!(
        out.list
            .cards
            .iter()
            .any(|c| c.accepted && c.country_iso == "is"),
        "fixture Clear should allow Iceland Tier A; log={:?}",
        out.probe_log
    );
}

#[test]
fn accepted_cards_carry_disclaimer_and_sources() {
    let mut se = FixtureHost {
        country: "se".into(),
        subdivision: None,
        protected: ProtectedAreaStatus::Unknown,
        cmz_ready: false,
        cmz_inside: false,
        safety: OvernightSafety::default(),
        date: LocalDate {
            year: 2026,
            month: 7,
            day: 1,
        },
        kv: HashMap::new(),
    };
    let out = suggest_overnight_fixed_probes(&mut se, &[(59.89, 12.19)], Some(1));
    let card = out
        .list
        .cards
        .iter()
        .find(|c| c.accepted)
        .expect("SE accept");
    assert_eq!(card.disclaimer, DISCLAIMER);
    assert!(!card.sources.is_empty());
    assert!(!card.legal_basis.is_empty());
    assert!(!card.notes.is_empty());
    assert!(card.not_checked.protected_area);
    assert!(card.not_checked.landcover);
    // One NotChecked line per topic — pack farmland/park guidance dropped while layers unknown.
    let farmland_lines = card
        .notes
        .iter()
        .filter(|n| n.to_ascii_lowercase().contains("farmland"))
        .count();
    assert_eq!(
        farmland_lines, 1,
        "expected single farmland/landcover not-checked line; notes={:?}",
        card.notes
    );
    let park_lines = card
        .notes
        .iter()
        .filter(|n| {
            let l = n.to_ascii_lowercase();
            l.contains("national park") || l.contains("nature reserve")
        })
        .count();
    assert_eq!(
        park_lines, 1,
        "expected single protected-area not-checked line; notes={:?}",
        card.notes
    );
    assert!(card
        .fire_text
        .as_deref()
        .is_some_and(|t| t.contains("Naturvårdsverket")));
    assert!(card
        .fire_text
        .as_deref()
        .is_some_and(|t| t.contains("eldningsförbud")));
}

#[test]
fn norway_on_foot_cards_carry_fire_and_bare_rock() {
    struct Motor {
        inner: FixtureHost,
    }
    impl CampingHost for Motor {
        fn safety_config(&self) -> Option<OvernightSafety> {
            self.inner.safety_config()
        }
        fn clock_local(&self) -> Option<LocalDate> {
            self.inner.clock_local()
        }
        fn plugin_kv_available(&self) -> bool {
            true
        }
        fn kv_get(&self, key: &str) -> Option<String> {
            self.inner.kv_get(key)
        }
        fn kv_set(&mut self, key: &str, value: &str) -> Result<(), String> {
            self.inner.kv_set(key, value)
        }
        fn admin_country_iso(&self, lat: f64, lon: f64) -> Option<String> {
            self.inner.admin_country_iso(lat, lon)
        }
        fn admin_subdivision_iso(&self, lat: f64, lon: f64) -> Option<String> {
            self.inner.admin_subdivision_iso(lat, lon)
        }
        fn travel_mode(&self) -> TravelMode {
            TravelMode::Motorised
        }
        fn overnight_buildings(&self) -> &[(f64, f64)] {
            self.inner.overnight_buildings()
        }
        fn overnight_glacier_rings(&self) -> &[Vec<[f64; 2]>] {
            self.inner.overnight_glacier_rings()
        }
        fn vehicle_overnight_profile(&self) -> navi_right_to_roam_camping::VehicleProfile {
            navi_right_to_roam_camping::VehicleProfile {
                class: navi_right_to_roam_camping::VehicleClass::CampervanMotorhome,
                is_professional_driver_under_rest_rules: false,
            }
        }
    }
    let mut h = Motor {
        inner: FixtureHost {
            country: "no".into(),
            subdivision: Some("no-34".into()),
            protected: ProtectedAreaStatus::Unknown,
            cmz_ready: false,
            cmz_inside: false,
            safety: OvernightSafety::default(),
            date: LocalDate {
                year: 2026,
                month: 7,
                day: 1,
            },
            kv: HashMap::new(),
        },
    };
    let out = suggest_overnight_fixed_probes(&mut h, &[(61.12, 10.47)], Some(1));
    let card = out
        .on_foot_from_here
        .cards
        .iter()
        .find(|c| c.accepted)
        .expect("NO on-foot accept");
    assert!(
        card.fire_text
            .as_deref()
            .is_some_and(|t| t.contains("15 April") && t.contains("15 September")),
        "date-gated fire text missing; fire={:?}",
        card.fire_text
    );
    assert!(
        card.bare_rock_note
            .as_deref()
            .is_some_and(|t| t.contains("bare rock")),
        "bare-rock note missing; note={:?}",
        card.bare_rock_note
    );
    assert!(card.notes.iter().any(|n| n.contains("On foot from here")));
}
