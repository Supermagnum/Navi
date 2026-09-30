use super::{
    CitedSource, DistanceRule, DurationRule, FireRule, HardFilterSpec, PackId, RulePack, SuggestionMode,
    SourceQuality, Tier,
};

pub fn sweden_pack() -> RulePack {
    RulePack {
        id: PackId::Sweden,
        tier: Tier::A,
        country_iso: "se".into(),
        legal_basis: "Allemansrätten (customary / Naturvårdsverket guidance)",
        sources: SWEDEN_SOURCES,
        distance: DistanceRule::NoneInLawUsesSafetyDefault {
            label: "Navi safety default, not Swedish law",
        },
        duration: DurationRule::NoneInLaw {
            note: "No statutory night limit; official rule of thumb is a single day or so.",
        },
        fire: FireRule::GuidanceNote {
            // Spec Sweden pack has no Fire field. This is Navi general safety text,
            // not sourced from Swedish law or Naturvårdsverket.
            text: "General safety guidance, not Swedish law: follow local fire bans and \
never light a fire where it can spread.",
        },
        guidance_notes: &[
            "Pitch well away from homes, out of sight of their windows.",
            "Not on farmland, pasture or plantations (land cover not checked when layer unknown).",
            "National parks, nature reserves and municipal rules may ban tents.",
            "Clean up after yourself (leave no trace).",
        ],
        farmland_not_checked_when_landcover_unknown: true,
        hard_filters: SWEDEN_HARD,
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

const SWEDEN_SOURCES: &[CitedSource] = &[
    CitedSource {
        url: "https://www.naturvardsverket.se/allemansratten",
        quality: SourceQuality::Official,
    },
    CitedSource {
        url: "https://prod-egp.naturvardsverket.se/497366/globalassets/vagledning/allemansratten/material/handbok-gora-allemansratt-a4.pdf",
        quality: SourceQuality::Official,
    },
];

/// Building distance is Navi SafetyConfig (not a Swedish statutory metre).
const SWEDEN_HARD: &[HardFilterSpec] = &[HardFilterSpec {
    id: "building_distance_navi_safety_default",
    sources: &[CitedSource {
        url: "navi:safety_config/min_building_distance_m",
        quality: SourceQuality::NaviSafetyDefault,
    }],
}];
