//! Suggestion / decline cards returned by the camping engine.

use serde::{Deserialize, Serialize};

use crate::packs::Tier;
use crate::{NotCheckedLayers, DISCLAIMER};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeclineKind {
    /// Tier D / unknown jurisdiction — campsites only.
    CampsitesOnly,
    /// Svalbard / Jan Mayen — decline with polar-bear guidance.
    Svalbard,
    /// Hard filter failed (building, nights, safety unavailable, KV unavailable).
    HardFilter,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CampingCard {
    pub lat: f64,
    pub lon: f64,
    pub accepted: bool,
    pub decline: Option<DeclineKind>,
    pub reject_reason: Option<String>,
    pub tier: Tier,
    pub country_iso: String,
    pub subdivision_iso: Option<String>,
    pub legal_basis: String,
    pub sources: Vec<String>,
    pub fire_text: Option<String>,
    pub bare_rock_note: Option<String>,
    pub notes: Vec<String>,
    pub not_checked: NotCheckedLayers,
    pub disclaimer: String,
    pub location_id: String,
    pub seed_road_highway: Option<String>,
    pub walk_m: Option<f64>,
}

impl CampingCard {
    pub fn decline_campsites_only(
        lat: f64,
        lon: f64,
        country_iso: &str,
        legal_basis: &str,
        sources: &[&str],
        not_checked: NotCheckedLayers,
        extra_notes: &[&str],
    ) -> Self {
        let mut notes: Vec<String> = extra_notes.iter().map(|s| (*s).to_string()).collect();
        notes.extend(
            not_checked
                .card_notes(false)
                .into_iter()
                .map(str::to_string),
        );
        notes.push("Wild camping not suggested here — use designated campsites only.".into());
        Self {
            lat,
            lon,
            accepted: false,
            decline: Some(DeclineKind::CampsitesOnly),
            reject_reason: Some("tier_d_or_unknown".into()),
            tier: Tier::D,
            country_iso: country_iso.into(),
            subdivision_iso: None,
            legal_basis: legal_basis.into(),
            sources: sources.iter().map(|s| (*s).to_string()).collect(),
            fire_text: None,
            bare_rock_note: None,
            notes,
            not_checked,
            disclaimer: DISCLAIMER.into(),
            location_id: crate::night_store::location_id_from_lat_lon(lat, lon),
            seed_road_highway: None,
            walk_m: None,
        }
    }

    pub fn svalbard_decline(lat: f64, lon: f64, date: Option<crate::host::LocalDate>) -> Self {
        let pack = crate::packs::svalbard_decline_pack();
        let polar_m = svalbard_polar_bear_distance_m(date);
        let mut notes = vec![
            format!(
                "Do not travel or stay closer than {polar_m} m to a polar bear \
(500 m from 1 March to 30 June; otherwise 300 m). The distance limit does not \
apply inside tents or huts."
            ),
            "Anyone travelling outside settlements must carry means to scare off \
polar bears; the Governor recommends a firearm (deterrent requirement)."
                .into(),
            "Notification duty (meldeplikt) applies for travel over large parts of Svalbard."
                .into(),
            "Contact Sysselmesteren for current rules and notifications.".into(),
        ];
        let not_checked = NotCheckedLayers::both_unknown();
        notes.extend(
            not_checked
                .card_notes(false)
                .into_iter()
                .map(str::to_string),
        );
        Self {
            lat,
            lon,
            accepted: false,
            decline: Some(DeclineKind::Svalbard),
            reject_reason: Some("svalbard_decline".into()),
            tier: Tier::D,
            country_iso: "sj".into(),
            subdivision_iso: None,
            legal_basis: pack.legal_basis.into(),
            sources: pack.source_urls().iter().map(|s| (*s).to_string()).collect(),
            fire_text: None,
            bare_rock_note: None,
            notes,
            not_checked,
            disclaimer: DISCLAIMER.into(),
            location_id: crate::night_store::location_id_from_lat_lon(lat, lon),
            seed_road_highway: None,
            walk_m: None,
        }
    }
}

/// Inclusive 1 Mar – 30 Jun → 500 m; otherwise 300 m.
pub fn svalbard_polar_bear_distance_m(date: Option<crate::host::LocalDate>) -> u32 {
    match date {
        Some(d) => match (d.month, d.day) {
            (3..=5, _) => 500,
            (6, day) if day <= 30 => 500,
            _ => 300,
        },
        // Unknown date → stricter distance (fail-safe).
        None => 500,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SuggestionList {
    pub cards: Vec<CampingCard>,
    pub seeds_considered: usize,
    pub probes_accepted: usize,
    pub probes_rejected: usize,
    pub disclaimer: String,
}

impl SuggestionList {
    pub fn new() -> Self {
        Self {
            cards: Vec::new(),
            seeds_considered: 0,
            probes_accepted: 0,
            probes_rejected: 0,
            disclaimer: DISCLAIMER.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::LocalDate;

    #[test]
    fn svalbard_card_carries_polar_bear_and_sysselmesteren() {
        let c = CampingCard::svalbard_decline(
            78.22,
            15.63,
            Some(LocalDate {
                year: 2026,
                month: 4,
                day: 1,
            }),
        );
        assert!(!c.accepted);
        assert_eq!(c.decline, Some(DeclineKind::Svalbard));
        assert_eq!(c.tier, Tier::D);
        let blob = c.notes.join(" ");
        assert!(blob.contains("500"));
        assert!(blob.contains("scare") || blob.contains("deterrent"));
        assert!(blob.contains("Sysselmesteren"));
        assert!(blob.contains("meldeplikt") || blob.contains("Notification"));
    }

    #[test]
    fn svalbard_unknown_date_uses_stricter_500m() {
        assert_eq!(svalbard_polar_bear_distance_m(None), 500);
        assert_eq!(
            svalbard_polar_bear_distance_m(Some(LocalDate {
                year: 2026,
                month: 7,
                day: 1,
            })),
            300
        );
    }
}
