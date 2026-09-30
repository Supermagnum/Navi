//! Tier C Europe — designated TentSite POIs only; no wild-camp algorithm.

use super::{
    CitedSource, DesignatedLayer, DistanceRule, DurationRule, FireRule, PackId, RulePack,
    SourceQuality, SuggestionMode, Tier,
};

fn tier_c(
    id: PackId,
    country: &str,
    legal: &'static str,
    sources: &'static [CitedSource],
    notes: &'static [&'static str],
    layer: Option<DesignatedLayer>,
) -> RulePack {
    RulePack {
        id,
        tier: Tier::C,
        country_iso: country.into(),
        legal_basis: legal,
        sources,
        distance: DistanceRule::NotApplicable,
        duration: DurationRule::NotVerified,
        fire: FireRule::NotVerified,
        guidance_notes: notes,
        farmland_not_checked_when_landcover_unknown: false,
        hard_filters: &[],
        maintainer_flag_default_off: false,
        required_subdivision: None,
        missing_subdivision_fallback: None,
        decline_when_protected_unknown: false,
        cmz: None,
        cloudberry_note: false,
        suggestion_mode: SuggestionMode::DesignatedTentSitesOnly,
        flag_id: None,
        designated_layer: layer,
        host_conditions: &[],
        flag_off_fallback: None,
        conditions_unmet_fallback: None,
        requires_land_tenure: false,
        stay_policy: None,
        secondary_card_notes: &[],
    }
}

pub fn england_wales_pack() -> RulePack {
    tier_c(
        PackId::EnglandWales,
        "gb",
        "England/Wales — no general wild-camp right after Darwall; designated sites only",
        &[CitedSource {
            url: "https://www.supremecourt.uk/cases/judgments/uksc-2023-0126",
            quality: SourceQuality::Official,
        }],
        &[
            "Wild camping not a general right in England/Wales.",
            "Suggest only designated/legal campsites present in TentSite POI data.",
            "Dartmoor commons are a separate Tier B pack (flag OFF).",
        ],
        None,
    )
}

pub fn denmark_pack() -> RulePack {
    tier_c(
        PackId::Denmark,
        "dk",
        "Denmark — no general wild-camp right; designated sites / fri-teltning special case",
        DK,
        &[
            "Wild camping is not a general right in Denmark.",
            "Suggest TentSite POIs only unless the fri-teltning Tier B flag and layer are on.",
        ],
        None,
    )
}

pub fn netherlands_pack() -> RulePack {
    tier_c(
        PackId::Netherlands,
        "nl",
        "Netherlands — wild camping not allowed; remaining paalkamp only when in POI data",
        NL,
        &[
            "Wild camping not allowed.",
            "Staatsbosbeheer paalkampeerterreinen closed (2020).",
            "Suggest TentSite POIs only; paalkamp polygon layer not classified → empty if that layer were required.",
        ],
        Some(DesignatedLayer::Paalkamp),
    )
}

pub fn belgium_flanders_pack() -> RulePack {
    tier_c(
        PackId::BelgiumFlanders,
        "be",
        "Flanders — wild camping not allowed; bivakzones only",
        BE,
        &[
            "Wild camping not allowed in Flanders.",
            "Bivakzones only; without a classified bivakzones layer the result is empty.",
        ],
        Some(DesignatedLayer::Bivakzones),
    )
}

pub fn poland_pack() -> RulePack {
    tier_c(
        PackId::Poland,
        "pl",
        "Poland — ban outside designated places; Zanocuj w lesie when layer classified",
        PL,
        &[
            "Camping outside designated places is banned.",
            "Without a classified Zanocuj w lesie layer, suggest only TentSite POIs (empty if none).",
        ],
        Some(DesignatedLayer::ZanocujWLesie),
    )
}

pub fn czechia_pack() -> RulePack {
    tier_c(
        PackId::Czechia,
        "cz",
        "Czechia — no camping in forests outside designated places (Zákon 289/1995)",
        CZ,
        &["Designated places / TentSite POIs only; no wild-camp algorithm."],
        None,
    )
}

pub fn lithuania_pack() -> RulePack {
    tier_c(
        PackId::Lithuania,
        "lt",
        "Lithuania — tents only at campsites / marked tent sites",
        LT,
        &["TentSite POIs only."],
        None,
    )
}

pub fn ireland_pack() -> RulePack {
    tier_c(
        PackId::Ireland,
        "ie",
        "Ireland — designated recreation sites only (Coillte policy)",
        IE,
        &["TentSite POIs only; no wild-camp algorithm."],
        None,
    )
}

