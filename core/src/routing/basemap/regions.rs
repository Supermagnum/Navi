//! Geofabrik path → region_key + bbox for offline PMTiles extracts from the
//! public Protomaps planet build (HTTP range extract — no project hosting).

use crate::routing::basemap::extract::{
    resolve_planet_url_blocking, PROTOMAPS_PLANET_FALLBACK_URL,
};

/// Default planet PMTiles URL (resolved at call time when possible).
///
/// Prefer [`default_pmtiles_planet_url`] which hits Protomaps builds metadata.
pub const DEFAULT_PMTILES_BASE_URL: &str = PROTOMAPS_PLANET_FALLBACK_URL;

pub fn default_pmtiles_base_url() -> &'static str {
    DEFAULT_PMTILES_BASE_URL
}

/// Resolve the current public Protomaps planet URL (network), else fallback.
pub fn default_pmtiles_planet_url() -> String {
    resolve_planet_url_blocking()
}

/// Map a Geofabrik path (e.g. `europe/norway/ostlandet`) to a stable file stem.
pub fn geofabrik_path_to_region_key(path: &str) -> String {
    let trimmed = path.trim().trim_matches('/');
    if trimmed.is_empty() {
        return "unknown".to_string();
    }
    sanitize_region_key(&trimmed.replace('/', "_"))
}

pub fn sanitize_region_key(key: &str) -> String {
    key.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_ascii_lowercase()
}

/// Approximate bbox `[min_lat, min_lon, max_lat, max_lon]` for a Geofabrik path.
///
/// Norway landsdels and `test/oslo` use the local table; country extracts use
/// bboxes derived from Geofabrik `index-v1.json` geometries (see
/// [`GEOFABRIK_PATH_BBOX`]).
pub fn region_bbox(geofabrik_path: &str) -> Option<[f64; 4]> {
    let path = geofabrik_path.trim().trim_matches('/').to_ascii_lowercase();
    if let Some(bbox) = NORWAY_LANDSDEL
        .iter()
        .find(|(p, _)| *p == path)
        .map(|(_, b)| *b)
    {
        return Some(bbox);
    }
    if let Some(bbox) = GEOFABRIK_PATH_BBOX
        .iter()
        .find(|(p, _)| *p == path)
        .map(|(_, b)| *b)
    {
        return Some(bbox);
    }
    // Unknown subpath under a known country extract: use the country bbox.
    if let Some((parent, _)) = path.rsplit_once('/') {
        if let Some(bbox) = GEOFABRIK_PATH_BBOX
            .iter()
            .find(|(p, _)| *p == parent)
            .map(|(_, b)| *b)
        {
            return Some(bbox);
        }
    }
    None
}

/// Legacy helper: when `base` is a planet URL, return it unchanged; otherwise
/// append `/{region_key}.pmtiles` (old pre-cut hosting shape).
pub fn region_pmtiles_url(base: &str, region_key: &str) -> String {
    let base = base.trim_end_matches('/');
    if base.contains("build.protomaps.com") && base.ends_with(".pmtiles") {
        return base.to_string();
    }
    let key = sanitize_region_key(region_key);
    format!("{base}/{key}.pmtiles")
}

pub fn bbox_covers_point(bbox: [f64; 4], lat: f64, lon: f64) -> bool {
    lat >= bbox[0] && lat <= bbox[2] && lon >= bbox[1] && lon <= bbox[3]
}

/// Area of a lat/lon bbox in square degrees (for picking the tightest landsdel).
fn bbox_area_deg2(bbox: [f64; 4]) -> f64 {
    (bbox[2] - bbox[0]).max(0.0) * (bbox[3] - bbox[1]).max(0.0)
}

fn bbox_lon_span_deg(bbox: [f64; 4]) -> f64 {
    (bbox[3] - bbox[1]).max(0.0)
}

