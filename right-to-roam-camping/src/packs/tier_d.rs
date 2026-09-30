use super::{DistanceRule, DurationRule, FireRule, PackId, RulePack, Tier};

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
    }
}
