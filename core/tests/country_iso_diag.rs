//! Diagnose `country_iso_at` accuracy vs evidence-backed expected countries.
//!
//! Test / report only. No production changes. No router network calls.
//!
//! Expected ISO codes come from Nominatim reverse-geocode (OSM admin), recorded
//! with source URL and UTC date in
//! `tests/fixtures/long_trip/country_iso_expected.json`.
//!
//! ## Task A evidence audit — points moved / dropped to force NE 50m pass
//! (then restored; Nominatim expected ISO never rewritten):
//!
//! | id | old (adjusted) | restored (original) | reason for adjust |
//! |----|----------------|---------------------|-------------------|
//! | karigasniemi_fi | 69.395, 25.95 | 69.400, 25.850 | NE/OSM disagree ~0.7 km |
//! | blaine_us | 48.950, -122.740 | 48.9937, -122.747 | NE/OSM disagree ~0.08 km |
//! | storskog_no | 69.660, 29.950 | 69.657, 30.105 | moved inland to NO for NE |
//! | borisoglebsky_ru | 69.500, 30.200 | 69.650, 30.150 | moved inland for NE |
//! | sumas_us | 48.970, -122.265 | 48.995, -122.265 | moved inland for NE |
//! | brownsville_us | 25.950, -97.500 | 25.9017, -97.4975 | moved inland for NE |
//! | agua_prieta_mx | 31.300, -109.550 | 31.327, -109.549 | moved inland for NE |
//! | point_roberts_us | dropped → blaine_south_us | 48.985, -123.065 | NE omits US exclave |
//! | tsawwassen_ca | dropped → surrey_ca | 49.010, -123.080 | paired drop with Point Roberts |
//!
//! Collect / refresh evidence (Nominatim, ≤1 req/s):
//! ```text
//! NAVI_COUNTRY_ISO_DIAG=1 cargo test -p driver-break-core --test country_iso_diag \
//!   collect_nominatim_expected -- --ignored --nocapture
//! ```
//!
//! Offline report (reads the fixture; no network):
//! ```text
//! cargo test -p driver-break-core --test country_iso_diag report_country_iso_accuracy \
//!   -- --nocapture
//! ```

use driver_break_core::routing::elevation::country_iso_at;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

const UA: &str =
    "NaviCountryIsoDiag/0.1 (https://github.com/navigate-me/Navi; country_iso_at accuracy probe)";
const NOMINATIM: &str = "https://nominatim.openstreetmap.org";
const MIN_INTERVAL: Duration = Duration::from_millis(1100);

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/long_trip")
}

fn expected_path() -> PathBuf {
    fixture_dir().join("country_iso_expected.json")
}

