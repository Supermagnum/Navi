use std::collections::HashMap;

/// Resolve a stable OSM icon key from tags (consistent across basemap layers).
pub fn osm_icon_key(tags: &HashMap<String, String>) -> String {
    if tags.get("microbrewery").map(String::as_str) == Some("yes")
        || tags.get("craft").map(String::as_str) == Some("brewery")
        || tags.get("shop").map(String::as_str) == Some("alcohol")
    {
        return "shop-alcohol".to_string();
    }
    // Bundled Navit-derived `fish.svg` (GPL v2; not a custom drop-in) maps to
    // leisure-fishing / shop-fishing.
    if tags.get("leisure").map(String::as_str) == Some("fishing")
        || tags.get("leisure").map(String::as_str) == Some("fishing_pier")
        || tags.get("sport").map(String::as_str) == Some("fishing")
        || tags.get("shop").map(String::as_str) == Some("fishing")
    {
        return "leisure-fishing".to_string();
    }
    if let Some(amenity) = tags.get("amenity") {
        return format!("amenity-{amenity}");
    }
    if let Some(tourism) = tags.get("tourism") {
        return format!("tourism-{tourism}");
    }
    if let Some(natural) = tags.get("natural") {
        return format!("natural-{natural}");
    }
    if let Some(leisure) = tags.get("leisure") {
        return format!("leisure-{leisure}");
    }
    if let Some(shop) = tags.get("shop") {
        return format!("shop-{shop}");
    }
    if tags.get("highway").map(String::as_str) == Some("speed_camera") {
        // Custom lean-pack key (see docs/icons.md); not highway-speed_camera.
        return "speed_camera".to_string();
    }
    if tags.get("enforcement").map(String::as_str) == Some("maxspeed")
        || tags.get("enforcement").map(String::as_str) == Some("average_speed")
    {
        return "speed_camera".to_string();
    }
    if let Some(highway) = tags.get("highway") {
        return format!("highway-{highway}");
    }
    "poi-generic".to_string()
}
