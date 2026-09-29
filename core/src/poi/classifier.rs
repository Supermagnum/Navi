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
        || tourism == Some("artwork")
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

    // Craft alcohol producers + retail: beer, cider, wine, spirits/distillery.
    // Any one OSM convention qualifies (OR). Large industrial sites tagged
    // industrial=distillery are included so whiskey/aquavit/etc. stops surface.
    let microbrewery = tags.get("microbrewery").map(String::as_str) == Some("yes");
    let shop_alcohol = matches!(
        tags.get("shop").map(String::as_str),
        Some("alcohol") | Some("wine")
    );
    let craft_alcohol = matches!(
        tags.get("craft").map(String::as_str),
        Some("brewery") | Some("winery") | Some("distillery")
    );
    let brewery_kind = matches!(
        tags.get("brewery").map(String::as_str),
        Some("cider") | Some("wine") | Some("mead") | Some("beer")
    );
    let industrial_distillery = tags.get("industrial").map(String::as_str) == Some("distillery");
    if microbrewery || shop_alcohol || craft_alcohol || brewery_kind || industrial_distillery {
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

/// Whether OSM tags name a recognised network-hut operator (same tokens as
/// [`classify_tags`] NetworkHut detection: `operator` / `network` / `brand`).
fn tags_name_network_hut_operator(tags: &HashMap<String, String>) -> bool {
    for key in ["operator", "network", "brand"] {
        if tags
            .get(key)
            .is_some_and(|v| NETWORK_TAGS.iter().any(|tag| v.contains(tag)))
        {
            return true;
        }
    }
    false
}

/// Strict "unlocked without key/membership" test for overnight cabins.
///
/// A cabin is unlocked **only** if:
/// - it has explicit `locked=no` (always wins, including network huts); or
/// - it is `amenity=shelter` with `shelter_type=basic_hut` or `lean_to`, is not
///   classified as a network hut (no operator/network/brand matching
///   [`NETWORK_TAGS`]), and is not otherwise a NetworkHut candidate.
///
/// Untagged `tourism=wilderness_hut` (any operator) is **not** unlocked — many
/// DNT unstaffed cabins omit `locked=*` yet require the DNT key.
pub fn poi_is_unlocked_overnight(tags: &HashMap<String, String>) -> bool {
    let locked = tags.get("locked").map(|s| s.to_ascii_lowercase());
    if locked.as_deref() == Some("yes") || locked.as_deref() == Some("true") {
        return false;
    }
    // Explicit unlocked always wins (including network / wilderness_hut).
    if locked.as_deref() == Some("no") || locked.as_deref() == Some("false") {
        return true;
    }

    let amenity = tags.get("amenity").map(String::as_str);
    let shelter_type = tags.get("shelter_type").map(String::as_str);
    if amenity == Some("shelter")
        && matches!(shelter_type, Some("basic_hut") | Some("lean_to"))
        && !tags_name_network_hut_operator(tags)
    {
        // Also reject if classify_tags would mark NetworkHut (alpine/wilderness + network).
        let cats = classify_tags(tags);
        if !cats.contains(&PoiCategory::NetworkHut) {
            return true;
        }
    }
    false
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
    fn craft_brewery_matches_beer_cider_wine_spirits_and_retail() {
        assert!(
            classify_tags(&tags(&[("microbrewery", "yes")])).contains(&PoiCategory::CraftBrewery)
        );
        assert!(classify_tags(&tags(&[("shop", "alcohol")])).contains(&PoiCategory::CraftBrewery));
        assert!(classify_tags(&tags(&[("shop", "wine")])).contains(&PoiCategory::CraftBrewery));
        assert!(classify_tags(&tags(&[("craft", "brewery")])).contains(&PoiCategory::CraftBrewery));
        assert!(classify_tags(&tags(&[("craft", "winery")])).contains(&PoiCategory::CraftBrewery));
        assert!(
            classify_tags(&tags(&[("craft", "distillery")])).contains(&PoiCategory::CraftBrewery)
        );
        assert!(classify_tags(&tags(&[("brewery", "cider")])).contains(&PoiCategory::CraftBrewery));
        assert!(classify_tags(&tags(&[("brewery", "wine")])).contains(&PoiCategory::CraftBrewery));
        assert!(classify_tags(&tags(&[("brewery", "mead")])).contains(&PoiCategory::CraftBrewery));
        assert!(classify_tags(&tags(&[("brewery", "beer")])).contains(&PoiCategory::CraftBrewery));
        assert!(classify_tags(&tags(&[("industrial", "distillery")]))
            .contains(&PoiCategory::CraftBrewery));
        // Beer brewery that also distills: craft=brewery + industrial=distillery.
        assert!(classify_tags(&tags(&[
            ("craft", "brewery"),
            ("industrial", "distillery"),
            ("name", "Dual Craft"),
        ]))
        .contains(&PoiCategory::CraftBrewery));
        assert!(!classify_tags(&tags(&[("shop", "bakery")])).contains(&PoiCategory::CraftBrewery));
        assert!(!classify_tags(&tags(&[("brewery", "yes")])).contains(&PoiCategory::CraftBrewery));
    }

    #[test]
    fn craft_brewery_does_not_require_all_three_tags() {
        let only_shop = classify_tags(&tags(&[("shop", "alcohol"), ("name", "Tap Room")]));
        assert_eq!(only_shop, vec![PoiCategory::CraftBrewery]);
        let only_cider = classify_tags(&tags(&[("brewery", "cider"), ("name", "Fosmoen")]));
        assert_eq!(only_cider, vec![PoiCategory::CraftBrewery]);
        let only_winery = classify_tags(&tags(&[("craft", "winery"), ("name", "Vineyard")]));
        assert_eq!(only_winery, vec![PoiCategory::CraftBrewery]);
        let only_distillery = classify_tags(&tags(&[("craft", "distillery"), ("name", "Aquavit")]));
        assert_eq!(only_distillery, vec![PoiCategory::CraftBrewery]);
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

    #[test]
    fn general_matches_tourism_artwork() {
        assert!(classify_tags(&tags(&[("tourism", "artwork")])).contains(&PoiCategory::General));
        assert!(!classify_tags(&tags(&[("tourism", "artwork")])).contains(&PoiCategory::Lodging));
        assert!(!classify_tags(&tags(&[("tourism", "artwork")])).contains(&PoiCategory::Cabin));
    }

    #[test]
    fn unlocked_requires_locked_no_or_non_network_basic_shelter() {
        // Untagged DNT wilderness_hut is NOT unlocked.
        assert!(!poi_is_unlocked_overnight(&tags(&[
            ("tourism", "wilderness_hut"),
            ("operator", "DNT"),
        ])));
        assert!(!poi_is_unlocked_overnight(&tags(&[(
            "tourism",
            "wilderness_hut"
        )])));
        // Explicit locked=no wins even on a network hut.
        assert!(poi_is_unlocked_overnight(&tags(&[
            ("tourism", "wilderness_hut"),
            ("operator", "DNT"),
            ("locked", "no"),
        ])));
        // Basic open shelter without network tags.
        assert!(poi_is_unlocked_overnight(&tags(&[
            ("amenity", "shelter"),
            ("shelter_type", "basic_hut"),
        ])));
        assert!(poi_is_unlocked_overnight(&tags(&[
            ("amenity", "shelter"),
            ("shelter_type", "lean_to"),
        ])));
        // Shelter with DNT operator is not unlocked.
        assert!(!poi_is_unlocked_overnight(&tags(&[
            ("amenity", "shelter"),
            ("shelter_type", "basic_hut"),
            ("operator", "DNT Oslo og Omegn"),
        ])));
    }
}
