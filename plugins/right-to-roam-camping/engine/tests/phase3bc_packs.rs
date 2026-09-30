//! Phase 3b/3c: territories, Tier B flag-off degrade, tenure unknown, designated layers.

use navi_right_to_roam_camping::{
    all_declared_packs, assert_tier_b_and_land_manager_flags_default_off, pack_for_location,
    pack_for_location_with_tenure, suggest_overnight_fixed_probes, territory_pack, CampingHost,
    DesignatedLayer, LandTenureStatus, LocalDate, OvernightSafety, ProtectedAreaStatus, Tier,
    TravelMode,
};
use std::collections::HashMap;

struct H {
    country: String,
    subdivision: Option<String>,
    tenure_mgr: Option<String>,
    tenure: LandTenureStatus,
    flag_on: HashMap<String, bool>,
    layer_ready: HashMap<String, bool>,
    tents: Vec<(f64, f64)>,
    date: LocalDate,
}

impl CampingHost for H {
    fn safety_config(&self) -> Option<OvernightSafety> {
        Some(OvernightSafety::default())
    }
    fn clock_local(&self) -> Option<LocalDate> {
        Some(self.date)
    }
    fn plugin_kv_available(&self) -> bool {
        true
    }
    fn kv_get(&self, _: &str) -> Option<String> {
        None
    }
    fn kv_set(&mut self, _: &str, _: &str) -> Result<(), String> {
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
    fn camping_pack_flag_enabled(&self, flag_id: &str) -> bool {
        self.flag_on.get(flag_id).copied().unwrap_or(false)
    }
    fn designated_layer_ready(&self, layer: DesignatedLayer) -> bool {
        self.layer_ready
            .get(format!("{layer:?}").as_str())
            .copied()
            .unwrap_or(false)
    }
    fn tent_sites_near(&self, _: f64, _: f64, _: f64) -> Vec<navi_right_to_roam_camping::TentSiteHit> {
        self.tents
            .iter()
            .map(|(la, lo)| navi_right_to_roam_camping::TentSiteHit {
                lat: *la,
                lon: *lo,
                name: Some("fixture".into()),
            })
            .collect()
    }
    fn land_tenure_status(&self, _: f64, _: f64) -> LandTenureStatus {
        self.tenure
    }
    fn land_tenure_manager(&self, _: f64, _: f64) -> Option<String> {
        self.tenure_mgr.clone()
    }
}

fn base(country: &str) -> H {
    H {
        country: country.into(),
        subdivision: None,
        tenure_mgr: None,
        tenure: LandTenureStatus::Unknown,
        flag_on: HashMap::new(),
        layer_ready: HashMap::new(),
        tents: vec![],
        date: LocalDate {
            year: 2026,
            month: 7,
            day: 1,
        },
    }
}

#[test]
fn every_territory_has_own_tier_d_pack() {
    for iso in ["ax", "sj", "fo", "gl", "gb-nir", "im", "je", "gg", "gi"] {
        let p = territory_pack(iso).expect(iso);
        assert_eq!(p.tier, Tier::D, "{iso}");
        let out = suggest_overnight_fixed_probes(&mut base(iso), &[(60.0, 10.0)], Some(1));
        assert!(
            out.probe_log.iter().any(|e| !e.accepted),
            "{iso} must not accept wild camp; log={:?}",
            out.probe_log
        );
    }
}

#[test]
fn tier_b_flag_off_degrades_even_when_layers_unknown() {
    assert_tier_b_and_land_manager_flags_default_off();
    let mut h = base("de");
    h.subdivision = Some("DE-BB".into());
    let out = suggest_overnight_fixed_probes(&mut h, &[(52.5, 13.4)], Some(1));
    assert!(
        out.probe_log
            .iter()
            .any(|e| e.reason.contains("flag_off") || e.reason.contains("fallback_c")),
        "Brandenburg flag OFF must degrade to C/D path; log={:?}",
        out.probe_log
    );
    assert!(!out.list.cards.iter().any(|c| c.accepted && c.tier == Tier::B));
}

#[test]
fn tier_b_flag_on_but_layers_unknown_still_degrades() {
    let mut h = base("de");
    h.subdivision = Some("DE-BB".into());
    h.flag_on
        .insert("tier_b_de_brandenburg".into(), true);
    // Flag ON but forest/protected/residential unknown → conditions unmet → C fallback.
    let out = suggest_overnight_fixed_probes(&mut h, &[(52.5, 13.4)], Some(1));
    assert!(
        out.probe_log.iter().any(|e| e.reason.contains("condition_")
            || e.reason.contains("fallback_c")
            || e.reason.contains("designated_layer")),
        "flag ON with unknown layers must still degrade; log={:?}",
        out.probe_log
    );
    assert!(!out.list.cards.iter().any(|c| c.accepted && c.tier == Tier::B));
}

#[test]
fn designated_layer_not_classified_yields_empty() {
    let mut h = base("nl");
    // Tier C NL requires Paalkamp layer; not ready → empty (no card).
    let out = suggest_overnight_fixed_probes(&mut h, &[(52.1, 5.1)], Some(1));
    assert!(out
        .probe_log
        .iter()
        .any(|e| e.reason == "designated_layer_not_classified"));
    assert!(out.list.cards.is_empty());
}

#[test]
fn usa_canada_russia_tenure_unknown_is_tier_d() {
    for iso in ["us", "ca", "ru"] {
        let p = pack_for_location(Some(iso), None);
        assert!(
            p.tier == Tier::D || p.requires_land_tenure || p.maintainer_flag_default_off,
            "{iso} {:?}",
            p.id
        );
        let mut h = base(iso);
        let out = suggest_overnight_fixed_probes(&mut h, &[(40.0, -105.0)], Some(1));
        assert!(
            !out.list.cards.iter().any(|c| c.accepted && c.tier == Tier::B),
            "{iso} must not accept Tier B with unknown tenure/flag; log={:?}",
            out.probe_log
        );
    }
    let known = pack_for_location_with_tenure(Some("us"), None, Some("blm"));
    assert_eq!(known.tier, Tier::B);
    assert!(known.maintainer_flag_default_off);
}

#[test]
fn chile_carries_secondary_conaf_fire_note() {
    let p = pack_for_location(Some("cl"), None);
    assert!(p
        .secondary_card_notes
        .iter()
        .any(|n| n.contains("CONAF") && n.contains("re-verify")));
}

#[test]
fn declared_pack_inventory_is_non_empty() {
    let packs = all_declared_packs();
    assert!(packs.len() > 30, "expected Phase 3b/3c pack inventory");
    assert!(packs.iter().any(|p| p.tier == Tier::C));
    assert!(packs.iter().any(|p| p.tier == Tier::B && p.maintainer_flag_default_off));
}
