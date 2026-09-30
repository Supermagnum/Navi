//! Explicit rule packs — no shared Default; every field set per pack.

mod estonia;
mod finland;
mod iceland;
mod land_manager;
mod norway;
mod scotland;
mod stay;
mod sweden;
mod svalbard;
mod territories;
mod tier_b;
mod tier_c;
mod tier_d;
mod validate;
mod world;

pub use estonia::estonia_pack;
pub use finland::{aland_tier_d_pack, finland_pack};
pub use iceland::iceland_pack;
pub use land_manager::{
    all_land_manager_packs, canada_alberta_pack, canada_bc_pack, canada_ontario_pack,
    canada_quebec_pack, canada_tenure_unknown_pack, parks_canada_pack, russia_forest_fund_pack,
    usa_blm_pack, usa_nps_alaska_pack, usa_nps_pack, usa_tenure_unknown_pack, usa_usfs_pack,
};
pub use norway::norway_pack;
pub use scotland::scotland_pack;
pub use stay::{
    stay_would_exceed, StayDecision, StayPolicy, BLM_RADIUS_MILES, STAY_POLICIES,
};
pub use sweden::sweden_pack;
pub use svalbard::svalbard_decline_pack;
pub use territories::{all_territory_packs, territory_pack};
pub use tier_b::{all_tier_b_packs, tier_b_pack_by_flag};
pub use tier_c::{all_tier_c_packs, england_wales_pack};
pub use tier_d::{gb_unknown_subdivision_tier_d_pack, tier_d_pack};
pub use validate::{
    assert_hard_filters_official_only, assert_tier_b_and_land_manager_flags_default_off,
    validate_all_builtin_packs,
};
pub use world::{
    argentina_apn_pack, chile_conaf_pack, japan_pack, mexico_pack, south_korea_pack,
    world_tier_d_countries,
};

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
    Sweden,
    Finland,
    Estonia,
    Scotland,
    Iceland,
    SvalbardDecline,
    AlandTierD,
    TierD,
    // Tier C
    EnglandWales,
    Denmark,
    Netherlands,
    BelgiumFlanders,
    Poland,
    Czechia,
    Lithuania,
    Ireland,
    FranceTents,
    GermanyOtherLander,
    AustriaKarntenNoeTirol,
    ParksCanada,
    ChileConaf,
    ArgentinaApn,
    SouthKoreaParks,
    // Tier B (flag OFF)
    DeBrandenburg,
    DeMecklenburgVorpommern,
    DeSchleswigHolstein,
    AtAboveTreeline,
    ChAboveTreeline,
    LvStateForest,
    DartmoorCommons,
    EcrinsCore,
    DkFriTeltning,
    // Land manager
    UsaBlm,
    UsaUsfs,
    UsaNps,
    UsaNpsAlaska,
    CaOntario,
    CaBc,
    CaAlberta,
    CaQuebec,
    RussiaForestFund,
    // Territories
    TerritoryFo,
    TerritoryGl,
    TerritoryGbNir,
    TerritoryIm,
    TerritoryJe,
    TerritoryGg,
    TerritoryGi,
    // World D data
    Mexico,
    Japan,
    WorldTierD,
}

