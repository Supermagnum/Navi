//! Hard edge exclusion for bicycle routing by bike capability / terrain tags.
//!
//! Missing OSM tags never exclude a way — only present tags that exceed the
//! selected profile's thresholds remove edges before A* (same discipline as
//! access forbids and wetland hard-avoid).

use std::collections::{HashMap, HashSet};
use std::path::Path;

use osmpbf::{Element, ElementReader};

use super::builder::RouteGraph;

/// User-selected bike capability (stored in config; applies to Bicycle and
/// Electric cycle — both share the bicycle graph).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BikeCapability {
    /// City / road bike: pavement and good gravel only.
    Road,
    /// Trekking / gravel: moderate unpaved and low MTB difficulty.
    #[default]
    Trekking,
    /// Mountain bike: technical trails; still avoids extreme MTB scale.
    Mountain,
}

impl BikeCapability {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "road" | "city" => Self::Road,
            "mountain" | "mtb" => Self::Mountain,
            _ => Self::Trekking,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Road => "road",
            Self::Trekking => "trekking",
            Self::Mountain => "mountain",
        }
    }
}

fn parse_mtb_scale(raw: &str) -> Option<u8> {
    let head = raw.split(':').next()?.trim();
    head.parse::<u8>().ok().filter(|&v| v <= 6)
}

fn parse_incline_pct(raw: &str) -> Option<f64> {
    let t = raw.trim().trim_end_matches('%');
    t.parse::<f64>().ok()
}

fn smoothness_rank(raw: &str) -> Option<u8> {
    Some(match raw.trim().to_ascii_lowercase().as_str() {
        "excellent" | "good" => 0,
        "intermediate" => 1,
        "bad" => 2,
        "very_bad" => 3,
        "horrible" => 4,
        "very_horrible" => 5,
        "impassable" => 6,
        _ => return None,
    })
}

fn tracktype_grade(raw: &str) -> Option<u8> {
    let t = raw.trim().to_ascii_lowercase();
    if let Some(rest) = t.strip_prefix("grade") {
        return rest.parse::<u8>().ok().filter(|&g| (1..=5).contains(&g));
    }
    None
}

fn is_rough_surface(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "ground"
            | "dirt"
            | "earth"
            | "grass"
            | "sand"
            | "mud"
            | "snow"
            | "ice"
            | "compacted"
            | "fine_gravel"
            | "pebblestone"
            | "gravel"
    )
}

fn is_paved_surface(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "paved"
            | "asphalt"
            | "concrete"
            | "concrete:plates"
            | "concrete:lanes"
            | "paving_stones"
            | "sett"
            | "cobblestone"
            | "metal"
            | "wood"
    )
}

/// True when present tags make this way unsuitable for `cap` (missing tags → false).
pub fn tags_unsuitable_for(cap: BikeCapability, tags: &HashMap<String, String>) -> bool {
    if tags.is_empty() {
        return false;
    }
    if tag_eq(tags, "route", "mtb") && matches!(cap, BikeCapability::Road) {
        return true;
    }
    if let Some(raw) = tags
        .get("mtb:scale")
        .or_else(|| tags.get("mtb:scale:uphill"))
    {
        if let Some(scale) = parse_mtb_scale(raw) {
            let limit = match cap {
                BikeCapability::Road => 1,
                BikeCapability::Trekking => 2,
                BikeCapability::Mountain => 5,
            };
            if scale >= limit {
                return true;
            }
        }
    }
    if let Some(raw) = tags.get("smoothness") {
        if let Some(rank) = smoothness_rank(raw) {
            let limit = match cap {
                BikeCapability::Road => 2,     // rough or worse
                BikeCapability::Trekking => 4, // horrible or worse
                BikeCapability::Mountain => 6, // impassable only
            };
            if rank >= limit {
                return true;
            }
        }
    }
    if let Some(raw) = tags.get("tracktype") {
        if let Some(grade) = tracktype_grade(raw) {
            let limit = match cap {
                BikeCapability::Road => 3,
                BikeCapability::Trekking => 4,
                BikeCapability::Mountain => 5,
            };
            if grade >= limit {
                return true;
            }
        }
    }
    if let Some(raw) = tags.get("incline") {
        if let Some(pct) = parse_incline_pct(raw) {
            let limit = match cap {
                BikeCapability::Road => 12.0,
                BikeCapability::Trekking => 18.0,
                BikeCapability::Mountain => 30.0,
            };
            if pct.abs() > limit {
                return true;
            }
        }
    }
    if let Some(raw) = tags.get("surface") {
        let s = raw.trim().to_ascii_lowercase();
        match cap {
            BikeCapability::Road => {
                if is_rough_surface(&s) && !is_paved_surface(&s) {
                    return true;
                }
            }
            BikeCapability::Trekking => {
                if matches!(s.as_str(), "sand" | "mud" | "snow" | "ice") {
                    return true;
                }
            }
            BikeCapability::Mountain => {}
        }
    }
    false
}

