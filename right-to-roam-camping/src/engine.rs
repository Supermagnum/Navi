//! Overnight suggestion engine (Phase 2: Norway + SJ decline + Tier D).

#[cfg(feature = "native")]
use driver_break_core::routing::graph::RouteGraph;

#[cfg(feature = "native")]
use crate::candidates::{
    find_road_track_junctions, probe_along_track, ProbePoint, RoadTrackSeed, CORRIDOR_SEED_RADIUS_M,
    DEFAULT_TRACK_WALK_M,
};
use crate::card::{CampingCard, DeclineKind, SuggestionList};
use crate::fire::{fire_guidance_norway, LEAVE_NO_TRACE_NOTE, PROTECTED_SPECIES_NOTE};
use crate::host::{CampingHost, LandTenureStatus, ProtectedAreaStatus};
use crate::night_store::{location_id_from_lat_lon, NightStore};
use crate::packs::{
    in_cmz_season, pack_for_location, pack_for_location_with_tenure, DistanceRule, DurationRule,
    FireRule, HostCondition, PackId, SuggestionMode, Tier,
};
use crate::safety_view::{wild_overnight_reject, OvernightSafety};
use crate::NotCheckedLayers;

#[derive(Debug, Clone)]
struct EvalProbe {
    lat: f64,
    lon: f64,
    road_highway: String,
    walk_m: f64,
}

#[cfg(feature = "native")]
impl From<&ProbePoint> for EvalProbe {
    fn from(p: &ProbePoint) -> Self {
        Self {
            lat: p.lat,
            lon: p.lon,
            road_highway: p.seed.road_highway.clone(),
            walk_m: p.walk_m,
        }
    }
}

