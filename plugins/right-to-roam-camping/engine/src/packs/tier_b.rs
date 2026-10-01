//! Tier B Europe — data present, maintainer flags default OFF.

use super::{
    CitedSource, DesignatedLayer, DistanceRule, DurationRule, FireRule, HardFilterSpec,
    HostCondition, PackId, RulePack, SourceQuality, SuggestionMode, Tier,
};

fn tier_b(
    id: PackId,
    country: &str,
    flag: &'static str,
    legal: &'static str,
    sources: &'static [CitedSource],
    notes: &'static [&'static str],
    conditions: &'static [HostCondition],
    fallback: Tier,
    layer: Option<DesignatedLayer>,
    distance: DistanceRule,
    hard: &'static [HardFilterSpec],
) -> RulePack {
    RulePack {
        id,
        tier: Tier::B,
        country_iso: country.into(),
        legal_basis: legal,
        sources,
        distance,
        duration: DurationRule::HardMaxConsecutiveNights {
            nights: 1,
            store_key: flag,
            retention_days: 1,
        },
        fire: FireRule::GuidanceNote {
            text: "Follow local fire bans; municipal rules may be stricter.",
        },
        guidance_notes: notes,
        farmland_not_checked_when_landcover_unknown: false,
        hard_filters: hard,
        maintainer_flag_default_off: true,
        required_subdivision: None,
        missing_subdivision_fallback: None,
        decline_when_protected_unknown: false,
        cmz: None,
        cloudberry_note: false,
        suggestion_mode: SuggestionMode::WildCamp,
        flag_id: Some(flag),
        designated_layer: layer,
        host_conditions: conditions,
        flag_off_fallback: Some(fallback),
        conditions_unmet_fallback: Some(fallback),
        requires_land_tenure: false,
        stay_policy: None,
        secondary_card_notes: &[],
    }
}

const DE_COND: &[HostCondition] = &[
    HostCondition::NonMotorisedTravel,
    HostCondition::NotForest,
    HostCondition::NotProtectedArea,
    HostCondition::NotResidential,
];

pub fn de_brandenburg_pack() -> RulePack {
    tier_b(
        PackId::DeBrandenburg,
        "de",
        "tier_b_de_brandenburg",
        "Brandenburg § 22 (1) BbgNatSchAG — one night in open landscape (non-motorised)",
        &[CitedSource {
            url: "https://bravors.brandenburg.de/gesetze/bbgnatschag",
            quality: SourceQuality::Official,
        }],
        &[
            "One night in freie Landschaft for walkers/cyclists/riders/paddlers.",
            "Not in gardens, farmyards or residential grounds; forest excluded.",
            "Show landowner-consent guidance (county practice may require consent).",
            "Flag default OFF until every host condition is checkable.",
        ],
        DE_COND,
        Tier::C,
        Some(DesignatedLayer::Trekkingplaetze),
        DistanceRule::NotApplicable,
        &[HardFilterSpec {
            id: "max_consecutive_nights_1",
            sources: &[CitedSource {
                url: "https://bravors.brandenburg.de/gesetze/bbgnatschag",
                quality: SourceQuality::Official,
            }],
        }],
    )
}

pub fn de_mv_pack() -> RulePack {
    tier_b(
        PackId::DeMecklenburgVorpommern,
        "de",
        "tier_b_de_mv",
        "Mecklenburg-Vorpommern § 28 (2) NatSchAG M-V — one night, non-motorised, open landscape",
        &[CitedSource {
            url: "https://www.landesrecht-mv.de/bsmv/document/jlr-NatSchGMV2010pP28",
            quality: SourceQuality::Official,
        }],
        &[
            "One night for non-motorised walkers in open landscape.",
            "Excluded: national parks, NSG, forest, dunes/beach ridges/dikes.",
            "Flag default OFF until every host condition is checkable.",
        ],
        DE_COND,
        Tier::C,
        Some(DesignatedLayer::Trekkingplaetze),
        DistanceRule::NotApplicable,
        &[HardFilterSpec {
            id: "max_consecutive_nights_1",
            sources: &[CitedSource {
                url: "https://www.landesrecht-mv.de/bsmv/document/jlr-NatSchGMV2010pP28",
                quality: SourceQuality::Official,
            }],
        }],
    )
}

