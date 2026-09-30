use super::{DistanceRule, DurationRule, FireRule, PackId, RulePack, SuggestionMode, Tier};

pub fn tier_d_pack(country_iso: &str) -> RulePack {
    RulePack {
        id: PackId::TierD,
        tier: Tier::D,
        country_iso: country_iso.to_ascii_lowercase(),
        legal_basis: "No verified wild-camping pack for this jurisdiction",
        sources: &[],
        distance: DistanceRule::NotApplicable,
        duration: DurationRule::NotVerified,
        fire: FireRule::NotVerified,
        guidance_notes: &["Wild camping not suggested here — use designated campsites only."],
        farmland_not_checked_when_landcover_unknown: false,
        hard_filters: &[],
        maintainer_flag_default_off: false,
        required_subdivision: None,
        missing_subdivision_fallback: None,
        decline_when_protected_unknown: false,
        cmz: None,
        cloudberry_note: false,
        suggestion_mode: SuggestionMode::DeclineCampsitesGuidance,
        flag_id: None,
        designated_layer: None,
        host_conditions: &[],
        flag_off_fallback: None,
        conditions_unmet_fallback: None,
        requires_land_tenure: false,
        stay_policy: None,
        secondary_card_notes: &[],
    }
}

/// GB with unknown subdivision (Phase 1): Tier D, no England/Wales legal claim.
pub fn gb_unknown_subdivision_tier_d_pack() -> RulePack {
    RulePack {
        id: PackId::TierD,
        tier: Tier::D,
        country_iso: "gb".into(),
        legal_basis: "Great Britain — subdivision unknown; no jurisdiction pack applied",
        sources: &[],
        distance: DistanceRule::NotApplicable,
        duration: DurationRule::NotVerified,
        fire: FireRule::NotVerified,
        guidance_notes: &[
            "Wild camping not suggested — country subdivision is unknown.",
            "No regional access-code pack is applied until a positive subdivision is known.",
        ],
        farmland_not_checked_when_landcover_unknown: false,
        hard_filters: &[],
        maintainer_flag_default_off: false,
        required_subdivision: None,
        missing_subdivision_fallback: Some(Tier::D),
        decline_when_protected_unknown: false,
        cmz: None,
        cloudberry_note: false,
        suggestion_mode: SuggestionMode::DeclineCampsitesGuidance,
        flag_id: None,
        designated_layer: None,
        host_conditions: &[],
        flag_off_fallback: None,
        conditions_unmet_fallback: None,
        requires_land_tenure: false,
        stay_policy: None,
        secondary_card_notes: &[],
    }
}
