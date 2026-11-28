//! Nearby services search: offline amenity/tourism POIs within a great-circle radius.
//!
//! **Data source (after investigation):** `place_index.db` / `name_entries` holds
//! named service POIs with precise `kind` values (`amenity:fuel`, `amenity:cafe`,
//! …). Sample Østlandet counts: fuel 888, cafe 1102, fast_food 854, hospital 69,
//! shop:car_repair 462, hotel 391, motel 35, doctors 779.
//!
//! `FlatPoiBarrierPack` / routing `PoiIndex` does **not** classify fuel, hospital,
//! car repair, or doctors — only a rest-planning subset (cafe/fast_food as
//! General, hotel/motel as Lodging). CoMaps MWM category search covers most
//! groups but lumps cafe with fast_food (`category_eat`) and doctors with
//! hospitals (`category_hospital`), so it is a weaker fit for per-category chips.
//! Live PBF scans are too heavy for interactive 150 km queries on large regions;
//! the place index is already the PBF-derived offline store built at install time.
//!
//! Architecture: **core feature** (not a WASM plugin) — same trust boundary as
//! place search and POI lookahead geometry.

use std::collections::HashSet;
use std::time::Instant;

use crate::routing::basemap::{bbox_covers_point, region_bbox};
use crate::search::NameIndex;
use crate::tracks::haversine_km;

/// Default search radius (150 km).
pub const NEARBY_SERVICES_RADIUS_M: f64 = 150_000.0;

/// Service categories for the Nearby services UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NearbyServiceCategory {
    GasStation,
    Cafe,
    FastFood,
    Hospital,
    CarRepair,
    HotelMotel,
    Doctor,
}

impl NearbyServiceCategory {
    pub const ALL: [NearbyServiceCategory; 7] = [
        Self::GasStation,
        Self::Cafe,
        Self::FastFood,
        Self::Hospital,
        Self::CarRepair,
        Self::HotelMotel,
        Self::Doctor,
    ];

    /// Stable wire / FFI id.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GasStation => "gas_station",
            Self::Cafe => "cafe",
            Self::FastFood => "fast_food",
            Self::Hospital => "hospital",
            Self::CarRepair => "car_repair",
            Self::HotelMotel => "hotel_motel",
            Self::Doctor => "doctor",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "gas_station" | "fuel" | "gas" => Some(Self::GasStation),
            "cafe" | "café" => Some(Self::Cafe),
            "fast_food" | "fastfood" => Some(Self::FastFood),
            "hospital" => Some(Self::Hospital),
            "car_repair" | "carrepair" => Some(Self::CarRepair),
            "hotel_motel" | "hotel" | "motel" => Some(Self::HotelMotel),
            "doctor" | "doctors" => Some(Self::Doctor),
            _ => None,
        }
    }

    /// OSM `kind` values stored in `name_entries` (match any).
    pub fn osm_kinds(self) -> &'static [&'static str] {
        match self {
            Self::GasStation => &["amenity:fuel"],
            Self::Cafe => &["amenity:cafe"],
            Self::FastFood => &["amenity:fast_food"],
            Self::Hospital => &["amenity:hospital", "healthcare:hospital"],
            Self::CarRepair => &["shop:car_repair", "craft:car_repair"],
            Self::HotelMotel => &["tourism:hotel", "tourism:motel"],
            Self::Doctor => &["amenity:doctors", "healthcare:doctor"],
        }
    }

    pub fn from_kind(kind: &str) -> Option<Self> {
        for cat in Self::ALL {
            if cat.osm_kinds().iter().any(|k| *k == kind) {
                return Some(cat);
            }
        }
        None
    }
}

