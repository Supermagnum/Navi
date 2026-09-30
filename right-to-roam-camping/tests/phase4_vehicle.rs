//! Phase 4 — vehicle overnight + "On foot from here" section.

use navi_right_to_roam_camping::{
    annotate_on_foot_from_here, exclude_non_motorised_only_pack_in_motorised,
    suggest_overnight_fixed_probes, CampingHost, LocalDate, OvernightSafety, TravelMode,
    VehicleClass, VehicleProfile, VehicleSiteHit, VehicleSiteKind,
};
use std::collections::HashMap;

struct H {
    country: String,
    sub: Option<String>,
    mode: TravelMode,
    profile: VehicleProfile,
    kv: HashMap<String, String>,
}

impl CampingHost for H {
    fn safety_config(&self) -> Option<OvernightSafety> {
        Some(OvernightSafety::default())
    }
    fn clock_local(&self) -> Option<LocalDate> {
        Some(LocalDate {
            year: 2026,
            month: 7,
            day: 1,
        })
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
        self.sub.clone()
    }
    fn travel_mode(&self) -> TravelMode {
        self.mode
    }
    fn overnight_buildings(&self) -> &[(f64, f64)] {
        &[]
    }
    fn overnight_glacier_rings(&self) -> &[Vec<[f64; 2]>] {
        &[]
    }
    fn vehicle_overnight_profile(&self) -> VehicleProfile {
        self.profile
    }
    fn protected_area_layer_ready(&self) -> bool {
        true
    }
    fn landcover_layer_ready(&self) -> bool {
        true
    }
}

fn motor_no(profile: VehicleProfile) -> H {
    H {
        country: "no".into(),
        sub: None,
        mode: TravelMode::Motorised,
        profile,
        kv: HashMap::new(),
    }
}

#[test]
fn motorised_tent_spots_only_in_on_foot_section_with_notes() {
    let mut h = motor_no(VehicleProfile {
        class: VehicleClass::CampervanMotorhome,
        is_professional_driver_under_rest_rules: false,
    });
    // Without NVDB → no vehicle cards; Norwegian pack still yields on-foot.
    let out = suggest_overnight_fixed_probes(&mut h, &[(61.12, 10.47)], Some(3));
    assert!(
        out.list.cards.iter().filter(|c| c.accepted).count() == 0,
        "tent accepts must not appear in main list in motorised mode"
    );
    assert!(
        out.vehicle.cards.is_empty(),
        "no NVDB → no vehicle overnight invented"
    );
    let foot = out
        .on_foot_from_here
        .cards
        .iter()
        .filter(|c| c.accepted)
        .collect::<Vec<_>>();
    assert!(!foot.is_empty(), "expected on-foot tent cards; log={:?}", out.probe_log);
    for c in foot {
        assert!(c.notes.iter().any(|n| n.contains("On foot from here")));
        assert!(c.notes.iter().any(|n| n.contains("motorferdselloven")));
        assert!(c.walk_m.is_some());
        assert!(c.fire_text.is_some());
        assert!(c.bare_rock_note.is_some());
        assert!(!c
            .seed_road_highway
            .as_deref()
            .unwrap_or("")
            .starts_with("vehicle:"));
    }
}

#[test]
fn swedish_on_foot_says_general_guidance() {
    let card = navi_right_to_roam_camping::CampingCard {
        lat: 59.9,
        lon: 12.2,
        accepted: true,
        decline: None,
        reject_reason: None,
        tier: navi_right_to_roam_camping::Tier::A,
        country_iso: "se".into(),
        subdivision_iso: None,
        legal_basis: "Allemansrätten".into(),
        sources: vec![],
        fire_text: None,
        bare_rock_note: None,
        notes: vec![],
        not_checked: navi_right_to_roam_camping::NotCheckedLayers::default(),
        disclaimer: navi_right_to_roam_camping::DISCLAIMER.into(),
        location_id: "x".into(),
        seed_road_highway: Some("track".into()),
        walk_m: Some(90.0),
    };
    let c = annotate_on_foot_from_here(card, 90.0, "se");
    assert!(c.notes.iter().any(|n| n.contains("general guidance, not law")));
    assert!(!c.notes.iter().any(|n| n.contains("motorferdselloven")));
}

#[test]
fn german_tier_b_excluded_in_motorised_mode() {
    assert!(exclude_non_motorised_only_pack_in_motorised(
        true,
        TravelMode::Motorised
    ));
    let mut h = H {
        country: "de".into(),
        // Brandenburg — Tier B requires NonMotorisedTravel among other conditions.
        sub: Some("de-bb".into()),
        mode: TravelMode::Motorised,
        profile: VehicleProfile {
            class: VehicleClass::CampervanMotorhome,
            is_professional_driver_under_rest_rules: false,
        },
        kv: HashMap::new(),
    };
    let out = suggest_overnight_fixed_probes(&mut h, &[(52.4, 13.0)], Some(3));
    assert!(
        out.on_foot_from_here
            .cards
            .iter()
            .filter(|c| c.accepted)
            .count()
            == 0,
        "German Tier B non-motorised pack must be absent on foot in motorised mode; log={:?}",
        out.probe_log
    );
    assert!(out
        .probe_log
        .iter()
        .any(|e| e.reason.contains("non_motorised") || e.reason.contains("host_condition") || !e.accepted));
}

#[test]
fn tent_pack_spots_never_appear_as_vehicle_cards() {
    let mut h = motor_no(VehicleProfile {
        class: VehicleClass::CampervanMotorhome,
        is_professional_driver_under_rest_rules: false,
    });
    let out = suggest_overnight_fixed_probes(&mut h, &[(61.12, 10.47)], Some(3));
    for c in &out.vehicle.cards {
        assert!(c
            .seed_road_highway
            .as_deref()
            .is_some_and(|s| s.starts_with("vehicle:")));
    }
    for c in out.on_foot_from_here.cards.iter().filter(|c| c.accepted) {
        assert!(!c
            .seed_road_highway
            .as_deref()
            .unwrap_or("")
            .starts_with("vehicle:"));
    }
    let _ = VehicleSiteKind::CaravanSite;
    let _ = VehicleSiteHit {
        lat: 0.0,
        lon: 0.0,
        kind: VehicleSiteKind::RoadsideParking,
        name: None,
    };
}