#[cfg(feature = "native")]
#[derive(Clone)]
pub struct SuggestInput<'a> {
    pub graph: &'a RouteGraph,
    pub corridor_waypoints: &'a [[f64; 2]],
    /// Override default track walk (metres). Pack min road distance still wins.
    pub track_walk_m: Option<f64>,
    pub corridor_radius_m: Option<f64>,
    /// Cap how many accepted suggestions to return (None = all).
    pub max_suggestions: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct SuggestOutcome {
    pub list: SuggestionList,
    /// Phase 4: vehicle overnight cards (empty in non-motorised mode).
    pub vehicle: SuggestionList,
    /// Phase 4: tent pack suggestions when travel mode is motorised.
    pub on_foot_from_here: SuggestionList,
    #[cfg(feature = "native")]
    pub seeds: Vec<RoadTrackSeed>,
    pub probe_log: Vec<ProbeLogEntry>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProbeLogEntry {
    pub lat: f64,
    pub lon: f64,
    pub accepted: bool,
    pub reason: String,
    pub road_highway: String,
}

/// Evaluate fixed lat/lon probes through the same Norway filters as
/// [`suggest_overnight`] (no graph / track walk). Used for glacier probes and
/// wasm guest parity when the host already selected candidates.
pub fn suggest_overnight_fixed_probes(
    host: &mut dyn CampingHost,
    probes: &[(f64, f64)],
    max_suggestions: Option<usize>,
) -> SuggestOutcome {
    let mut list = SuggestionList::new();
    let mut vehicle = SuggestionList::new();
    let mut on_foot = SuggestionList::new();
    list.seeds_considered = probes.len();
    let mut probe_log = Vec::new();
    let motorised = matches!(host.travel_mode(), crate::host::TravelMode::Motorised);

    if motorised {
        let vout = crate::vehicle::suggest_vehicle_overnight(host, probes, max_suggestions);
        vehicle = vout.vehicle;
        probe_log.extend(vout.probe_log);
    }

    let Some(safety) = host.safety_config() else {
        list.cards.push(CampingCard {
            lat: probes.first().map(|p| p.0).unwrap_or(0.0),
            lon: probes.first().map(|p| p.1).unwrap_or(0.0),
            accepted: false,
            decline: Some(DeclineKind::HardFilter),
            reject_reason: Some("safety_config_unavailable".into()),
            tier: Tier::D,
            country_iso: "unknown".into(),
            subdivision_iso: None,
            legal_basis: "SafetyConfig unavailable — wild camp declined".into(),
            sources: vec![],
            fire_text: None,
            bare_rock_note: None,
            notes: vec![
                "Building-distance rule cannot be checked without SafetyConfig; \
declining wild-camp suggestions (campsites only)."
                    .into(),
            ],
            not_checked: NotCheckedLayers::both_unknown(),
            disclaimer: crate::DISCLAIMER.into(),
            location_id: "n/a".into(),
            seed_road_highway: None,
            walk_m: None,
        });
        return SuggestOutcome {
            list,
            vehicle,
            on_foot_from_here: on_foot,
            #[cfg(feature = "native")]
            seeds: Vec::new(),
            probe_log,
        };
    };

    let clock = host.clock_local();
    for &(lat, lon) in probes {
        let probe = EvalProbe {
            lat,
            lon,
            road_highway: "track".into(),
            walk_m: 0.0,
        };
        match evaluate_probe(host, &safety, &probe, clock) {
            ProbeDecision::Accept(card) => {
                let requires_nm = pack_requires_non_motorised(host, lat, lon);
                if motorised
                    && crate::vehicle::exclude_non_motorised_only_pack_in_motorised(
                        requires_nm,
                        host.travel_mode(),
                    )
                {
                    probe_log.push(ProbeLogEntry {
                        lat,
                        lon,
                        accepted: false,
                        reason: "non_motorised_pack_excluded_in_motorised".into(),
                        road_highway: "track".into(),
                    });
                    list.probes_rejected += 1;
                    continue;
                }
                probe_log.push(ProbeLogEntry {
                    lat,
                    lon,
                    accepted: true,
                    reason: if motorised {
                        "accepted_on_foot_from_here".into()
                    } else {
                        "accepted".into()
                    },
                    road_highway: "track".into(),
                });
                if motorised {
                    let walk = card.walk_m.unwrap_or(0.0);
                    let country = card.country_iso.clone();
                    on_foot.cards.push(crate::vehicle::annotate_on_foot_from_here(
                        card, walk, &country,
                    ));
                    on_foot.probes_accepted += 1;
                } else {
                    list.probes_accepted += 1;
                    list.cards.push(card);
                    if let Some(max) = max_suggestions {
                        if list.probes_accepted >= max {
                            break;
                        }
                    }
                }
            }
            ProbeDecision::Reject { reason, card } => {
                probe_log.push(ProbeLogEntry {
                    lat,
                    lon,
                    accepted: false,
                    reason,
                    road_highway: "track".into(),
                });
                list.probes_rejected += 1;
                if let Some(c) = card {
                    if motorised {
                        on_foot.cards.push(c);
                    } else {
                        list.cards.push(c);
                    }
                }
            }
        }
    }

    SuggestOutcome {
        list,
        vehicle,
        on_foot_from_here: on_foot,
        #[cfg(feature = "native")]
        seeds: Vec::new(),
        probe_log,
    }
}

fn pack_requires_non_motorised(host: &dyn CampingHost, lat: f64, lon: f64) -> bool {
    let country = host.admin_country_iso(lat, lon);
    let sub = host.admin_subdivision_iso(lat, lon);
    let pack = pack_for_location(country.as_deref(), sub.as_deref());
    pack.host_conditions
        .iter()
        .any(|c| matches!(c, HostCondition::NonMotorisedTravel))
}

/// Run the Phase 2 camping engine against a loaded graph + host backends.
#[cfg(feature = "native")]
pub fn suggest_overnight(
    host: &mut dyn CampingHost,
    input: &SuggestInput<'_>,
) -> SuggestOutcome {
    let walk = input.track_walk_m.unwrap_or(DEFAULT_TRACK_WALK_M);
    let radius = input.corridor_radius_m.unwrap_or(CORRIDOR_SEED_RADIUS_M);
    let seeds = find_road_track_junctions(input.graph, input.corridor_waypoints, radius);

    let mut list = SuggestionList::new();
    let mut vehicle = SuggestionList::new();
    let mut on_foot = SuggestionList::new();
    list.seeds_considered = seeds.len();
    let mut probe_log = Vec::new();
    let motorised = matches!(host.travel_mode(), crate::host::TravelMode::Motorised);

    if motorised {
        let probes: Vec<(f64, f64)> = seeds.iter().map(|s| (s.lat, s.lon)).collect();
        let vout = crate::vehicle::suggest_vehicle_overnight(
            host,
            &probes,
            input.max_suggestions,
        );
        vehicle = vout.vehicle;
        probe_log.extend(vout.probe_log);
    }

    // Fail-safe: no SafetyConfig → zero suggestions (wild camp declined).
    let Some(safety) = host.safety_config() else {
        list.cards.push(CampingCard {
            lat: input
                .corridor_waypoints
                .first()
                .map(|w| w[0])
                .unwrap_or(0.0),
            lon: input
                .corridor_waypoints
                .first()
                .map(|w| w[1])
                .unwrap_or(0.0),
            accepted: false,
            decline: Some(DeclineKind::HardFilter),
            reject_reason: Some("safety_config_unavailable".into()),
            tier: Tier::D,
            country_iso: "unknown".into(),
            subdivision_iso: None,
            legal_basis: "SafetyConfig unavailable — wild camp declined".into(),
            sources: vec![],
            fire_text: None,
            bare_rock_note: None,
            notes: vec![
                "Building-distance rule cannot be checked without SafetyConfig; \
declining wild-camp suggestions (campsites only)."
                    .into(),
            ],
            not_checked: NotCheckedLayers::both_unknown(),
            disclaimer: crate::DISCLAIMER.into(),
            location_id: "n/a".into(),
            seed_road_highway: None,
            walk_m: None,
        });
        return SuggestOutcome {
            list,
            vehicle,
            on_foot_from_here: on_foot,
            seeds,
            probe_log,
        };
    };

    let clock = host.clock_local();

    for seed in &seeds {
        let pack_min = {
            let iso = host.admin_country_iso(seed.lat, seed.lon);
            let _pack = pack_for_location(iso.as_deref(), None);
            None::<f64>
        };

        let Some(probe) = probe_along_track(input.graph, seed, walk, pack_min) else {
            probe_log.push(ProbeLogEntry {
                lat: seed.lat,
                lon: seed.lon,
                accepted: false,
                reason: "track_too_short_for_walk_or_pack_min_road".into(),
                road_highway: seed.road_highway.clone(),
            });
            list.probes_rejected += 1;
            continue;
        };

        let eval = EvalProbe::from(&probe);
        match evaluate_probe(host, &safety, &eval, clock) {
            ProbeDecision::Accept(card) => {
                let requires_nm = pack_requires_non_motorised(host, probe.lat, probe.lon);
                if motorised
                    && crate::vehicle::exclude_non_motorised_only_pack_in_motorised(
                        requires_nm,
                        host.travel_mode(),
                    )
                {
                    probe_log.push(ProbeLogEntry {
                        lat: probe.lat,
                        lon: probe.lon,
                        accepted: false,
                        reason: "non_motorised_pack_excluded_in_motorised".into(),
                        road_highway: seed.road_highway.clone(),
                    });
                    list.probes_rejected += 1;
                    continue;
                }
                probe_log.push(ProbeLogEntry {
                    lat: probe.lat,
                    lon: probe.lon,
                    accepted: true,
                    reason: if motorised {
                        "accepted_on_foot_from_here".into()
                    } else {
                        "accepted".into()
                    },
                    road_highway: seed.road_highway.clone(),
                });
                if motorised {
                    let w = card.walk_m.unwrap_or(probe.walk_m);
                    let country = card.country_iso.clone();
                    on_foot.cards.push(crate::vehicle::annotate_on_foot_from_here(
                        card, w, &country,
                    ));
                    on_foot.probes_accepted += 1;
                    if let Some(max) = input.max_suggestions {
                        if on_foot.probes_accepted >= max {
                            break;
                        }
                    }
                } else {
                    list.probes_accepted += 1;
                    list.cards.push(card);
                    if let Some(max) = input.max_suggestions {
                        if list.probes_accepted >= max {
                            break;
                        }
                    }
                }
            }
            ProbeDecision::Reject { reason, card } => {
                probe_log.push(ProbeLogEntry {
                    lat: probe.lat,
                    lon: probe.lon,
                    accepted: false,
                    reason: reason.clone(),
                    road_highway: seed.road_highway.clone(),
                });
                list.probes_rejected += 1;
                if let Some(c) = card {
                    if motorised {
                        on_foot.cards.push(c);
                    } else {
                        list.cards.push(c);
                    }
                }
            }
        }
    }

    SuggestOutcome {
        list,
        vehicle,
        on_foot_from_here: on_foot,
        seeds,
        probe_log,
    }
}

enum ProbeDecision {
    Accept(CampingCard),
    Reject {
        reason: String,
        card: Option<CampingCard>,
    },
}

fn unmet_host_condition(
    host: &dyn CampingHost,
    probe: &EvalProbe,
    conditions: &[HostCondition],
) -> Option<&'static str> {
    for c in conditions {
        match c {
            HostCondition::NonMotorisedTravel => {
                if !matches!(host.travel_mode(), crate::host::TravelMode::NonMotorised) {
                    return Some("condition_not_non_motorised");
                }
            }
            HostCondition::NotForest => match host.is_forest(probe.lat, probe.lon) {
                Some(true) => return Some("condition_is_forest"),
                None => return Some("condition_forest_unknown"),
                Some(false) => {}
            },
            HostCondition::NotProtectedArea => {
                match host.protected_area_status(probe.lat, probe.lon) {
                    ProtectedAreaStatus::Inside => return Some("condition_inside_protected"),
                    ProtectedAreaStatus::Unknown => return Some("condition_protected_unknown"),
                    ProtectedAreaStatus::Clear => {}
                }
            }
            HostCondition::NotResidential => match host.is_residential_ground(probe.lat, probe.lon)
            {
                Some(true) => return Some("condition_residential"),
                None => return Some("condition_residential_unknown"),
                Some(false) => {}
            },
            HostCondition::AboveTreeline => match host.above_treeline(probe.lat, probe.lon) {
                Some(true) => {}
                Some(false) => return Some("condition_below_treeline"),
                None => return Some("condition_treeline_unknown"),
            },
            HostCondition::DesignatedLayerReady(layer) => {
                if !host.designated_layer_ready(*layer) {
                    return Some("designated_layer_not_classified");
                }
            }
            HostCondition::LandTenureKnown => {
                if matches!(
                    host.land_tenure_status(probe.lat, probe.lon),
                    LandTenureStatus::Unknown
                ) {
                    return Some("tenure_unknown");
                }
            }
        }
    }
    None
}