pub fn de_sh_pack() -> RulePack {
    tier_b(
        PackId::DeSchleswigHolstein,
        "de",
        "tier_b_de_sh",
        "Schleswig-Holstein — walkers may camp one night away from campsites",
        &[CitedSource {
            url: "https://www.gesetze-im-internet.de/abweichendes_Landesrecht/natschg_sh__57.html",
            quality: SourceQuality::Official,
        }],
        &[
            "One night for walkers; longer is an offence.",
            "Flag default OFF until every host condition is checkable.",
        ],
        DE_COND,
        Tier::C,
        Some(DesignatedLayer::Trekkingplaetze),
        DistanceRule::NotApplicable,
        &[HardFilterSpec {
            id: "max_consecutive_nights_1",
            sources: &[CitedSource {
                url: "https://www.gesetze-im-internet.de/abweichendes_Landesrecht/natschg_sh__57.html",
                quality: SourceQuality::Official,
            }],
        }],
    )
}

pub fn at_above_treeline_pack() -> RulePack {
    tier_b(
        PackId::AtAboveTreeline,
        "at",
        "tier_b_at_above_treeline",
        "Austria (OÖ/Sbg/Stmk/Vbg) — above treeline only; not in protected areas",
        &[
            CitedSource {
                url: "https://www.bmluk.gv.at/themen/wald/wald-freizeit/verhalten_wald/lagern_zelten_wohnen.html",
                quality: SourceQuality::Official,
            },
            CitedSource {
                url: "https://www.alpenverein.at/portal/service/presse/2023/2023_06_22-Wildcampen.php",
                quality: SourceQuality::Ngo,
            },
        ],
        &[
            "Above treeline only; forest camping needs owner consent nationwide.",
            "Municipal rules may apply.",
            "Flag default OFF; without treeline/protected layers → Tier D fallback.",
        ],
        &[
            HostCondition::AboveTreeline,
            HostCondition::NotProtectedArea,
            HostCondition::NotForest,
        ],
        Tier::D,
        None,
        DistanceRule::NotApplicable,
        &[],
    )
}

pub fn ch_above_treeline_pack() -> RulePack {
    tier_b(
        PackId::ChAboveTreeline,
        "ch",
        "tier_b_ch_above_treeline",
        "Switzerland — single night above treeline generally tolerated (Art. 699 ZGB practice)",
        &[CitedSource {
            url: "https://www.fedlex.admin.ch/eli/cc/24/233_245_233/de#art_699",
            quality: SourceQuality::Official,
        }],
        &[
            "Above treeline; exclude Swiss National Park, hunting reserves, wildlife rest zones.",
            "Below treeline → Tier C designated sites.",
            "Flag default OFF.",
        ],
        &[
            HostCondition::AboveTreeline,
            HostCondition::NotProtectedArea,
        ],
        Tier::C,
        None,
        DistanceRule::NotApplicable,
        &[],
    )
}

pub fn lv_state_forest_pack() -> RulePack {
    tier_b(
        PackId::LvStateForest,
        "lv",
        "tier_b_lv_state_forest",
        "Latvia — state forest stay (Forest Law art. 5); LVM land layer required",
        &[CitedSource {
            url: "https://www.vestnesis.lv/ta/id/2825",
            quality: SourceQuality::Official,
        }],
        &[
            "State/municipal forest; without LVM layer → Tier C (LVM rest sites / TentSite).",
            "Flag default OFF.",
        ],
        &[HostCondition::DesignatedLayerReady(
            DesignatedLayer::LvmStateForest,
        )],
        Tier::C,
        Some(DesignatedLayer::LvmStateForest),
        DistanceRule::NotApplicable,
        &[],
    )
}

pub fn dartmoor_commons_pack() -> RulePack {
    tier_b(
        PackId::DartmoorCommons,
        "gb",
        "tier_b_dartmoor_commons",
        "Dartmoor Commons — backpack camping under s.10(1) Dartmoor Commons Act 1985 (Darwall)",
        &[
            CitedSource {
                url: "https://www.supremecourt.uk/cases/judgments/uksc-2023-0126",
                quality: SourceQuality::Official,
            },
            CitedSource {
                url: "https://www.dartmoor.gov.uk/enjoy-dartmoor/outdoor-activities/camping",
                quality: SourceQuality::Official,
            },
        ],
        &[
            "Requires Dartmoor commons polygon; without it → Tier C (England/Wales designated).",
            "National park byelaws apply.",
            "Flag default OFF.",
        ],
        &[HostCondition::DesignatedLayerReady(
            DesignatedLayer::DartmoorCommons,
        )],
        Tier::C,
        Some(DesignatedLayer::DartmoorCommons),
        DistanceRule::NotApplicable,
        &[],
    )
}

