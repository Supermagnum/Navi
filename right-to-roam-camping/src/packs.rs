//! Data-driven rule packs. Phase 2 populates Norway (Tier A) only; all else Tier D.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    A,
    B,
    C,
    D,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PackId {
    Norway,
    SvalbardDecline,
    TierD,
}

#[derive(Debug, Clone)]
pub struct RulePack {
    pub id: PackId,
    pub tier: Tier,
    pub country_iso: String,
    pub legal_basis: &'static str,
    pub sources: &'static [&'static str],
    /// When true, building distance comes from SafetyConfig (shared with core).
    pub distance_uses_safety_config: bool,
    pub max_consecutive_nights: Option<u32>,
    pub min_road_distance_m: Option<f64>,
    pub farmland_filter: bool,
    pub maintainer_flag_default_off: bool,
}

pub fn pack_for_country(country_iso: Option<&str>) -> RulePack {
    match country_iso {
        Some("no") => norway_pack(),
        Some("sj") => svalbard_decline_pack(),
        Some(c) => tier_d_pack(c),
        None => tier_d_pack("unknown"),
    }
}

pub fn norway_pack() -> RulePack {
    RulePack {
        id: PackId::Norway,
        tier: Tier::A,
        country_iso: "no".into(),
        legal_basis: "Friluftsloven (allemannsretten); Motorferdselloven; forskrift om brannforebygging § 3",
        sources: &[
            "https://lovdata.no/dokument/NL/lov/1957-06-28-16",
            "https://lovdata.no/dokument/NL/lov/1977-06-10-82",
            "https://lovdata.no/dokument/SF/forskrift/2015-12-17-1710",
        ],
        distance_uses_safety_config: true,
        max_consecutive_nights: Some(2),
        min_road_distance_m: None,
        farmland_filter: false,
        maintainer_flag_default_off: false,
    }
}

pub fn svalbard_decline_pack() -> RulePack {
    RulePack {
        id: PackId::SvalbardDecline,
        tier: Tier::D,
        country_iso: "sj".into(),
        legal_basis: "Svalbard Environmental Protection Act (svalbardmiljøloven) — not friluftsloven",
        sources: &[
            "https://www.sysselmesteren.no/contentassets/9de976c28bcb4205a65bc6403ba8e2b2/informasjonsmote-i-longyearbyen-om-endringer-i-miljoregelverket.pdf",
            "https://sysselmesteren.no/nb/publikasjoner/brosjyrer/sikkerhet-pa-svalbard",
        ],
        distance_uses_safety_config: false,
        max_consecutive_nights: None,
        min_road_distance_m: None,
        farmland_filter: false,
        maintainer_flag_default_off: false,
    }
}

pub fn tier_d_pack(country_iso: &str) -> RulePack {
    RulePack {
        id: PackId::TierD,
        tier: Tier::D,
        country_iso: country_iso.to_ascii_lowercase(),
        legal_basis: "No verified wild-camping pack for this jurisdiction",
        sources: &[],
        distance_uses_safety_config: false,
        max_consecutive_nights: None,
        min_road_distance_m: None,
        farmland_filter: false,
        maintainer_flag_default_off: false,
    }
}