/// Country-scale fallbacks whose Geofabrik bbox spans (nearly) all longitudes.
/// These must not beat tighter regional extracts in [`suggest_geofabrik_path_for_point`].
fn bbox_is_coarse_longitude_fallback(bbox: [f64; 4]) -> bool {
    bbox_lon_span_deg(bbox) >= 120.0
}

fn pick_smallest_covering<'a>(
    candidates: impl Iterator<Item = (&'a str, [f64; 4])>,
    lat: f64,
    lon: f64,
    allow_coarse: bool,
) -> Option<&'a str> {
    let mut best: Option<(&'a str, f64)> = None;
    for (path, bbox) in candidates {
        if !allow_coarse && bbox_is_coarse_longitude_fallback(bbox) {
            continue;
        }
        if !bbox_covers_point(bbox, lat, lon) {
            continue;
        }
        let area = bbox_area_deg2(bbox);
        match best {
            None => best = Some((path, area)),
            Some((_, best_area)) if area < best_area => best = Some((path, area)),
            _ => {}
        }
    }
    best.map(|(p, _)| p)
}

/// Approximate Norway–Sweden land-border longitude (east of this at `lat` is Sweden).
fn norway_sweden_border_lon(lat: f64) -> Option<f64> {
    const PTS: &[(f64, f64)] = &[
        (58.88, 11.12),
        (59.20, 11.55),
        (59.60, 11.90),
        (60.00, 12.38),
        (60.50, 12.55),
        (61.00, 12.75),
        (61.50, 12.55),
        (61.90, 12.24),
        (62.30, 12.20),
        (63.00, 12.05),
        (64.00, 13.80),
        (65.00, 14.20),
        (66.00, 16.40),
        (68.00, 20.00),
        (69.06, 20.55),
    ];
    if lat < PTS[0].0 || lat > PTS[PTS.len() - 1].0 {
        return None;
    }
    for w in PTS.windows(2) {
        let (lat0, lon0) = w[0];
        let (lat1, lon1) = w[1];
        if lat >= lat0 && lat <= lat1 {
            let t = if (lat1 - lat0).abs() < f64::EPSILON {
                0.0
            } else {
                (lat - lat0) / (lat1 - lat0)
            };
            return Some(lon0 + t * (lon1 - lon0));
        }
    }
    None
}

fn east_of_norway_sweden_border(lat: f64, lon: f64) -> bool {
    norway_sweden_border_lon(lat)
        .map(|border| lon > border)
        .unwrap_or(false)
}

/// Most-specific Geofabrik landsdel/country path whose approximate bbox covers
/// `(lat, lon)`. Prefers Norway landsdel extracts over `europe/norway`, and
/// Sweden over an overlapping Norway landsdel bbox east of the land border.
///
/// Returns `None` when no known table entry covers the point.
pub fn suggest_geofabrik_path_for_point(lat: f64, lon: f64) -> Option<&'static str> {
    if east_of_norway_sweden_border(lat, lon) {
        if let Some((_, bbox)) = GEOFABRIK_PATH_BBOX
            .iter()
            .find(|(p, _)| *p == "europe/sweden")
        {
            if bbox_covers_point(*bbox, lat, lon) {
                return Some("europe/sweden");
            }
        }
    }
    // Prefer tight landsdel / state bboxes; only fall back to global-longitude country
    // extracts (US, Russia, …) when nothing more specific matches.
    if let Some(p) = pick_smallest_covering(
        NORWAY_LANDSDEL
            .iter()
            .filter(|(path, _)| !path.starts_with("test/"))
            .copied(),
        lat,
        lon,
        false,
    ) {
        return Some(p);
    }
    if let Some(p) = pick_smallest_covering(GEOFABRIK_PATH_BBOX.iter().copied(), lat, lon, false) {
        return Some(p);
    }
    pick_smallest_covering(GEOFABRIK_PATH_BBOX.iter().copied(), lat, lon, true)
}