fn tag_eq(tags: &HashMap<String, String>, key: &str, want: &str) -> bool {
    tags.get(key).is_some_and(|v| v.eq_ignore_ascii_case(want))
}

pub fn way_id_from_edge_id(edge_id: &str) -> Option<i64> {
    edge_id.split('-').next()?.parse().ok()
}

/// Load OSM way tags for terrain suitability (single PBF pass, ways of interest only).
pub fn load_way_terrain_tags(
    pbf: &Path,
    way_ids: &HashSet<i64>,
) -> anyhow::Result<HashMap<i64, HashMap<String, String>>> {
    if way_ids.is_empty() {
        return Ok(HashMap::new());
    }
    const KEYS: &[&str] = &[
        "surface",
        "smoothness",
        "tracktype",
        "mtb:scale",
        "mtb:scale:uphill",
        "incline",
        "route",
        "highway",
    ];
    let mut out = HashMap::new();
    let file = std::fs::File::open(pbf)?;
    let reader = ElementReader::new(file);
    reader.for_each(|element| {
        let Element::Way(way) = element else {
            return;
        };
        if !way_ids.contains(&way.id()) {
            return;
        }
        let tags: HashMap<String, String> = way
            .tags()
            .filter(|(k, _)| KEYS.contains(k))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        if !tags.is_empty() {
            out.insert(way.id(), tags);
        }
    })?;
    Ok(out)
}

/// Hard-remove edges whose OSM way tags exceed the capability thresholds.
pub fn apply_bike_suitability(
    graph: &mut RouteGraph,
    way_tags: &HashMap<i64, HashMap<String, String>>,
    cap: BikeCapability,
) -> usize {
    if graph.profile() != super::builder::RoutingProfile::Bicycle {
        return 0;
    }
    let before = graph.edges.len();
    let mut kept = Vec::with_capacity(before);
    for edge in graph.edges.drain(..) {
        let Some(wid) = way_id_from_edge_id(&edge.id) else {
            kept.push(edge);
            continue;
        };
        let tags = way_tags.get(&wid);
        if tags.is_some_and(|t| tags_unsuitable_for(cap, t)) {
            continue;
        }
        kept.push(edge);
    }
    graph.edges = kept;
    let removed = before.saturating_sub(graph.edges.len());
    if removed > 0 {
        graph.rebuild_after_edge_filter();
    }
    removed
}

/// PBF-backed filter: load tags then hard-remove unsuitable edges.
pub fn apply_bike_suitability_from_pbf(
    graph: &mut RouteGraph,
    pbf: &Path,
    cap: BikeCapability,
) -> anyhow::Result<usize> {
    let mut ids = HashSet::new();
    for e in &graph.edges {
        if let Some(id) = way_id_from_edge_id(&e.id) {
            ids.insert(id);
        }
    }
    let tags = load_way_terrain_tags(pbf, &ids)?;
    Ok(apply_bike_suitability(graph, &tags, cap))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_tags_never_unsuitable() {
        assert!(!tags_unsuitable_for(BikeCapability::Road, &HashMap::new()));
    }

    #[test]
    fn road_excludes_mtb_scale_and_mtb_route() {
        let mut t = HashMap::new();
        t.insert("mtb:scale".into(), "2".into());
        assert!(tags_unsuitable_for(BikeCapability::Road, &t));
        assert!(!tags_unsuitable_for(BikeCapability::Mountain, &t));
        t.clear();
        t.insert("route".into(), "mtb".into());
        assert!(tags_unsuitable_for(BikeCapability::Road, &t));
        assert!(!tags_unsuitable_for(BikeCapability::Mountain, &t));
    }

    #[test]
    fn paved_road_unaffected() {
        let mut t = HashMap::new();
        t.insert("surface".into(), "asphalt".into());
        t.insert("smoothness".into(), "good".into());
        for cap in [
            BikeCapability::Road,
            BikeCapability::Trekking,
            BikeCapability::Mountain,
        ] {
            assert!(!tags_unsuitable_for(cap, &t), "{cap:?}");
        }
    }
}
