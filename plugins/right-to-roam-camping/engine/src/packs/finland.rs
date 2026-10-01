use super::{
    CitedSource, DistanceRule, DurationRule, FireRule, HardFilterSpec, PackId, RulePack,
    SourceQuality, SuggestionMode, Tier,
};

pub fn finland_pack() -> RulePack {
    RulePack {
        id: PackId::Finland,
        tier: Tier::A,
        country_iso: "fi".into(),
        legal_basis: "Everyman's rights (jokaisenoikeudet) — mainland Finland",
        sources: FINLAND_SOURCES,
        distance: DistanceRule::NotVerifiedUsesSafetyDefault {
            label: "Navi safety default, not Finnish law",
        },
        duration: DurationRule::SoftGuidance {
            note: "Temporary stay is typically 1–2 nights where movement is allowed.",
        },
        fire: FireRule::AlwaysNeedsLandownerPermission {
            text: "Open fire always needs landowner permission; it is never part of \
everyman's rights. In national parks, fire only at maintained fire sites.",
        },
        guidance_notes: &[
            "Not in yards, plantings or cultivated fields.",
            "Everyman's rights do not apply as-is in nature conservation areas.",
            "Clean up after yourself (leave no trace).",
        ],
        farmland_not_checked_when_landcover_unknown: true,
        hard_filters: FINLAND_HARD,
        maintainer_flag_default_off: false,
        required_subdivision: None,
        missing_subdivision_fallback: None,
        decline_when_protected_unknown: false,
        cmz: None,
        cloudberry_note: false,
        suggestion_mode: SuggestionMode::WildCamp,
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

/// Åland is its own ISO territory — never the FI mainland pack.
pub fn aland_tier_d_pack() -> RulePack {
    RulePack {
        id: PackId::AlandTierD,
        tier: Tier::D,
        country_iso: "ax".into(),
        legal_basis:
            "Åland (AX) — not mainland Finnish everyman's rights; no verified wild-camp pack",
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
        suggestion_mode: SuggestionMode::WildCamp,
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

const FINLAND_SOURCES: &[CitedSource] = &[
    CitedSource {
        url: "https://valtioneuvosto.fi/-//1410903/saako-toisen-mailla-hiihtaa-enta-saako-jaalle-tehda-avannon-usein-kysyttya-ymparistosta-palveluun-koottu-yhteen-kysymyksia-ja-vastauksia-jokaisenoikeuksista",
        quality: SourceQuality::Official,
    },
    CitedSource {
        url: "https://luontoon.fi",
        quality: SourceQuality::Official,
    },
];

const FINLAND_HARD: &[HardFilterSpec] = &[HardFilterSpec {
    id: "building_distance_navi_safety_default",
    sources: &[CitedSource {
        url: "navi:safety_config/min_building_distance_m",
        quality: SourceQuality::NaviSafetyDefault,
    }],
}];
