//! Land-manager-keyed packs (USA / Canada / Russia). Flags default OFF; tenure unknown → Tier D.

use super::{
    CitedSource, DistanceRule, DurationRule, FireRule, HostCondition, PackId, RulePack,
    SourceQuality, SuggestionMode, Tier,
};

fn land_mgr(
    id: PackId,
    country: &str,
    flag: &'static str,
    legal: &'static str,
    sources: &'static [CitedSource],
    notes: &'static [&'static str],
    stay: Option<&'static str>,
    mode: SuggestionMode,
    tier: Tier,
) -> RulePack {
    RulePack {
        id,
        tier,
        country_iso: country.into(),
        legal_basis: legal,
        sources,
        distance: DistanceRule::NotApplicable,
        duration: DurationRule::NotVerified,
        fire: FireRule::GuidanceNote {
            text: "Check current fire restrictions for the managing unit; they change with conditions.",
        },
        guidance_notes: notes,
        farmland_not_checked_when_landcover_unknown: false,
        hard_filters: &[],
        maintainer_flag_default_off: true,
        required_subdivision: None,
        missing_subdivision_fallback: None,
        decline_when_protected_unknown: false,
        cmz: None,
        cloudberry_note: false,
        suggestion_mode: mode,
        flag_id: Some(flag),
        designated_layer: None,
        host_conditions: &[HostCondition::LandTenureKnown],
        flag_off_fallback: Some(Tier::D),
        conditions_unmet_fallback: Some(Tier::D),
        requires_land_tenure: true,
        stay_policy: stay,
        secondary_card_notes: &[],
    }
}

pub fn usa_blm_pack() -> RulePack {
    land_mgr(
        PackId::UsaBlm,
        "us",
        "land_mgr_usa_blm",
        "USA BLM dispersed camping — generally 14 days in any 28-day period",
        &[CitedSource {
            url: "https://www.blm.gov/programs/recreation/camping",
            quality: SourceQuality::Official,
        }],
        &[
            "Dispersed camping on most BLM land unless posted closed.",
            "Stay counter: 14 days in 28 with 25-mile radius key.",
            "Posted closures and fire restrictions override this suggestion.",
            "Flag default OFF; without PAD-US tenure → Tier D.",
        ],
        Some("blm_14_in_28"),
        SuggestionMode::WildCamp,
        Tier::B,
    )
}

pub fn usa_usfs_pack() -> RulePack {
    land_mgr(
        PackId::UsaUsfs,
        "us",
        "land_mgr_usa_usfs",
        "USA USFS dispersed camping — default 14 nights / 30 days; check forest order",
        &[CitedSource {
            url: "https://www.fs.usda.gov/r04/fishlake/recreation/dispersed-camping",
            quality: SourceQuality::Official,
        }],
        &[
            "Default stay 14/30; card must say check the forest order for {unit_name}.",
            "Flag default OFF; without PAD-US tenure → Tier D.",
        ],
        Some("usfs_14_in_30"),
        SuggestionMode::WildCamp,
        Tier::B,
    )
}

pub fn usa_nps_pack() -> RulePack {
    land_mgr(
        PackId::UsaNps,
        "us",
        "land_mgr_usa_nps",
        "USA NPS (lower 48 + HI) — designated sites / permit zones only (36 CFR 2.10)",
        &[CitedSource {
            url: "https://www.ecfr.gov/current/title-36/chapter-I/part-2/section-2.10",
            quality: SourceQuality::Official,
        }],
        &["Designated / permit sites only — Tier C path when flag/tenure ready."],
        None,
        SuggestionMode::DesignatedTentSitesOnly,
        Tier::C,
    )
}

pub fn usa_nps_alaska_pack() -> RulePack {
    land_mgr(
        PackId::UsaNpsAlaska,
        "us",
        "land_mgr_usa_nps_alaska",
        "USA NPS Alaska — 36 CFR 13.25; 14 consecutive days then move ≥ 2 miles",
        &[CitedSource {
            url: "https://www.ecfr.gov/current/title-36/chapter-I/part-13/section-13.25",
            quality: SourceQuality::Official,
        }],
        &[
            "Needs host flag for Alaska NPS units.",
            "Flag default OFF; without tenure → Tier D.",
        ],
        Some("nps_ak_14"),
        SuggestionMode::WildCamp,
        Tier::B,
    )
}

pub fn canada_ontario_pack() -> RulePack {
    land_mgr(
        PackId::CaOntario,
        "ca",
        "land_mgr_ca_ontario",
        "Ontario Crown land — 21 days per site per calendar year; move ≥ 100 m",
        &[CitedSource {
            url: "https://www.ontario.ca/page/recreational-activities-on-crown-land",
            quality: SourceQuality::Official,
        }],
        &[
            "Unknown residency → non-resident path (stricter permit/green-zone rules).",
            "Flag default OFF; without tenure → Tier D.",
        ],
        Some("on_21_per_year"),
        SuggestionMode::WildCamp,
        Tier::B,
    )
}

pub fn canada_bc_pack() -> RulePack {
    land_mgr(
        PackId::CaBc,
        "ca",
        "land_mgr_ca_bc",
        "BC Crown land — 14 consecutive days; reset after ≥ 72 h absence",
        &[CitedSource {
            url: "https://www2.gov.bc.ca/assets/gov/farming-natural-resources-and-industry/natural-resource-use/land-water-use/crown-land/permissions.pdf",
            quality: SourceQuality::Official,
        }],
        &["Flag default OFF; without tenure → Tier D."],
        Some("bc_14_consecutive_72h_reset"),
        SuggestionMode::WildCamp,
        Tier::B,
    )
}