pub fn france_tents_pack() -> RulePack {
    tier_c(
        PackId::FranceTents,
        "fr",
        "France — R111-32 requires land-user consent; no general wild-camp suggestions",
        FR,
        &[
            "Camping outside campsites needs consent the plugin cannot verify.",
            "Suggest TentSite POIs only. Écrins core is a separate Tier B pack (flag OFF; distance not verified → decline).",
        ],
        None,
    )
}

pub fn germany_other_lander_pack() -> RulePack {
    tier_c(
        PackId::GermanyOtherLander,
        "de",
        "German Länder (non-BB/MV/SH) — Trekkingplätze / Biwakplätze only",
        DE,
        &[
            "Federal access rights are not a right to camp.",
            "Without a classified Trekkingplätze layer, TentSite POIs only (empty if none).",
        ],
        Some(DesignatedLayer::Trekkingplaetze),
    )
}

pub fn austria_karnten_noe_tirol_pack() -> RulePack {
    tier_c(
        PackId::AustriaKarntenNoeTirol,
        "at",
        "Kärnten / NÖ / Tirol — camping outside campsites not allowed (above treeline also C)",
        AT,
        &["Designated campsites / TentSite POIs only."],
        None,
    )
}

pub fn all_tier_c_packs() -> Vec<RulePack> {
    vec![
        england_wales_pack(),
        denmark_pack(),
        netherlands_pack(),
        belgium_flanders_pack(),
        poland_pack(),
        czechia_pack(),
        lithuania_pack(),
        ireland_pack(),
        france_tents_pack(),
        germany_other_lander_pack(),
        austria_karnten_noe_tirol_pack(),
    ]
}

const DK: &[CitedSource] = &[
    CitedSource {
        url: "https://naturstyrelsen.dk/aktiviteter-i-naturen/overnat-og-spis-i-naturen/fri-teltning",
        quality: SourceQuality::Official,
    },
    CitedSource {
        url: "https://naturstyrelsen.dk/aktiviteter-i-naturen/overnat-og-spis-i-naturen",
        quality: SourceQuality::Official,
    },
];
const NL: &[CitedSource] = &[CitedSource {
    url: "https://www.staatsbosbeheer.nl/wat-we-doen/nieuws/2020/05/sluiten-paalkampeerterreinen",
    quality: SourceQuality::Official,
}];
const BE: &[CitedSource] = &[CitedSource {
    url: "https://natuurenbos.vlaanderen.be/faq/mag-ik-kamperen-een-bos-natuurgebied",
    quality: SourceQuality::Official,
}];
const PL: &[CitedSource] = &[
    CitedSource {
        url: "https://zanocujwlesie.lasy.gov.pl/",
        quality: SourceQuality::Official,
    },
    CitedSource {
        url: "https://isap.sejm.gov.pl/isap.nsf/DocDetails.xsp?id=WDU20250000567",
        quality: SourceQuality::Official,
    },
];
const CZ: &[CitedSource] = &[CitedSource {
    url: "https://www.e-sbirka.cz/sb/1995/289",
    quality: SourceQuality::Official,
}];
const LT: &[CitedSource] = &[CitedSource {
    url: "https://aad.lrv.lt/en/memos-on-environmental-requirements/camping-responsibly/",
    quality: SourceQuality::Official,
}];
const IE: &[CitedSource] = &[CitedSource {
    url: "https://www.coillte.ie/media/2019/07/Coillte-Recreation-Policy.pdf",
    quality: SourceQuality::Official,
}];
const FR: &[CitedSource] = &[
    CitedSource {
        url: "https://www.legifrance.gouv.fr/codes/article_lc/LEGIARTI000031721244",
        quality: SourceQuality::Official,
    },
    CitedSource {
        url: "https://www.legifrance.gouv.fr/codes/article_lc/LEGIARTI000034355031",
        quality: SourceQuality::Official,
    },
];
const DE: &[CitedSource] = &[
    CitedSource {
        url: "https://www.gesetze-im-internet.de/bnatschg_2009/__59.html",
        quality: SourceQuality::Official,
    },
    CitedSource {
        url: "https://www.gesetze-im-internet.de/bwaldg/__14.html",
        quality: SourceQuality::Official,
    },
];
const AT: &[CitedSource] = &[
    CitedSource {
        url: "https://www.bmluk.gv.at/themen/wald/wald-freizeit/verhalten_wald/lagern_zelten_wohnen.html",
        quality: SourceQuality::Official,
    },
    CitedSource {
        url: "https://www.alpenverein.at/portal/service/presse/2023/2023_06_22-Wildcampen.php",
        quality: SourceQuality::Ngo,
    },
];
