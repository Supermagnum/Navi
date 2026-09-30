//! Overnight suggestion engine (Phase 2: Norway + SJ decline + Tier D).

use std::collections::HashMap;

use driver_break_core::poi::PoiRecord;
use driver_break_core::routing::graph::RouteGraph;
use driver_break_core::routing::safety::{check_overnight_candidate, OvernightRejectReason};

use crate::candidates::{
    find_road_track_junctions, probe_along_track, ProbePoint, RoadTrackSeed, CORRIDOR_SEED_RADIUS_M,
    DEFAULT_TRACK_WALK_M,
};
use crate::card::{CampingCard, DeclineKind, SuggestionList};
use crate::fire::{fire_guidance_norway, LEAVE_NO_TRACE_NOTE, PROTECTED_SPECIES_NOTE};
use crate::host::CampingHost;
use crate::night_store::{location_id_from_lat_lon, NightStore};
use crate::packs::{pack_for_country, PackId, Tier};
use crate::NotCheckedLayers;

fn wild_camp_poi() -> PoiRecord {
    PoiRecord {
        osm_id: 0,
        lat: 0.0,
        lon: 0.0,
        categories: vec![],
        icon_key: String::new(),
        tags: HashMap::new(),
        name: None,
    }
}

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
    pub seeds: Vec<RoadTrackSeed>,
    pub probe_log: Vec<ProbeLogEntry>,
}

#[derive(Debug, Clone)]
pub struct ProbeLogEntry {
    pub lat: f64,
    pub lon: f64,
    pub accepted: bool,
    pub reason: String,
    pub road_highway: String,
}

