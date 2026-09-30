//! Phase 3a pack fixture tests (SYNTHETIC hosts — no map pack required).

use navi_right_to_roam_camping::{
    builtin_enabled_packs, suggest_overnight_fixed_probes, CampingHost, LocalDate, OvernightSafety,
    ProtectedAreaStatus, TravelMode, DISCLAIMER, Tier,
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
            pack.tier == Tier::A || pack.tier == Tier::D,
            "{:?} unexpected tier {:?}",
            pack.id,
            pack.tier
        );
        if pack.tier == Tier::A {
            assert!(!pack.guidance_notes.is_empty(), "{:?} missing guidance", pack.id);
            assert!(!pack.sources.is_empty(), "{:?} missing sources", pack.id);
        }
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
    assert!(out.list.cards.iter().any(|c| c.accepted && c.country_iso == "fi"));
    assert!(out
        .list
        .cards
        .iter()
        .any(|c| c.fire_text.as_deref().is_some_and(|t| t.contains("landowner permission"))));

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
    assert!(out.list.cards.iter().any(|c| c.tier == Tier::D && c.country_iso == "ax"));

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
    let card = out.list.cards.iter().find(|c| c.accepted).expect("SE accept");
    assert_eq!(card.disclaimer, DISCLAIMER);
    assert!(!card.sources.is_empty());
    assert!(!card.legal_basis.is_empty());
    assert!(!card.notes.is_empty());
    assert!(card.not_checked.protected_area || card.not_checked.landcover);
}
