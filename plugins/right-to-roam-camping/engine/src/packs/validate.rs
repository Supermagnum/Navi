use super::{builtin_enabled_packs, DistanceRule, RulePack, SourceQuality};

/// Law-backed HARD filters may only cite Official. NGO / secondary → test failure.
/// SafetyConfig-derived building-distance HARD filters must cite NaviSafetyDefault only.
pub fn assert_hard_filters_official_only(pack: &RulePack) {
    for hf in pack.hard_filters {
        assert!(
            !hf.sources.is_empty(),
            "pack {:?}: hard filter {} has no sources",
            pack.id,
            hf.id
        );
        let safety_default = is_navi_safety_default_filter(hf.id, &pack.distance);
        for src in hf.sources {
            if safety_default {
                assert_eq!(
                    src.quality,
                    SourceQuality::NaviSafetyDefault,
                    "pack {:?}: SafetyConfig-derived hard filter {} must use NaviSafetyDefault, got {:?} ({})",
                    pack.id,
                    hf.id,
                    src.quality,
                    src.url
                );
            } else {
                assert_eq!(
                    src.quality,
                    SourceQuality::Official,
                    "pack {:?}: hard filter {} cites non-official source {} ({:?})",
                    pack.id,
                    hf.id,
                    src.url,
                    src.quality
                );
            }
        }
    }
}

fn is_navi_safety_default_filter(id: &str, distance: &DistanceRule) -> bool {
    id.contains("navi_safety_default")
        || matches!(
            distance,
            DistanceRule::NotVerifiedUsesSafetyDefault { .. }
                | DistanceRule::NoneInLawUsesSafetyDefault { .. }
        ) && id.contains("building_distance")
}

pub fn assert_tier_b_and_land_manager_flags_default_off() {
    for pack in crate::packs::all_declared_packs() {
        if pack.tier == crate::packs::Tier::B || pack.requires_land_tenure {
            assert!(
                pack.maintainer_flag_default_off,
                "{:?} must ship with maintainer_flag_default_off",
                pack.id
            );
            assert!(
                pack.flag_id.is_some(),
                "{:?} Tier B / land-manager pack must declare flag_id",
                pack.id
            );
        }
        if let Some(flag) = pack.flag_id {
            assert!(
                pack.maintainer_flag_default_off,
                "flag {flag} on {:?} must default OFF in committed config",
                pack.id
            );
        }
    }
}

pub fn validate_all_builtin_packs() {
    for pack in builtin_enabled_packs() {
        assert_hard_filters_official_only(&pack);
        assert!(
            !pack.legal_basis.is_empty(),
            "pack {:?}: empty legal_basis",
            pack.id
        );
        match pack.distance {
            DistanceRule::NotVerifiedUsesSafetyDefault { label }
            | DistanceRule::NoneInLawUsesSafetyDefault { label } => {
                assert!(
                    label.contains("Navi safety default")
                        && label.contains("not ")
                        && label.contains("law"),
                    "pack {:?}: SafetyConfig-as-default card label must say \
'Navi safety default, not <country> law', got {label}",
                    pack.id
                );
            }
            DistanceRule::SafetyConfigLabeled { .. }
            | DistanceRule::NotApplicable
            | DistanceRule::NotVerifiedDeclines => {}
        }
        let _ = (&pack.distance, &pack.duration, &pack.fire);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packs::{estonia_pack, finland_pack, sweden_pack, SourceQuality};

    #[test]
    fn all_builtin_hard_filters_are_official() {
        validate_all_builtin_packs();
    }

    #[test]
    fn every_tier_b_and_land_manager_flag_defaults_off() {
        assert_tier_b_and_land_manager_flags_default_off();
    }

    #[test]
    #[should_panic(expected = "non-official source")]
    fn hard_filter_backed_only_by_ngo_fails_validator() {
        use super::super::{
            CitedSource, DistanceRule, DurationRule, FireRule, HardFilterSpec, PackId, RulePack,
            SourceQuality, SuggestionMode, Tier,
        };
        let pack = RulePack {
            id: PackId::TierD,
            tier: Tier::A,
            country_iso: "xx".into(),
            legal_basis: "synthetic NGO-only hard filter",
            sources: &[],
            distance: DistanceRule::NotApplicable,
            duration: DurationRule::NotVerified,
            fire: FireRule::NotVerified,
            guidance_notes: &[],
            farmland_not_checked_when_landcover_unknown: false,
            hard_filters: &[HardFilterSpec {
                id: "synthetic_ngo_hard",
                sources: &[CitedSource {
                    url: "https://example.org/ngo",
                    quality: SourceQuality::Ngo,
                }],
            }],
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
        };
        assert_hard_filters_official_only(&pack);
    }

    #[test]
    #[should_panic(expected = "NaviSafetyDefault")]
    fn safety_config_hard_filter_tagged_official_fails_validator() {
        use super::super::{
            CitedSource, DistanceRule, DurationRule, FireRule, HardFilterSpec, PackId, RulePack,
            SourceQuality, SuggestionMode, Tier,
        };
        let pack = RulePack {
            id: PackId::Sweden,
            tier: Tier::A,
            country_iso: "se".into(),
            legal_basis: "synthetic mis-tagged safety default",
            sources: &[],
            distance: DistanceRule::NoneInLawUsesSafetyDefault {
                label: "Navi safety default, not Swedish law",
            },
            duration: DurationRule::NotVerified,
            fire: FireRule::NotVerified,
            guidance_notes: &[],
            farmland_not_checked_when_landcover_unknown: false,
            hard_filters: &[HardFilterSpec {
                id: "building_distance_navi_safety_default",
                sources: &[CitedSource {
                    url: "https://example.org/mislabelled",
                    quality: SourceQuality::Official,
                }],
            }],
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
        };
        assert_hard_filters_official_only(&pack);
    }

    #[test]
    fn se_fi_ee_safety_default_hard_filters_are_navi_not_official() {
        for pack in [sweden_pack(), finland_pack(), estonia_pack()] {
            let safety_hards: Vec<_> = pack
                .hard_filters
                .iter()
                .filter(|hf| hf.id.contains("navi_safety_default"))
                .collect();
            assert!(
                !safety_hards.is_empty(),
                "{:?} must declare a navi_safety_default hard filter",
                pack.id
            );
            for hf in safety_hards {
                for s in hf.sources {
                    assert_eq!(
                        s.quality,
                        SourceQuality::NaviSafetyDefault,
                        "{:?} {} must not be Official",
                        pack.id,
                        hf.id
                    );
                }
            }
            let label = pack.distance_card_label().expect("label");
            assert!(
                label.contains("Navi safety default") && label.contains("not ") && label.contains("law"),
                "{:?} card label must say Navi safety default, not <country> law; got {label}",
                pack.id
            );
        }
    }

    #[test]
    fn estonia_secondary_150m_is_not_a_hard_filter() {
        let p = estonia_pack();
        assert!(p.sources.iter().any(|s| s.quality == SourceQuality::Secondary));
        for hf in p.hard_filters {
            assert!(
                !hf.id.contains("150"),
                "150 m must not be a hard filter id"
            );
            for s in hf.sources {
                assert_ne!(s.quality, SourceQuality::Secondary);
            }
        }
        assert!(
            p.guidance_notes.iter().any(|n| n.contains("re-verify")),
            "secondary 150 m must appear as a re-verify note"
        );
    }
}
