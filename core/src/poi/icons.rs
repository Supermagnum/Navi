use std::collections::HashMap;

/// Resolve a stable OSM icon key from tags (consistent across basemap layers).
pub fn osm_icon_key(tags: &HashMap<String, String>) -> String {
    if tags.get("microbrewery").map(String::as_str) == Some("yes")
        || tags.get("craft").map(String::as_str) == Some("brewery")
        || tags.get("shop").map(String::as_str) == Some("alcohol")
    {
        return "shop-alcohol".to_string();
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
    "poi-generic".to_string()
}
