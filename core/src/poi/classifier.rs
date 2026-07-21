use std::collections::HashMap;

use super::PoiCategory;

const NETWORK_TAGS: &[&str] = &[
    "DNT",
    "STF",
    "DAV",
    "SAC",
    "OeAV",
    "Metsähallitus",
    "Metsahallitus",
];

/// Classify OSM tags into POI categories (may return multiple).
pub fn classify_tags(tags: &HashMap<String, String>) -> Vec<PoiCategory> {
    let mut out = Vec::new();
    let amenity = tags.get("amenity").map(String::as_str);
    let tourism = tags.get("tourism").map(String::as_str);
    let natural = tags.get("natural").map(String::as_str);
    let network = tags.get("operator").or_else(|| tags.get("network"));

    if matches!(
        amenity,
        Some("drinking_water") | Some("fountain") | Some("water_point")
    ) || natural == Some("spring")
    {
        out.push(PoiCategory::Water);
    }

    if amenity == Some("toilets") {
        out.push(PoiCategory::Restroom);
    }

    if matches!(
        tourism,
        Some("wilderness_hut")
            | Some("alpine_hut")
            | Some("hostel")
            | Some("camp_site")
            | Some("camp_pitch")
    ) || amenity == Some("shelter")
    {
        out.push(PoiCategory::Cabin);
        out.push(PoiCategory::OvernightFacility);
    }

    if matches!(
        amenity,
        Some("cafe")
            | Some("restaurant")
            | Some("fast_food")
            | Some("museum")
            | Some("gallery")
            | Some("zoo")
            | Some("aquarium")
            | Some("viewpoint")
            | Some("picnic_site")
    ) || tourism == Some("viewpoint")
        || tourism == Some("attraction")
        || tourism == Some("museum")
    {
        out.push(PoiCategory::General);
    }

    if tourism == Some("wilderness_hut") || tourism == Some("alpine_hut") {
        if network.is_some_and(|n| NETWORK_TAGS.iter().any(|tag| n.contains(tag))) {
            out.push(PoiCategory::NetworkHut);
        }
        if tags
            .get("operator")
            .is_some_and(|op| NETWORK_TAGS.iter().any(|tag| op.contains(tag)))
        {
            out.push(PoiCategory::NetworkHut);
        }
    }

    // Craft brewery / alcohol retail: any one of these OSM conventions qualifies.
    let microbrewery = tags.get("microbrewery").map(String::as_str) == Some("yes");
    let shop_alcohol = tags.get("shop").map(String::as_str) == Some("alcohol");
    let craft_brewery = tags.get("craft").map(String::as_str) == Some("brewery");
    if microbrewery || shop_alcohol || craft_brewery {
        out.push(PoiCategory::CraftBrewery);
    }

    out.sort_unstable();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn tags(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn craft_brewery_matches_any_of_three_tag_styles() {
        assert!(classify_tags(&tags(&[("microbrewery", "yes")]))
            .contains(&PoiCategory::CraftBrewery));
        assert!(classify_tags(&tags(&[("shop", "alcohol")])).contains(&PoiCategory::CraftBrewery));
        assert!(
            classify_tags(&tags(&[("craft", "brewery")])).contains(&PoiCategory::CraftBrewery)
        );
        assert!(
            !classify_tags(&tags(&[("shop", "bakery")])).contains(&PoiCategory::CraftBrewery)
        );
    }

    #[test]
    fn craft_brewery_does_not_require_all_three_tags() {
        let only_shop = classify_tags(&tags(&[("shop", "alcohol"), ("name", "Tap Room")]));
        assert_eq!(only_shop, vec![PoiCategory::CraftBrewery]);
    }
}