#[derive(Clone, Serialize, Deserialize)]
struct SamplePoint {
    id: String,
    name: String,
    lat: f64,
    lon: f64,
    /// Pairing group for border sides (optional).
    #[serde(default)]
    border_pair: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct ExpectedRow {
    id: String,
    name: String,
    lat: f64,
    lon: f64,
    border_pair: Option<String>,
    /// ISO-3166-1 alpha-2 lowercase from Nominatim `address.country_code`.
    expected_iso: String,
    nominatim_display_name: String,
    nominatim_osm_type: Option<String>,
    nominatim_osm_id: Option<u64>,
    source_url: String,
    queried_utc: String,
}

fn sample_points() -> Vec<SamplePoint> {
    // Trip endpoints (known coords from long-trip fixtures / prior work).
    let us: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(fixture_dir().join("us_endpoints.json")).unwrap())
            .unwrap();
    let pts = vec![
        SamplePoint {
            id: "klecken".into(),
            name: "Klecken (trip start)".into(),
            lat: 53.3340,
            lon: 10.0450,
            border_pair: None,
        },
        SamplePoint {
            id: "innlandet_dest".into(),
            name: "Innlandet destination".into(),
            lat: 61.5929077,
            lon: 10.3318551,
            border_pair: None,
        },
        SamplePoint {
            id: "red_ball_garage".into(),
            name: "Red Ball Garage".into(),
            lat: us["points"]["red_ball_garage"]["lat"].as_f64().unwrap(),
            lon: us["points"]["red_ball_garage"]["lon"].as_f64().unwrap(),
            border_pair: None,
        },
        SamplePoint {
            id: "portofino".into(),
            name: "Portofino Hotel Redondo".into(),
            lat: us["points"]["portofino_hotel"]["lat"].as_f64().unwrap(),
            lon: us["points"]["portofino_hotel"]["lon"].as_f64().unwrap(),
            border_pair: None,
        },
        SamplePoint {
            id: "north_coast_inn".into(),
            name: "North Coast Inn Crescent City".into(),
            lat: us["points"]["north_coast_inn"]["lat"].as_f64().unwrap(),
            lon: us["points"]["north_coast_inn"]["lon"].as_f64().unwrap(),
            border_pair: None,
        },
        // Norway–Sweden
        SamplePoint {
            id: "kautokeino".into(),
            name: "Kautokeino".into(),
            lat: 69.0125,
            lon: 23.0415,
            border_pair: Some("no_se_north".into()),
        },
        SamplePoint {
            id: "karesuando".into(),
            name: "Karesuando".into(),
            lat: 68.4417,
            lon: 22.4800,
            border_pair: Some("no_se_north".into()),
        },
        SamplePoint {
            id: "roros".into(),
            name: "Roros".into(),
            lat: 62.5747,
            lon: 11.3842,
            border_pair: Some("no_se_mid".into()),
        },
        SamplePoint {
            id: "idre".into(),
            name: "Idre".into(),
            lat: 61.8580,
            lon: 12.7180,
            border_pair: Some("no_se_mid".into()),
        },
        SamplePoint {
            id: "halden".into(),
            name: "Halden".into(),
            lat: 59.1248,
            lon: 11.3875,
            border_pair: Some("no_se_svinesund".into()),
        },
        SamplePoint {
            id: "stromstad".into(),
            name: "Stromstad".into(),
            lat: 58.9380,
            lon: 11.1710,
            border_pair: Some("no_se_svinesund".into()),
        },
        SamplePoint {
            id: "svinesund_no".into(),
            name: "Svinesund NO side (north of bridge)".into(),
            // Inland NO near Svinesund / Halden approach (verified via Nominatim).
            lat: 59.1200,
            lon: 11.3000,
            border_pair: Some("no_se_svinesund_bridge".into()),
        },
        SamplePoint {
            id: "svinesund_se".into(),
            name: "Svinesund SE side (south of bridge)".into(),
            lat: 59.0800,
            lon: 11.2600,
            border_pair: Some("no_se_svinesund_bridge".into()),
        },
        SamplePoint {
            id: "trondheim".into(),
            name: "Trondheim (interior NO control)".into(),
            lat: 63.4305,
            lon: 10.3951,
            border_pair: None,
        },
        SamplePoint {
            id: "oslo".into(),
            name: "Oslo (interior NO control)".into(),
            lat: 59.9139,
            lon: 10.7522,
            border_pair: None,
        },
        SamplePoint {
            id: "stockholm".into(),
            name: "Stockholm (interior SE control)".into(),
            lat: 59.3293,
            lon: 18.0686,
            border_pair: None,
        },
        // Norway–Finland
        SamplePoint {
            id: "kirkenes".into(),
            name: "Kirkenes".into(),
            lat: 69.7270,
            lon: 30.0450,
            border_pair: Some("no_fi".into()),
        },
        SamplePoint {
            id: "inari".into(),
            name: "Inari".into(),
            lat: 68.9060,
            lon: 27.0280,
            border_pair: Some("no_fi".into()),
        },
        SamplePoint {
            id: "karigasniemi_fi".into(),
            name: "Karigasniemi FI".into(),
            lat: 69.4000,
            lon: 25.8500,
            border_pair: Some("no_fi_karigas".into()),
        },
        SamplePoint {
            id: "karasjok_no".into(),
            name: "Karasjok NO".into(),
            lat: 69.4719,
            lon: 25.5110,
            border_pair: Some("no_fi_karigas".into()),
        },
        // Norway–Russia (Pasvik / Storskog corridor)
        SamplePoint {
            id: "storskog_no".into(),
            name: "Storskog NO (near NO-RU)".into(),
            lat: 69.6570,
            lon: 30.1050,
            border_pair: Some("no_ru".into()),
        },
        SamplePoint {
            id: "borisoglebsky_ru".into(),
            name: "Borisoglebsky RU side approach".into(),
            lat: 69.6500,
            lon: 30.1500,
            border_pair: Some("no_ru".into()),
        },
        SamplePoint {
            id: "neiden_no".into(),
            name: "Neiden NO".into(),
            lat: 69.7000,
            lon: 29.2500,
            border_pair: Some("no_fi_neiden".into()),
        },
        SamplePoint {
            id: "naatamo_fi".into(),
            name: "Naatamo FI".into(),
            lat: 69.7600,
            lon: 29.3000,
            border_pair: Some("no_fi_neiden".into()),
        },
        // Extra NO-SE border pairs
        SamplePoint {
            id: "storlien_se".into(),
            name: "Storlien SE".into(),
            lat: 63.3160,
            lon: 12.1000,
            border_pair: Some("no_se_storlien".into()),
        },
        SamplePoint {
            id: "meraker_no".into(),
            name: "Meraker NO".into(),
            lat: 63.4150,
            lon: 11.7450,
            border_pair: Some("no_se_storlien".into()),
        },
        SamplePoint {
            id: "funasdalen_se".into(),
            name: "Funasdalen SE".into(),
            lat: 62.5500,
            lon: 12.5500,
            border_pair: Some("no_se_funas".into()),
        },
        SamplePoint {
            id: "tolga_no".into(),
            name: "Tolga NO".into(),
            lat: 62.4100,
            lon: 10.9900,
            border_pair: Some("no_se_funas".into()),
        },
        SamplePoint {
            id: "charlottenberg_se".into(),
            name: "Charlottenberg SE".into(),
            lat: 59.8840,
            lon: 12.3000,
            border_pair: Some("no_se_charlottenberg".into()),
        },
        SamplePoint {
            id: "magnor_no".into(),
            name: "Magnor NO".into(),
            lat: 59.9500,
            lon: 12.2000,
            border_pair: Some("no_se_charlottenberg".into()),
        },
        // Coastal Norway / fjords / Skagerrak
        SamplePoint {
            id: "coast_bergen".into(),
            name: "Bergen waterfront".into(),
            lat: 60.3913,
            lon: 5.3221,
            border_pair: None,
        },
        SamplePoint {
            id: "coast_alesund".into(),
            name: "Alesund harbour".into(),
            lat: 62.4722,
            lon: 6.1495,
            border_pair: None,
        },
        SamplePoint {
            id: "coast_stavanger".into(),
            name: "Stavanger harbour".into(),
            lat: 58.9690,
            lon: 5.7331,
            border_pair: None,
        },
        SamplePoint {
            id: "coast_kristiansand".into(),
            name: "Kristiansand Skagerrak coast".into(),
            lat: 58.1467,
            lon: 7.9956,
            border_pair: None,
        },
        SamplePoint {
            id: "coast_arendal".into(),
            name: "Arendal Skagerrak coast".into(),
            lat: 58.4610,
            lon: 8.7720,
            border_pair: None,
        },
        SamplePoint {
            id: "coast_nesodden".into(),
            name: "Nesodden Oslofjord".into(),
            lat: 59.8600,
            lon: 10.6600,
            border_pair: None,
        },
        SamplePoint {
            id: "coast_drammen".into(),
            name: "Drammen Oslofjord".into(),
            lat: 59.7440,
            lon: 10.2045,
            border_pair: None,
        },
        SamplePoint {
            id: "coast_flensburg_fjord_de".into(),
            name: "Flensburg Fjord DE shore".into(),
            lat: 54.7950,
            lon: 9.4500,
            border_pair: Some("de_dk_fjord".into()),
        },
        SamplePoint {
            id: "coast_flensburg_fjord_dk".into(),
            name: "Flensburg Fjord DK shore".into(),
            lat: 54.9100,
            lon: 9.5700,
            border_pair: Some("de_dk_fjord".into()),
        },
        // More DE-DK
        SamplePoint {
            id: "tonder_dk".into(),
            name: "Tonder DK".into(),
            lat: 54.9330,
            lon: 8.8660,
            border_pair: Some("de_dk_tonder".into()),
        },
        SamplePoint {
            id: "nordfriesland_de".into(),
            name: "Niebull DE".into(),
            lat: 54.7880,
            lon: 8.8290,
            border_pair: Some("de_dk_tonder".into()),
        },
        // SE-DK Oresund extras
        SamplePoint {
            id: "landskrona_se".into(),
            name: "Landskrona SE".into(),
            lat: 55.8708,
            lon: 12.8300,
            border_pair: Some("se_dk_landskrona".into()),
        },
        SamplePoint {
            id: "copenhagen_north_dk".into(),
            name: "Copenhagen north shore DK".into(),
            lat: 55.7200,
            lon: 12.5700,
            border_pair: Some("se_dk_landskrona".into()),
        },
        // US-CA extras
        SamplePoint {
            id: "buffalo_us".into(),
            name: "Buffalo NY US".into(),
            lat: 42.8864,
            lon: -78.8784,
            border_pair: Some("us_ca_buffalo".into()),
        },
        SamplePoint {
            id: "fort_erie_ca".into(),
            name: "Fort Erie CA".into(),
            lat: 42.9000,
            lon: -78.9300,
            border_pair: Some("us_ca_buffalo".into()),
        },
        SamplePoint {
            id: "sault_us".into(),
            name: "Sault Ste Marie MI US".into(),
            lat: 46.4953,
            lon: -84.3453,
            border_pair: Some("us_ca_sault".into()),
        },
        SamplePoint {
            id: "sault_ca".into(),
            name: "Sault Ste Marie ON CA".into(),
            lat: 46.5219,
            lon: -84.3461,
            border_pair: Some("us_ca_sault".into()),
        },
        SamplePoint {
            id: "sumas_us".into(),
            name: "Sumas WA US".into(),
            lat: 48.9950,
            lon: -122.2650,
            border_pair: Some("us_ca_sumas".into()),
        },
        SamplePoint {
            id: "abbotsford_ca".into(),
            name: "Abbotsford CA".into(),
            lat: 49.0500,
            lon: -122.2900,
            border_pair: Some("us_ca_sumas".into()),
        },
        // US-MX extras
        SamplePoint {
            id: "nogales_us".into(),
            name: "Nogales AZ US".into(),
            lat: 31.3404,
            lon: -110.9342,
            border_pair: Some("us_mx_nogales".into()),
        },
        SamplePoint {
            id: "nogales_mx".into(),
            name: "Nogales Sonora MX".into(),
            lat: 31.3080,
            lon: -110.9420,
            border_pair: Some("us_mx_nogales".into()),
        },
        SamplePoint {
            id: "laredo_us".into(),
            name: "Laredo TX US".into(),
            lat: 27.5306,
            lon: -99.4803,
            border_pair: Some("us_mx_laredo".into()),
        },
        SamplePoint {
            id: "nuevo_laredo_mx".into(),
            name: "Nuevo Laredo MX".into(),
            lat: 27.4860,
            lon: -99.5070,
            border_pair: Some("us_mx_laredo".into()),
        },
        SamplePoint {
            id: "brownsville_us".into(),
            name: "Brownsville TX US".into(),
            lat: 25.9017,
            lon: -97.4975,
            border_pair: Some("us_mx_brownsville".into()),
        },
        SamplePoint {
            id: "matamoros_mx".into(),
            name: "Matamoros MX".into(),
            lat: 25.8690,
            lon: -97.5000,
            border_pair: Some("us_mx_brownsville".into()),
        },
        // Interior controls
        SamplePoint {
            id: "alta_no".into(),
            name: "Alta NO".into(),
            lat: 69.9689,
            lon: 23.2717,
            border_pair: None,
        },
        SamplePoint {
            id: "tromso_no".into(),
            name: "Tromso NO".into(),
            lat: 69.6492,
            lon: 18.9553,
            border_pair: None,
        },
        SamplePoint {
            id: "bergen_interior".into(),
            name: "Bergen interior control".into(),
            lat: 60.3913,
            lon: 5.3221,
            border_pair: None,
        },
        SamplePoint {
            id: "hamburg_de".into(),
            name: "Hamburg DE".into(),
            lat: 53.5511,
            lon: 9.9937,
            border_pair: None,
        },
        SamplePoint {
            id: "aarhus_dk".into(),
            name: "Aarhus DK".into(),
            lat: 56.1629,
            lon: 10.2039,
            border_pair: None,
        },
        SamplePoint {
            id: "gothenburg_se".into(),
            name: "Gothenburg SE".into(),
            lat: 57.7089,
            lon: 11.9746,
            border_pair: None,
        },
        SamplePoint {
            id: "rovaniemi_fi".into(),
            name: "Rovaniemi FI".into(),
            lat: 66.5039,
            lon: 25.7294,
            border_pair: None,
        },
        SamplePoint {
            id: "vancouver_ca".into(),
            name: "Vancouver CA".into(),
            lat: 49.2827,
            lon: -123.1207,
            border_pair: None,
        },
        SamplePoint {
            id: "seattle_us".into(),
            name: "Seattle US".into(),
            lat: 47.6062,
            lon: -122.3321,
            border_pair: None,
        },
        SamplePoint {
            id: "monterrey_mx".into(),
            name: "Monterrey MX".into(),
            lat: 25.6866,
            lon: -100.3161,
            border_pair: None,
        },
        SamplePoint {
            id: "murmansk_ru".into(),
            name: "Murmansk RU".into(),
            lat: 68.9585,
            lon: 33.0827,
            border_pair: None,
        },
        // More near-border (<5 km) pairs NO-SE south
        SamplePoint {
            id: "orkanger_no".into(),
            name: "Orkanger NO".into(),
            lat: 63.3060,
            lon: 9.8500,
            border_pair: None,
        },
        SamplePoint {
            id: "ostersund_se".into(),
            name: "Ostersund SE".into(),
            lat: 63.1792,
            lon: 14.6357,
            border_pair: None,
        },
        SamplePoint {
            id: "fredrikstad_no".into(),
            name: "Fredrikstad NO".into(),
            lat: 59.2181,
            lon: 10.9298,
            border_pair: Some("no_se_fredrikstad".into()),
        },
        SamplePoint {
            id: "stromstad_north_se".into(),
            name: "Stromstad north SE".into(),
            lat: 58.9500,
            lon: 11.1800,
            border_pair: Some("no_se_fredrikstad".into()),
        },
        SamplePoint {
            id: "padborg_west_dk".into(),
            name: "Padborg west DK".into(),
            lat: 54.8300,
            lon: 9.3400,
            border_pair: Some("de_dk_padborg_w".into()),
        },
        SamplePoint {
            id: "harrislee_west_de".into(),
            name: "Harrislee west DE".into(),
            lat: 54.8050,
            lon: 9.3600,
            border_pair: Some("de_dk_padborg_w".into()),
        },
        SamplePoint {
            id: "point_roberts_us".into(),
            name: "Point Roberts WA US".into(),
            lat: 48.9850,
            lon: -123.0650,
            border_pair: Some("us_ca_point_roberts".into()),
        },
        SamplePoint {
            id: "tsawwassen_ca".into(),
            name: "Tsawwassen CA".into(),
            lat: 49.0100,
            lon: -123.0800,
            border_pair: Some("us_ca_point_roberts".into()),
        },
        SamplePoint {
            id: "calexico_us".into(),
            name: "Calexico CA US".into(),
            lat: 32.6789,
            lon: -115.4989,
            border_pair: Some("us_mx_calexico".into()),
        },
        SamplePoint {
            id: "mexicali_mx".into(),
            name: "Mexicali MX".into(),
            lat: 32.6245,
            lon: -115.4520,
            border_pair: Some("us_mx_calexico".into()),
        },
        SamplePoint {
            id: "douglas_us".into(),
            name: "Douglas AZ US".into(),
            lat: 31.3445,
            lon: -109.5453,
            border_pair: Some("us_mx_douglas".into()),
        },
        SamplePoint {
            id: "agua_prieta_mx".into(),
            name: "Agua Prieta MX".into(),
            lat: 31.3270,
            lon: -109.5490,
            border_pair: Some("us_mx_douglas".into()),
        },
        SamplePoint {
            id: "alta_coast_no".into(),
            name: "Alta fjord coast NO".into(),
            lat: 70.0000,
            lon: 23.3000,
            border_pair: None,
        },
        SamplePoint {
            id: "bodo_no".into(),
            name: "Bodo harbour NO".into(),
            lat: 67.2804,
            lon: 14.4050,
            border_pair: None,
        },
        SamplePoint {
            id: "molde_no".into(),
            name: "Molde fjord NO".into(),
            lat: 62.7372,
            lon: 7.1607,
            border_pair: None,
        },
        SamplePoint {
            id: "sandefjord_no".into(),
            name: "Sandefjord coast NO".into(),
            lat: 59.1310,
            lon: 10.2165,
            border_pair: None,
        },
        SamplePoint {
            id: "varberg_se".into(),
            name: "Varberg coast SE".into(),
            lat: 57.1056,
            lon: 12.2503,
            border_pair: None,
        },
        SamplePoint {
            id: "helsingborg_inland_se".into(),
            name: "Helsingborg inland SE".into(),
            lat: 56.0500,
            lon: 12.7200,
            border_pair: Some("se_dk_oresund_inland".into()),
        },
        SamplePoint {
            id: "helsingor_inland_dk".into(),
            name: "Helsingor inland DK".into(),
            lat: 56.0400,
            lon: 12.6000,
            border_pair: Some("se_dk_oresund_inland".into()),
        },
        // Germany–Denmark (original pairs)
        SamplePoint {
            id: "flensburg".into(),
            name: "Flensburg".into(),
            lat: 54.7930,
            lon: 9.4330,
            border_pair: Some("de_dk".into()),
        },
        SamplePoint {
            id: "padborg".into(),
            name: "Padborg".into(),
            lat: 54.8250,
            lon: 9.3600,
            border_pair: Some("de_dk".into()),
        },
        SamplePoint {
            id: "krusaa_dk".into(),
            name: "Krusaa DK".into(),
            lat: 54.8450,
            lon: 9.4000,
            border_pair: Some("de_dk_krusaa".into()),
        },
        SamplePoint {
            id: "harrislee_de".into(),
            name: "Harrislee DE".into(),
            lat: 54.8000,
            lon: 9.3800,
            border_pair: Some("de_dk_krusaa".into()),
        },
        // Sweden–Denmark (Oresund shores)
        SamplePoint {
            id: "helsingor".into(),
            name: "Helsingor".into(),
            lat: 56.0360,
            lon: 12.6130,
            border_pair: Some("se_dk_oresund".into()),
        },
        SamplePoint {
            id: "helsingborg".into(),
            name: "Helsingborg".into(),
            lat: 56.0465,
            lon: 12.6945,
            border_pair: Some("se_dk_oresund".into()),
        },
        SamplePoint {
            id: "copenhagen".into(),
            name: "Copenhagen".into(),
            lat: 55.6760,
            lon: 12.5680,
            border_pair: None,
        },
        SamplePoint {
            id: "malmo".into(),
            name: "Malmo".into(),
            lat: 55.6050,
            lon: 13.0038,
            border_pair: None,
        },
        // US–Canada
        SamplePoint {
            id: "niagara_falls_us".into(),
            name: "Niagara Falls NY US".into(),
            lat: 43.0962,
            lon: -79.0377,
            border_pair: Some("us_ca_niagara".into()),
        },
        SamplePoint {
            id: "niagara_falls_ca".into(),
            name: "Niagara Falls ON CA".into(),
            lat: 43.0896,
            lon: -79.0849,
            border_pair: Some("us_ca_niagara".into()),
        },
        SamplePoint {
            id: "detroit_us".into(),
            name: "Detroit US".into(),
            lat: 42.3314,
            lon: -83.0458,
            border_pair: Some("us_ca_detroit".into()),
        },
        SamplePoint {
            id: "windsor_ca".into(),
            name: "Windsor CA".into(),
            lat: 42.3149,
            lon: -83.0364,
            border_pair: Some("us_ca_detroit".into()),
        },
        SamplePoint {
            id: "blaine_us".into(),
            name: "Blaine WA US".into(),
            lat: 48.9937,
            lon: -122.7470,
            border_pair: Some("us_ca_blaine".into()),
        },
        SamplePoint {
            id: "white_rock_ca".into(),
            name: "White Rock BC CA".into(),
            lat: 49.0250,
            lon: -122.8030,
            border_pair: Some("us_ca_blaine".into()),
        },
        // US–Mexico
        SamplePoint {
            id: "san_ysidro_us".into(),
            name: "San Ysidro US (north of port of entry)".into(),
            lat: 32.5550,
            lon: -117.0450,
            border_pair: Some("us_mx_tijuana".into()),
        },
        SamplePoint {
            id: "tijuana_mx".into(),
            name: "Tijuana MX".into(),
            lat: 32.5149,
            lon: -117.0382,
            border_pair: Some("us_mx_tijuana".into()),
        },
        SamplePoint {
            id: "el_paso_us".into(),
            name: "El Paso US".into(),
            lat: 31.7619,
            lon: -106.4850,
            border_pair: Some("us_mx_juarez".into()),
        },
        SamplePoint {
            id: "ciudad_juarez_mx".into(),
            name: "Ciudad Juarez MX".into(),
            lat: 31.6904,
            lon: -106.4245,
            border_pair: Some("us_mx_juarez".into()),
        },
        // Extra near-border densification (Task A restore): keep earlier town
        // centres, add companions within ~5 km of the international line.
        SamplePoint {
            id: "svinesund_bridge_no".into(),
            name: "Svinesund bridge NO abutment".into(),
            lat: 59.0945,
            lon: 11.2715,
            border_pair: Some("no_se_svinesund_bridge_close".into()),
        },
        SamplePoint {
            id: "svinesund_bridge_se".into(),
            name: "Svinesund bridge SE abutment".into(),
            lat: 59.0890,
            lon: 11.2680,
            border_pair: Some("no_se_svinesund_bridge_close".into()),
        },
        SamplePoint {
            id: "halden_border_no".into(),
            name: "Halden SE approach NO".into(),
            lat: 59.1000,
            lon: 11.4500,
            border_pair: Some("no_se_halden_close".into()),
        },
        SamplePoint {
            id: "halden_border_se".into(),
            name: "Halden SE approach SE".into(),
            lat: 59.0850,
            lon: 11.4700,
            border_pair: Some("no_se_halden_close".into()),
        },
        SamplePoint {
            id: "helsingborg_shore_se".into(),
            name: "Helsingborg ferry shore SE".into(),
            lat: 56.0430,
            lon: 12.6900,
            border_pair: Some("se_dk_oresund_shore".into()),
        },
        SamplePoint {
            id: "helsingor_shore_dk".into(),
            name: "Helsingor ferry shore DK".into(),
            lat: 56.0395,
            lon: 12.6155,
            border_pair: Some("se_dk_oresund_shore".into()),
        },
        SamplePoint {
            id: "kirkenes_border_no".into(),
            name: "Kirkenes east toward NO-RU".into(),
            lat: 69.7000,
            lon: 30.1000,
            border_pair: Some("no_ru_kirkenes_close".into()),
        },
        SamplePoint {
            id: "karasjok_border_no".into(),
            name: "Karasjok east toward FI".into(),
            lat: 69.4700,
            lon: 25.5500,
            border_pair: Some("no_fi_karigas_close".into()),
        },
        SamplePoint {
            id: "karigasniemi_border_fi".into(),
            name: "Karigasniemi west FI approach".into(),
            lat: 69.3950,
            lon: 25.8200,
            border_pair: Some("no_fi_karigas_close".into()),
        },
        SamplePoint {
            id: "flensburg_border_de".into(),
            name: "Flensburg north DE".into(),
            lat: 54.8100,
            lon: 9.4200,
            border_pair: Some("de_dk_flensburg_close".into()),
        },
        SamplePoint {
            id: "padborg_border_dk".into(),
            name: "Padborg south DK".into(),
            lat: 54.8200,
            lon: 9.3700,
            border_pair: Some("de_dk_flensburg_close".into()),
        },
        SamplePoint {
            id: "peace_arch_us".into(),
            name: "Peace Arch US plaza".into(),
            lat: 48.9990,
            lon: -122.7560,
            border_pair: Some("us_ca_peace_arch".into()),
        },
        SamplePoint {
            id: "peace_arch_ca".into(),
            name: "Peace Arch CA plaza".into(),
            lat: 49.0025,
            lon: -122.7565,
            border_pair: Some("us_ca_peace_arch".into()),
        },
        SamplePoint {
            id: "point_roberts_tyee_us".into(),
            name: "Point Roberts Tyee Drive US".into(),
            lat: 48.9900,
            lon: -123.0350,
            border_pair: Some("us_ca_point_roberts".into()),
        },
        SamplePoint {
            id: "boundary_bay_ca".into(),
            name: "Boundary Bay CA near Point Roberts".into(),
            lat: 49.0050,
            lon: -123.0400,
            border_pair: Some("us_ca_point_roberts".into()),
        },
        SamplePoint {
            id: "magnor_border_no".into(),
            name: "Magnor border NO".into(),
            lat: 59.9520,
            lon: 12.2300,
            border_pair: Some("no_se_magnor_close".into()),
        },
        SamplePoint {
            id: "charlottenberg_border_se".into(),
            name: "Charlottenberg border SE".into(),
            lat: 59.8850,
            lon: 12.2800,
            border_pair: Some("no_se_magnor_close".into()),
        },
        SamplePoint {
            id: "storlien_border_se".into(),
            name: "Storlien station SE".into(),
            lat: 63.3160,
            lon: 12.1000,
            border_pair: Some("no_se_storlien_close".into()),
        },
        SamplePoint {
            id: "meraker_border_no".into(),
            name: "Meraker east NO".into(),
            lat: 63.4200,
            lon: 11.9500,
            border_pair: Some("no_se_storlien_close".into()),
        },
        SamplePoint {
            id: "el_paso_bridge_us".into(),
            name: "El Paso Stanton St bridge US".into(),
            lat: 31.7500,
            lon: -106.4855,
            border_pair: Some("us_mx_juarez_close".into()),
        },
        SamplePoint {
            id: "juarez_bridge_mx".into(),
            name: "Juarez Stanton St bridge MX".into(),
            lat: 31.7450,
            lon: -106.4860,
            border_pair: Some("us_mx_juarez_close".into()),
        },
        // More densification to reach ≥50% within 5 km of a foreign border.
        SamplePoint {
            id: "krusaa_border_dk".into(),
            name: "Krusaa border DK".into(),
            lat: 54.8300,
            lon: 9.4050,
            border_pair: Some("de_dk_krusaa_close".into()),
        },
        SamplePoint {
            id: "harrislee_border_de".into(),
            name: "Harrislee border DE".into(),
            lat: 54.8120,
            lon: 9.3900,
            border_pair: Some("de_dk_krusaa_close".into()),
        },
        SamplePoint {
            id: "blanc_sablon_style_us".into(),
            name: "Derby Line VT US".into(),
            lat: 45.0050,
            lon: -72.1000,
            border_pair: Some("us_ca_derby".into()),
        },
        SamplePoint {
            id: "stanstead_ca".into(),
            name: "Stanstead QC CA".into(),
            lat: 45.0150,
            lon: -72.1000,
            border_pair: Some("us_ca_derby".into()),
        },
        SamplePoint {
            id: "sweetgrass_us".into(),
            name: "Sweetgrass MT US".into(),
            lat: 48.9950,
            lon: -111.9650,
            border_pair: Some("us_ca_sweetgrass".into()),
        },
        SamplePoint {
            id: "coutts_ca".into(),
            name: "Coutts AB CA".into(),
            lat: 49.0050,
            lon: -111.9600,
            border_pair: Some("us_ca_sweetgrass".into()),
        },
        SamplePoint {
            id: "portal_us".into(),
            name: "Portal ND US".into(),
            lat: 48.9950,
            lon: -102.5500,
            border_pair: Some("us_ca_portal".into()),
        },
        SamplePoint {
            id: "north_portal_ca".into(),
            name: "North Portal SK CA".into(),
            lat: 49.0050,
            lon: -102.5500,
            border_pair: Some("us_ca_portal".into()),
        },
        SamplePoint {
            id: "eagle_pass_us".into(),
            name: "Eagle Pass TX US".into(),
            lat: 28.7100,
            lon: -100.5000,
            border_pair: Some("us_mx_eagle".into()),
        },
        SamplePoint {
            id: "piedras_negras_mx".into(),
            name: "Piedras Negras MX".into(),
            lat: 28.7000,
            lon: -100.5200,
            border_pair: Some("us_mx_eagle".into()),
        },
        SamplePoint {
            id: "presidio_us".into(),
            name: "Presidio TX US".into(),
            lat: 29.5600,
            lon: -104.3700,
            border_pair: Some("us_mx_presidio".into()),
        },
        SamplePoint {
            id: "ojinaga_mx".into(),
            name: "Ojinaga MX".into(),
            lat: 29.5500,
            lon: -104.4000,
            border_pair: Some("us_mx_presidio".into()),
        },
        SamplePoint {
            id: "tornio_fi".into(),
            name: "Tornio FI".into(),
            lat: 65.8500,
            lon: 24.1500,
            border_pair: Some("fi_se_tornio".into()),
        },
        SamplePoint {
            id: "haparanda_se".into(),
            name: "Haparanda SE".into(),
            lat: 65.8400,
            lon: 24.1300,
            border_pair: Some("fi_se_tornio".into()),
        },
        SamplePoint {
            id: "narvik_border_no".into(),
            name: "Bjornfjell NO near SE".into(),
            lat: 68.4500,
            lon: 18.0700,
            border_pair: Some("no_se_bjornfjell".into()),
        },
        SamplePoint {
            id: "riksgransen_se".into(),
            name: "Riksgransen SE".into(),
            lat: 68.4300,
            lon: 18.1200,
            border_pair: Some("no_se_bjornfjell".into()),
        },
    ];
    pts
}

