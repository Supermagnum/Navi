//! Static land-neighbour table for catalog countries → ORS `avoid_countries` ids.
//!
//! OpenRouteService documents `options.avoid_countries` as an **integer array of
//! country ids** (see https://giscience.github.io/openrouteservice/technical-details/country-list),
//! not ISO alpha-2/3 strings. We map ISO-3166-1 alpha-2 → ORS id here.

use std::collections::{BTreeMap, BTreeSet};

/// ORS numeric country id (country-list docs, retrieved 2026-09-19).
pub fn ors_country_id(iso_alpha2: &str) -> Option<u32> {
    match iso_alpha2.trim().to_ascii_lowercase().as_str() {
        "af" => Some(1),
        "al" => Some(2),
        "dz" => Some(3),
        "ad" => Some(4),
        "ao" => Some(5),
        "ar" => Some(8),
        "am" => Some(9),
        "au" => Some(10),
        "at" => Some(11),
        "az" => Some(12),
        "by" => Some(16),
        "be" => Some(17),
        "ba" => Some(23),
        "br" => Some(25),
        "bg" => Some(30),
        "ca" => Some(35),
        "cl" => Some(40),
        "cn" => Some(41),
        "co" => Some(42),
        "hr" => Some(49),
        "cz" => Some(52),
        "dk" => Some(53),
        "ee" => Some(63),
        "fi" => Some(69),
        "fr" => Some(70),
        "de" => Some(74),
        "gr" => Some(78),
        "hu" => Some(88),
        "is" => Some(89),
        "in" => Some(90),
        "ie" => Some(94),
        "it" => Some(97),
        "jp" => Some(100),
        "lv" => Some(110),
        "li" => Some(115),
        "lt" => Some(116),
        "lu" => Some(117),
        "mx" => Some(128),
        "md" => Some(129),
        "me" => Some(132),
        "ma" => Some(134),
        "nl" => Some(200),
        "nz" => Some(142),
        "no" => Some(148),
        "pl" => Some(159),
        "pt" => Some(160),
        "ro" => Some(162),
        "ru" => Some(163),
        "rs" => Some(175),
        "sk" => Some(179),
        "si" => Some(180),
        "za" => Some(183),
        "kr" => Some(185),
        "es" => Some(187),
        "se" => Some(192),
        "ch" => Some(193),
        "tr" => Some(206),
        "ua" => Some(211),
        "ae" => Some(212),
        "gb" | "uk" => Some(213),
        "us" => Some(214),
        _ => None,
    }
}

/// Undirected land neighbours among countries that appear in the Navi catalog
/// footprint (Europe + North America focus). Pairs are stored once; lookup is
/// symmetric.
fn neighbour_pairs() -> &'static [(&'static str, &'static str)] {
    &[
        ("no", "se"),
        ("no", "fi"),
        ("no", "ru"),
        ("se", "fi"),
        ("se", "dk"),
        ("dk", "de"),
        ("de", "nl"),
        ("de", "be"),
        ("de", "lu"),
        ("de", "fr"),
        ("de", "ch"),
        ("de", "at"),
        ("de", "cz"),
        ("de", "pl"),
        ("de", "dk"),
        ("nl", "be"),
        ("be", "lu"),
        ("be", "fr"),
        ("fr", "ch"),
        ("fr", "it"),
        ("fr", "es"),
        ("fr", "lu"),
        ("fr", "be"),
        ("fr", "de"),
        ("ch", "at"),
        ("ch", "it"),
        ("ch", "fr"),
        ("ch", "de"),
        ("at", "it"),
        ("at", "si"),
        ("at", "hu"),
        ("at", "sk"),
        ("at", "cz"),
        ("at", "de"),
        ("at", "ch"),
        ("it", "si"),
        ("it", "at"),
        ("it", "ch"),
        ("it", "fr"),
        ("pl", "cz"),
        ("pl", "sk"),
        ("pl", "ua"),
        ("pl", "by"),
        ("pl", "lt"),
        ("pl", "ru"),
        ("pl", "de"),
        ("cz", "sk"),
        ("cz", "at"),
        ("cz", "pl"),
        ("cz", "de"),
        ("fi", "ru"),
        ("fi", "se"),
        ("fi", "no"),
        ("us", "ca"),
        ("us", "mx"),
        ("ca", "us"),
        ("mx", "us"),
    ]
}

/// Land neighbours of `iso` (lowercase alpha-2).
pub fn land_neighbours_iso(iso: &str) -> Vec<&'static str> {
    let key = iso.trim().to_ascii_lowercase();
    let mut out = BTreeSet::new();
    for &(a, b) in neighbour_pairs() {
        if a == key {
            out.insert(b);
        } else if b == key {
            out.insert(a);
        }
    }
    out.into_iter().collect()
}

/// ORS `avoid_countries` ids derived from the complement of `allowed` ISO codes:
/// every land neighbour of each allowed country that is **not** itself allowed.
pub fn avoid_country_ids_for_allowed(allowed: &[String]) -> Vec<u32> {
    let allowed_set: BTreeSet<String> = allowed
        .iter()
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    let mut ids = BTreeSet::new();
    for iso in &allowed_set {
        for nb in land_neighbours_iso(iso) {
            if allowed_set.contains(nb) {
                continue;
            }
            if let Some(id) = ors_country_id(nb) {
                ids.insert(id);
            }
        }
    }
    ids.into_iter().collect()
}

/// Test helper: every listed pair appears in both directions via lookup.
pub fn neighbour_table_is_symmetric() -> bool {
    let mut map: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for iso in [
        "no", "se", "fi", "ru", "dk", "de", "us", "ca", "mx", "fr", "ch",
    ] {
        for nb in land_neighbours_iso(iso) {
            map.entry(iso).or_default().insert(nb);
        }
    }
    for (a, nbs) in &map {
        for b in nbs {
            if !land_neighbours_iso(b).contains(a) {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_symmetric() {
        assert!(neighbour_table_is_symmetric());
    }

    #[test]
    fn norway_avoids_se_fi_ru() {
        let ids = avoid_country_ids_for_allowed(&["no".into()]);
        let mut expected = vec![
            ors_country_id("se").unwrap(),
            ors_country_id("fi").unwrap(),
            ors_country_id("ru").unwrap(),
        ];
        expected.sort_unstable();
        assert_eq!(ids, expected);
    }

    #[test]
    fn us_avoids_ca_mx() {
        let ids = avoid_country_ids_for_allowed(&["us".into()]);
        let mut expected = vec![ors_country_id("ca").unwrap(), ors_country_id("mx").unwrap()];
        expected.sort_unstable();
        assert_eq!(ids, expected);
    }
}
