//! Phase 4 — vehicle overnight mode (§3.5). Never derived from right-to-roam packs.

use crate::card::{CampingCard, SuggestionList};
use crate::host::{CampingHost, TravelMode};
use crate::night_store::{location_id_from_lat_lon, NightStore};
use crate::packs::Tier;
use crate::{NotCheckedLayers, DISCLAIMER};

/// Host-facing vehicle class (mirrors core VehicleOvernightClass strings).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VehicleClass {
    Car,
    CampervanMotorhome,
    Hgv,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VehicleProfile {
    pub class: VehicleClass,
    pub is_professional_driver_under_rest_rules: bool,
}

impl Default for VehicleProfile {
    fn default() -> Self {
        Self {
            class: VehicleClass::Unknown,
            is_professional_driver_under_rest_rules: false,
        }
    }
}

/// NVDB rest-site kinds. Absent answer → never invent døgnhvile / rasteplass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvdbRestKind {
    /// Object type 809 — døgnhvileplass.
    Dognhvileplass809,
    /// Object type 39 — rasteplass.
    Rasteplass39,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VehicleSiteKind {
    Dognhvileplass,
    Rasteplass,
    CaravanSite,
    TruckBayHgvDesignated,
    RoadsideParking,
}

#[derive(Debug, Clone)]
pub struct VehicleSiteHit {
    pub lat: f64,
    pub lon: f64,
    pub kind: VehicleSiteKind,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct VehicleSuggestOutcome {
    pub vehicle: SuggestionList,
    pub on_foot_from_here: SuggestionList,
    pub probe_log: Vec<crate::engine::ProbeLogEntry>,
}

/// Suggest vehicle overnight sites. Fail-safe: missing layers → empty / decline.
pub fn suggest_vehicle_overnight(
    host: &mut dyn CampingHost,
    probes: &[(f64, f64)],
    max_suggestions: Option<usize>,
) -> VehicleSuggestOutcome {
    let mut out = VehicleSuggestOutcome::default();
    out.vehicle.disclaimer = DISCLAIMER.into();
    out.on_foot_from_here.disclaimer = DISCLAIMER.into();

    let profile = host.vehicle_overnight_profile();
    let clock = host.clock_local();

    // Unknown / caravan_combo → designated sites only (unclassified today → empty).
    if matches!(profile.class, VehicleClass::Unknown) {
        out.probe_log.push(crate::engine::ProbeLogEntry {
            lat: probes.first().map(|p| p.0).unwrap_or(0.0),
            lon: probes.first().map(|p| p.1).unwrap_or(0.0),
            accepted: false,
            reason: "vehicle_class_unknown_designated_only".into(),
            road_highway: "vehicle".into(),
        });
        // Designated caravan_site not classified → empty.
        return out;
    }

    for &(lat, lon) in probes {
        let country = host
            .admin_country_iso(lat, lon)
            .unwrap_or_else(|| "unknown".into());
        let c = country.to_ascii_lowercase();

        match c.as_str() {
            "no" => {
                // NVDB 809: only HGV professional with positive 809 answer.
                if let Some(NvdbRestKind::Dognhvileplass809) = host.nvdb_rest_kind(lat, lon) {
                    if matches!(profile.class, VehicleClass::Hgv)
                        && profile.is_professional_driver_under_rest_rules
                    {
                        push_vehicle_card(
                            &mut out,
                            lat,
                            lon,
                            "no",
                            VehicleSiteKind::Dognhvileplass,
                            &profile,
                            "NVDB døgnhvileplass (809) — professional HGV daily/break rest only",
                            &[
                                "Checked: class=hgv AND is_professional_driver_under_rest_rules.",
                                "Follow the parking terms on the site's sign.",
                                "Uses: break (45 min), daily rest (11 h), or reduced weekly rest at most sites.",
                            ],
                            &[
                                "https://www.vegvesen.no/kjoretoy/yrkestransport/kjore-og-hviletid/hvileplasser/",
                            ],
                            max_suggestions,
                        );
                    }
                    // Campervan / non-professional HGV: hard exclude (no card).
                } else if let Some(NvdbRestKind::Rasteplass39) = host.nvdb_rest_kind(lat, lon) {
                    match profile.class {
                        VehicleClass::Car | VehicleClass::CampervanMotorhome => {
                            push_vehicle_card(
                                &mut out,
                                lat,
                                lon,
                                "no",
                                VehicleSiteKind::Rasteplass,
                                &profile,
                                "NVDB rasteplass (39) — short rest; check the sign for overnight rules",
                                &[
                                    "Checked vehicle class: car/campervan.",
                                    "Short rest; check the sign for overnight rules.",
                                    "Never claimed as overnight permitted without a site attribute.",
                                ],
                                &["https://www.vegvesen.no/trafikkinformasjon/vei-og-skilt/drift-og-vedlikehold-av-vei/rasteplasser/"],
                                max_suggestions,
                            );
                        }
                        VehicleClass::Hgv => {
                            push_vehicle_card(
                                &mut out,
                                lat,
                                lon,
                                "no",
                                VehicleSiteKind::Rasteplass,
                                &profile,
                                "NVDB rasteplass (39) — fallback break stop; no daily-rest facilities",
                                &[
                                    "Checked vehicle class: hgv.",
                                    "Fallback break stop; no daily-rest facilities.",
                                ],
                                &["https://www.vegvesen.no/trafikkinformasjon/vei-og-skilt/drift-og-vedlikehold-av-vei/rasteplasser/"],
                                max_suggestions,
                            );
                        }
                        VehicleClass::Unknown => {}
                    }
                } else {
                    out.probe_log.push(crate::engine::ProbeLogEntry {
                        lat,
                        lon,
                        accepted: false,
                        reason: "no_nvdb_rest_kind".into(),
                        road_highway: "vehicle".into(),
                    });
                }
            }
            "de" => {
                let dest = host.route_destination();
                if dest.is_none() {
                    out.probe_log.push(crate::engine::ProbeLogEntry {
                        lat,
                        lon,
                        accepted: false,
                        reason: "de_null_destination_no_roadside".into(),
                        road_highway: "vehicle".into(),
                    });
                    continue;
                }
                let (dlat, dlon) = dest.unwrap();
                if haversine_m(lat, lon, dlat, dlon) < 2_000.0 {
                    out.probe_log.push(crate::engine::ProbeLogEntry {
                        lat,
                        lon,
                        accepted: false,
                        reason: "de_near_destination_exclude".into(),
                        road_highway: "vehicle".into(),
                    });
                    continue;
                }
                // Truck bays never for car/campervan.
                for site in host.vehicle_sites_near(lat, lon, 1_500.0) {
                    if matches!(site.kind, VehicleSiteKind::TruckBayHgvDesignated)
                        && !matches!(profile.class, VehicleClass::Hgv)
                    {
                        continue;
                    }
                    if matches!(site.kind, VehicleSiteKind::TruckBayHgvDesignated)
                        && matches!(profile.class, VehicleClass::Hgv)
                        && !profile.is_professional_driver_under_rest_rules
                    {
                        continue;
                    }
                    if matches!(site.kind, VehicleSiteKind::RoadsideParking) {
                        let loc = location_id_from_lat_lon(site.lat, site.lon);
                        if let Some(d) = clock {
                            if NightStore::would_exceed(host, "de_vehicle", &loc, d, 1) {
                                out.probe_log.push(crate::engine::ProbeLogEntry {
                                    lat: site.lat,
                                    lon: site.lon,
                                    accepted: false,
                                    reason: "de_vehicle_2nd_night_exclude".into(),
                                    road_highway: "vehicle".into(),
                                });
                                continue;
                            }
                        }
                        push_vehicle_card(
                            &mut out,
                            site.lat,
                            site.lon,
                            "de",
                            VehicleSiteKind::RoadsideParking,
                            &profile,
                            "Germany — interrupted-journey roadside rest only",
                            &[
                                "Checked vehicle class on card.",
                                "Sleeping only to restore fitness to drive during an interrupted journey.",
                                "No camping furniture / awning.",
                                "About 10 hours — secondary-source convention, not law.",
                                "Signs forbidding parking or overnight stays override this.",
                            ],
                            &["https://www.gesetze-im-internet.de/stvo_2013/__12.html"],
                            max_suggestions,
                        );
                    } else if matches!(site.kind, VehicleSiteKind::CaravanSite)
                        && matches!(
                            profile.class,
                            VehicleClass::CampervanMotorhome | VehicleClass::Car
                        )
                    {
                        push_vehicle_card(
                            &mut out,
                            site.lat,
                            site.lon,
                            "de",
                            VehicleSiteKind::CaravanSite,
                            &profile,
                            "Germany — Wohnmobilstellplatz / caravan site",
                            &[
                                "Checked vehicle class: car/campervan.",
                                "Preferred designated site.",
                            ],
                            &[],
                            max_suggestions,
                        );
                    } else if matches!(site.kind, VehicleSiteKind::TruckBayHgvDesignated)
                        && matches!(profile.class, VehicleClass::Hgv)
                        && profile.is_professional_driver_under_rest_rules
                    {
                        push_vehicle_card(
                            &mut out,
                            site.lat,
                            site.lon,
                            "de",
                            VehicleSiteKind::TruckBayHgvDesignated,
                            &profile,
                            "Germany — HGV designated truck bay",
                            &["Checked: class=hgv AND professional driver under rest rules."],
                            &[],
                            max_suggestions,
                        );
                    }
                }
            }
            "fr" => {
                // AN article numbers (R111-32 / R111-33) are not mapped to current
                // Code de l'urbanisme numbering — listed as not encoded.
                out.probe_log.push(crate::engine::ProbeLogEntry {
                    lat,
                    lon,
                    accepted: false,
                    reason: "fr_an_articles_r111_not_encoded".into(),
                    road_highway: "vehicle".into(),
                });
                if !host.france_vehicle_exclude_layers_ready() {
                    out.probe_log.push(crate::engine::ProbeLogEntry {
                        lat,
                        lon,
                        accepted: false,
                        reason: "fr_exclude_layers_unavailable".into(),
                        road_highway: "vehicle".into(),
                    });
                    continue;
                }
                if !host.france_vehicle_exclude_clear(lat, lon) {
                    out.probe_log.push(crate::engine::ProbeLogEntry {
                        lat,
                        lon,
                        accepted: false,
                        reason: "fr_hard_exclude_hit".into(),
                        road_highway: "vehicle".into(),
                    });
                    continue;
                }
                for site in host.vehicle_sites_near(lat, lon, 1_500.0) {
                    if matches!(
                        site.kind,
                        VehicleSiteKind::RoadsideParking | VehicleSiteKind::CaravanSite
                    ) {
                        push_vehicle_card(
                            &mut out,
                            site.lat,
                            site.lon,
                            "fr",
                            site.kind,
                            &profile,
                            "France — ordinary public parking / aire (exclude layers clear)",
                            &[
                                "Checked vehicle class.",
                                "Local parking orders and signs apply.",
                                "On the public road: no levelling blocks, awning, table or chairs.",
                                "AN articles R111-32 / R111-33 not encoded (unmapped to current Code de l'urbanisme numbering).",
                            ],
                            &["https://www.legifrance.gouv.fr/codes/article_lc/LEGIARTI000043976727"],
                            max_suggestions,
                        );
                    }
                }
            }
            "us" | "ca" => {
                handle_us_ca_vehicle(host, &mut out, lat, lon, &c, &profile, max_suggestions);
            }
            "ru" | "nz" => {
                out.probe_log.push(crate::engine::ProbeLogEntry {
                    lat,
                    lon,
                    accepted: false,
                    reason: format!("vehicle_todo_{c}"),
                    road_highway: "vehicle".into(),
                });
            }
            _ => {
                // tourism=caravan_site unclassified → empty; HGV professional truck parking only if host provides.
                for site in host.vehicle_sites_near(lat, lon, 1_500.0) {
                    if matches!(site.kind, VehicleSiteKind::CaravanSite)
                        && matches!(
                            profile.class,
                            VehicleClass::CampervanMotorhome | VehicleClass::Car
                        )
                    {
                        push_vehicle_card(
                            &mut out,
                            site.lat,
                            site.lon,
                            &c,
                            VehicleSiteKind::CaravanSite,
                            &profile,
                            "Designated motorhome / caravan site",
                            &["Checked vehicle class.", "Vehicle overnight rules not researched for this country — designated only."],
                            &[],
                            max_suggestions,
                        );
                    }
                    if matches!(site.kind, VehicleSiteKind::TruckBayHgvDesignated)
                        && matches!(profile.class, VehicleClass::Hgv)
                        && profile.is_professional_driver_under_rest_rules
                    {
                        push_vehicle_card(
                            &mut out,
                            site.lat,
                            site.lon,
                            &c,
                            VehicleSiteKind::TruckBayHgvDesignated,
                            &profile,
                            "Signed HGV parking — professional drivers only",
                            &["Checked: class=hgv AND professional driver under rest rules."],
                            &[],
                            max_suggestions,
                        );
                    }
                }
            }
        }

        if let Some(max) = max_suggestions {
            if out.vehicle.probes_accepted >= max {
                break;
            }
        }
    }

    out
}

fn handle_us_ca_vehicle(
    host: &dyn CampingHost,
    out: &mut VehicleSuggestOutcome,
    lat: f64,
    lon: f64,
    country: &str,
    profile: &VehicleProfile,
    max_suggestions: Option<usize>,
) {
    use crate::host::LandTenureStatus;

    let manager = host
        .land_tenure_manager(lat, lon)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let tenure = host.land_tenure_status(lat, lon);

    // Flags OFF / tenure unknown → effective D: designated only.
    let blm_flag = host.camping_pack_flag_enabled("land_mgr_usa_blm");
    let usfs_flag = host.camping_pack_flag_enabled("land_mgr_usa_usfs");
    let flags_off_or_unknown =
        matches!(tenure, LandTenureStatus::Unknown) || (!blm_flag && !usfs_flag);

    if flags_off_or_unknown {
        // Designated caravan_site unclassified → empty; record reason.
        out.probe_log.push(crate::engine::ProbeLogEntry {
            lat,
            lon,
            accepted: false,
            reason: "us_ca_vehicle_flag_off_or_tenure_unknown_effective_d".into(),
            road_highway: "vehicle".into(),
        });
        for site in host.vehicle_sites_near(lat, lon, 1_500.0) {
            if matches!(site.kind, VehicleSiteKind::CaravanSite)
                && !matches!(profile.class, VehicleClass::Hgv | VehicleClass::Unknown)
            {
                push_vehicle_card(
                    out,
                    site.lat,
                    site.lon,
                    country,
                    VehicleSiteKind::CaravanSite,
                    profile,
                    "USA/Canada — designated campground only (vehicle path effective Tier D)",
                    &[
                        "Checked vehicle class.",
                        "Tenure unknown or land-manager flags OFF.",
                    ],
                    &[],
                    max_suggestions,
                );
            }
        }
        return;
    }

    if manager == "blm" {
        if matches!(profile.class, VehicleClass::Hgv) {
            out.probe_log.push(crate::engine::ProbeLogEntry {
                lat,
                lon,
                accepted: false,
                reason: "blm_never_to_hgv".into(),
                road_highway: "vehicle".into(),
            });
            return;
        }
        if blm_flag && matches!(tenure, LandTenureStatus::Known) {
            push_vehicle_card(
                out,
                lat,
                lon,
                country,
                VehicleSiteKind::RoadsideParking,
                profile,
                "USA BLM — dispersed vehicle camping (same stay limits as tent path)",
                &[
                    "Checked vehicle class: car/campervan (never HGV).",
                    "Stay on existing routes; agency guidance typically within 150 ft of designated routes.",
                ],
                &["https://www.blm.gov/programs/recreation/camping"],
                max_suggestions,
            );
        }
        return;
    }

    if manager == "usfs" {
        if !host.usfs_mvum_layer_ready() {
            out.probe_log.push(crate::engine::ProbeLogEntry {
                lat,
                lon,
                accepted: false,
                reason: "usfs_without_mvum_designated_only".into(),
                road_highway: "vehicle".into(),
            });
            for site in host.vehicle_sites_near(lat, lon, 1_500.0) {
                if matches!(site.kind, VehicleSiteKind::CaravanSite)
                    && !matches!(profile.class, VehicleClass::Unknown)
                {
                    push_vehicle_card(
                        out,
                        site.lat,
                        site.lon,
                        country,
                        VehicleSiteKind::CaravanSite,
                        profile,
                        "USA USFS — designated campground only (MVUM unavailable)",
                        &[
                            "Checked vehicle class.",
                            "Without MVUM data, dispersed vehicle camping is not offered.",
                        ],
                        &[],
                        max_suggestions,
                    );
                }
            }
            return;
        }
        // MVUM ready + flag on: still only offer designated until MVUM polygons are classified.
        out.probe_log.push(crate::engine::ProbeLogEntry {
            lat,
            lon,
            accepted: false,
            reason: "usfs_mvum_ready_but_dispersed_not_encoded".into(),
            road_highway: "vehicle".into(),
        });
        return;
    }

    out.probe_log.push(crate::engine::ProbeLogEntry {
        lat,
        lon,
        accepted: false,
        reason: "us_ca_manager_not_vehicle_encoded".into(),
        road_highway: "vehicle".into(),
    });
}

fn push_vehicle_card(
    out: &mut VehicleSuggestOutcome,
    lat: f64,
    lon: f64,
    country: &str,
    kind: VehicleSiteKind,
    profile: &VehicleProfile,
    legal: &str,
    notes: &[&str],
    sources: &[&str],
    max: Option<usize>,
) {
    if let Some(m) = max {
        if out.vehicle.probes_accepted >= m {
            return;
        }
    }
    let class_note = match profile.class {
        VehicleClass::Car => "Vehicle class checked: car",
        VehicleClass::CampervanMotorhome => "Vehicle class checked: campervan/motorhome",
        VehicleClass::Hgv => {
            if profile.is_professional_driver_under_rest_rules {
                "Vehicle class checked: hgv (professional driver under rest rules)"
            } else {
                "Vehicle class checked: hgv (not marked professional under rest rules)"
            }
        }
        VehicleClass::Unknown => "Vehicle class checked: unknown",
    };
    let mut notes_v: Vec<String> = notes.iter().map(|s| (*s).to_string()).collect();
    notes_v.insert(0, class_note.into());
    notes_v.push(format!("Site kind: {kind:?}"));
    out.vehicle.cards.push(CampingCard {
        lat,
        lon,
        accepted: true,
        decline: None,
        reject_reason: None,
        tier: Tier::C,
        country_iso: country.into(),
        subdivision_iso: None,
        legal_basis: legal.into(),
        sources: sources.iter().map(|s| (*s).to_string()).collect(),
        fire_text: None,
        bare_rock_note: None,
        notes: notes_v,
        not_checked: NotCheckedLayers::default(),
        disclaimer: DISCLAIMER.into(),
        location_id: location_id_from_lat_lon(lat, lon),
        seed_road_highway: Some(format!("vehicle:{kind:?}")),
        walk_m: None,
    });
    out.vehicle.probes_accepted += 1;
    out.probe_log.push(crate::engine::ProbeLogEntry {
        lat,
        lon,
        accepted: true,
        reason: format!("vehicle_accepted_{kind:?}").to_ascii_lowercase(),
        road_highway: "vehicle".into(),
    });
}

fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6_371_000.0_f64;
    let p1 = lat1.to_radians();
    let p2 = lat2.to_radians();
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

/// Annotate a tent (right-to-roam) card for the "On foot from here" section.
pub fn annotate_on_foot_from_here(
    mut card: CampingCard,
    walk_m: f64,
    country_iso: &str,
) -> CampingCard {
    card.walk_m = Some(walk_m);
    card.notes.push(format!(
        "On foot from here — walking distance from the road ≈ {walk_m:.0} m."
    ));
    if country_iso.eq_ignore_ascii_case("no") {
        card.notes.push(
            "Vehicle must stay legally parked on the public road and must not be driven onto the track \
(motorferdselloven)."
                .into(),
        );
        if !card.sources.iter().any(|s| s.contains("1977-06-10-82")) {
            card.sources
                .push("https://lovdata.no/dokument/NL/lov/1977-06-10-82".into());
        }
        card.notes.push(
            "Legal basis for the parking/track note: motorferdselloven \
(https://lovdata.no/dokument/NL/lov/1977-06-10-82)."
                .into(),
        );
    } else {
        card.notes.push(
            "Vehicle must stay legally parked on the public road and must not be driven onto the track \
(general guidance, not law)."
                .into(),
        );
    }
    card
}

/// Whether German Tier B (and similar) packs that require non-motorised travel
/// should exclude on-foot tent suggestions when travel mode is motorised.
pub fn exclude_non_motorised_only_pack_in_motorised(
    pack_requires_non_motorised: bool,
    mode: TravelMode,
) -> bool {
    pack_requires_non_motorised && matches!(mode, TravelMode::Motorised)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{CampingHost, LocalDate, ProtectedAreaStatus};
    use crate::safety_view::OvernightSafety;
    use std::collections::HashMap;

    struct VH {
        country: String,
        profile: VehicleProfile,
        dest: Option<(f64, f64)>,
        nvdb: Option<NvdbRestKind>,
        sites: Vec<VehicleSiteHit>,
        fr_layers: bool,
        fr_clear: bool,
        kv: HashMap<String, String>,
        date: LocalDate,
    }

    impl CampingHost for VH {
        fn safety_config(&self) -> Option<OvernightSafety> {
            Some(OvernightSafety::default())
        }
        fn clock_local(&self) -> Option<LocalDate> {
            Some(self.date)
        }
        fn plugin_kv_available(&self) -> bool {
            true
        }
        fn kv_get(&self, key: &str) -> Option<String> {
            self.kv.get(key).cloned()
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
            None
        }
        fn travel_mode(&self) -> TravelMode {
            TravelMode::Motorised
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
        fn route_destination(&self) -> Option<(f64, f64)> {
            self.dest
        }
        fn nvdb_rest_kind(&self, _: f64, _: f64) -> Option<NvdbRestKind> {
            self.nvdb
        }
        fn vehicle_sites_near(&self, _: f64, _: f64, _: f64) -> Vec<VehicleSiteHit> {
            self.sites.clone()
        }
        fn france_vehicle_exclude_layers_ready(&self) -> bool {
            self.fr_layers
        }
        fn france_vehicle_exclude_clear(&self, _: f64, _: f64) -> bool {
            self.fr_clear
        }
    }

    fn date() -> LocalDate {
        LocalDate {
            year: 2026,
            month: 7,
            day: 1,
        }
    }

    #[test]
    fn campervan_never_gets_dognhvile_or_truck_bays() {
        let mut h = VH {
            country: "no".into(),
            profile: VehicleProfile {
                class: VehicleClass::CampervanMotorhome,
                is_professional_driver_under_rest_rules: false,
            },
            dest: None,
            nvdb: Some(NvdbRestKind::Dognhvileplass809),
            sites: vec![],
            fr_layers: false,
            fr_clear: false,
            kv: HashMap::new(),
            date: date(),
        };
        let out = suggest_vehicle_overnight(&mut h, &[(60.0, 10.0)], Some(5));
        assert!(out.vehicle.cards.is_empty(), "campervan must not get 809");

        h.country = "de".into();
        h.dest = Some((50.0, 8.0));
        h.sites = vec![VehicleSiteHit {
            lat: 51.0,
            lon: 9.0,
            kind: VehicleSiteKind::TruckBayHgvDesignated,
            name: None,
        }];
        let out = suggest_vehicle_overnight(&mut h, &[(51.0, 9.0)], Some(5));
        assert!(
            out.vehicle.cards.is_empty(),
            "campervan must not get truck bays"
        );
    }

    #[test]
    fn hgv_non_professional_never_gets_dognhvile() {
        let mut h = VH {
            country: "no".into(),
            profile: VehicleProfile {
                class: VehicleClass::Hgv,
                is_professional_driver_under_rest_rules: false,
            },
            dest: None,
            nvdb: Some(NvdbRestKind::Dognhvileplass809),
            sites: vec![],
            fr_layers: false,
            fr_clear: false,
            kv: HashMap::new(),
            date: date(),
        };
        let out = suggest_vehicle_overnight(&mut h, &[(60.0, 10.0)], Some(5));
        assert!(out.vehicle.cards.is_empty());
    }

    #[test]
    fn de_null_destination_no_roadside() {
        let mut h = VH {
            country: "de".into(),
            profile: VehicleProfile {
                class: VehicleClass::CampervanMotorhome,
                is_professional_driver_under_rest_rules: false,
            },
            dest: None,
            nvdb: None,
            sites: vec![VehicleSiteHit {
                lat: 51.0,
                lon: 9.0,
                kind: VehicleSiteKind::RoadsideParking,
                name: None,
            }],
            fr_layers: false,
            fr_clear: false,
            kv: HashMap::new(),
            date: date(),
        };
        let out = suggest_vehicle_overnight(&mut h, &[(51.0, 9.0)], Some(5));
        assert!(out
            .probe_log
            .iter()
            .any(|e| e.reason == "de_null_destination_no_roadside"));
        assert!(out.vehicle.cards.is_empty());
    }

    #[test]
    fn de_2nd_night_excluded() {
        let mut h = VH {
            country: "de".into(),
            profile: VehicleProfile {
                class: VehicleClass::Car,
                is_professional_driver_under_rest_rules: false,
            },
            dest: Some((48.0, 11.0)),
            nvdb: None,
            sites: vec![VehicleSiteHit {
                lat: 51.0,
                lon: 9.0,
                kind: VehicleSiteKind::RoadsideParking,
                name: None,
            }],
            fr_layers: false,
            fr_clear: false,
            kv: HashMap::new(),
            date: date(),
        };
        let loc = location_id_from_lat_lon(51.0, 9.0);
        NightStore::record_night(&mut h, "de_vehicle", &loc, date()).unwrap();
        // Second consecutive calendar night at the same spot.
        h.date = LocalDate {
            year: 2026,
            month: 7,
            day: 2,
        };
        let out = suggest_vehicle_overnight(&mut h, &[(51.0, 9.0)], Some(5));
        assert!(out
            .probe_log
            .iter()
            .any(|e| e.reason == "de_vehicle_2nd_night_exclude"));
        assert!(out.vehicle.cards.is_empty());
    }

    #[test]
    fn fr_missing_layers_no_roadside() {
        let mut h = VH {
            country: "fr".into(),
            profile: VehicleProfile {
                class: VehicleClass::CampervanMotorhome,
                is_professional_driver_under_rest_rules: false,
            },
            dest: None,
            nvdb: None,
            sites: vec![VehicleSiteHit {
                lat: 45.0,
                lon: 5.0,
                kind: VehicleSiteKind::RoadsideParking,
                name: None,
            }],
            fr_layers: false,
            fr_clear: false,
            kv: HashMap::new(),
            date: date(),
        };
        let out = suggest_vehicle_overnight(&mut h, &[(45.0, 5.0)], Some(5));
        assert!(out
            .probe_log
            .iter()
            .any(|e| e.reason == "fr_exclude_layers_unavailable"));
        assert!(out.vehicle.cards.is_empty());
    }

    #[test]
    fn on_foot_annotation_norway_cites_motorferdselloven_sweden_general() {
        let base = CampingCard {
            lat: 61.1,
            lon: 10.5,
            accepted: true,
            decline: None,
            reject_reason: None,
            tier: Tier::A,
            country_iso: "no".into(),
            subdivision_iso: None,
            legal_basis: "Friluftsloven".into(),
            sources: vec![],
            fire_text: None,
            bare_rock_note: None,
            notes: vec![],
            not_checked: NotCheckedLayers::both_unknown(),
            disclaimer: DISCLAIMER.into(),
            location_id: "x".into(),
            seed_road_highway: Some("track".into()),
            walk_m: Some(120.0),
        };
        let no = annotate_on_foot_from_here(base.clone(), 120.0, "no");
        assert!(no.notes.iter().any(|n| n.contains("motorferdselloven")));
        assert!(no.sources.iter().any(|s| s.contains("1977-06-10-82")));
        let se = annotate_on_foot_from_here(base, 80.0, "se");
        assert!(se
            .notes
            .iter()
            .any(|n| n.contains("general guidance, not law")));
        assert!(!se.notes.iter().any(|n| n.contains("motorferdselloven")));
    }

    #[test]
    fn tent_spots_never_marked_as_vehicle_site_kind() {
        // Vehicle cards always carry seed_road_highway vehicle:* ; tent cards do not.
        let mut h = VH {
            country: "no".into(),
            profile: VehicleProfile {
                class: VehicleClass::Hgv,
                is_professional_driver_under_rest_rules: true,
            },
            dest: None,
            nvdb: Some(NvdbRestKind::Dognhvileplass809),
            sites: vec![],
            fr_layers: false,
            fr_clear: false,
            kv: HashMap::new(),
            date: date(),
        };
        let out = suggest_vehicle_overnight(&mut h, &[(60.0, 10.0)], Some(5));
        assert!(!out.vehicle.cards.is_empty());
        for c in &out.vehicle.cards {
            assert!(c
                .seed_road_highway
                .as_deref()
                .is_some_and(|s| s.starts_with("vehicle:")));
        }
        let _ = ProtectedAreaStatus::Unknown;
    }

    #[test]
    fn us_flags_off_effective_d_usfs_no_mvum_designated_only_blm_never_hgv() {
        struct US {
            manager: Option<String>,
            tenure: crate::host::LandTenureStatus,
            flags: HashMap<String, bool>,
            mvum: bool,
            sites: Vec<VehicleSiteHit>,
            profile: VehicleProfile,
            kv: HashMap<String, String>,
        }
        impl CampingHost for US {
            fn safety_config(&self) -> Option<OvernightSafety> {
                Some(OvernightSafety::default())
            }
            fn clock_local(&self) -> Option<LocalDate> {
                Some(date())
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
                Some("us".into())
            }
            fn admin_subdivision_iso(&self, _: f64, _: f64) -> Option<String> {
                None
            }
            fn travel_mode(&self) -> TravelMode {
                TravelMode::Motorised
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
            fn land_tenure_status(&self, _: f64, _: f64) -> crate::host::LandTenureStatus {
                self.tenure
            }
            fn land_tenure_manager(&self, _: f64, _: f64) -> Option<String> {
                self.manager.clone()
            }
            fn camping_pack_flag_enabled(&self, flag_id: &str) -> bool {
                self.flags.get(flag_id).copied().unwrap_or(false)
            }
            fn usfs_mvum_layer_ready(&self) -> bool {
                self.mvum
            }
            fn vehicle_sites_near(&self, _: f64, _: f64, _: f64) -> Vec<VehicleSiteHit> {
                self.sites.clone()
            }
        }

        // Flags OFF + known tenure → effective D (no dispersed vehicle).
        let mut h = US {
            manager: Some("blm".into()),
            tenure: crate::host::LandTenureStatus::Known,
            flags: HashMap::new(),
            mvum: false,
            sites: vec![],
            profile: VehicleProfile {
                class: VehicleClass::CampervanMotorhome,
                is_professional_driver_under_rest_rules: false,
            },
            kv: HashMap::new(),
        };
        let out = suggest_vehicle_overnight(&mut h, &[(40.0, -110.0)], Some(5));
        assert!(out
            .probe_log
            .iter()
            .any(|e| { e.reason == "us_ca_vehicle_flag_off_or_tenure_unknown_effective_d" }));
        assert!(out.vehicle.cards.is_empty());

        // USFS without MVUM → designated only.
        h.manager = Some("usfs".into());
        h.flags.insert("land_mgr_usa_usfs".into(), true);
        h.mvum = false;
        h.sites = vec![VehicleSiteHit {
            lat: 40.0,
            lon: -110.0,
            kind: VehicleSiteKind::CaravanSite,
            name: Some("NF CG".into()),
        }];
        let out = suggest_vehicle_overnight(&mut h, &[(40.0, -110.0)], Some(5));
        assert!(out
            .probe_log
            .iter()
            .any(|e| e.reason == "usfs_without_mvum_designated_only"));
        assert_eq!(out.vehicle.cards.len(), 1);
        assert!(out.vehicle.cards[0]
            .legal_basis
            .contains("designated campground only"));

        // BLM never to HGV even with flag on.
        h.manager = Some("blm".into());
        h.flags.insert("land_mgr_usa_blm".into(), true);
        h.sites.clear();
        h.profile = VehicleProfile {
            class: VehicleClass::Hgv,
            is_professional_driver_under_rest_rules: true,
        };
        let out = suggest_vehicle_overnight(&mut h, &[(40.0, -110.0)], Some(5));
        assert!(out.probe_log.iter().any(|e| e.reason == "blm_never_to_hgv"));
        assert!(out.vehicle.cards.is_empty());
    }

    #[test]
    fn hgv_professional_gets_dognhvile() {
        let mut h = VH {
            country: "no".into(),
            profile: VehicleProfile {
                class: VehicleClass::Hgv,
                is_professional_driver_under_rest_rules: true,
            },
            dest: None,
            nvdb: Some(NvdbRestKind::Dognhvileplass809),
            sites: vec![],
            fr_layers: false,
            fr_clear: false,
            kv: HashMap::new(),
            date: date(),
        };
        let out = suggest_vehicle_overnight(&mut h, &[(60.0, 10.0)], Some(5));
        assert_eq!(out.vehicle.cards.len(), 1);
        assert!(out.vehicle.cards[0].notes.iter().any(|n| n.contains("hgv")));
    }
}