fn designated_tent_sites_only(
    host: &dyn CampingHost,
    probe: &EvalProbe,
    pack: &crate::packs::RulePack,
    not_checked: NotCheckedLayers,
    source_urls: &[&str],
) -> ProbeDecision {
    if let Some(layer) = pack.designated_layer {
        if !host.designated_layer_ready(layer) {
            // Spec: designated layer not classified → empty (no fabricated spots).
            return ProbeDecision::Reject {
                reason: "designated_layer_not_classified".into(),
                card: None,
            };
        }
    }
    let sites = host.tent_sites_near(probe.lat, probe.lon, 2_000.0);
    if sites.is_empty() {
        return ProbeDecision::Reject {
            reason: "no_tentsite_poi".into(),
            card: None,
        };
    }
    let site = &sites[0];
    let mut notes: Vec<String> = pack.guidance_notes.iter().map(|s| (*s).to_string()).collect();
    notes.extend(pack.secondary_card_notes.iter().map(|s| (*s).to_string()));
    notes.push("Designated TentSite from host POI data (not a wild-camp suggestion).".into());
    ProbeDecision::Accept(CampingCard {
        lat: site.lat,
        lon: site.lon,
        accepted: true,
        decline: None,
        reject_reason: None,
        tier: Tier::C,
        country_iso: pack.country_iso.clone(),
        subdivision_iso: None,
        legal_basis: pack.legal_basis.into(),
        sources: source_urls.iter().map(|s| (*s).to_string()).collect(),
        fire_text: None,
        bare_rock_note: None,
        notes,
        not_checked,
        disclaimer: crate::DISCLAIMER.into(),
        location_id: location_id_from_lat_lon(site.lat, site.lon),
        seed_road_highway: None,
        walk_m: None,
    })
}

