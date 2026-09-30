use super::{
    CitedSource, DistanceRule, DurationRule, FireRule, HardFilterSpec, PackId, RulePack,
    SourceQuality, Tier,
};

pub fn estonia_pack() -> RulePack {
    RulePack {
        id: PackId::Estonia,
        tier: Tier::A,
        country_iso: "ee".into(),
        legal_basis: "Freedom to roam / General Part of the Environmental Code Act",
        sources: ESTONIA_SOURCES,
        // 150 m figure is secondary (Postimees) → NOT a hard rule.
        distance: DistanceRule::NotVerifiedUsesSafetyDefault {
            label: "Navi safety default — Estonian 150 m figure is secondary (re-verify)",
        },
        duration: DurationRule::HardMaxConsecutiveNights {
            nights: 1,
            store_key: "ee",
            retention_days: 1,
        },
        fire: FireRule::GuidanceNote {
            text: "Follow local fire rules; longer stays need landowner permission.",
        },
        guidance_notes: &[
            "Camping for one day (24 h) on unfenced, unsigned land; longer needs landowner permission.",
            "Pitch out of sight and hearing of dwellings.",
            "Secondary source cited ≥150 m on open terrain — re-verify against the Code; \
not applied as a hard metre rule.",
            "Clean up after yourself (leave no trace).",
        ],
        farmland_not_checked_when_landcover_unknown: false,
        hard_filters: ESTONIA_HARD,
        maintainer_flag_default_off: false,
        required_subdivision: None,
        missing_subdivision_fallback: None,
        decline_when_protected_unknown: false,
        cmz: None,
        cloudberry_note: false,
    }
}

const ESTONIA_SOURCES: &[CitedSource] = &[
    CitedSource {
        url: "https://rmk.ee/en/exploring-nature/rules-of-conduct/freedom-to-roam/",
        quality: SourceQuality::Official,
    },
    CitedSource {
        url: "https://www.riigiteataja.ee/en/eli/523122024010/consolide",
        quality: SourceQuality::Official,
    },
    // Secondary 150 m report — notes only; must not appear in hard_filters.
    CitedSource {
        url: "https://www.postimees.ee/",
        quality: SourceQuality::Secondary,
    },
];

const ESTONIA_HARD: &[HardFilterSpec] = &[
    HardFilterSpec {
        id: "max_consecutive_nights_1",
        sources: &[CitedSource {
            url: "https://rmk.ee/en/exploring-nature/rules-of-conduct/freedom-to-roam/",
            quality: SourceQuality::Official,
        }],
    },
    HardFilterSpec {
        id: "building_distance_navi_safety_default",
        sources: &[CitedSource {
            url: "https://rmk.ee/en/exploring-nature/rules-of-conduct/freedom-to-roam/",
            quality: SourceQuality::Official,
        }],
    },
];