/// Run the Phase 2 camping engine against a loaded graph + host backends.
pub fn suggest_overnight(
    host: &mut dyn CampingHost,
    input: &SuggestInput<'_>,
) -> SuggestOutcome {
    let walk = input.track_walk_m.unwrap_or(DEFAULT_TRACK_WALK_M);
    let radius = input.corridor_radius_m.unwrap_or(CORRIDOR_SEED_RADIUS_M);
    let seeds = find_road_track_junctions(input.graph, input.corridor_waypoints, radius);

    let mut list = SuggestionList::new();
    list.seeds_considered = seeds.len();
    let mut probe_log = Vec::new();

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
            seeds,
            probe_log,
        };
    };

    let clock = host.clock_local();
    let wild_poi = wild_camp_poi();

    for seed in &seeds {
        let pack_min = {
            // Probe needs country at seed first for pack min road — use seed coords.
            let iso = host.admin_country_iso(seed.lat, seed.lon);
            let pack = pack_for_country(iso.as_deref());
            pack.min_road_distance_m
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

        match evaluate_probe(host, &safety, &wild_poi, &probe, clock) {
            ProbeDecision::Accept(card) => {
                probe_log.push(ProbeLogEntry {
                    lat: probe.lat,
                    lon: probe.lon,
                    accepted: true,
                    reason: "accepted".into(),
                    road_highway: seed.road_highway.clone(),
                });
                list.probes_accepted += 1;
                list.cards.push(card);
                if let Some(max) = input.max_suggestions {
                    if list.probes_accepted >= max {
                        break;
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
                    // Keep decline cards for SJ / Tier D sample reporting.
                    list.cards.push(c);
                }
            }
        }
    }

    SuggestOutcome {
        list,
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

fn evaluate_probe(
    host: &mut dyn CampingHost,
    safety: &driver_break_core::config::SafetyConfig,
    wild_poi: &PoiRecord,
    probe: &ProbePoint,
    clock: Option<crate::host::LocalDate>,
) -> ProbeDecision {
    let country = host.admin_country_iso(probe.lat, probe.lon);
    let subdivision = host.admin_subdivision_iso(probe.lat, probe.lon);
    let pack = pack_for_country(country.as_deref());
    let not_checked = NotCheckedLayers::from_host_status(
        host.protected_area_layer_ready(),
        host.landcover_layer_ready(),
    );
    let loc_id = location_id_from_lat_lon(probe.lat, probe.lon);

    match pack.id {
        PackId::SvalbardDecline => {
            return ProbeDecision::Reject {
                reason: "svalbard_decline".into(),
                card: Some(CampingCard::svalbard_decline(probe.lat, probe.lon, clock)),
            };
        }
        PackId::TierD => {
            return ProbeDecision::Reject {
                reason: format!(
                    "tier_d_country_{}",
                    country.as_deref().unwrap_or("unknown")
                ),
                card: Some(CampingCard::decline_campsites_only(
                    probe.lat,
                    probe.lon,
                    country.as_deref().unwrap_or("unknown"),
                    pack.legal_basis,
                    pack.sources,
                    not_checked,
                    &[],
                )),
            };
        }
        PackId::Norway => {}
    }

    // Norway hard filters
    if pack.max_consecutive_nights.is_some() && !host.plugin_kv_available() {
        return ProbeDecision::Reject {
            reason: "plugin_kv_unavailable".into(),
            card: Some(CampingCard {
                lat: probe.lat,
                lon: probe.lon,
                accepted: false,
                decline: Some(DeclineKind::HardFilter),
                reject_reason: Some("plugin_kv_unavailable".into()),
                tier: pack.tier,
                country_iso: "no".into(),
                subdivision_iso: subdivision.clone(),
                legal_basis: pack.legal_basis.into(),
                sources: pack.sources.iter().map(|s| (*s).to_string()).collect(),
                fire_text: None,
                bare_rock_note: None,
                notes: vec![
                    "2-night consecutive limit cannot be enforced without plugin KV; \
declining (no silent rule bypass)."
                        .into(),
                ],
                not_checked,
                disclaimer: crate::DISCLAIMER.into(),
                location_id: loc_id,
                seed_road_highway: Some(probe.seed.road_highway.clone()),
                walk_m: Some(probe.walk_m),
            }),
        };
    }

    if let Some(max_n) = pack.max_consecutive_nights {
        let tonight = match clock {
            Some(d) => d,
            None => {
                // Without a date the night store cannot run — decline.
                return ProbeDecision::Reject {
                    reason: "clock_unavailable_for_night_store".into(),
                    card: None,
                };
            }
        };
        if NightStore::would_exceed(host, "no", &loc_id, tonight, max_n) {
            return ProbeDecision::Reject {
                reason: "max_consecutive_nights".into(),
                card: None,
            };
        }
    }

    if let Some(reason) = check_overnight_candidate(
        probe.lat,
        probe.lon,
        safety,
        wild_poi,
        host.overnight_buildings(),
        host.overnight_glacier_rings(),
    ) {
        let label = match reason {
            OvernightRejectReason::TooCloseToBuilding => "too_close_to_building",
            OvernightRejectReason::TooCloseToGlacier => "too_close_to_glacier",
        };
        return ProbeDecision::Reject {
            reason: label.into(),
            card: None,
        };
    }

    let fire = fire_guidance_norway(clock);
    let mut notes: Vec<String> = Vec::new();
    notes.push(PROTECTED_SPECIES_NOTE.into());
    notes.push(LEAVE_NO_TRACE_NOTE.into());
    // Cloudberry only when subdivision is known Nordland/Troms/Finnmark.
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
    notes.extend(
        not_checked
            .card_notes(pack.farmland_filter)
            .into_iter()
            .map(str::to_string),
    );
    match host.travel_mode() {
        crate::host::TravelMode::Unknown => {
            notes.push("travel mode not checked".into());
        }
        crate::host::TravelMode::Motorised => {
            notes.push(
                "travel mode is motorised — vehicle overnight rules are not applied in Phase 2 \
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
        tier: Tier::A,
        country_iso: "no".into(),
        subdivision_iso: subdivision,
        legal_basis: pack.legal_basis.into(),
        sources: pack.sources.iter().map(|s| (*s).to_string()).collect(),
        fire_text: Some(fire.text),
        bare_rock_note: Some(fire.bare_rock_note.into()),
        notes,
        not_checked,
        disclaimer: crate::DISCLAIMER.into(),
        location_id: loc_id,
        seed_road_highway: Some(probe.seed.road_highway.clone()),
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
    // Omitted while subdivision is unknown (Phase 2 decision).
    match subdivision_iso {
        Some(s) => {
            let u = s.to_ascii_uppercase();
            u.contains("NO-18") // Nordland
                || u.contains("NO-19") // Troms (legacy)
                || u.contains("NO-20") // Finnmark (legacy)
                || u.contains("NO-54") // Troms og Finnmark
                || u.contains("NO-55") // Troms (2024+)
                || u.contains("NO-56") // Finnmark (2024+)
                || u.contains("NORDLAND")
                || u.contains("TROMS")
                || u.contains("FINNMARK")
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{CampingHost, LocalDate, TravelMode};
    use driver_break_core::config::SafetyConfig;
    use std::collections::HashMap;

    struct MemHost {
        kv: HashMap<String, String>,
        kv_ok: bool,
        safety: Option<SafetyConfig>,
        date: Option<LocalDate>,
        buildings: Vec<(f64, f64)>,
        country: Option<String>,
    }

    impl CampingHost for MemHost {
        fn safety_config(&self) -> Option<SafetyConfig> {
            self.safety.clone()
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
            safety: Some(SafetyConfig::default()),
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
        let mut safety = SafetyConfig::default();
        safety.min_building_distance_m = 150.0;
        // ~111 m north of building (0.001° lat).
        let buildings = vec![(61.1000, 10.5000)];
        let mut h = MemHost {
            kv: HashMap::new(),
            kv_ok: true,
            safety: Some(safety.clone()),
            date: Some(LocalDate {
                year: 2026,
                month: 10,
                day: 1,
            }),
            buildings: buildings.clone(),
            country: Some("no".into()),
        };
        let probe = ProbePoint {
            lat: 61.1010,
            lon: 10.5000,
            seed: RoadTrackSeed {
                lat: 61.1,
                lon: 10.5,
                node: osm4routing::NodeId(1),
                road_highway: "tertiary".into(),
                rank: crate::candidates::JunctionRank::Preferred,
                track_edge_idx: 0,
                track_continues_m: 200.0,
            },
            walk_m: 120.0,
        };
        let date = h.date;
        let d1 = evaluate_probe(&mut h, &safety, &wild_camp_poi(), &probe, date);
        assert!(
            matches!(d1, ProbeDecision::Reject { reason, .. } if reason == "too_close_to_building"),
            "expected reject at 150 m threshold"
        );

        safety.min_building_distance_m = 80.0;
        h.safety = Some(safety.clone());
        let d2 = evaluate_probe(&mut h, &safety, &wild_camp_poi(), &probe, date);
        assert!(
            matches!(d2, ProbeDecision::Accept(_)),
            "expected accept when threshold drops below building distance"
        );
    }

    #[test]
    fn sweden_is_tier_d() {
        let mut h = MemHost {
            kv: HashMap::new(),
            kv_ok: true,
            safety: Some(SafetyConfig::default()),
            date: Some(LocalDate {
                year: 2026,
                month: 7,
                day: 1,
            }),
            buildings: vec![],
            country: Some("se".into()),
        };
        let probe = ProbePoint {
            lat: 60.0,
            lon: 12.5,
            seed: RoadTrackSeed {
                lat: 60.0,
                lon: 12.5,
                node: osm4routing::NodeId(1),
                road_highway: "tertiary".into(),
                rank: crate::candidates::JunctionRank::Preferred,
                track_edge_idx: 0,
                track_continues_m: 200.0,
            },
            walk_m: 120.0,
        };
        let date = h.date;
        let d = evaluate_probe(
            &mut h,
            &SafetyConfig::default(),
            &wild_camp_poi(),
            &probe,
            date,
        );
        match d {
            ProbeDecision::Reject { reason, card } => {
                assert!(reason.contains("tier_d"));
                let c = card.unwrap();
                assert_eq!(c.tier, Tier::D);
                assert_eq!(c.country_iso, "se");
            }
            _ => panic!("expected Tier D reject"),
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
}