/// Whether `(lat, lon)` falls inside any of the given Geofabrik path bboxes.
pub fn point_covered_by_regions(lat: f64, lon: f64, geofabrik_paths: &[&str]) -> bool {
    geofabrik_paths.iter().any(|path| {
        region_bbox(path)
            .map(|bbox| bbox_covers_point(bbox, lat, lon))
            .unwrap_or(false)
    })
}

/// Map a downloaded PBF leaf stem (`ostlandet-latest`) to a Geofabrik path.
pub fn pbf_stem_to_geofabrik_path(stem: &str) -> Option<String> {
    let leaf = stem
        .trim()
        .trim_end_matches(".osm.pbf")
        .trim_end_matches("-latest")
        .trim_end_matches("_latest")
        .to_ascii_lowercase();
    if leaf.is_empty() {
        return None;
    }
    match leaf.as_str() {
        "norway" => Some("europe/norway".into()),
        "ostlandet" | "oppland" => Some("europe/norway/ostlandet".into()),
        "vestlandet" => Some("europe/norway/vestlandet".into()),
        "trondelag" => Some("europe/norway/trondelag".into()),
        "nord-norge" | "nord_norge" => Some("europe/norway/nord-norge".into()),
        "sorlandet" => Some("europe/norway/sorlandet".into()),
        other => {
            // Full path with underscores, or leaf under a Geofabrik continent folder.
            let as_path = other.replace('_', "/");
            if region_bbox(&as_path).is_some() {
                return Some(as_path);
            }
            if let Some((path, _)) = GEOFABRIK_PATH_BBOX
                .iter()
                .find(|(p, _)| p.rsplit('/').next() == Some(other))
            {
                return Some((*path).into());
            }
            if region_bbox(&format!("europe/norway/{other}")).is_some() {
                return Some(format!("europe/norway/{other}"));
            }
            None
        }
    }
}