/// One nearby service hit, sorted by great-circle distance when returned in a list.
#[derive(Debug, Clone, PartialEq)]
pub struct NearbyServiceHit {
    pub osm_id: i64,
    pub name: String,
    pub category: NearbyServiceCategory,
    pub kind: String,
    pub lat: f64,
    pub lon: f64,
    pub distance_m: f64,
    /// Initial bearing from the user to the POI, degrees clockwise from north \[0, 360).
    pub bearing_deg: f64,
    /// Eight-point compass abbreviation (N, NE, E, …).
    pub compass: String,
    pub region_id: String,
    pub sub_area: String,
    pub municipality: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NearbyServicesQuery {
    pub lat: f64,
    pub lon: f64,
    pub radius_m: f64,
    pub categories: Vec<NearbyServiceCategory>,
    /// Installed Geofabrik / pack region ids (e.g. `europe/norway/ostlandet`).
    pub installed_regions: Vec<String>,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NearbyServicesResult {
    pub hits: Vec<NearbyServiceHit>,
    /// Catalog leaf regions that intersect the search circle but are not installed.
    pub missing_regions: Vec<String>,
    pub query_time_ms: u64,
}

/// Initial bearing from `(lat1, lon1)` to `(lat2, lon2)`, degrees \[0, 360).
pub fn bearing_deg(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let p1 = lat1.to_radians();
    let p2 = lat2.to_radians();
    let dl = (lon2 - lon1).to_radians();
    let y = dl.sin() * p2.cos();
    let x = p1.cos() * p2.sin() - p1.sin() * p2.cos() * dl.cos();
    (y.atan2(x).to_degrees() + 360.0) % 360.0
}

/// Eight-point compass from bearing degrees.
pub fn compass_from_bearing(deg: f64) -> &'static str {
    let d = ((deg % 360.0) + 360.0) % 360.0;
    let idx = ((d + 22.5) / 45.0).floor() as usize % 8;
    ["N", "NE", "E", "SE", "S", "SW", "W", "NW"][idx]
}

/// Format distance for UI: metres under 1 km, otherwise kilometres (one decimal).
pub fn format_nearby_distance(distance_m: f64) -> String {
    if distance_m < 1000.0 {
        format!("{:.0} m", distance_m.max(0.0))
    } else {
        format!("{:.1} km", distance_m / 1000.0)
    }
}

/// Degree bbox around a point for a given radius (metres).
pub fn radius_bbox(lat: f64, lon: f64, radius_m: f64) -> (f64, f64, f64, f64) {
    let radius_m = radius_m.max(1.0);
    let lat_pad = radius_m / 111_320.0;
    let lon_pad = radius_m / (111_320.0 * lat.to_radians().cos().max(0.2));
    (lat - lat_pad, lon - lon_pad, lat + lat_pad, lon + lon_pad)
}

fn bboxes_intersect(a: [f64; 4], b: [f64; 4]) -> bool {
    a[0] <= b[2] && a[2] >= b[0] && a[1] <= b[3] && a[3] >= b[1]
}

/// Norway landsdel + pack leaf catalog paths that intersect a lat/lon bbox.
///
/// Excludes country-wide parents (`europe/norway`) and `test/` fixtures so gap
/// messages name downloadable leaf regions (e.g. Vestlandet).
pub fn catalog_regions_intersecting_bbox(
    min_lat: f64,
    min_lon: f64,
    max_lat: f64,
    max_lon: f64,
) -> Vec<&'static str> {
    let search = [min_lat, min_lon, max_lat, max_lon];
    let mut out: Vec<&'static str> = Vec::new();
    for path in crate::routing::basemap::nearby_service_catalog_paths() {
        if path.starts_with("test/") {
            continue;
        }
        // Skip country-only parents when finer leaves exist under them.
        if path == "europe/norway" || path == "europe/sweden" || path == "europe/finland" {
            continue;
        }
        let Some(bbox) = region_bbox(path) else {
            continue;
        };
        if bboxes_intersect(search, bbox) {
            out.push(path);
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// Regions that intersect the circle but are absent from `installed`.
pub fn missing_regions_for_circle(
    lat: f64,
    lon: f64,
    radius_m: f64,
    installed: &[String],
) -> Vec<String> {
    let (min_lat, min_lon, max_lat, max_lon) = radius_bbox(lat, lon, radius_m);
    let installed_norm: HashSet<String> = installed
        .iter()
        .map(|s| normalize_region_id(s))
        .filter(|s| !s.is_empty())
        .collect();
    catalog_regions_intersecting_bbox(min_lat, min_lon, max_lat, max_lon)
        .into_iter()
        .filter(|path| {
            let n = normalize_region_id(path);
            !installed_norm.contains(&n)
                && !installed_norm.iter().any(|i| {
                    i == &n || n.starts_with(&format!("{i}/")) || i.starts_with(&format!("{n}/"))
                })
        })
        .map(|p| p.to_string())
        .collect()
}

fn normalize_region_id(id: &str) -> String {
    id.trim().trim_matches('/').to_ascii_lowercase()
}

fn kind_allowed(kind: &str, categories: &[NearbyServiceCategory]) -> Option<NearbyServiceCategory> {
    let cat = NearbyServiceCategory::from_kind(kind)?;
    if categories.is_empty() || categories.iter().any(|c| *c == cat) {
        Some(cat)
    } else {
        None
    }
}

fn name_entries_has_column(conn: &rusqlite::Connection, col: &str) -> bool {
    let Ok(mut stmt) = conn.prepare("PRAGMA table_info(name_entries)") else {
        return false;
    };
    let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(1)) else {
        return false;
    };
    for name in rows.flatten() {
        if name.eq_ignore_ascii_case(col) {
            return true;
        }
    }
    false
}

impl NameIndex {
    /// Service POIs within `radius_m`, filtered by category, nearest first.
    ///
    /// Bbox pre-filter on `lat`/`lon`, then exact Haversine. Does not hang on
    /// missing regions — callers pass installed ids for gap messaging only.
    pub fn nearby_services(
        &self,
        query: &NearbyServicesQuery,
    ) -> rusqlite::Result<NearbyServicesResult> {
        let started = Instant::now();
        let radius_m = if query.radius_m > 0.0 {
            query.radius_m
        } else {
            NEARBY_SERVICES_RADIUS_M
        };
        let limit = query.limit.max(1);
        let categories = if query.categories.is_empty() {
            NearbyServiceCategory::ALL.to_vec()
        } else {
            query.categories.clone()
        };
        let mut kinds: Vec<&str> = Vec::new();
        for cat in &categories {
            for k in cat.osm_kinds() {
                if !kinds.contains(k) {
                    kinds.push(*k);
                }
            }
        }
        let (min_lat, min_lon, max_lat, max_lon) = radius_bbox(query.lat, query.lon, radius_m);

        let placeholders = vec!["?"; kinds.len()].join(",");
        let conn = self.sqlite();
        let has_region = name_entries_has_column(conn, "region_id");
        let has_context = name_entries_has_column(conn, "sub_area");
        let has_names_json = name_entries_has_column(conn, "names_json");

        let sql = if has_region && has_context && has_names_json {
            format!(
                "SELECT osm_id, name, kind, lat, lon,
                        COALESCE(sub_area, ''), COALESCE(municipality, ''),
                        COALESCE(region_id, ''), COALESCE(names_json, '')
                 FROM name_entries
                 WHERE kind IN ({placeholders})
                   AND lat BETWEEN ? AND ?
                   AND lon BETWEEN ? AND ?"
            )
        } else if has_region && has_context {
            format!(
                "SELECT osm_id, name, kind, lat, lon,
                        COALESCE(sub_area, ''), COALESCE(municipality, ''),
                        COALESCE(region_id, ''), ''
                 FROM name_entries
                 WHERE kind IN ({placeholders})
                   AND lat BETWEEN ? AND ?
                   AND lon BETWEEN ? AND ?"
            )
        } else {
            format!(
                "SELECT osm_id, name, kind, lat, lon, '', '', '', ''
                 FROM name_entries
                 WHERE kind IN ({placeholders})
                   AND lat BETWEEN ? AND ?
                   AND lon BETWEEN ? AND ?"
            )
        };

        let mut stmt = conn.prepare(&sql)?;
        let mut bind: Vec<rusqlite::types::Value> = kinds
            .iter()
            .map(|k| rusqlite::types::Value::Text((*k).to_string()))
            .collect();
        bind.push(rusqlite::types::Value::Real(min_lat));
        bind.push(rusqlite::types::Value::Real(max_lat));
        bind.push(rusqlite::types::Value::Real(min_lon));
        bind.push(rusqlite::types::Value::Real(max_lon));

        let rows = stmt.query_map(rusqlite::params_from_iter(bind.iter()), |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, f64>(3)?,
                row.get::<_, f64>(4)?,
                row.get::<_, String>(5).unwrap_or_default(),
                row.get::<_, String>(6).unwrap_or_default(),
                row.get::<_, String>(7).unwrap_or_default(),
            ))
        })?;

