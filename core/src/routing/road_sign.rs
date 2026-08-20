//! Norwegian road-sign approach warnings from OSM `traffic_sign=NO:…` tags.
//!
//! Catalogue: vendored Supermagnum/road-signs snapshot (`core/src/icons/road-signs/`).
//! Standalone flat icons only — compound underskilt assemblies are not rendered; see
//! `docs/road-signs.md`.

use std::collections::HashMap;
use std::path::Path;

use osmpbf::{Element, ElementReader};
use serde::Deserialize;

use crate::nav::{ApproachPhase, APPROACH_APPEAR_M, APPROACH_HIDE_M, APPROACH_URGENCY_M};
use crate::routing::elevation::country_iso_at;

const OSM_TAGS_JSON: &str = include_str!("../icons/road-signs/database/osm_tags.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoadSignJurisdiction {
    Norway,
    Other,
}

pub fn resolve_road_sign_jurisdiction_at(lat: f64, lon: f64) -> RoadSignJurisdiction {
    match country_iso_at(lat, lon) {
        Some("no") => RoadSignJurisdiction::Norway,
        _ => RoadSignJurisdiction::Other,
    }
}

#[derive(Debug, Clone)]
pub struct RoadSignRecord {
    pub osm_id: i64,
    pub lat: f64,
    pub lon: f64,
    pub icon_key: String,
    pub code: String,
    pub name_en: String,
    pub traffic_sign_raw: String,
}

#[derive(Debug, Clone)]
pub struct RoadSignWarning {
    pub phase: ApproachPhase,
    pub distance_m: f64,
    pub icon_key: String,
    pub code: String,
    pub name_en: String,
    pub label: String,
}

#[derive(Debug, Deserialize)]
struct OsmTagsFile {
    signs: Vec<CatalogSign>,
}

#[derive(Debug, Deserialize)]
struct CatalogSign {
    code: String,
    name: String,
    #[serde(default)]
    svg: Option<String>,
    traffic_sign: Option<TrafficSignMapping>,
    #[serde(default)]
    implied_tags: Vec<ImpliedTag>,
    match_status: String,
    navi_usable_as_fixed_symbol: bool,
}

