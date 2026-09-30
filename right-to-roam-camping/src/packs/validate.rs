use super::{builtin_enabled_packs, RulePack, SourceQuality};

/// HARD filters may only cite official sources. NGO / secondary → test failure.
pub fn assert_hard_filters_official_only(pack: &RulePack) {
    for hf in pack.hard_filters {
        assert!(
            !hf.sources.is_empty(),
            "pack {:?}: hard filter {} has no sources",
            pack.id,
            hf.id
        );
        for src in hf.sources {
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

pub fn validate_all_builtin_packs() {
    for pack in builtin_enabled_packs() {
        assert_hard_filters_official_only(&pack);
        assert!(
            !pack.legal_basis.is_empty(),
            "pack {:?}: empty legal_basis",
            pack.id
        );
        // Tier A packs must declare distance/duration/fire explicitly (enum variants).
        let _ = (&pack.distance, &pack.duration, &pack.fire);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packs::{estonia_pack, SourceQuality};

    #[test]
    fn all_builtin_hard_filters_are_official() {
        validate_all_builtin_packs();
    }

    #[test]
    #[should_panic(expected = "non-official source")]
    fn hard_filter_backed_only_by_ngo_fails_validator() {
        use super::super::{
            CitedSource, DistanceRule, DurationRule, FireRule, HardFilterSpec, PackId, RulePack,
            SourceQuality, Tier,
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
        };
        assert_hard_filters_official_only(&pack);
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