/// Écrins: distance not verified → decline (do not invent one-hour walk metres).
pub fn ecrins_core_pack() -> RulePack {
    RulePack {
        id: PackId::EcrinsCore,
        tier: Tier::B,
        country_iso: "fr".into(),
        legal_basis: "Écrins core bivouac order 2026 — distance from access not verified → decline",
        sources: &[
            CitedSource {
                url: "https://www.ecrins-parcnational.fr/sites/ecrins-parcnational.com/files/article/27274/2606148arretebivouacvf1.pdf",
                quality: SourceQuality::Official,
            },
        ],
        distance: DistanceRule::NotVerifiedDeclines,
        duration: DurationRule::HardMaxConsecutiveNights {
            nights: 1,
            store_key: "tier_b_ecrins_core",
            retention_days: 1,
        },
        fire: FireRule::GuidanceNote {
            text: "Follow park fire rules; daytime tent left up counts as prohibited campement.",
        },
        guidance_notes: &[
            "Bivouac after 19:00, packed before 09:00; one night; small tent only.",
            "Access-distance rule not verified for 2026 order → pack declines wild-camp suggestions.",
            "Flag default OFF.",
        ],
        farmland_not_checked_when_landcover_unknown: false,
        hard_filters: &[HardFilterSpec {
            id: "distance_not_verified_declines",
            sources: &[CitedSource {
                url: "https://www.ecrins-parcnational.fr/sites/ecrins-parcnational.com/files/article/27274/2606148arretebivouacvf1.pdf",
                quality: SourceQuality::Official,
            }],
        }],
        maintainer_flag_default_off: true,
        required_subdivision: None,
        missing_subdivision_fallback: None,
        decline_when_protected_unknown: false,
        cmz: None,
        cloudberry_note: false,
        suggestion_mode: SuggestionMode::DeclineCampsitesGuidance,
        flag_id: Some("tier_b_ecrins_core"),
        designated_layer: Some(DesignatedLayer::EcrinsCore),
        host_conditions: &[HostCondition::DesignatedLayerReady(DesignatedLayer::EcrinsCore)],
        flag_off_fallback: Some(Tier::C),
        conditions_unmet_fallback: Some(Tier::C),
        requires_land_tenure: false,
        stay_policy: None,
        secondary_card_notes: &[],
    }
}

pub fn dk_fri_teltning_pack() -> RulePack {
    tier_b(
        PackId::DkFriTeltning,
        "dk",
        "tier_b_dk_fri_teltning",
        "Denmark fri-teltning — 1 night / 2 tents / 3 people; under trees in designated state forests",
        &[CitedSource {
            url: "https://naturstyrelsen.dk/aktiviteter-i-naturen/overnat-og-spis-i-naturen/fri-teltning",
            quality: SourceQuality::Official,
        }],
        &[
            "Only inside fri-teltning forests; not on beaches/dunes/meadows/clearings.",
            "Without fri-teltning layer → Tier C TentSite/shelters only.",
            "Flag default OFF.",
        ],
        &[HostCondition::DesignatedLayerReady(DesignatedLayer::FriTeltning)],
        Tier::C,
        Some(DesignatedLayer::FriTeltning),
        DistanceRule::NotApplicable,
        &[HardFilterSpec {
            id: "max_consecutive_nights_1",
            sources: &[CitedSource {
                url: "https://naturstyrelsen.dk/aktiviteter-i-naturen/overnat-og-spis-i-naturen/fri-teltning",
                quality: SourceQuality::Official,
            }],
        }],
    )
}

pub fn all_tier_b_packs() -> Vec<RulePack> {
    vec![
        de_brandenburg_pack(),
        de_mv_pack(),
        de_sh_pack(),
        at_above_treeline_pack(),
        ch_above_treeline_pack(),
        lv_state_forest_pack(),
        dartmoor_commons_pack(),
        ecrins_core_pack(),
        dk_fri_teltning_pack(),
    ]
}

pub fn tier_b_pack_by_flag(flag: &str) -> Option<RulePack> {
    all_tier_b_packs()
        .into_iter()
        .find(|p| p.flag_id == Some(flag))
}
