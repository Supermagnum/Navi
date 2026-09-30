use super::{
    CitedSource, DistanceRule, DurationRule, FireRule, HardFilterSpec, PackId, RulePack,
    SourceQuality, Tier,
};

/// Iceland Tier A with Phase 3a exception: while protected_area_query is unknown,
/// decline (campsites only). Many popular areas ban camping outright.
pub fn iceland_pack() -> RulePack {
    RulePack {
        id: PackId::Iceland,
        tier: Tier::A,
        country_iso: "is".into(),
        legal_basis: "Nature Conservation Act no. 60/2013 — camping provisions",
        sources: ICELAND_SOURCES,
        distance: DistanceRule::NotVerifiedUsesSafetyDefault {
            label: "Navi safety default — Icelandic statutory metres not verified",
        },
        duration: DurationRule::SoftGuidance {
            note: "Along public routes in inhabited areas: one night, traditional tent only, \
on uncultivated land if no campsite nearby and no signs prohibit it. Uninhabited areas: \
allowed unless special rules apply.",
        },
        fire: FireRule::GuidanceNote {
            text: "Follow local rules; landowner permission is needed near dwellings/farms \
and for more than one night or more than three tents.",
        },
        guidance_notes: &[
            "Decline in protected areas unless the host has per-area rules.",
            "Many areas ban camping outright or restrict it to marked sites \
(e.g. Þingvellir, Hornstrandir, Mývatn, parts of Vatnajökull).",
            "Campervans, tent trailers and caravans: never outside campsites or urban areas \
without permission.",
            "While protected-area status is unknown, this pack declines wild-camp suggestions \
(campsites only) — Iceland exception to the Tier A 'not checked' note pattern.",
            "Clean up after yourself (leave no trace).",
        ],
        farmland_not_checked_when_landcover_unknown: true,
        hard_filters: ICELAND_HARD,
        maintainer_flag_default_off: false,
        required_subdivision: None,
        missing_subdivision_fallback: None,
        decline_when_protected_unknown: true,
        cmz: None,
        cloudberry_note: false,
    }
}

const ICELAND_SOURCES: &[CitedSource] = &[CitedSource {
    url: "https://ust.is/english/visiting-iceland/travel-information/where-can-you-camp/",
    quality: SourceQuality::Official,
}];

const ICELAND_HARD: &[HardFilterSpec] = &[
    HardFilterSpec {
        id: "decline_when_protected_unknown",
        sources: &[CitedSource {
            url: "https://ust.is/english/visiting-iceland/travel-information/where-can-you-camp/",
            quality: SourceQuality::Official,
        }],
    },
    HardFilterSpec {
        id: "building_distance_navi_safety_default",
        sources: &[CitedSource {
            url: "https://ust.is/english/visiting-iceland/travel-information/where-can-you-camp/",
            quality: SourceQuality::Official,
        }],
    },
];