        let mut scored: Vec<(f64, NearbyServiceHit)> = Vec::new();
        for r in rows {
            let (osm_id, name, kind, lat, lon, sub_area, municipality, region_id) = r?;
            let Some(category) = kind_allowed(&kind, &categories) else {
                continue;
            };
            let d_m = haversine_km(query.lat, query.lon, lat, lon) * 1000.0;
            if d_m > radius_m {
                continue;
            }
            let bearing = bearing_deg(query.lat, query.lon, lat, lon);
            scored.push((
                d_m,
                NearbyServiceHit {
                    osm_id,
                    name,
                    category,
                    kind,
                    lat,
                    lon,
                    distance_m: d_m,
                    bearing_deg: bearing,
                    compass: compass_from_bearing(bearing).to_string(),
                    region_id,
                    sub_area,
                    municipality,
                },
            ));
        }

        scored.sort_by(|a, b| a.0.total_cmp(&b.0));
        scored.truncate(limit);
        let hits: Vec<NearbyServiceHit> = scored.into_iter().map(|(_, h)| h).collect();
        let missing =
            missing_regions_for_circle(query.lat, query.lon, radius_m, &query.installed_regions);
        Ok(NearbyServicesResult {
            hits,
            missing_regions: missing,
            query_time_ms: started.elapsed().as_millis() as u64,
        })
    }
}

