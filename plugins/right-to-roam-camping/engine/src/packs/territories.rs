//! Territories with own ISO codes — Tier D, never the parent pack.

use super::{
    DistanceRule, DurationRule, FireRule, PackId, RulePack, SuggestionMode, Tier,
};

fn territory(
    id: PackId,
    iso: &str,
    legal: &'static str,
    notes: &'static [&'static str],
) -> RulePack {
    RulePack {
        id,
        tier: Tier::D,
        country_iso: iso.into(),
        legal_basis: legal,
        sources: &[],
        distance: DistanceRule::NotApplicable,
        duration: DurationRule::NotVerified,
        fire: FireRule::NotVerified,
        guidance_notes: notes,
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

pub fn territory_pack(iso: &str) -> Option<RulePack> {
    match iso.to_ascii_lowercase().as_str() {
        "ax" => Some(crate::packs::aland_tier_d_pack()),
        "sj" => Some(crate::packs::svalbard_decline_pack()),
        "fo" => Some(territory(
            PackId::TerritoryFo,
            "fo",
            "Faroe Islands (FO) — outside Danish land-access law; fri-teltning does not apply",
            &["Wild camping not suggested — use designated campsites only."],
        )),
        "gl" => Some(territory(
            PackId::TerritoryGl,
            "gl",
            "Greenland (GL) — own legislation; Danish rules do not apply",
            &["Wild camping not suggested — use designated campsites only."],
        )),
        "gb-nir" | "nir" => Some(territory(
            PackId::TerritoryGbNir,
            "gb-nir",
            "Northern Ireland (GB-NIR) — neither Scottish Code nor England/Wales pack",
            &["Wild camping not suggested — use designated campsites only."],
        )),
        "im" => Some(territory(
            PackId::TerritoryIm,
            "im",
            "Isle of Man (IM) — Crown dependency with own law",
            &["Wild camping not suggested — use designated campsites only."],
        )),
        "je" => Some(territory(
            PackId::TerritoryJe,
            "je",
            "Jersey (JE) — Crown dependency with own law",
            &["Wild camping not suggested — use designated campsites only."],
        )),
        "gg" => Some(territory(
            PackId::TerritoryGg,
            "gg",
            "Guernsey (GG) — Crown dependency with own law",
            &["Wild camping not suggested — use designated campsites only."],
        )),
        "gi" => Some(territory(
            PackId::TerritoryGi,
            "gi",
            "Gibraltar (GI) — own law",
            &["Wild camping not suggested — use designated campsites only."],
        )),
        _ => None,
    }
}

pub fn all_territory_packs() -> Vec<RulePack> {
    ["fo", "gl", "gb-nir", "im", "je", "gg", "gi"]
        .into_iter()
        .filter_map(territory_pack)
        .collect()
}
