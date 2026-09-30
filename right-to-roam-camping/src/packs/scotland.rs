use super::{
    CitedSource, CmzPolicy, DistanceRule, DurationRule, FireRule, HardFilterSpec, PackId,
    RulePack, SourceQuality, Tier,
};

pub fn scotland_pack() -> RulePack {
    RulePack {
        id: PackId::Scotland,
        tier: Tier::A,
        country_iso: "gb".into(),
        legal_basis: "Scottish Outdoor Access Code — responsible camping",
        sources: SCOTLAND_SOURCES,
        distance: DistanceRule::NotVerifiedUsesSafetyDefault {
            label: "Navi safety default, not Scottish law",
        },
        duration: DurationRule::SoftGuidance {
            note: "Lightweight, small numbers, max 2–3 nights in one place.",
        },
        fire: FireRule::GuidanceNote {
            text: "Follow the Scottish Outdoor Access Code; local fire bans may apply.",
        },
        guidance_notes: &[
            "Not in enclosed fields of crops or animals.",
            "Keep well away from buildings, roads and historic structures.",
            "Ask permission to camp close to a house.",
            "Vehicle-based camping is not covered by this pack.",
            "Loch Lomond & Trossachs Camping Management Zones need a permit or campsite \
1 Mar–30 Sep — without a CMZ polygon layer this pack will not suggest spots it cannot \
prove are outside the CMZ.",
            "Clean up after yourself (leave no trace).",
        ],
        farmland_not_checked_when_landcover_unknown: true,
        hard_filters: SCOTLAND_HARD,
        maintainer_flag_default_off: false,
        required_subdivision: Some("GB-SCT"),
        missing_subdivision_fallback: Some(Tier::D),
        decline_when_protected_unknown: false,
        cmz: Some(CmzPolicy {
            season_start_month: 3,
            season_start_day: 1,
            season_end_month: 9,
            season_end_day: 30,
        }),
        cloudberry_note: false,
    }
}

const SCOTLAND_SOURCES: &[CitedSource] = &[
    CitedSource {
        url: "https://www.outdooraccess-scotland.scot/practical-guide-all/camping",
        quality: SourceQuality::Official,
    },
    CitedSource {
        url: "https://www.lochlomond-trossachs.org/things-to-do/camping/go-wild",
        quality: SourceQuality::Official,
    },
];

const SCOTLAND_HARD: &[HardFilterSpec] = &[
    HardFilterSpec {
        id: "building_distance_navi_safety_default",
        sources: &[CitedSource {
            url: "navi:safety_config/min_building_distance_m",
            quality: SourceQuality::NaviSafetyDefault,
        }],
    },
    HardFilterSpec {
        id: "cmz_unproven_outside_in_season",
        sources: &[CitedSource {
            url: "https://www.lochlomond-trossachs.org/things-to-do/camping/go-wild",
            quality: SourceQuality::Official,
        }],
    },
];