/// Citation quality. Law-backed HARD filters may only cite [`SourceQuality::Official`].
/// SafetyConfig-derived building distance must use [`SourceQuality::NaviSafetyDefault`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceQuality {
    Official,
    Ngo,
    Secondary,
    /// Navi OvernightSafety metres — not a statutory figure for the jurisdiction.
    NaviSafetyDefault,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CitedSource {
    pub url: &'static str,
    pub quality: SourceQuality,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistanceRule {
    SafetyConfigLabeled { label: &'static str },
    NotVerifiedUsesSafetyDefault { label: &'static str },
    NoneInLawUsesSafetyDefault { label: &'static str },
    NotApplicable,
    NotVerifiedDeclines,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurationRule {
    HardMaxConsecutiveNights {
        nights: u32,
        store_key: &'static str,
        retention_days: u32,
    },
    SoftGuidance { note: &'static str },
    NoneInLaw { note: &'static str },
    NotVerified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FireRule {
    NorwayDateGated,
    AlwaysNeedsLandownerPermission { text: &'static str },
    GuidanceNote { text: &'static str },
    NoneInLaw,
    NotVerified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HardFilterSpec {
    pub id: &'static str,
    pub sources: &'static [CitedSource],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CmzPolicy {
    pub season_start_month: u32,
    pub season_start_day: u32,
    pub season_end_month: u32,
    pub season_end_day: u32,
}

/// How the engine suggests overnight spots for this pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionMode {
    /// Tier A wild-camp algorithm (road∩track probes).
    WildCamp,
    /// Tier C: only TentSite POIs; never invent designated-layer spots.
    DesignatedTentSitesOnly,
    /// Decline wild camp (territory / Tier D / land-manager with unknown tenure).
    DeclineCampsitesGuidance,
}

/// Designated-site polygon layers; when required and not classified → empty result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DesignatedLayer {
    Paalkamp,
    Bivakzones,
    Trekkingplaetze,
    FriTeltning,
    ZanocujWLesie,
    DartmoorCommons,
    LvmStateForest,
    EcrinsCore,
}

/// Host conditions every Tier B pack needs before it may run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HostCondition {
    NonMotorisedTravel,
    NotForest,
    NotProtectedArea,
    NotResidential,
    AboveTreeline,
    DesignatedLayerReady(DesignatedLayer),
    LandTenureKnown,
}

#[derive(Debug, Clone)]
pub struct RulePack {
    pub id: PackId,
    pub tier: Tier,
    pub country_iso: String,
    pub legal_basis: &'static str,
    pub sources: &'static [CitedSource],
    pub distance: DistanceRule,
    pub duration: DurationRule,
    pub fire: FireRule,
    pub guidance_notes: &'static [&'static str],
    pub farmland_not_checked_when_landcover_unknown: bool,
    pub hard_filters: &'static [HardFilterSpec],
    /// When true, pack is behind a maintainer flag that must default OFF.
    pub maintainer_flag_default_off: bool,
    pub required_subdivision: Option<&'static str>,
    pub missing_subdivision_fallback: Option<Tier>,
    pub decline_when_protected_unknown: bool,
    pub cmz: Option<CmzPolicy>,
    pub cloudberry_note: bool,
    pub suggestion_mode: SuggestionMode,
    /// Stable flag id for Tier B / land-manager packs (`None` = not flagged).
    pub flag_id: Option<&'static str>,
    pub designated_layer: Option<DesignatedLayer>,
    pub host_conditions: &'static [HostCondition],
    pub flag_off_fallback: Option<Tier>,
    pub conditions_unmet_fallback: Option<Tier>,
    pub requires_land_tenure: bool,
    pub stay_policy: Option<&'static str>,
    /// Secondary-only notes (e.g. Chile CONAF fire penalty) — never HARD filters.
    pub secondary_card_notes: &'static [&'static str],
}

impl RulePack {
    pub fn source_urls(&self) -> Vec<&'static str> {
        self.sources.iter().map(|s| s.url).collect()
    }

    pub fn uses_safety_config_distance(&self) -> bool {
        matches!(
            self.distance,
            DistanceRule::SafetyConfigLabeled { .. }
                | DistanceRule::NotVerifiedUsesSafetyDefault { .. }
                | DistanceRule::NoneInLawUsesSafetyDefault { .. }
        )
    }

    pub fn distance_card_label(&self) -> Option<&'static str> {
        match self.distance {
            DistanceRule::SafetyConfigLabeled { label }
            | DistanceRule::NotVerifiedUsesSafetyDefault { label }
            | DistanceRule::NoneInLawUsesSafetyDefault { label } => Some(label),
            DistanceRule::NotApplicable | DistanceRule::NotVerifiedDeclines => None,
        }
    }

    pub fn hard_max_nights(&self) -> Option<(u32, &'static str)> {
        match self.duration {
            DurationRule::HardMaxConsecutiveNights {
                nights, store_key, ..
            } => Some((nights, store_key)),
            _ => None,
        }
    }

    pub fn night_store_retention_days(&self) -> Option<u32> {
        match self.duration {
            DurationRule::HardMaxConsecutiveNights { retention_days, .. } => Some(retention_days),
            _ => None,
        }
    }
}

fn sub_is(sub: Option<&str>, codes: &[&str]) -> bool {
    let Some(s) = sub else {
        return false;
    };
    let u = s.to_ascii_uppercase();
    codes.iter().any(|c| u == *c || u.ends_with(&format!("-{c}")) || u.contains(c))
}

/// Resolve pack for a point. Subdivision / tenure refine Tier B/C/land-manager.
pub fn pack_for_location(country_iso: Option<&str>, subdivision_iso: Option<&str>) -> RulePack {
    pack_for_location_with_tenure(country_iso, subdivision_iso, None)
}

pub fn pack_for_location_with_tenure(
    country_iso: Option<&str>,
    subdivision_iso: Option<&str>,
    tenure_manager: Option<&str>,
) -> RulePack {
    let c = country_iso.map(|s| s.to_ascii_lowercase());
    let sub = subdivision_iso.map(|s| s.to_ascii_uppercase());
    let tenure = tenure_manager.map(|s| s.to_ascii_lowercase());

    // Territories with own ISO — never parent pack.
    if let Some(iso) = c.as_deref() {
        if let Some(p) = territory_pack(iso) {
            return p;
        }
    }

    match c.as_deref() {
        Some("no") => norway_pack(),
        Some("se") => sweden_pack(),
        Some("fi") => {
            if sub_is(sub.as_deref(), &["AX", "ALAND", "ÅLAND"]) {
                aland_tier_d_pack()
            } else {
                finland_pack()
            }
        }
        Some("ee") => estonia_pack(),
        Some("is") => iceland_pack(),
        Some("gb") => {
            if sub_is(sub.as_deref(), &["SCT", "GB-SCT"]) {
                scotland_pack()
            } else if sub_is(sub.as_deref(), &["NIR", "GB-NIR"]) {
                territory_pack("gb-nir").unwrap_or_else(|| tier_d_pack("gb-nir"))
            } else if sub_is(sub.as_deref(), &["ENG", "GB-ENG", "WLS", "GB-WLS"])
                || sub.as_deref().is_some_and(|s| {
                    // Positive England/Wales ISO-3166-2 country subdivision only.
                    s.starts_with("GB-E") || s.starts_with("GB-W")
                })
            {
                england_wales_pack()
            } else {
                // Phase 1: unknown GB subdivision → Tier D; never claim England/Wales law.
                gb_unknown_subdivision_tier_d_pack()
            }
        }
        Some("dk") => tier_c::denmark_pack(),
        Some("nl") => tier_c::netherlands_pack(),
        Some("be") => {
            if sub_is(sub.as_deref(), &["VLG", "VAN", "VWV", "VLI", "VOV", "VBR"])
                || sub.as_deref().is_some_and(|s| s.contains("FLANDER") || s.contains("VLAANDER"))
            {
                tier_c::belgium_flanders_pack()
            } else {
                // Wallonia not verified → Tier D
                tier_d_pack("be")
            }
        }
        Some("pl") => tier_c::poland_pack(),
        Some("cz") => tier_c::czechia_pack(),
        Some("lt") => tier_c::lithuania_pack(),
        Some("ie") => tier_c::ireland_pack(),
        Some("fr") => tier_c::france_tents_pack(),
        Some("de") => {
            if sub_is(sub.as_deref(), &["BB", "DE-BB", "BRANDENBURG"]) {
                tier_b::de_brandenburg_pack()
            } else if sub_is(sub.as_deref(), &["MV", "DE-MV"]) {
                tier_b::de_mv_pack()
            } else if sub_is(sub.as_deref(), &["SH", "DE-SH"]) {
                tier_b::de_sh_pack()
            } else if sub.is_some() {
                tier_c::germany_other_lander_pack()
            } else {
                // Subdivision unknown → cannot pick Land pack; Tier C designated only.
                tier_c::germany_other_lander_pack()
            }
        }
        Some("at") => {
            if sub_is(sub.as_deref(), &["K", "AT-K", "KARNTEN", "KÄRNTEN", "3", "AT-3", "NOE", "NÖ", "NIEDER", "7", "AT-7", "TIROL", "T", "AT-T"])
            {
                tier_c::austria_karnten_noe_tirol_pack()
            } else if sub_is(
                sub.as_deref(),
                &["4", "AT-4", "OOE", "OBER", "5", "AT-5", "SALZBURG", "6", "AT-6", "STEIER", "8", "AT-8", "VORARL"],
            ) {
                tier_b::at_above_treeline_pack()
            } else {
                tier_d_pack("at")
            }
        }
        Some("ch") => tier_b::ch_above_treeline_pack(),
        Some("lv") => tier_b::lv_state_forest_pack(),
        Some("us") => match tenure.as_deref() {
            Some("blm") => usa_blm_pack(),
            Some("usfs") | Some("forest_service") => usa_usfs_pack(),
            Some("nps") => usa_nps_pack(),
            Some("nps_alaska") => usa_nps_alaska_pack(),
            _ => usa_tenure_unknown_pack(),
        },
        Some("ca") => match tenure.as_deref() {
            Some("parks_canada") => parks_canada_pack(),
            Some("crown_on") | Some("ontario_crown") => canada_ontario_pack(),
            Some("crown_bc") | Some("bc_crown") => canada_bc_pack(),
            Some("crown_ab") | Some("alberta_public") => canada_alberta_pack(),
            Some("crown_qc") | Some("quebec_public") => canada_quebec_pack(),
            _ => {
                if sub_is(sub.as_deref(), &["ON", "CA-ON"]) {
                    canada_ontario_pack()
                } else if sub_is(sub.as_deref(), &["BC", "CA-BC"]) {
                    canada_bc_pack()
                } else if sub_is(sub.as_deref(), &["AB", "CA-AB"]) {
                    canada_alberta_pack()
                } else if sub_is(sub.as_deref(), &["QC", "CA-QC"]) {
                    canada_quebec_pack()
                } else {
                    canada_tenure_unknown_pack()
                }
            }
        },
        Some("ru") => russia_forest_fund_pack(),
        Some("cl") => chile_conaf_pack(),
        Some("ar") => argentina_apn_pack(),
        Some("kr") => south_korea_pack(),
        Some("mx") => mexico_pack(),
        Some("jp") => japan_pack(),
        Some(other) => {
            if world_tier_d_countries().iter().any(|c| *c == other) {
                let mut p = tier_d_pack(other);
                p.id = PackId::WorldTierD;
                p
            } else {
                tier_d_pack(other)
            }
        }
        None => tier_d_pack("unknown"),
    }
}

pub fn pack_for_country(country_iso: Option<&str>) -> RulePack {
    pack_for_location(country_iso, None)
}

/// Packs that participate in night-store retention / Official HARD validation (enabled today).
pub fn builtin_enabled_packs() -> Vec<RulePack> {
    let mut v = vec![
        norway_pack(),
        sweden_pack(),
        finland_pack(),
        estonia_pack(),
        scotland_pack(),
        iceland_pack(),
        svalbard_decline_pack(),
        aland_tier_d_pack(),
        england_wales_pack(),
        tier_c::denmark_pack(),
        tier_c::netherlands_pack(),
        tier_c::belgium_flanders_pack(),
        tier_c::poland_pack(),
        tier_c::czechia_pack(),
        tier_c::lithuania_pack(),
        tier_c::ireland_pack(),
        tier_c::france_tents_pack(),
        tier_c::germany_other_lander_pack(),
        tier_c::austria_karnten_noe_tirol_pack(),
    ];
    v.extend(all_tier_b_packs());
    v.extend(all_territory_packs());
    v.extend(all_land_manager_packs());
    v.push(chile_conaf_pack());
    v.push(argentina_apn_pack());
    v.push(south_korea_pack());
    v.push(mexico_pack());
    v.push(japan_pack());
    v
}

/// Every pack that ships as data (for the Phase 3b/3c report table).
pub fn all_declared_packs() -> Vec<RulePack> {
    builtin_enabled_packs()
}

pub fn in_cmz_season(policy: &CmzPolicy, d: crate::host::LocalDate) -> bool {
    let after_start = (d.month > policy.season_start_month)
        || (d.month == policy.season_start_month && d.day >= policy.season_start_day);
    let before_end = (d.month < policy.season_end_month)
        || (d.month == policy.season_end_month && d.day <= policy.season_end_day);
    after_start && before_end
}