#[derive(Debug, Deserialize)]
struct TrafficSignMapping {
    preferred: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ImpliedTag {
    key: String,
    value: String,
}

#[derive(Debug, Clone)]
pub struct RoadSignMatch {
    pub code: String,
    pub name_en: String,
    pub icon_key: String,
}

#[derive(Debug, Clone)]
struct CatalogEntry {
    code: String,
    name_en: String,
    icon_key: String,
}

#[derive(Debug, Clone)]
pub struct RoadSignCatalog {
    by_traffic_sign: HashMap<String, CatalogEntry>,
    by_hazard: HashMap<String, CatalogEntry>,
}

fn code_to_icon_key(code: &str) -> String {
    let mut out = String::from("no_sign_");
    for ch in code.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

fn normalize_traffic_sign_token(token: &str) -> String {
    let token = token.trim();
    let base = token.split('[').next().unwrap_or(token).trim();
    if base.starts_with("NO:") {
        base.to_string()
    } else {
        format!("NO:{base}")
    }
}

fn is_excluded_match_status(status: &str) -> bool {
    status == "variable_content" || status == "not_for_navigation"
}

pub fn load_catalog() -> anyhow::Result<RoadSignCatalog> {
    let parsed: OsmTagsFile = serde_json::from_str(OSM_TAGS_JSON)?;
    let mut by_traffic_sign = HashMap::new();
    let mut by_hazard = HashMap::new();

    for sign in parsed.signs {
        if sign.svg.is_none() {
            continue;
        }
        if is_excluded_match_status(&sign.match_status) {
            continue;
        }
        if !sign.navi_usable_as_fixed_symbol {
            continue;
        }
        let icon_key = code_to_icon_key(&sign.code);
        let entry = CatalogEntry {
            code: sign.code.clone(),
            name_en: sign.name.clone(),
            icon_key: icon_key.clone(),
        };
        if let Some(ts) = sign.traffic_sign.as_ref().and_then(|t| t.preferred.clone()) {
            by_traffic_sign.insert(normalize_traffic_sign_token(&ts), entry.clone());
        }
        by_traffic_sign.insert(
            normalize_traffic_sign_token(&format!("NO:{}", sign.code)),
            entry.clone(),
        );
        for tag in sign.implied_tags {
            if tag.key == "hazard" && !tag.value.contains('{') {
                by_hazard.insert(tag.value.clone(), entry.clone());
            }
        }
    }

    Ok(RoadSignCatalog {
        by_traffic_sign,
        by_hazard,
    })
}

fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let rlat1 = lat1.to_radians();
    let rlat2 = lat2.to_radians();
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let h = (dlat / 2.0).sin().powi(2) + rlat1.cos() * rlat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * 6_378_100.0 * h.sqrt().asin()
}

fn phase_for_distance(distance_m: f64) -> ApproachPhase {
    if !distance_m.is_finite() || distance_m > APPROACH_APPEAR_M || distance_m <= APPROACH_HIDE_M {
        ApproachPhase::Hidden
    } else if distance_m <= APPROACH_URGENCY_M {
        ApproachPhase::Urgency
    } else {
        ApproachPhase::Appear
    }
}

fn parse_traffic_sign_tokens(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(normalize_traffic_sign_token)
        .filter(|s| !s.is_empty() && s != "NO:")
        .collect()
}

impl RoadSignCatalog {
    pub fn match_node_tags(&self, tags: &HashMap<String, String>) -> Option<RoadSignMatch> {
        if let Some(raw) = tags.get("traffic_sign") {
            for token in parse_traffic_sign_tokens(raw) {
                if let Some(entry) = self.by_traffic_sign.get(&token) {
                    return Some(RoadSignMatch {
                        code: entry.code.clone(),
                        name_en: entry.name_en.clone(),
                        icon_key: entry.icon_key.clone(),
                    });
                }
            }
        }
        if let Some(hazard) = tags.get("hazard") {
            if let Some(entry) = self.by_hazard.get(hazard) {
                return Some(RoadSignMatch {
                    code: entry.code.clone(),
                    name_en: entry.name_en.clone(),
                    icon_key: entry.icon_key.clone(),
                });
            }
        }
        None
    }
}

/// Index tagged OSM nodes that map to the vendored fixed-symbol catalogue.
pub fn load_road_signs_from_pbf(
    path: impl AsRef<Path>,
    catalog: &RoadSignCatalog,
) -> anyhow::Result<Vec<RoadSignRecord>> {
    let path = path.as_ref();
    let mut out = Vec::new();
    let reader = ElementReader::from_path(path)?;
    reader.for_each(|el| match el {
        Element::Node(n) => {
            let tags: HashMap<String, String> =
                n.tags().map(|(k, v)| (k.into(), v.into())).collect();
            ingest_node_tags(n.id(), n.lat(), n.lon(), &tags, catalog, &mut out);
        }
        Element::DenseNode(n) => {
            let tags: HashMap<String, String> =
                n.tags().map(|(k, v)| (k.into(), v.into())).collect();
            ingest_node_tags(n.id, n.lat(), n.lon(), &tags, catalog, &mut out);
        }
        _ => {}
    })?;
    Ok(out)
}

fn ingest_node_tags(
    id: i64,
    lat: f64,
    lon: f64,
    tags: &HashMap<String, String>,
    catalog: &RoadSignCatalog,
    out: &mut Vec<RoadSignRecord>,
) {
    let Some(entry) = catalog.match_node_tags(tags) else {
        return;
    };
    let traffic_sign_raw = tags
        .get("traffic_sign")
        .cloned()
        .or_else(|| tags.get("hazard").map(|h| format!("hazard={h}")))
        .unwrap_or_default();
    out.push(RoadSignRecord {
        osm_id: id,
        lat,
        lon,
        icon_key: entry.icon_key,
        code: entry.code,
        name_en: entry.name_en,
        traffic_sign_raw,
    });
}

/// Nearest upcoming sign warning for the driver's position (Norway only).
pub fn nearest_road_sign_warning(
    signs: &[RoadSignRecord],
    lat: f64,
    lon: f64,
) -> Option<RoadSignWarning> {
    if resolve_road_sign_jurisdiction_at(lat, lon) != RoadSignJurisdiction::Norway {
        return None;
    }
    let mut best: Option<RoadSignWarning> = None;
    let mut best_d = f64::INFINITY;
    for sign in signs {
        let d = haversine_m(lat, lon, sign.lat, sign.lon);
        let phase = phase_for_distance(d);
        if phase == ApproachPhase::Hidden {
            continue;
        }
        if d < best_d {
            best_d = d;
            best = Some(RoadSignWarning {
                phase,
                distance_m: d,
                icon_key: sign.icon_key.clone(),
                code: sign.code.clone(),
                name_en: sign.name_en.clone(),
                label: sign.name_en.clone(),
            });
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_excludes_variable_content_and_null_svg() {
        let cat = load_catalog().expect("catalog");
        let mut tags = HashMap::new();
        for code in ["812", "140", "723.71"] {
            tags.insert("traffic_sign".into(), format!("NO:{code}"));
            assert!(
                cat.match_node_tags(&tags).is_none(),
                "expected {code} excluded"
            );
        }
        tags.insert("traffic_sign".into(), "NO:100.1".into());
        assert!(cat.match_node_tags(&tags).is_some());
        tags.insert("traffic_sign".into(), "NO:104.1".into());
        assert!(cat.match_node_tags(&tags).is_some());
    }

    #[test]
    fn compound_traffic_sign_prefers_first_usable_base_sign() {
        let cat = load_catalog().expect("catalog");
        let mut tags = HashMap::new();
        tags.insert("traffic_sign".into(), "NO:100.1,812[40 km/t],807.2".into());
        let matched = cat.match_node_tags(&tags).expect("match");
        assert_eq!(matched.code, "100.1");
    }

    #[test]
    fn hazard_companion_tag_matches_catalogue() {
        let cat = load_catalog().expect("catalog");
        let mut tags = HashMap::new();
        tags.insert("hazard".into(), "curve".into());
        let matched = cat.match_node_tags(&tags);
        assert!(matched.is_some(), "expected hazard=curve mapping");
        assert_eq!(matched.unwrap().code, "100.2");
    }

    #[test]
    fn phase_thresholds_match_approach_model() {
        assert_eq!(phase_for_distance(800.0), ApproachPhase::Hidden);
        assert_eq!(phase_for_distance(400.0), ApproachPhase::Appear);
        assert_eq!(phase_for_distance(100.0), ApproachPhase::Urgency);
        assert_eq!(phase_for_distance(10.0), ApproachPhase::Hidden);
    }
}
