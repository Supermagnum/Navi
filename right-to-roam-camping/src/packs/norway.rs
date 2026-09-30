use super::{
    CitedSource, DistanceRule, DurationRule, FireRule, HardFilterSpec, PackId, RulePack,
    SourceQuality, Tier,
};

pub fn norway_pack() -> RulePack {
    RulePack {
        id: PackId::Norway,
        tier: Tier::A,
        country_iso: "no".into(),
        legal_basis: "Friluftsloven (allemannsretten); Motorferdselloven; forskrift om brannforebygging § 3",
        sources: NORWAY_SOURCES,
        distance: DistanceRule::SafetyConfigLabeled {
            label: "shared SafetyConfig building distance (Norwegian pack default)",
        },
        duration: DurationRule::HardMaxConsecutiveNights {
            nights: 2,
            store_key: "no",
            retention_days: 2,
        },
        fire: FireRule::NorwayDateGated,
        guidance_notes: &[
            "Some rare berry, mushroom, and flower species are protected from picking.",
            "Clean up after yourself (leave no trace).",
        ],
        farmland_not_checked_when_landcover_unknown: false,
        hard_filters: NORWAY_HARD,
        maintainer_flag_default_off: false,
        required_subdivision: None,
        missing_subdivision_fallback: None,
        decline_when_protected_unknown: false,
        cmz: None,
        cloudberry_note: true,
    }
}

const NORWAY_SOURCES: &[CitedSource] = &[
    CitedSource {
        url: "https://lovdata.no/dokument/NL/lov/1957-06-28-16",
        quality: SourceQuality::Official,
    },
    CitedSource {
        url: "https://lovdata.no/dokument/NL/lov/1977-06-10-82",
        quality: SourceQuality::Official,
    },
    CitedSource {
        url: "https://lovdata.no/dokument/SF/forskrift/2015-12-17-1710",
        quality: SourceQuality::Official,
    },
];

const NORWAY_HARD: &[HardFilterSpec] = &[
    HardFilterSpec {
        id: "max_consecutive_nights_2",
        sources: &[CitedSource {
            url: "https://lovdata.no/dokument/NL/lov/1957-06-28-16",
            quality: SourceQuality::Official,
        }],
    },
    HardFilterSpec {
        id: "building_distance_safety_config",
        sources: &[CitedSource {
            url: "https://lovdata.no/dokument/NL/lov/1957-06-28-16",
            quality: SourceQuality::Official,
        }],
    },
];
