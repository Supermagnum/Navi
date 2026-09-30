//! Explicit rule packs — no shared Default; every field set per pack.

mod estonia;
mod finland;
mod iceland;
mod norway;
mod scotland;
mod sweden;
mod svalbard;
mod tier_d;
mod validate;

pub use estonia::estonia_pack;
pub use finland::{aland_tier_d_pack, finland_pack};
pub use iceland::iceland_pack;
pub use norway::norway_pack;
pub use scotland::scotland_pack;
pub use sweden::sweden_pack;
pub use svalbard::svalbard_decline_pack;
pub use tier_d::tier_d_pack;
pub use validate::{assert_hard_filters_official_only, validate_all_builtin_packs};

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
}

/// Citation quality. HARD filters may only cite [`SourceQuality::Official`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceQuality {
    Official,
    Ngo,
    Secondary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CitedSource {
    pub url: &'static str,
    pub quality: SourceQuality,
}

/// Building / privacy distance — every pack sets this explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistanceRule {
    /// Hard building filter via shared SafetyConfig; card must show `label`.
    SafetyConfigLabeled { label: &'static str },
    /// Law has no verified metres; still apply SafetyConfig as Navi safety with `label`.
    NotVerifiedUsesSafetyDefault { label: &'static str },
    /// Explicit: no distance in law; still apply labelled SafetyConfig (e.g. Sweden).
    NoneInLawUsesSafetyDefault { label: &'static str },
    /// Explicit: distance not used / not applicable for this pack path.
    NotApplicable,
}

/// Stay duration — hard limits vs soft notes vs none / not verified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurationRule {
    HardMaxConsecutiveNights {
        nights: u32,
        /// Plugin-kv pack segment (e.g. `"no"`, `"ee"`).
        store_key: &'static str,
        /// How many calendar days of night-store history this pack needs retained.
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

/// A hard algorithmic filter. Validator requires every source to be Official.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HardFilterSpec {
    pub id: &'static str,
    pub sources: &'static [CitedSource],
}

/// Loch Lomond & Trossachs Camping Management Zones (Scotland).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CmzPolicy {
    /// Inclusive local calendar months/days: 1 Mar – 30 Sep.
    pub season_start_month: u32,
    pub season_start_day: u32,
    pub season_end_month: u32,
    pub season_end_day: u32,
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
    /// When landcover is unknown, append farmland/pasture/plantation NotChecked note.
    pub farmland_not_checked_when_landcover_unknown: bool,
    pub hard_filters: &'static [HardFilterSpec],
    pub maintainer_flag_default_off: bool,
    /// If set, Tier A only when host subdivision matches (e.g. `GB-SCT`).
    pub required_subdivision: Option<&'static str>,
    /// Without a matching subdivision → this fallback tier (usually D).
    pub missing_subdivision_fallback: Option<Tier>,
    /// Iceland exception: decline while protected-area status is unknown.
    pub decline_when_protected_unknown: bool,
    /// Scotland CMZ: in season without a CMZ layer → do not suggest.
    pub cmz: Option<CmzPolicy>,
    /// Cloudberry-style notes only for Norway northern fylker.
    pub cloudberry_note: bool,
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
            DistanceRule::NotApplicable => None,
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

/// Resolve pack for a point. Subdivision required for Scotland (GB-SCT) and AX.
pub fn pack_for_location(country_iso: Option<&str>, subdivision_iso: Option<&str>) -> RulePack {
    let c = country_iso.map(|s| s.to_ascii_lowercase());
    let sub = subdivision_iso.map(|s| s.to_ascii_uppercase());
    match c.as_deref() {
        Some("no") => norway_pack(),
        Some("se") => sweden_pack(),
        Some("fi") => {
            // Mainland FI only; Åland as FI subdivision must never use the FI pack.
            if sub
                .as_deref()
                .map(|s| s == "AX" || s.ends_with("-AX") || s.contains("ALAND") || s.contains("ÅLAND"))
                .unwrap_or(false)
            {
                aland_tier_d_pack()
            } else {
                finland_pack()
            }
        }
        Some("ax") => aland_tier_d_pack(),
        Some("ee") => estonia_pack(),
        Some("is") => iceland_pack(),
        Some("sj") => svalbard_decline_pack(),
        Some("gb") => {
            // Scotland Tier A only on positive GB-SCT; else Tier D until England/Wales Tier C (3b).
            if sub
                .as_deref()
                .map(|s| s == "GB-SCT" || s.ends_with("-SCT") || s == "SCT")
                .unwrap_or(false)
            {
                scotland_pack()
            } else {
                tier_d_pack("gb")
            }
        }
        Some(other) => tier_d_pack(other),
        None => tier_d_pack("unknown"),
    }
}

/// Back-compat: country only (Scotland without subdivision → Tier D).
pub fn pack_for_country(country_iso: Option<&str>) -> RulePack {
    pack_for_location(country_iso, None)
}

/// All Tier A / special packs that ship in Phase 3a (for validators + retention).
pub fn builtin_enabled_packs() -> Vec<RulePack> {
    vec![
        norway_pack(),
        sweden_pack(),
        finland_pack(),
        estonia_pack(),
        scotland_pack(),
        iceland_pack(),
        svalbard_decline_pack(),
        aland_tier_d_pack(),
    ]
}

pub fn in_cmz_season(policy: &CmzPolicy, d: crate::host::LocalDate) -> bool {
    let after_start = (d.month > policy.season_start_month)
        || (d.month == policy.season_start_month && d.day >= policy.season_start_day);
    let before_end = (d.month < policy.season_end_month)
        || (d.month == policy.season_end_month && d.day <= policy.season_end_day);
    after_start && before_end
}
