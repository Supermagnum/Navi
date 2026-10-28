//! Localized road names from offline Protomaps PMTiles (option D).
//!
//! For each guidance / route-summary / "Currently on" label, look up road
//! features near that point in the region's downloaded `.pmtiles`, match by
//! local `name` (+ `ref` when present), and apply [`super::localized_name`].
//! No match → keep the local pack name (option B fallback).
//!
//! Lookups are for the active route only (once per plan / recalculation) and
//! stay in memory — no per-frame work, no new on-disk files.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use flate2::read::GzDecoder;
use geozero::mvt::tile::{GeomType, Value as MvtValue};
use geozero::mvt::{Message as ProstMessage, Tile as MvtTile};
use std::io::Read;

use crate::i18n::{localized_name, NameTags};
use crate::routing::basemap::{read_pmtiles_max_zoom, read_pmtiles_tile};

/// One street label to localize (maneuver / current-road / summary).
#[derive(Debug, Clone)]
pub struct StreetLabelQuery {
    pub lat: f64,
    pub lon: f64,
    /// OSM / pack local `name` (or empty when only a ref is shown).
    pub local_name: String,
    /// OSM `ref` when present.
    pub road_ref: String,
}

/// In-memory cache for one planned route (dropped on next plan).
#[derive(Debug, Default)]
pub struct RouteRoadNameCache {
    /// `(normalized local name, normalized ref)` → localized display string.
    map: HashMap<(String, String), String>,
}