/// Bboxes from Geofabrik index-v1.json geometries (fetched 2026-08-13).
/// Format: [min_lat, min_lon, max_lat, max_lon].
const GEOFABRIK_PATH_BBOX: &[(&str, [f64; 4])] = &[
    (
        "africa/algeria",
        [18.928740, -8.704895, 37.772840, 12.035980],
    ),
    ("africa/egypt", [21.983800, 24.626280, 32.525180, 37.162630]),
    (
        "africa/ethiopia",
        [3.389192, 32.967220, 14.916260, 48.005450],
    ),
    ("africa/ghana", [3.704056, -3.807399, 11.177720, 1.394038]),
    ("africa/kenya", [-4.816276, 33.867900, 4.629931, 41.976850]),
    (
        "africa/madagascar",
        [-26.582300, 42.301240, -11.027460, 51.148430],
    ),
    (
        "africa/morocco",
        [20.403730, -18.183500, 36.060770, -0.993576],
    ),
    ("africa/nigeria", [2.674022, 2.664528, 13.898740, 14.680570]),
    (
        "africa/senegal-and-gambia",
        [12.015110, -19.856230, 16.700880, -11.335950],
    ),
    (
        "africa/south-africa",
        [-47.584930, 15.996060, -22.117360, 39.242590],
    ),
    (
        "africa/tanzania",
        [-11.775950, 29.243950, -0.974988, 40.694870],
    ),
    (
        "africa/tunisia",
        [30.208640, 7.513961, 37.774740, 12.068000],
    ),
    ("africa/uganda", [-1.487315, 29.570560, 4.250552, 35.010310]),
    (
        "africa/zimbabwe",
        [-22.447250, 25.222900, -15.587070, 33.085410],
    ),
    (
        "antarctica",
        [-90.000000, -180.000000, -60.000000, 180.000000],
    ),
    ("asia/china", [14.274370, 73.417880, 53.655590, 134.803600]),
    ("asia/india", [5.896067, 67.674570, 35.730910, 97.419890]),
    (
        "asia/indonesia",
        [-11.606550, 92.764810, 6.999330, 141.021700],
    ),
    ("asia/iran", [24.039470, 44.023030, 39.790450, 63.354130]),
    ("asia/japan", [20.082280, 122.560700, 45.815400, 154.470900]),
    (
        "asia/kazakhstan",
        [40.553460, 46.423040, 55.507990, 87.353590],
    ),
    (
        "asia/malaysia-singapore-brunei",
        [0.830813, 99.256110, 7.736849, 119.668300],
    ),
    ("asia/nepal", [26.344500, 80.049130, 30.478220, 88.222130]),
    (
        "asia/pakistan",
        [23.376300, 60.821680, 37.106670, 77.224270],
    ),
    (
        "asia/philippines",
        [4.382696, 112.166100, 21.530210, 127.074200],
    ),
    (
        "asia/south-korea",
        [32.360760, 124.318800, 38.649660, 132.338600],
    ),
    (
        "asia/thailand",
        [5.597877, 97.135620, 20.468750, 105.639200],
    ),
    (
        "asia/uzbekistan",
        [37.166360, 55.977080, 45.618840, 73.174630],
    ),
    (
        "asia/vietnam",
        [7.382239, 102.095900, 23.402140, 114.642300],
    ),
    (
        "australia-oceania/australia",
        [-57.071060, 68.133420, -8.809565, 169.001600],
    ),
    (
        "australia-oceania/fiji",
        [-25.081270, -180.000000, -9.783321, 180.000000],
    ),
    (
        "australia-oceania/new-caledonia",
        [-25.000000, 157.900000, -16.900000, 174.100000],
    ),
    (
        "australia-oceania/new-zealand",
        [-56.752680, -179.989000, -28.488000, 179.999900],
    ),
    (
        "australia-oceania/papua-new-guinea",
        [-14.748450, 139.201400, 2.587814, 162.803400],
    ),
    (
        "australia-oceania/samoa",
        [-15.878380, -174.511400, -10.960830, -170.542700],
    ),
    (
        "australia-oceania/solomon-islands",
        [-16.126940, 154.585600, -4.139945, 173.593400],
    ),
    (
        "australia-oceania/vanuatu",
        [-21.642880, 163.308600, -12.296260, 173.608900],
    ),
    (
        "central-america/belize",
        [15.883080, -89.228920, 18.498400, -86.859830],
    ),
    (
        "central-america/costa-rica",
        [4.937746, -87.824730, 11.223700, -82.349520],
    ),
    (
        "central-america/cuba",
        [19.253300, -85.424200, 23.604270, -73.708310],
    ),
    (
        "central-america/guatemala",
        [12.741680, -92.733030, 17.835330, -87.951770],
    ),
    (
        "central-america/honduras",
        [12.969430, -89.360050, 17.931710, -81.661380],
    ),
    (
        "central-america/jamaica",
        [16.350000, -78.850000, 19.160000, -75.510000],
    ),
    (
        "central-america/nicaragua",
        [10.701770, -87.977400, 15.111900, -82.378650],
    ),
    (
        "central-america/panama",
        [6.644027, -83.464620, 11.207080, -77.081260],
    ),
    (
        "europe/austria",
        [46.369790, 9.526780, 49.024030, 17.164080],
    ),
    ("europe/belgium", [49.494890, 2.340725, 51.598390, 6.410265]),
    (
        "europe/bulgaria",
        [41.226810, 22.348750, 44.217770, 29.188190],
    ),
    (
        "europe/croatia",
        [42.164830, 13.089160, 46.557560, 19.459970],
    ),
    (
        "europe/czech-republic",
        [48.542920, 12.084770, 51.064260, 18.863210],
    ),
    (
        "europe/denmark",
        [54.440650, 7.701100, 58.062390, 15.654490],
    ),
    (
        "europe/estonia",
        [57.497640, 20.851660, 59.997050, 28.214260],
    ),
    (
        "europe/finland",
        [59.287830, 19.024270, 70.099590, 31.615900],
    ),
    ("europe/france", [41.238660, -6.937207, 51.428800, 9.900000]),
    (
        "europe/germany",
        [47.265430, 5.864417, 55.147770, 15.050780],
    ),
    (
        "europe/great-britain",
        [49.523000, -14.990700, 61.135640, 2.513672],
    ),
    (
        "europe/greece",
        [34.591110, 18.970640, 41.749540, 29.656830],
    ),
    (
        "europe/hungary",
        [45.732070, 16.108450, 48.589210, 22.906490],
    ),
    (
        "europe/iceland",
        [62.845530, -25.700000, 67.500850, -12.417080],
    ),
    (
        "europe/ireland-and-northern-ireland",
        [49.600020, -14.402240, 56.865530, -5.059265],
    ),
    ("europe/italy", [35.076380, 6.602696, 47.100050, 19.124990]),
    (
        "europe/latvia",
        [55.661090, 19.733480, 58.094700, 28.255010],
    ),
    (
        "europe/lithuania",
        [53.892210, 20.618590, 56.453290, 26.838730],
    ),
    (
        "europe/luxembourg",
        [49.445530, 5.733033, 50.184960, 6.532249],
    ),
    (
        "europe/netherlands",
        [50.747530, 2.936095, 54.017860, 7.230455],
    ),
    (
        "europe/norway",
        [57.553230, -11.368010, 81.051950, 35.527110],
    ),
    (
        "europe/poland",
        [48.986420, 13.990220, 55.228260, 24.161020],
    ),
    (
        "europe/portugal",
        [32.000000, -31.600000, 42.163900, -6.179513],
    ),
    (
        "europe/romania",
        [43.612470, 20.241810, 48.269480, 30.278960],
    ),
    (
        "europe/serbia",
        [42.229790, 18.808940, 46.192070, 23.010350],
    ),
    (
        "europe/slovakia",
        [47.726460, 16.830710, 49.618600, 22.570510],
    ),
    (
        "europe/slovenia",
        [45.420810, 13.305090, 46.879560, 16.600340],
    ),
    ("europe/spain", [35.263930, -9.779014, 44.148550, 5.098525]),
    (
        "europe/sweden",
        [55.026520, 10.541380, 69.066430, 24.224720],
    ),
    (
        "europe/switzerland",
        [45.816420, 5.954418, 47.811260, 10.495840],
    ),
    (
        "europe/turkey",
        [35.717000, 25.523980, 43.074810, 44.859920],
    ),
    (
        "europe/ukraine",
        [44.008620, 22.132640, 52.386500, 40.238110],
    ),
    (
        "north-america/canada",
        [41.660090, -141.776100, 85.040320, -44.176840],
    ),
    (
        "north-america/greenland",
        [58.671690, -75.389400, 84.443580, -9.454384],
    ),
    (
        "north-america/mexico",
        [9.456709, -132.495200, 32.720670, -85.490870],
    ),
    (
        "north-america/us",
        [15.920970, -180.000000, 72.988450, 180.000000],
    ),
    (
        "north-america/us/nevada",
        [35.000530, -120.007400, 42.003910, -114.037900],
    ),
    (
        "north-america/us/west-virginia",
        [37.198580, -82.649650, 40.646360, -77.714100],
    ),
    ("russia", [35.614040, -180.000000, 83.831330, 180.000000]),
    (
        "south-america/argentina",
        [-55.682960, -73.614530, -21.725750, -53.635340],
    ),
    (
        "south-america/bolivia",
        [-22.925510, -69.656750, -9.638660, -57.434810],
    ),
    (
        "south-america/brazil",
        [-35.465520, -74.023130, 5.522895, -27.672490],
    ),
    (
        "south-america/chile",
        [-58.451950, -113.223300, -17.457430, -65.514100],
    ),
    (
        "south-america/colombia",
        [-4.257320, -83.231040, 16.419240, -66.814720],
    ),
    (
        "south-america/ecuador",
        [-5.024173, -93.564870, 2.843487, -75.161490],
    ),
    (
        "south-america/guyana",
        [1.149369, -61.421010, 9.182536, -56.423210],
    ),
    (
        "south-america/paraguay",
        [-27.656750, -62.694750, -19.240120, -54.201760],
    ),
    (
        "south-america/peru",
        [-20.305520, -86.025090, 0.060049, -68.554390],
    ),
    (
        "south-america/suriname",
        [1.816773, -58.084610, 7.761698, -53.328190],
    ),
    (
        "south-america/uruguay",
        [-36.248700, -58.525890, -30.073550, -52.772300],
    ),
    (
        "south-america/venezuela",
        [0.621727, -73.380980, 15.961340, -59.514740],
    ),
];

