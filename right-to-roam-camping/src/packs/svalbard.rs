use super::{
    CitedSource, DistanceRule, DurationRule, FireRule, PackId, RulePack, SourceQuality, Tier,
};

pub fn svalbard_decline_pack() -> RulePack {
    RulePack {
        id: PackId::SvalbardDecline,
        tier: Tier::D,
        country_iso: "sj".into(),
        legal_basis: "Svalbard Environmental Protection Act (svalbardmiljøloven) — not friluftsloven",
        sources: SJ_SOURCES,
        distance: DistanceRule::NotApplicable,
        duration: DurationRule::NotVerified,
        fire: FireRule::NotVerified,
        guidance_notes: &[],
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

const SJ_SOURCES: &[CitedSource] = &[
    CitedSource {
        url: "https://www.sysselmesteren.no/contentassets/9de976c28bcb4205a65bc6403ba8e2b2/informasjonsmote-i-longyearbyen-om-endringer-i-miljoregelverket.pdf",
        quality: SourceQuality::Official,
    },
    CitedSource {
        url: "https://sysselmesteren.no/nb/publikasjoner/brosjyrer/sikkerhet-pa-svalbard",
        quality: SourceQuality::Official,
    },
];