fn norm_key(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

impl RouteRoadNameCache {
    pub fn get(&self, local_name: &str, road_ref: &str) -> Option<&str> {
        self.map
            .get(&(norm_key(local_name), norm_key(road_ref)))
            .map(|s| s.as_str())
    }

    pub fn insert(&mut self, local_name: &str, road_ref: &str, localized: String) {
        self.map
            .insert((norm_key(local_name), norm_key(road_ref)), localized);
    }
}

static ACTIVE_ROUTE_CACHE: Mutex<Option<(String, RouteRoadNameCache)>> = Mutex::new(None);

/// Replace the in-memory route cache (locale + entries). Empty path clears it.
pub fn set_active_route_road_name_cache(cache_key: impl Into<String>, cache: RouteRoadNameCache) {
    if let Ok(mut g) = ACTIVE_ROUTE_CACHE.lock() {
        *g = Some((cache_key.into(), cache));
    }
}

pub fn clear_active_route_road_name_cache() {
    if let Ok(mut g) = ACTIVE_ROUTE_CACHE.lock() {
        *g = None;
    }
}

/// Look up a cached localized street for the active route, if any.
pub fn cached_localized_street(local_name: &str, road_ref: &str) -> Option<String> {
    let g = ACTIVE_ROUTE_CACHE.lock().ok()?;
    let (_k, cache) = g.as_ref()?;
    cache.get(local_name, road_ref).map(|s| s.to_string())
}

/// Web mercator tile for lon/lat at zoom `z`.
pub fn lonlat_to_tile(lon: f64, lat: f64, z: u8) -> (u32, u32) {
    let n = 2f64.powi(z as i32);
    let x = ((lon + 180.0) / 360.0 * n).floor() as i64;
    let lat = lat.clamp(-85.05112878, 85.05112878);
    let lat_rad = lat.to_radians();
    let y = ((1.0 - lat_rad.tan().asinh() / std::f64::consts::PI) / 2.0 * n).floor() as i64;
    let max = (n as i64) - 1;
    (x.clamp(0, max) as u32, y.clamp(0, max) as u32)
}

fn maybe_gunzip(bytes: &[u8]) -> Vec<u8> {
    if bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b {
        let mut out = Vec::new();
        let mut d = GzDecoder::new(bytes);
        if d.read_to_end(&mut out).is_ok() {
            return out;
        }
    }
    bytes.to_vec()
}

fn mvt_value_string(v: &MvtValue) -> Option<String> {
    if let Some(s) = &v.string_value {
        let t = s.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    None
}

/// Decode `roads` layer features' name/ref/name:* tags from one MVT blob.
pub fn road_features_from_mvt(tile_bytes: &[u8]) -> Vec<HashMap<String, String>> {
    let raw = maybe_gunzip(tile_bytes);
    let Ok(tile) = MvtTile::decode(raw.as_slice()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for layer in &tile.layers {
        if layer.name != "roads" {
            continue;
        }
        for feat in &layer.features {
            let geom = feat.r#type.unwrap_or(GeomType::Unknown as i32);
            if geom == GeomType::Point as i32 || geom == GeomType::Polygon as i32 {
                continue;
            }
            let mut tags = HashMap::new();
            let mut i = 0usize;
            while i + 1 < feat.tags.len() {
                let ki = feat.tags[i] as usize;
                let vi = feat.tags[i + 1] as usize;
                i += 2;
                let Some(key) = layer.keys.get(ki) else {
                    continue;
                };
                let Some(val) = layer.values.get(vi).and_then(mvt_value_string) else {
                    continue;
                };
                if key == "name"
                    || key == "ref"
                    || key == "int_name"
                    || (key.starts_with("name:") && !key.starts_with("name:etymology"))
                {
                    tags.insert(key.clone(), val);
                }
            }
            if tags.contains_key("name") || tags.contains_key("ref") {
                out.push(tags);
            }
        }
    }
    out
}

fn names_match(a: &str, b: &str) -> bool {
    norm_key(a) == norm_key(b)
}

fn feature_matches(tags: &HashMap<String, String>, local_name: &str, road_ref: &str) -> bool {
    let feat_name = tags.get("name").map(String::as_str).unwrap_or("");
    let feat_ref = tags.get("ref").map(String::as_str).unwrap_or("");
    let want_name = local_name.trim();
    let want_ref = road_ref.trim();
    if !want_name.is_empty() && !feat_name.is_empty() && names_match(want_name, feat_name) {
        if want_ref.is_empty() || feat_ref.is_empty() || names_match(want_ref, feat_ref) {
            return true;
        }
        // Name matched; ref optional when pack has no ref or tile has none.
        if want_ref.is_empty() || feat_ref.is_empty() {
            return true;
        }
    }
    // Ref-only labels (motorways): match ref when local name empty.
    if want_name.is_empty()
        && !want_ref.is_empty()
        && !feat_ref.is_empty()
        && names_match(want_ref, feat_ref)
    {
        return true;
    }
    false
}

fn localized_from_tags(tags: &HashMap<String, String>, locale: &str) -> Option<String> {
    let borrowed: NameTags<'_> = tags.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    localized_name(&borrowed, locale)
}

/// Build a route cache by reading PMTiles near each query point (highest zoom).
pub fn build_route_road_name_cache(
    pmtiles: &Path,
    locale: &str,
    queries: &[StreetLabelQuery],
) -> RouteRoadNameCache {
    let mut cache = RouteRoadNameCache::default();
    if queries.is_empty() || !pmtiles.is_file() {
        return cache;
    }
    let max_z = read_pmtiles_max_zoom(pmtiles).unwrap_or(14).min(15);
    // Prefer detailed road labels; floor at z12 if archive is coarse.
    let z = max_z.max(12);
    let mut tile_feats: HashMap<(u32, u32), Vec<HashMap<String, String>>> = HashMap::new();

    for q in queries {
        let key = (norm_key(&q.local_name), norm_key(&q.road_ref));
        if cache.map.contains_key(&key) {
            continue;
        }
        if q.local_name.trim().is_empty() && q.road_ref.trim().is_empty() {
            continue;
        }
        let (tx, ty) = lonlat_to_tile(q.lon, q.lat, z);
        let feats = tile_feats.entry((tx, ty)).or_insert_with(|| {
            match read_pmtiles_tile(pmtiles, z, tx, ty) {
                Ok(Some(bytes)) => road_features_from_mvt(&bytes),
                _ => Vec::new(),
            }
        });
        let mut found = None;
        for tags in feats.iter() {
            if feature_matches(tags, &q.local_name, &q.road_ref) {
                if let Some(loc) = localized_from_tags(tags, locale) {
                    found = Some(loc);
                    break;
                }
            }
        }
        if let Some(loc) = found {
            // Only cache when localization changes the label (or adds language form).
            let local = q.local_name.trim();
            if !local.is_empty() && names_match(&loc, local) {
                // Same as local — still cache to avoid re-reads; store local.
                cache.insert(&q.local_name, &q.road_ref, local.to_string());
            } else {
                cache.insert(&q.local_name, &q.road_ref, loc);
            }
        }
    }
    cache
}

/// Apply cache to a street string: if `street` equals local name (and we have a
/// localized form), return localized; else return `street` unchanged.
pub fn apply_cached_street(street: &str, road_ref: &str, cache: &RouteRoadNameCache) -> String {
    if let Some(loc) = cache.get(street, road_ref) {
        return loc.to_string();
    }
    if let Some(loc) = cache.get(street, "") {
        return loc.to_string();
    }
    street.to_string()
}

/// Localize street fields in a maneuvers JSON array (in place semantics).
///
/// Each object may have `street`, `lat`, `lon`. Optional top-level `ref` is
/// unused; matching uses street as local name.
pub fn localize_maneuvers_json(pmtiles: &Path, locale: &str, maneuvers_json: &str) -> String {
    let Ok(mut arr) = serde_json::from_str::<Vec<serde_json::Value>>(maneuvers_json) else {
        return maneuvers_json.to_string();
    };
    let mut queries = Vec::new();
    for m in &arr {
        let street = m
            .get("street")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if street.trim().is_empty() {
            continue;
        }
        let lat = m.get("lat").and_then(|v| v.as_f64()).unwrap_or(f64::NAN);
        let lon = m.get("lon").and_then(|v| v.as_f64()).unwrap_or(f64::NAN);
        if !lat.is_finite() || !lon.is_finite() {
            continue;
        }
        queries.push(StreetLabelQuery {
            lat,
            lon,
            local_name: street,
            road_ref: String::new(),
        });
    }
    let cache = build_route_road_name_cache(pmtiles, locale, &queries);
    let cache_key = format!("{}|{}", pmtiles.display(), locale);
    set_active_route_road_name_cache(cache_key, cache);
    let guard = ACTIVE_ROUTE_CACHE.lock().ok();
    let cache_ref = guard.as_ref().and_then(|g| g.as_ref().map(|(_, c)| c));
    if let Some(cache) = cache_ref {
        for m in &mut arr {
            let Some(street) = m.get("street").and_then(|v| v.as_str()) else {
                continue;
            };
            let localized = apply_cached_street(street, "", cache);
            if localized != street {
                m.as_object_mut()
                    .map(|o| o.insert("street".into(), serde_json::Value::String(localized)));
            }
        }
    }
    serde_json::to_string(&arr).unwrap_or_else(|_| maneuvers_json.to_string())
}

/// Localize sim-sample `street` fields the same way (route summary / current road).
pub fn localize_sim_samples_json(pmtiles: &Path, locale: &str, samples_json: &str) -> String {
    let Ok(mut arr) = serde_json::from_str::<Vec<serde_json::Value>>(samples_json) else {
        return samples_json.to_string();
    };
    // Reuse active cache when present for same locale; else build from samples.
    let need_build = ACTIVE_ROUTE_CACHE
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|(k, _)| !k.ends_with(&format!("|{locale}"))))
        .unwrap_or(true);
    if need_build {
        let mut queries = Vec::new();
        for s in &arr {
            let street = s
                .get("street")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if street.trim().is_empty() {
                continue;
            }
            let lat = s.get("lat").and_then(|v| v.as_f64()).unwrap_or(f64::NAN);
            let lon = s.get("lon").and_then(|v| v.as_f64()).unwrap_or(f64::NAN);
            if !lat.is_finite() || !lon.is_finite() {
                continue;
            }
            queries.push(StreetLabelQuery {
                lat,
                lon,
                local_name: street,
                road_ref: String::new(),
            });
        }
        // Cap sample queries — densified routes are huge; take unique names near
        // first occurrence only (already skipped via cache insert).
        let cache = build_route_road_name_cache(pmtiles, locale, &queries);
        set_active_route_road_name_cache(format!("{}|{locale}", pmtiles.display()), cache);
    }
    let guard = ACTIVE_ROUTE_CACHE.lock().ok();
    let Some((_, cache)) = guard.as_ref().and_then(|g| g.as_ref()) else {
        return samples_json.to_string();
    };
    for s in &mut arr {
        let Some(street) = s.get("street").and_then(|v| v.as_str()) else {
            continue;
        };
        let localized = apply_cached_street(street, "", cache);
        if localized != street {
            s.as_object_mut()
                .map(|o| o.insert("street".into(), serde_json::Value::String(localized)));
        }
    }
    serde_json::to_string(&arr).unwrap_or_else(|_| samples_json.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lonlat_tile_oslo() {
        let (x, y) = lonlat_to_tile(10.75, 59.91, 14);
        assert!(x > 0 && y > 0);
    }

    #[test]
    fn mvt_roads_decode_fixture_when_present() {
        // Optional: pulled Ostlandet tile probe path used in i18n verification.
        let path = Path::new("/tmp/navi-pmtiles-probe/europe_norway_ostlandet.pmtiles");
        if !path.is_file() {
            return;
        }
        let (x, y) = lonlat_to_tile(10.75, 59.91, 14);
        let bytes = read_pmtiles_tile(path, 14, x, y)
            .ok()
            .flatten()
            .expect("oslo tile");
        let feats = road_features_from_mvt(&bytes);
        assert!(!feats.is_empty(), "expected roads features in Oslo tile");
        // Ostlandet tiles rarely carry name:* — local name must still parse.
        assert!(feats.iter().any(|t| t.contains_key("name")));
    }

    #[test]
    fn feature_match_by_name() {
        let mut tags = HashMap::new();
        tags.insert("name".into(), "Kirkegata".into());
        tags.insert("name:en".into(), "Church Street".into());
        assert!(feature_matches(&tags, "Kirkegata", ""));
        assert!(!feature_matches(&tags, "Storgata", ""));
        let loc = localized_from_tags(&tags, "en").expect("en");
        assert_eq!(loc, "Church Street");
    }
}