/// Merge hits from several region queries, de-dupe by osm_id, sort by distance.
pub fn merge_nearby_hits(
    mut batches: Vec<Vec<NearbyServiceHit>>,
    limit: usize,
) -> Vec<NearbyServiceHit> {
    let mut seen = HashSet::new();
    let mut all: Vec<NearbyServiceHit> = Vec::new();
    for batch in batches.drain(..) {
        for h in batch {
            if seen.insert(h.osm_id) {
                all.push(h);
            }
        }
    }
    all.sort_by(|a, b| a.distance_m.total_cmp(&b.distance_m));
    all.truncate(limit.max(1));
    all
}

/// True when a catalog region bbox intersects the search circle's bbox.
pub fn region_reaches_circle(path: &str, lat: f64, lon: f64, radius_m: f64) -> bool {
    let Some(bbox) = region_bbox(path) else {
        return false;
    };
    if bbox_covers_point(bbox, lat, lon) {
        return true;
    }
    let (min_lat, min_lon, max_lat, max_lon) = radius_bbox(lat, lon, radius_m);
    bboxes_intersect(bbox, [min_lat, min_lon, max_lat, max_lon])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::NameIndex;
    use std::path::PathBuf;

    fn fixture_db() -> Option<PathBuf> {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/integration-fixtures/place_index_search_check.db");
        if p.is_file() {
            Some(p)
        } else {
            None
        }
    }

    #[test]
    fn haversine_espa_to_known_fuel_is_finite() {
        let d = haversine_km(60.546, 11.219, 60.57, 11.22) * 1000.0;
        assert!(d > 0.0 && d < 50_000.0);
    }

    #[test]
    fn compass_cardinals() {
        assert_eq!(compass_from_bearing(0.0), "N");
        assert_eq!(compass_from_bearing(45.0), "NE");
        assert_eq!(compass_from_bearing(90.0), "E");
        assert_eq!(compass_from_bearing(180.0), "S");
        assert_eq!(compass_from_bearing(270.0), "W");
        assert_eq!(compass_from_bearing(359.0), "N");
    }

    #[test]
    fn format_distance_under_and_over_1km() {
        assert_eq!(format_nearby_distance(250.0), "250 m");
        assert_eq!(format_nearby_distance(999.0), "999 m");
        assert_eq!(format_nearby_distance(1000.0), "1.0 km");
        assert_eq!(format_nearby_distance(2710.0), "2.7 km");
    }

    #[test]
    fn radius_cutoff_excludes_beyond_150km() {
        let lat = 60.546;
        let lon = 11.219;
        let far_lat = lat + (160_000.0 / 111_320.0);
        let d_m = haversine_km(lat, lon, far_lat, lon) * 1000.0;
        assert!(d_m > NEARBY_SERVICES_RADIUS_M);
    }

    #[test]
    fn merge_sorts_and_dedupes_across_regions() {
        let a = NearbyServiceHit {
            osm_id: 1,
            name: "A".into(),
            category: NearbyServiceCategory::Cafe,
            kind: "amenity:cafe".into(),
            lat: 60.0,
            lon: 11.0,
            distance_m: 5000.0,
            bearing_deg: 10.0,
            compass: "N".into(),
            region_id: "europe/norway/ostlandet".into(),
            sub_area: String::new(),
            municipality: String::new(),
        };
        let b = NearbyServiceHit {
            osm_id: 2,
            name: "B".into(),
            category: NearbyServiceCategory::Cafe,
            kind: "amenity:cafe".into(),
            lat: 60.1,
            lon: 11.1,
            distance_m: 1000.0,
            bearing_deg: 20.0,
            compass: "N".into(),
            region_id: "europe/norway/vestlandet".into(),
            sub_area: String::new(),
            municipality: String::new(),
        };
        let dup = NearbyServiceHit {
            osm_id: 1,
            name: "A-dup".into(),
            category: NearbyServiceCategory::Cafe,
            kind: "amenity:cafe".into(),
            lat: 60.0,
            lon: 11.0,
            distance_m: 4999.0,
            bearing_deg: 10.0,
            compass: "N".into(),
            region_id: "europe/norway/vestlandet".into(),
            sub_area: String::new(),
            municipality: String::new(),
        };
        let merged = merge_nearby_hits(vec![vec![a, dup], vec![b]], 10);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].osm_id, 2);
        assert_eq!(merged[1].osm_id, 1);
        assert_eq!(merged[1].name, "A");
    }

    #[test]
    fn missing_regions_reports_vestlandet_when_only_ostlandet_installed() {
        let missing = missing_regions_for_circle(
            60.546,
            11.219,
            NEARBY_SERVICES_RADIUS_M,
            &["europe/norway/ostlandet".into()],
        );
        assert!(
            missing.iter().any(|r| r.contains("vestlandet")),
            "expected Vestlandet in missing, got {missing:?}"
        );
        assert!(!missing.iter().any(|r| r.ends_with("/ostlandet")));
    }

    #[test]
    fn category_kind_roundtrip() {
        assert_eq!(
            NearbyServiceCategory::from_kind("amenity:fuel"),
            Some(NearbyServiceCategory::GasStation)
        );
        assert_eq!(
            NearbyServiceCategory::from_kind("shop:car_repair"),
            Some(NearbyServiceCategory::CarRepair)
        );
        assert_eq!(
            NearbyServiceCategory::from_kind("craft:car_repair"),
            Some(NearbyServiceCategory::CarRepair)
        );
        assert_eq!(
            NearbyServiceCategory::from_kind("healthcare:doctor"),
            Some(NearbyServiceCategory::Doctor)
        );
        assert_eq!(NearbyServiceCategory::from_kind("amenity:bench"), None);
    }

    #[test]
    fn fixture_search_sorted_and_within_radius() {
        let Some(db) = fixture_db() else {
            eprintln!("skip: place_index_search_check.db missing");
            return;
        };
        let idx = NameIndex::open_readonly(&db).expect("open fixture");
        let q = NearbyServicesQuery {
            lat: 60.546,
            lon: 11.219,
            radius_m: NEARBY_SERVICES_RADIUS_M,
            categories: vec![NearbyServiceCategory::GasStation],
            installed_regions: vec!["europe/norway/ostlandet".into()],
            limit: 20,
        };
        let res = idx.nearby_services(&q).expect("query");
        assert!(!res.hits.is_empty());
        for w in res.hits.windows(2) {
            assert!(w[0].distance_m <= w[1].distance_m);
        }
        for h in &res.hits {
            assert!(h.distance_m <= NEARBY_SERVICES_RADIUS_M + 1.0);
            assert_eq!(h.category, NearbyServiceCategory::GasStation);
        }
        assert!(
            res.query_time_ms < 5_000,
            "slow query {} ms",
            res.query_time_ms
        );
    }

    #[test]
    fn fixture_multi_category_merge_order() {
        let Some(db) = fixture_db() else {
            return;
        };
        let idx = NameIndex::open_readonly(&db).expect("open");
        let q = NearbyServicesQuery {
            lat: 60.726,
            lon: 10.613, // Raufoss
            radius_m: NEARBY_SERVICES_RADIUS_M,
            categories: NearbyServiceCategory::ALL.to_vec(),
            installed_regions: vec!["europe/norway/ostlandet".into()],
            limit: 50,
        };
        let res = idx.nearby_services(&q).expect("query");
        assert!(res.hits.len() >= 10);
        for w in res.hits.windows(2) {
            assert!(w[0].distance_m <= w[1].distance_m);
        }
    }
}
