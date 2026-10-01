//! Rest-of-world packs (Chile, Argentina, KR, MX, JP) and world Tier D country list.

use super::{
    CitedSource, DistanceRule, DurationRule, FireRule, PackId, RulePack, SourceQuality,
    SuggestionMode, Tier,
};

pub fn chile_conaf_pack() -> RulePack {
    RulePack {
        id: PackId::ChileConaf,
        tier: Tier::C,
        country_iso: "cl".into(),
        legal_basis: "Chile CONAF protected areas — designated sites only; elsewhere Tier D",
        sources: &[
            CitedSource {
                url: "https://chile.travel/blog/explora-los-parques-y-reservas-naturales-de-chile/",
                quality: SourceQuality::Official,
            },
            CitedSource {
                url: "https://www.semanariolocal.cl/?p=33017",
                quality: SourceQuality::Secondary,
            },
        ],
        distance: DistanceRule::NotApplicable,
        duration: DurationRule::NotVerified,
        fire: FireRule::GuidanceNote {
            text: "Fire prohibited in most parks except enabled zones.",
        },
        guidance_notes: &[
            "Inside CONAF SNASPE: TentSite / designated only.",
            "Outside protected areas: Tier D (no wild-camp pack).",
        ],
        farmland_not_checked_when_landcover_unknown: false,
        hard_filters: &[],
        maintainer_flag_default_off: false,
        required_subdivision: None,
        missing_subdivision_fallback: None,
        decline_when_protected_unknown: false,
        cmz: None,
        cloudberry_note: false,
        suggestion_mode: SuggestionMode::DesignatedTentSitesOnly,
        flag_id: None,
        designated_layer: None,
        host_conditions: &[],
        flag_off_fallback: None,
        conditions_unmet_fallback: None,
        requires_land_tenure: false,
        stay_policy: None,
        secondary_card_notes: &[
            "CONAF (secondary report): unauthorised fire in protected areas may carry \
61 days to 3 years' imprisonment and a fine; serious forest fire can bring expulsion \
for foreigners (Ley 20.653) — re-verify on conaf.cl / leychile.cl.",
        ],
    }
}

pub fn argentina_apn_pack() -> RulePack {
    RulePack {
        id: PackId::ArgentinaApn,
        tier: Tier::C,
        country_iso: "ar".into(),
        legal_basis: "Argentina national parks (APN) — designated sites only; elsewhere Tier D",
        sources: &[],
        distance: DistanceRule::NotApplicable,
        duration: DurationRule::NotVerified,
        fire: FireRule::NotVerified,
        guidance_notes: &["Inside APN: TentSite only. Outside: Tier D."],
        farmland_not_checked_when_landcover_unknown: false,
        hard_filters: &[],
        maintainer_flag_default_off: false,
        required_subdivision: None,
        missing_subdivision_fallback: None,
        decline_when_protected_unknown: false,
        cmz: None,
        cloudberry_note: false,
        suggestion_mode: SuggestionMode::DesignatedTentSitesOnly,
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

pub fn south_korea_pack() -> RulePack {
    RulePack {
        id: PackId::SouthKoreaParks,
        tier: Tier::C,
        country_iso: "kr".into(),
        legal_basis: "South Korea — Tier C inside natural parks; Tier D elsewhere",
        sources: &[],
        distance: DistanceRule::NotApplicable,
        duration: DurationRule::NotVerified,
        fire: FireRule::NotVerified,
        guidance_notes: &["Natural parks: designated sites only."],
        farmland_not_checked_when_landcover_unknown: false,
        hard_filters: &[],
        maintainer_flag_default_off: false,
        required_subdivision: None,
        missing_subdivision_fallback: None,
        decline_when_protected_unknown: false,
        cmz: None,
        cloudberry_note: false,
        suggestion_mode: SuggestionMode::DesignatedTentSitesOnly,
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

pub fn mexico_pack() -> RulePack {
    RulePack {
        id: PackId::Mexico,
        tier: Tier::D,
        country_iso: "mx".into(),
        legal_basis: "Mexico — beach access is not a right to camp overnight; not verified",
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
        requires_land_tenure: false,
        stay_policy: None,
        secondary_card_notes: &[],
    }
}

pub fn japan_pack() -> RulePack {
    RulePack {
        id: PackId::Japan,
        tier: Tier::D,
        country_iso: "jp".into(),
        legal_basis: "Japan — no verified wild-camp pack",
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
        requires_land_tenure: false,
        stay_policy: None,
        secondary_card_notes: &[],
    }
}

/// Spec world Tier D table (countries without a researched wild-camp pack).
pub fn world_tier_d_countries() -> &'static [&'static str] {
    &[
        "hr", "si", "es", "it", "pt", "gr", "hu", "sk", "lu", "li", "bg", "ro", "rs", "ba", "me",
        "al", "mk", "tr", "ua", "by", "md", "ge", "am", "az", "cn", "in", "au", "nz", "za", "br",
        "pe", "co", "ec", "uy", "py", "bo", "ve",
    ]
}