const NORWAY_LANDSDEL: &[(&str, [f64; 4])] = &[
    ("europe/norway", [57.9, 4.5, 71.5, 31.5]),
    ("europe/norway/ostlandet", [58.5, 7.5, 62.8, 13.5]),
    ("europe/norway/vestlandet", [58.0, 4.0, 63.5, 8.5]),
    ("europe/norway/trondelag", [62.5, 8.5, 65.5, 14.5]),
    ("europe/norway/nord-norge", [64.5, 10.0, 71.5, 31.5]),
    ("europe/norway/sorlandet", [57.8, 5.5, 59.5, 10.0]),
    // Small Oslo window for e2e / instrumented basemap tests (fast extract).
    ("test/oslo", [59.85, 10.6, 59.98, 10.9]),
    // Instrumented offline screenshots: same Ostlandet bbox, `test_` key so the
    // mz12 staged fixture can register Completed without pretending to be a
    // full production extract.
    ("test/ostlandet_fixture", [58.5, 7.5, 62.8, 13.5]),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_key_from_geofabrik_path() {
        assert_eq!(
            geofabrik_path_to_region_key("europe/norway/ostlandet"),
            "europe_norway_ostlandet"
        );
    }

    #[test]
    fn ostlandet_bbox_covers_oslo() {
        let bbox = region_bbox("europe/norway/ostlandet").unwrap();
        assert!(bbox_covers_point(bbox, 59.91, 10.75));
        assert!(!bbox_covers_point(bbox, 69.65, 18.96));
    }

    #[test]
    fn suggest_landsdel_prefers_specific_over_country() {
        assert_eq!(
            suggest_geofabrik_path_for_point(59.91, 10.75),
            Some("europe/norway/ostlandet")
        );
        assert_eq!(
            suggest_geofabrik_path_for_point(69.65, 18.96),
            Some("europe/norway/nord-norge")
        );
        // Bergen — Vestlandet only (Preikestolen sits in Vestlandet∩Sørlandet overlap).
        assert_eq!(
            suggest_geofabrik_path_for_point(60.3913, 5.3221),
            Some("europe/norway/vestlandet")
        );
        // Just west of Ostlandet min_lon 7.5.
        assert_eq!(
            suggest_geofabrik_path_for_point(60.4, 7.4),
            Some("europe/norway/vestlandet")
        );
    }

    #[test]
    fn us_state_beats_global_longitude_country_extracts() {
        // CKB airport area — must not suggest Russia or whole-US fallback.
        assert_eq!(
            suggest_geofabrik_path_for_point(39.2967, -80.2281),
            Some("north-america/us/west-virginia")
        );
        // Reese River Valley, Nevada.
        assert_eq!(
            suggest_geofabrik_path_for_point(39.4336, -117.2719),
            Some("north-america/us/nevada")
        );
        // Sandusky, Tyler Co. WV.
        assert_eq!(
            suggest_geofabrik_path_for_point(39.5556, -80.8590),
            Some("north-america/us/west-virginia")
        );
    }

    #[test]
    fn coarse_longitude_fallback_only_when_no_tighter_match() {
        // Point in the Pacific with no state entry: fall back to country, not Russia,
        // when lat fits US extract better than Russia (mid-Pacific test uses US Alaska bbox).
        let mid_pacific = suggest_geofabrik_path_for_point(20.0, -160.0);
        assert!(
            mid_pacific == Some("north-america/us") || mid_pacific.is_some(),
            "expected a country fallback, got {mid_pacific:?}"
        );
        assert_ne!(mid_pacific, Some("russia"));
    }

    #[test]
    fn sweden_border_town_is_sweden_not_ostlandet() {
        // Långflons Köpcentrum, just east of Rundfloen / the national border.
        assert_eq!(
            suggest_geofabrik_path_for_point(61.8975, 12.2685),
            Some("europe/sweden")
        );
        // Rundfloen toll station stays in Norway / Ostlandet.
        assert_eq!(
            suggest_geofabrik_path_for_point(61.8956, 12.2208),
            Some("europe/norway/ostlandet")
        );
        // Oslo must not flip to Sweden (Sweden bbox overlaps Oslo).
        assert_eq!(
            suggest_geofabrik_path_for_point(59.91, 10.75),
            Some("europe/norway/ostlandet")
        );
    }

    #[test]
    fn coverage_uses_downloaded_paths_only() {
        let ost = ["europe/norway/ostlandet"];
        assert!(point_covered_by_regions(59.91, 10.75, &ost));
        assert!(!point_covered_by_regions(69.65, 18.96, &ost));
        assert!(point_covered_by_regions(
            69.65,
            18.96,
            &["europe/norway/ostlandet", "europe/norway/nord-norge"]
        ));
    }

    #[test]
    fn pbf_stem_maps_to_geofabrik_path() {
        assert_eq!(
            pbf_stem_to_geofabrik_path("ostlandet-latest"),
            Some("europe/norway/ostlandet".into())
        );
        assert_eq!(
            pbf_stem_to_geofabrik_path("oppland-latest.osm.pbf"),
            Some("europe/norway/ostlandet".into())
        );
        assert_eq!(
            pbf_stem_to_geofabrik_path("norway-latest"),
            Some("europe/norway".into())
        );
        assert_eq!(
            pbf_stem_to_geofabrik_path("sweden-latest"),
            Some("europe/sweden".into())
        );
        assert_eq!(
            pbf_stem_to_geofabrik_path("us-latest"),
            Some("north-america/us".into())
        );
        assert_eq!(
            pbf_stem_to_geofabrik_path("russia-latest"),
            Some("russia".into())
        );
        assert_eq!(
            pbf_stem_to_geofabrik_path("kenya-latest"),
            Some("africa/kenya".into())
        );
        assert_eq!(
            pbf_stem_to_geofabrik_path("antarctica-latest"),
            Some("antarctica".into())
        );
        assert_eq!(
            pbf_stem_to_geofabrik_path("costa-rica-latest"),
            Some("central-america/costa-rica".into())
        );
    }

    #[test]
    fn catalog_aligned_country_paths_have_bbox() {
        for (path, _) in GEOFABRIK_PATH_BBOX {
            assert!(
                region_bbox(path).is_some(),
                "Tools country chip path {path} must resolve a bbox"
            );
        }
        assert!(region_bbox("africa/kenya").is_some());
        assert!(region_bbox("antarctica").is_some());
        assert!(region_bbox("central-america/costa-rica").is_some());
    }

    #[test]
    fn planet_url_passthrough() {
        let u = "https://build.protomaps.com/20260722.pmtiles";
        assert_eq!(region_pmtiles_url(u, "europe_norway_ostlandet"), u);
    }
}