pub fn canada_alberta_pack() -> RulePack {
    land_mgr(
        PackId::CaAlberta,
        "ca",
        "land_mgr_ca_alberta",
        "Alberta public land / PLUZ — 14 days then move 1 km for 72 h; pass required in Eastern Slopes",
        &[CitedSource {
            url: "https://www.alberta.ca/camping-on-public-land.aspx",
            quality: SourceQuality::Official,
        }],
        &["Flag default OFF; without tenure → Tier D."],
        Some("ab_14_then_1km_72h"),
        SuggestionMode::WildCamp,
        Tier::B,
    )
}

pub fn canada_quebec_pack() -> RulePack {
    land_mgr(
        PackId::CaQuebec,
        "ca",
        "land_mgr_ca_quebec",
        "Québec open public land — temporary occupation; structured territories excluded",
        &[CitedSource {
            url: "https://www.quebec.ca/en/tourism-and-recreation/sporting-and-outdoor-activities/activities-permitted-public-land",
            quality: SourceQuality::Official,
        }],
        &["Flag default OFF; without tenure → Tier D."],
        None,
        SuggestionMode::WildCamp,
        Tier::B,
    )
}

pub fn parks_canada_pack() -> RulePack {
    land_mgr(
        PackId::ParksCanada,
        "ca",
        "land_mgr_parks_canada",
        "Parks Canada — backcountry permit / designated zones only",
        &[CitedSource {
            url: "https://parkscanada.gc.ca/banff-backcountry",
            quality: SourceQuality::Official,
        }],
        &["Designated / permit sites only."],
        None,
        SuggestionMode::DesignatedTentSitesOnly,
        Tier::C,
    )
}

pub fn russia_forest_fund_pack() -> RulePack {
    RulePack {
        id: PackId::RussiaForestFund,
        tier: Tier::B,
        country_iso: "ru".into(),
        legal_basis: "Russia Forest Code art. 11 — forest-fund presence; camping duration/distance not verified",
        sources: &[CitedSource {
            url: "https://www.consultant.ru/document/cons_doc_LAW_64299/fdc3eb1198e1ac4458b4fc50c923d51cb84abab6/",
            quality: SourceQuality::Official,
        }],
        distance: DistanceRule::NotVerifiedDeclines,
        duration: DurationRule::NotVerified,
        fire: FireRule::GuidanceNote {
            text: "Regional special fire regime may ban fire or forest entry; cannot be checked from map.",
        },
        guidance_notes: &[
            "Decline: border zone / ООПТ / defence / not-forest-fund.",
            "Duration and distance not verified — no invented numbers.",
            "Card: no statutory stay limit found.",
            "Flag default OFF; without land-category layer → Tier D.",
        ],
        farmland_not_checked_when_landcover_unknown: false,
        hard_filters: &[],
        maintainer_flag_default_off: true,
        required_subdivision: None,
        missing_subdivision_fallback: None,
        decline_when_protected_unknown: false,
        cmz: None,
        cloudberry_note: false,
        suggestion_mode: SuggestionMode::DeclineCampsitesGuidance,
        flag_id: Some("land_mgr_russia_forest_fund"),
        designated_layer: None,
        host_conditions: &[HostCondition::LandTenureKnown],
        flag_off_fallback: Some(Tier::D),
        conditions_unmet_fallback: Some(Tier::D),
        requires_land_tenure: true,
        stay_policy: None,
        secondary_card_notes: &[],
    }
}

pub fn usa_tenure_unknown_pack() -> RulePack {
    RulePack {
        id: PackId::TierD,
        tier: Tier::D,
        country_iso: "us".into(),
        legal_basis: "USA — land tenure unknown; land-manager packs require PAD-US",
        sources: &[],
        distance: DistanceRule::NotApplicable,
        duration: DurationRule::NotVerified,
        fire: FireRule::NotVerified,
        guidance_notes: &[
            "Wild camping not suggested — land manager (BLM/USFS/NPS) unknown.",
            "Use designated campsites only until tenure is available.",
        ],
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
        requires_land_tenure: true,
        stay_policy: None,
        secondary_card_notes: &[],
    }
}

pub fn canada_tenure_unknown_pack() -> RulePack {
    RulePack {
        id: PackId::TierD,
        tier: Tier::D,
        country_iso: "ca".into(),
        legal_basis: "Canada — province/tenure unknown",
        sources: &[],
        distance: DistanceRule::NotApplicable,
        duration: DurationRule::NotVerified,
        fire: FireRule::NotVerified,
        guidance_notes: &["Wild camping not suggested — use designated campsites only."],
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
        requires_land_tenure: true,
        stay_policy: None,
        secondary_card_notes: &[],
    }
}

pub fn all_land_manager_packs() -> Vec<RulePack> {
    vec![
        usa_blm_pack(),
        usa_usfs_pack(),
        usa_nps_pack(),
        usa_nps_alaska_pack(),
        canada_ontario_pack(),
        canada_bc_pack(),
        canada_alberta_pack(),
        canada_quebec_pack(),
        parks_canada_pack(),
        russia_forest_fund_pack(),
    ]
}
