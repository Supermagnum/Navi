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

    // Named peaks / hills are terrain features — never pause labels.
    // Tent fallback uses tourism=camp_site / camp_pitch (and synthetic corridor points).
    if matches!(tourism, Some("camp_site") | Some("camp_pitch")) || amenity == Some("camping") {
        out.push(PoiCategory::TentSite);
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

    let leisure = tags.get("leisure").map(String::as_str);
    let sport = tags.get("sport").map(String::as_str);
    let shop = tags.get("shop").map(String::as_str);
    if leisure == Some("fishing")
        || leisure == Some("fishing_pier")
        || sport == Some("fishing")
        || shop == Some("fishing")
    {
        out.push(PoiCategory::Fishing);
    }

    // Truck rest / services: any one of these qualifies (OR, not AND).
    let highway = tags.get("highway").map(String::as_str);
    let hgv = tags.get("hgv").map(String::as_str);
    let access_hgv = tags.get("access:hgv").map(String::as_str);
    let parking_hgv = hgv == Some("yes")
        || hgv == Some("designated")
        || access_hgv == Some("yes")
        || access_hgv == Some("designated");
    if highway == Some("rest_area")
        || highway == Some("services")
        || (amenity == Some("parking") && parking_hgv)
    {
        out.push(PoiCategory::RestArea);
    }

    // Motor overnight lodging: any one of these tourism values qualifies (OR).
    if matches!(
        tourism,
        Some("hotel")
            | Some("motel")
            | Some("guest_house")
            | Some("apartment")
            | Some("chalet")
            | Some("hostel")
    ) {
        out.push(PoiCategory::Lodging);
    }

    out.sort_unstable();
    out.dedup();
    out
}

/// True when tags (or derived icon key) suggest a full-service stop suitable
/// for EC 561 weekly rest (typically `highway=services`, not bare rest areas).
pub fn rest_area_suitable_for_weekly(tags: &HashMap<String, String>, icon_key: &str) -> bool {
    if tags.get("highway").map(String::as_str) == Some("services") {
        return true;
    }
    if icon_key.contains("services") {
        return true;
    }
    tags.get("name")
        .is_some_and(|n| n.to_ascii_lowercase().contains("service"))
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
        assert!(
            classify_tags(&tags(&[("microbrewery", "yes")])).contains(&PoiCategory::CraftBrewery)
        );
        assert!(classify_tags(&tags(&[("shop", "alcohol")])).contains(&PoiCategory::CraftBrewery));
        assert!(classify_tags(&tags(&[("craft", "brewery")])).contains(&PoiCategory::CraftBrewery));
        assert!(!classify_tags(&tags(&[("shop", "bakery")])).contains(&PoiCategory::CraftBrewery));
    }

    #[test]
    fn craft_brewery_does_not_require_all_three_tags() {
        let only_shop = classify_tags(&tags(&[("shop", "alcohol"), ("name", "Tap Room")]));
        assert_eq!(only_shop, vec![PoiCategory::CraftBrewery]);
    }

    #[test]
    fn fishing_matches_leisure_and_related() {
        assert!(classify_tags(&tags(&[("leisure", "fishing")])).contains(&PoiCategory::Fishing));
        assert!(
            classify_tags(&tags(&[("leisure", "fishing_pier")])).contains(&PoiCategory::Fishing)
        );
        assert!(classify_tags(&tags(&[("sport", "fishing")])).contains(&PoiCategory::Fishing));
        assert!(classify_tags(&tags(&[("shop", "fishing")])).contains(&PoiCategory::Fishing));
        assert!(!classify_tags(&tags(&[("leisure", "park")])).contains(&PoiCategory::Fishing));
    }

    #[test]
    fn rest_area_matches_highway_or_hgv_parking() {
        assert!(classify_tags(&tags(&[("highway", "rest_area")])).contains(&PoiCategory::RestArea));
        assert!(classify_tags(&tags(&[("highway", "services")])).contains(&PoiCategory::RestArea));
        assert!(
            classify_tags(&tags(&[("amenity", "parking"), ("hgv", "yes")]))
                .contains(&PoiCategory::RestArea)
        );
        assert!(!classify_tags(&tags(&[("amenity", "parking")])).contains(&PoiCategory::RestArea));
    }

    #[test]
    fn rest_area_weekly_suitable_for_services_not_bare_rest_area() {
        use crate::poi::osm_icon_key;

        let services = tags(&[("highway", "services")]);
        assert!(rest_area_suitable_for_weekly(
            &services,
            &osm_icon_key(&services)
        ));
        let rest = tags(&[("highway", "rest_area")]);
        assert!(!rest_area_suitable_for_weekly(&rest, &osm_icon_key(&rest)));
        let named = tags(&[("highway", "rest_area"), ("name", "North Services Plaza")]);
        assert!(rest_area_suitable_for_weekly(&named, &osm_icon_key(&named)));
    }

    #[test]
    fn lodging_matches_hotel_motel_guest_house_or_hostel() {
        assert!(classify_tags(&tags(&[("tourism", "hotel")])).contains(&PoiCategory::Lodging));
        assert!(classify_tags(&tags(&[("tourism", "motel")])).contains(&PoiCategory::Lodging));
        assert!(classify_tags(&tags(&[("tourism", "guest_house")])).contains(&PoiCategory::Lodging));
        assert!(classify_tags(&tags(&[("tourism", "apartment")])).contains(&PoiCategory::Lodging));
        assert!(classify_tags(&tags(&[("tourism", "chalet")])).contains(&PoiCategory::Lodging));
        // Hostel is both Lodging and OvernightFacility.
        let hostel = classify_tags(&tags(&[("tourism", "hostel")]));
        assert!(hostel.contains(&PoiCategory::Lodging));
        assert!(hostel.contains(&PoiCategory::OvernightFacility));
        assert!(!classify_tags(&tags(&[("tourism", "attraction")])).contains(&PoiCategory::Lodging));
    }
}