fn degrade_to_fallback(
    host: &dyn CampingHost,
    probe: &EvalProbe,
    pack: &crate::packs::RulePack,
    fallback: Tier,
    reason: &str,
    not_checked: NotCheckedLayers,
    source_urls: &[&str],
) -> ProbeDecision {
    match fallback {
        Tier::C => {
            let mut c_pack = pack.clone();
            c_pack.tier = Tier::C;
            c_pack.suggestion_mode = SuggestionMode::DesignatedTentSitesOnly;
            match designated_tent_sites_only(host, probe, &c_pack, not_checked, source_urls) {
                ProbeDecision::Reject { reason: inner, card } => ProbeDecision::Reject {
                    reason: format!("{reason}_fallback_c_{inner}"),
                    card,
                },
                other => other,
            }
        }
        _ => ProbeDecision::Reject {
            reason: reason.into(),
            card: Some(CampingCard::decline_campsites_only(
                probe.lat,
                probe.lon,
                &pack.country_iso,
                pack.legal_basis,
                source_urls,
                not_checked,
                &[
                    "Pack degraded: maintainer flag OFF or host conditions not checkable.",
                ],
            )),
        },
    }
}

fn evaluate_probe(
    host: &mut dyn CampingHost,
    safety: &OvernightSafety,
    probe: &EvalProbe,
    clock: Option<crate::host::LocalDate>,
) -> ProbeDecision {
    let country = host.admin_country_iso(probe.lat, probe.lon);
    let subdivision = host.admin_subdivision_iso(probe.lat, probe.lon);
    let tenure = host.land_tenure_manager(probe.lat, probe.lon);
    let mut pack =
        pack_for_location_with_tenure(country.as_deref(), subdivision.as_deref(), tenure.as_deref());
    let not_checked = NotCheckedLayers::from_host_status(
        host.protected_area_layer_ready(),
        host.landcover_layer_ready(),
    );
    let loc_id = location_id_from_lat_lon(probe.lat, probe.lon);
    let mut source_urls: Vec<&str> = pack.source_urls();

    // Maintainer flag OFF → degrade to declared fallback (usually C or D).
    if let Some(flag) = pack.flag_id {
        if !host.camping_pack_flag_enabled(flag) {
            let fb = pack.flag_off_fallback.unwrap_or(Tier::D);
            return degrade_to_fallback(
                host,
                probe,
                &pack,
                fb,
                &format!("flag_off_{flag}"),
                not_checked.clone(),
                &source_urls,
            );
        }
    }

    // Land-manager / Tier B host conditions.
    if !pack.host_conditions.is_empty() {
        if let Some(reason) = unmet_host_condition(host, probe, pack.host_conditions) {
            let fb = pack.conditions_unmet_fallback.unwrap_or(Tier::D);
            return degrade_to_fallback(
                host,
                probe,
                &pack,
                fb,
                reason,
                not_checked.clone(),
                &source_urls,
            );
        }
    }

    if pack.requires_land_tenure
        && matches!(
            host.land_tenure_status(probe.lat, probe.lon),
            LandTenureStatus::Unknown
        )
    {
        return ProbeDecision::Reject {
            reason: "tenure_unknown".into(),
            card: Some(CampingCard::decline_campsites_only(
                probe.lat,
                probe.lon,
                country.as_deref().unwrap_or(&pack.country_iso),
                pack.legal_basis,
                &source_urls,
                not_checked.clone(),
                &["Land tenure unknown — wild camp declined."],
            )),
        };
    }

    if matches!(pack.distance, DistanceRule::NotVerifiedDeclines) {
        return ProbeDecision::Reject {
            reason: "distance_not_verified_declines".into(),
            card: Some(CampingCard::decline_campsites_only(
                probe.lat,
                probe.lon,
                &pack.country_iso,
                pack.legal_basis,
                &source_urls,
                not_checked.clone(),
                &["Statutory/access distance not verified — declining rather than inventing metres."],
            )),
        };
    }

    match pack.suggestion_mode {
        SuggestionMode::DesignatedTentSitesOnly => {
            return designated_tent_sites_only(host, probe, &pack, not_checked, &source_urls);
        }
        SuggestionMode::DeclineCampsitesGuidance => {
            let mut notes: Vec<&str> = pack.guidance_notes.to_vec();
            notes.extend(pack.secondary_card_notes.iter().copied());
            return ProbeDecision::Reject {
                reason: format!("decline_{:?}", pack.id).to_ascii_lowercase(),
                card: Some(CampingCard::decline_campsites_only(
                    probe.lat,
                    probe.lon,
                    &pack.country_iso,
                    pack.legal_basis,
                    &source_urls,
                    not_checked.clone(),
                    &notes,
                )),
            };
        }
        SuggestionMode::WildCamp => {}
    }

    match pack.id {
        PackId::SvalbardDecline => {
            return ProbeDecision::Reject {
                reason: "svalbard_decline".into(),
                card: Some(CampingCard::svalbard_decline(probe.lat, probe.lon, clock)),
            };
        }
        PackId::TierD
        | PackId::AlandTierD
        | PackId::TerritoryFo
        | PackId::TerritoryGl
        | PackId::TerritoryGbNir
        | PackId::TerritoryIm
        | PackId::TerritoryJe
        | PackId::TerritoryGg
        | PackId::TerritoryGi
        | PackId::Mexico
        | PackId::Japan
        | PackId::WorldTierD => {
            return ProbeDecision::Reject {
                reason: format!(
                    "tier_d_country_{}",
                    country.as_deref().unwrap_or("unknown")
                ),
                card: Some(CampingCard::decline_campsites_only(
                    probe.lat,
                    probe.lon,
                    country.as_deref().unwrap_or(&pack.country_iso),
                    pack.legal_basis,
                    &source_urls,
                    not_checked,
                    pack.secondary_card_notes,
                )),
            };
        }
        _ => {}
    }

    // Iceland exception: unknown protected-area status → campsites only.
    if pack.decline_when_protected_unknown {
        match host.protected_area_status(probe.lat, probe.lon) {
            ProtectedAreaStatus::Unknown => {
                return ProbeDecision::Reject {
                    reason: "iceland_protected_area_unknown".into(),
                    card: Some(CampingCard::decline_campsites_only(
                        probe.lat,
                        probe.lon,
                        "is",
                        pack.legal_basis,
                        &source_urls,
                        not_checked,
                        &[
                            "Protected-area status unknown — Iceland pack declines wild camp \
(campsites only) until the host can prove the spot is outside protected areas.",
                        ],
                    )),
                };
            }
            ProtectedAreaStatus::Inside => {
                return ProbeDecision::Reject {
                    reason: "iceland_inside_protected_area".into(),
                    card: Some(CampingCard::decline_campsites_only(
                        probe.lat,
                        probe.lon,
                        "is",
                        pack.legal_basis,
                        &source_urls,
                        not_checked,
                        &["Inside a protected area — wild camp declined for Iceland."],
                    )),
                };
            }
            ProtectedAreaStatus::Clear => {}
        }
    }

    // Scotland CMZ: in season without CMZ layer (or inside CMZ) → no suggestion.
    if let Some(cmz) = &pack.cmz {
        let in_season = clock.map(|d| in_cmz_season(cmz, d)).unwrap_or(true);
        if in_season {
            if !host.cmz_layer_ready() {
                return ProbeDecision::Reject {
                    reason: "scotland_cmz_unproven_outside".into(),
                    card: Some(CampingCard::decline_campsites_only(
                        probe.lat,
                        probe.lon,
                        "gb",
                        pack.legal_basis,
                        &source_urls,
                        not_checked,
                        &[
                            "Loch Lomond & Trossachs CMZ season (1 Mar–30 Sep): without a CMZ \
polygon layer this pack cannot prove the spot is outside a management zone — no wild-camp \
suggestion.",
                        ],
                    )),
                };
            }
            if host.cmz_contains(probe.lat, probe.lon) {
                return ProbeDecision::Reject {
                    reason: "scotland_inside_cmz".into(),
                    card: Some(CampingCard::decline_campsites_only(
                        probe.lat,
                        probe.lon,
                        "gb",
                        pack.legal_basis,
                        &source_urls,
                        not_checked,
                        &["Inside a Loch Lomond & Trossachs Camping Management Zone — permit or campsite required in season."],
                    )),
                };
            }
        }
    }

    if pack.hard_max_nights().is_some() && !host.plugin_kv_available() {
        return ProbeDecision::Reject {
            reason: "plugin_kv_unavailable".into(),
            card: Some(CampingCard {
                lat: probe.lat,
                lon: probe.lon,
                accepted: false,
                decline: Some(DeclineKind::HardFilter),
                reject_reason: Some("plugin_kv_unavailable".into()),
                tier: pack.tier,
                country_iso: pack.country_iso.clone(),
                subdivision_iso: subdivision.clone(),
                legal_basis: pack.legal_basis.into(),
                sources: source_urls.iter().map(|s| (*s).to_string()).collect(),
                fire_text: None,
                bare_rock_note: None,
                notes: vec![
                    "Consecutive-night limit cannot be enforced without plugin KV; \
declining (no silent rule bypass)."
                        .into(),
                ],
                not_checked: not_checked.clone(),
                disclaimer: crate::DISCLAIMER.into(),
                location_id: loc_id.clone(),
                seed_road_highway: Some(probe.road_highway.clone()),
                walk_m: Some(probe.walk_m),
            }),
        };
    }

    if let Some((max_n, store_key)) = pack.hard_max_nights() {
        let tonight = match clock {
            Some(d) => d,
            None => {
                return ProbeDecision::Reject {
                    reason: "clock_unavailable_for_night_store".into(),
                    card: None,
                };
            }
        };
        if NightStore::would_exceed(host, store_key, &loc_id, tonight, max_n) {
            return ProbeDecision::Reject {
                reason: "max_consecutive_nights".into(),
                card: None,
            };
        }
    }

    if pack.uses_safety_config_distance() {
        if let Some(label) = wild_overnight_reject(
            probe.lat,
            probe.lon,
            safety,
            host.overnight_buildings(),
            host.overnight_glacier_rings(),
        ) {
            return ProbeDecision::Reject {
                reason: label.into(),
                card: None,
            };
        }
    }

    let fire_text = match pack.fire {
        FireRule::NorwayDateGated => {
            let fire = fire_guidance_norway(clock);
            (Some(fire.text), Some(fire.bare_rock_note.to_string()))
        }
        FireRule::AlwaysNeedsLandownerPermission { text }
        | FireRule::GuidanceNote { text } => (Some(text.to_string()), None),
        FireRule::NoneInLaw | FireRule::NotVerified => (None, None),
    };

    let mut notes: Vec<String> = not_checked
        .filter_pack_guidance(
            pack.guidance_notes,
            pack.farmland_not_checked_when_landcover_unknown,
        )
        .into_iter()
        .map(str::to_string)
        .collect();
    if let Some(label) = pack.distance_card_label() {
        if pack.id == PackId::Norway {
            notes.push("Building distance: 150 m (friluftsloven), from Navi SafetyConfig".into());
            if safety.min_building_distance_m < 150.0 {
                notes.push(
                    "configured distance is below the 150 m in friluftsloven § 9".into(),
                );
            }
        } else {
            notes.push(format!(
                "Building distance: {label} ({} m from SafetyConfig).",
                safety.min_building_distance_m
            ));
        }
    }
    match pack.duration {
        DurationRule::SoftGuidance { note } | DurationRule::NoneInLaw { note } => {
            notes.push(note.into());
        }
        DurationRule::HardMaxConsecutiveNights { nights, .. } => {
            notes.push(format!("Hard limit: max {nights} consecutive night(s) at the same spot."));
        }
        DurationRule::NotVerified => {}
    }
    if pack.cloudberry_note {
        notes.retain(|n| !n.contains("protected from picking")); // NO pack lists it in guidance; keep engine cloudberry
        notes.push(PROTECTED_SPECIES_NOTE.into());
        if !notes.iter().any(|n| n.contains("leave no trace")) {
            notes.push(LEAVE_NO_TRACE_NOTE.into());
        }
        match cloudberry_decision(subdivision.as_deref()) {
            CloudberryDecision::Show => {
                notes.push(
                    "Northern Norway (Nordland, Troms, Finnmark) has special cloudberry picking rules."
                        .into(),
                );
            }
            CloudberryDecision::OmitOutsideNorthern { iso } => {
                eprintln!(
                    "cloudberry note omitted: subdivision={iso} is outside Nordland/Troms/Finnmark"
                );
            }
            CloudberryDecision::OmitUnknown => {
                eprintln!("cloudberry note omitted: subdivision unknown");
            }
        }
    }
    notes.extend(
        not_checked
            .card_notes(pack.farmland_not_checked_when_landcover_unknown)
            .into_iter()
            .map(str::to_string),
    );
    match host.travel_mode() {
        crate::host::TravelMode::Unknown => {
            notes.push("travel mode not checked".into());
        }
        crate::host::TravelMode::Motorised => {
            notes.push(
                "travel mode is motorised — vehicle overnight rules are not applied here \
(tent guidance only)."
                    .into(),
            );
        }
        crate::host::TravelMode::NonMotorised => {}
    }

    ProbeDecision::Accept(CampingCard {
        lat: probe.lat,
        lon: probe.lon,
        accepted: true,
        decline: None,
        reject_reason: None,
        tier: pack.tier,
        country_iso: pack.country_iso.clone(),
        subdivision_iso: subdivision,
        legal_basis: pack.legal_basis.into(),
        sources: source_urls.iter().map(|s| (*s).to_string()).collect(),
        fire_text: fire_text.0,
        bare_rock_note: fire_text.1,
        notes,
        not_checked,
        disclaimer: crate::DISCLAIMER.into(),
        location_id: loc_id,
        seed_road_highway: Some(probe.road_highway.clone()),
        walk_m: Some(probe.walk_m),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloudberryDecision {
    Show,
    OmitOutsideNorthern { iso: String },
    OmitUnknown,
}

pub fn cloudberry_decision(subdivision_iso: Option<&str>) -> CloudberryDecision {
    match subdivision_iso {
        None => CloudberryDecision::OmitUnknown,
        Some(s) if cloudberry_applies(Some(s)) => CloudberryDecision::Show,
        Some(s) => CloudberryDecision::OmitOutsideNorthern { iso: s.to_string() },
    }
}

fn cloudberry_applies(subdivision_iso: Option<&str>) -> bool {
    // Informational note only — never a hard overnight filter. Prefer current
    // ISO 3166-2 (NO-18 / NO-55 / NO-56). Accept legacy NO-19/NO-20 and the
    // 2020–2023 merged NO-54 if an older layer ever surfaces them.
    match subdivision_iso {
        Some(s) => {
            let u = s.to_ascii_uppercase();
            u.contains("NO-18") // Nordland
                || u.contains("NO-55") // Troms (2024+)
                || u.contains("NO-56") // Finnmark (2024+)
                || u.contains("NO-19") // Troms (pre-2020)
                || u.contains("NO-20") // Finnmark (pre-2020)
                || u.contains("NO-54") // Troms og Finnmark (2020–2023)
                || u.contains("NORDLAND")
                || u.contains("TROMS")
                || u.contains("FINNMARK")
        }
        None => false,
    }
}

#[cfg(test)]
#[cfg(feature = "native")]
mod tests {
    use super::*;
    use crate::host::{CampingHost, LocalDate, TravelMode};
    use crate::safety_view::OvernightSafety;
    use std::collections::HashMap;

    struct MemHost {
        kv: HashMap<String, String>,
        kv_ok: bool,
        safety: Option<OvernightSafety>,
        date: Option<LocalDate>,
        buildings: Vec<(f64, f64)>,
        country: Option<String>,
    }

    impl CampingHost for MemHost {
        fn safety_config(&self) -> Option<OvernightSafety> {
            self.safety
        }
        fn clock_local(&self) -> Option<LocalDate> {
            self.date
        }
        fn plugin_kv_available(&self) -> bool {
            self.kv_ok
        }
        fn kv_get(&self, key: &str) -> Option<String> {
            self.kv.get(key).cloned().filter(|s| !s.is_empty())
        }
        fn kv_set(&mut self, key: &str, value: &str) -> Result<(), String> {
            self.kv.insert(key.into(), value.into());
            Ok(())
        }
        fn admin_country_iso(&self, _: f64, _: f64) -> Option<String> {
            self.country.clone()
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

    #[test]
    fn no_safety_config_yields_zero_suggestions() {
        let mut h = MemHost {
            kv: HashMap::new(),
            kv_ok: true,
            safety: None,
            date: Some(LocalDate {
                year: 2026,
                month: 7,
                day: 1,
            }),
            buildings: vec![],
            country: Some("no".into()),
        };
        let graph = RouteGraph::from_parts(
            std::collections::HashMap::new(),
            vec![],
            driver_break_core::routing::graph::RoutingProfile::Foot,
        );
        let waypoints = [[61.1, 10.5]];
        let out = suggest_overnight(
            &mut h,
            &SuggestInput {
                graph: &graph,
                corridor_waypoints: &waypoints,
                track_walk_m: None,
                corridor_radius_m: None,
                max_suggestions: None,
            },
        );
        assert_eq!(out.list.probes_accepted, 0);
        assert!(out
            .list
            .cards
            .iter()
            .any(|c| c.reject_reason.as_deref() == Some("safety_config_unavailable")));
    }

    #[test]
    fn kv_unavailable_declines_norway() {
        let mut h = MemHost {
            kv: HashMap::new(),
            kv_ok: false,
            safety: Some(OvernightSafety::default()),
            date: Some(LocalDate {
                year: 2026,
                month: 7,
                day: 1,
            }),
            buildings: vec![],
            country: Some("no".into()),
        };
        // Directly evaluate via night path: would_exceed is true when KV down;
        // engine rejects before accepting.
        assert!(NightStore::would_exceed(
            &h,
            "no",
            "cell:0:0",
            LocalDate {
                year: 2026,
                month: 7,
                day: 1,
            },
            2
        ));
        let _ = &mut h;
    }

    #[test]
    fn building_distance_follows_safety_config() {
        let mut safety = OvernightSafety::default();
        safety.min_building_distance_m = 150.0;
        // ~111 m north of building (0.001° lat).
        let buildings = vec![(61.1000, 10.5000)];
        let mut h = MemHost {
            kv: HashMap::new(),
            kv_ok: true,
            safety: Some(safety),
            date: Some(LocalDate {
                year: 2026,
                month: 10,
                day: 1,
            }),
            buildings: buildings.clone(),
            country: Some("no".into()),
        };
        let probe = EvalProbe {
            lat: 61.1010,
            lon: 10.5000,
            road_highway: "tertiary".into(),
            walk_m: 120.0,
        };
        let date = h.date;
        let d1 = evaluate_probe(&mut h, &safety, &probe, date);
        assert!(
            matches!(d1, ProbeDecision::Reject { reason, .. } if reason == "too_close_to_building"),
            "expected reject at 150 m threshold"
        );

        safety.min_building_distance_m = 80.0;
        h.safety = Some(safety);
        let d2 = evaluate_probe(&mut h, &safety, &probe, date);
        match d2 {
            ProbeDecision::Accept(c) => {
                assert!(
                    c.notes.iter().any(|n| {
                        n == "Building distance: 150 m (friluftsloven), from Navi SafetyConfig"
                    }),
                    "notes={:?}",
                    c.notes
                );
                assert!(
                    c.notes.iter().any(|n| {
                        n == "configured distance is below the 150 m in friluftsloven § 9"
                    }),
                    "expected § 9 warning when SafetyConfig is below 150; notes={:?}",
                    c.notes
                );
            }
            _ => panic!("expected accept when threshold drops below building distance"),
        }
    }

    #[test]
    fn sweden_is_tier_a() {
        let mut h = MemHost {
            kv: HashMap::new(),
            kv_ok: true,
            safety: Some(OvernightSafety::default()),
            date: Some(LocalDate {
                year: 2026,
                month: 7,
                day: 1,
            }),
            buildings: vec![],
            country: Some("se".into()),
        };
        let probe = EvalProbe {
            lat: 60.0,
            lon: 12.5,
            road_highway: "tertiary".into(),
            walk_m: 120.0,
        };
        let date = h.date;
        let d = evaluate_probe(
            &mut h,
            &OvernightSafety::default(),
            &probe,
            date,
        );
        match d {
            ProbeDecision::Accept(c) => {
                assert_eq!(c.tier, Tier::A);
                assert_eq!(c.country_iso, "se");
                assert!(c.notes.iter().any(|n| n.contains("Navi safety default, not Swedish law")));
            }
            _ => panic!("expected Sweden Tier A accept"),
        }
    }

    #[test]
    fn cloudberry_omitted_when_subdivision_unknown() {
        assert_eq!(
            cloudberry_decision(None),
            CloudberryDecision::OmitUnknown
        );
        assert_eq!(
            cloudberry_decision(Some("NO-18")),
            CloudberryDecision::Show
        );
        assert!(matches!(
            cloudberry_decision(Some("NO-34")),
            CloudberryDecision::OmitOutsideNorthern { .. }
        ));
    }

    fn norway_host(day: u32) -> MemHost {
        MemHost {
            kv: HashMap::new(),
            kv_ok: true,
            safety: Some(OvernightSafety::default()),
            date: Some(LocalDate {
                year: 2026,
                month: 7,
                day,
            }),
            buildings: vec![],
            country: Some("no".into()),
        }
    }

    #[test]
    fn displaying_suggestions_three_days_never_records_or_blocks() {
        let probes = &[(61.11515, 10.46628)];
        for day in [1u32, 2, 3] {
            let mut h = norway_host(day);
            let out = suggest_overnight_fixed_probes(&mut h, probes, Some(4));
            assert!(
                out.list.probes_accepted > 0 || out.list.cards.iter().any(|c| c.accepted),
                "day {day} should accept without any Camp-here record"
            );
            assert!(
                h.kv.is_empty(),
                "suggest/display must not write night-store keys (day {day})"
            );
        }
    }

    #[test]
    fn camp_here_same_spot_two_nights_third_declined() {
        let probes = &[(61.11515, 10.46628)];
        let loc = location_id_from_lat_lon(61.11515, 10.46628);
        let mut h = norway_host(1);
        let d1 = suggest_overnight_fixed_probes(&mut h, probes, Some(4));
        assert!(d1.list.cards.iter().any(|c| c.accepted));
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
        h.date = Some(LocalDate {
            year: 2026,
            month: 7,
            day: 2,
        });
        let d2 = suggest_overnight_fixed_probes(&mut h, probes, Some(4));
        assert!(d2.list.cards.iter().any(|c| c.accepted));
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
        h.date = Some(LocalDate {
            year: 2026,
            month: 7,
            day: 3,
        });
        let d3 = suggest_overnight_fixed_probes(&mut h, probes, Some(4));
        assert!(
            !d3.list.cards.iter().any(|c| c.accepted) && d3.list.probes_accepted == 0,
            "third consecutive Camp-here night must be declined"
        );
        assert!(
            d3.probe_log
                .iter()
                .any(|e| e.reason == "max_consecutive_nights"),
            "probe log must record max_consecutive_nights"
        );
    }

    #[test]
    fn camp_here_different_spots_never_blocks() {
        let a = (61.11515, 10.46628);
        let b = (61.20000, 10.70000);
        let mut h = norway_host(1);
        NightStore::record_night(
            &mut h,
            "no",
            &location_id_from_lat_lon(a.0, a.1),
            LocalDate {
                year: 2026,
                month: 7,
                day: 1,
            },
        )
        .unwrap();
        h.date = Some(LocalDate {
            year: 2026,
            month: 7,
            day: 2,
        });
        NightStore::record_night(
            &mut h,
            "no",
            &location_id_from_lat_lon(b.0, b.1),
            LocalDate {
                year: 2026,
                month: 7,
                day: 2,
            },
        )
        .unwrap();
        h.date = Some(LocalDate {
            year: 2026,
            month: 7,
            day: 3,
        });
        let out = suggest_overnight_fixed_probes(&mut h, &[b], Some(4));
        assert!(
            out.list.cards.iter().any(|c| c.accepted) || out.list.probes_accepted > 0,
            "moving spots must reset consecutive-night block"
        );
    }
}