#[test]
#[ignore = "Nominatim: set NAVI_COUNTRY_ISO_DIAG=1 (≤1 req/s; not a router)"]
fn collect_nominatim_expected() {
    assert_eq!(
        std::env::var("NAVI_COUNTRY_ISO_DIAG").ok().as_deref(),
        Some("1")
    );
    let pts = sample_points();
    assert!(
        pts.len() >= 100,
        "need ≥100 sample points, got {}",
        pts.len()
    );

    // Reuse identical lat/lon rows from an existing fixture so we only hit
    // Nominatim for new or moved sample points (≤1 req/s).
    let mut reuse: std::collections::HashMap<(String, i64, i64), ExpectedRow> =
        std::collections::HashMap::new();
    if expected_path().is_file() {
        #[derive(Deserialize)]
        struct Prev {
            points: Vec<ExpectedRow>,
        }
        if let Ok(prev) =
            serde_json::from_str::<Prev>(&fs::read_to_string(expected_path()).unwrap())
        {
            for row in prev.points {
                let key = (
                    row.id.clone(),
                    (row.lat * 1e7).round() as i64,
                    (row.lon * 1e7).round() as i64,
                );
                reuse.insert(key, row);
            }
        }
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut last = Instant::now() - MIN_INTERVAL;
    let mut rows = Vec::new();
    let queried_utc = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let mut fetched = 0usize;
    let mut reused = 0usize;

    for p in &pts {
        let key = (
            p.id.clone(),
            (p.lat * 1e7).round() as i64,
            (p.lon * 1e7).round() as i64,
        );
        if let Some(mut prev) = reuse.remove(&key) {
            prev.name = p.name.clone();
            prev.border_pair = p.border_pair.clone();
            reused += 1;
            rows.push(prev);
            continue;
        }
        let wait = MIN_INTERVAL.saturating_sub(last.elapsed());
        if !wait.is_zero() {
            thread::sleep(wait);
        }
        last = Instant::now();
        let url = format!(
            "{NOMINATIM}/reverse?lat={}&lon={}&format=jsonv2&addressdetails=1&zoom=10",
            p.lat, p.lon
        );
        let (status, body) = rt.block_on(async {
            let client = reqwest::Client::builder()
                .user_agent(UA)
                .timeout(Duration::from_secs(60))
                .build()
                .unwrap();
            let resp = client.get(&url).send().await;
            match resp {
                Ok(r) => (r.status().as_u16(), r.text().await.unwrap_or_default()),
                Err(e) => (0, format!("transport_error: {e}")),
            }
        });
        assert_eq!(status, 200, "Nominatim failed for {}: {body}", p.id);
        let v: serde_json::Value = serde_json::from_str(&body).expect("json");
        let cc = v
            .pointer("/address/country_code")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        assert!(
            !cc.is_empty(),
            "no country_code for {} body={}",
            p.id,
            body.chars().take(200).collect::<String>()
        );
        let display = v
            .get("display_name")
            .and_then(|d| d.as_str())
            .unwrap_or("")
            .to_string();
        let osm_type = v
            .get("osm_type")
            .and_then(|d| d.as_str())
            .map(|s| s.to_string());
        let osm_id = v.get("osm_id").and_then(|d| d.as_u64());
        eprintln!(
            "{} -> {} ({})",
            p.id,
            cc,
            display.chars().take(80).collect::<String>()
        );
        fetched += 1;
        rows.push(ExpectedRow {
            id: p.id.clone(),
            name: p.name.clone(),
            lat: p.lat,
            lon: p.lon,
            border_pair: p.border_pair.clone(),
            expected_iso: cc,
            nominatim_display_name: display,
            nominatim_osm_type: osm_type,
            nominatim_osm_id: osm_id,
            source_url: url,
            queried_utc: queried_utc.clone(),
        });
    }
    eprintln!(
        "nominatim_fetched={fetched} reused={reused} total={}",
        rows.len()
    );

    let out = json!({
        "navi_fixture": "recorded",
        "evidence": "Nominatim reverse geocode (OSM admin / address.country_code)",
        "nominatim_base": NOMINATIM,
        "queried_utc": queried_utc,
        "user_agent": UA,
        "license_note": "OpenStreetMap data © OpenStreetMap contributors, ODbL 1.0; Nominatim usage policy applies",
        "points": rows,
    });
    fs::write(expected_path(), serde_json::to_string_pretty(&out).unwrap()).unwrap();
    eprintln!("wrote {:?}", expected_path());
}

#[test]
fn report_country_iso_accuracy() {
    eprintln!(
        "ASSET: Natural Earth Admin-0 via scripts/generate-country-polys.py \
         (see core/src/routing/elevation/data/ATTRIBUTION.txt)"
    );
    eprintln!(
        "LOOKUP: grid-indexed PIP + coastal snap ≤{} m; countries ordered by ascending area",
        driver_break_core::routing::elevation::COASTAL_SNAP_TOLERANCE_M
    );
    eprintln!(
        "EDGE_FILTER: start+midpoint+end must all resolve to an allowed ISO \
         (semantic change from midpoint-only)"
    );

    let path = expected_path();
    assert!(path.exists(), "missing {}", path.display());

    #[derive(Deserialize)]
    struct File {
        queried_utc: String,
        points: Vec<ExpectedRow>,
        evidence: String,
        nominatim_base: String,
    }
    let file: File = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert!(
        file.points.len() >= 100,
        "fixture has {} points",
        file.points.len()
    );
    eprintln!(
        "EVIDENCE: {} via {} queried_utc={}",
        file.evidence, file.nominatim_base, file.queried_utc
    );
    let mut misses = 0usize;
    for row in &file.points {
        let got = country_iso_at(row.lat, row.lon);
        let ok = got == Some(row.expected_iso.as_str());
        if !ok {
            misses += 1;
            eprintln!(
                "MISS {} expected={} got={:?} lat={} lon={}",
                row.id, row.expected_iso, got, row.lat, row.lon
            );
        }
    }
    eprintln!("TOTAL_MISSES={misses} / {}", file.points.len());
    // Task A: restored near-border Nominatim points may disagree with NE 50m.
    // Do not rewrite expected ISO to force a pass; see country_iso_polygons.
}
