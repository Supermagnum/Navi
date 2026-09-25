//! Regression: ordinary car plans must not densify/chunk when long-trip is off.
//!
//! Confirmed bug: `allow_long_trip_chunk` was hardcoded true in `plan_car_route`,
//! so any OD with span > LONG_TRIP_CHUNK_DEG (1.15°) entered the long-trip
//! densify pipeline and produced Peer Gynt / Hanslisetra detours
//! (Hamar→Dombås ~327 km vs OSRM E6 ~213 km).
//!
//! Fixture: Ostlandet PBF + packs under `core/target/integration-fixtures`
//! or `NAVI_OSTLANDET_DATA` / `target/espa-dombas-e2e`.

use std::path::{Path, PathBuf};

use navi::{plan_car_route, FfiTollPolicy, FfiVehicleLimits, TravelProfile};

const HAMAR: (f64, f64) = (60.792206, 11.085951);
const DOMBAS: (f64, f64) = (62.0755539, 9.1278983);
const BOLLELAND: (f64, f64) = (60.562578, 11.256970);
const IMSROA: (f64, f64) = (61.465382, 11.024022);
const FISKEVOLLEN: (f64, f64) = (61.965017, 11.539533);
const TYINKRYSSET: (f64, f64) = (61.203492, 8.237243);
const BAD_BEVENSEN: (f64, f64) = (53.079686, 10.587198);

/// OSRM E6-class reference (investigation).
const HAMAR_DOMBAS_OSRM_KM: f64 = 213.46;
/// Buggy densify result (investigation).
const HAMAR_DOMBAS_BUGGY_KM: f64 = 327.0;
/// Dombås→Bolleland E6-class target (investigation).
const DOMBAS_BOLLELAND_E6_KM: f64 = 242.0;
const DOMBAS_BOLLELAND_BUGGY_KM: f64 = 347.0;

fn empty_vehicle() -> FfiVehicleLimits {
    FfiVehicleLimits {
        axle_weight_kg: None,
        bogie_weight_kg: None,
        height_m: None,
        width_m: None,
        length_m: None,
        total_weight_kg: None,
    }
}

fn ostlandet_data() -> Option<(PathBuf, PathBuf)> {
    if let Ok(dir) = std::env::var("NAVI_OSTLANDET_DATA") {
        let d = PathBuf::from(dir);
        let pbf = d.join("ostlandet-latest.osm.pbf");
        if pbf.is_file() {
            return Some((pbf, d));
        }
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let candidates = [
        root.join("core/target/integration-fixtures"),
        root.join("target/espa-dombas-e2e"),
    ];
    for dir in candidates {
        let pbf = dir.join("ostlandet-latest.osm.pbf");
        let manifest = dir.join("ostlandet-latest.navi-manifest.json");
        let pbf_ok = pbf.is_file();
        let large = pbf
            .metadata()
            .map(|m| m.len() > 10_000_000)
            .unwrap_or(false);
        // Prefer a pack-backed layout (stub or full PBF + manifest).
        if pbf_ok && (manifest.is_file() || large) {
            return Some((pbf, dir));
        }
    }
    None
}

fn plan(
    pbf: &Path,
    data_dir: &Path,
    start: (f64, f64),
    end: (f64, f64),
    long_trip_enabled: bool,
) -> navi::CorridorRouteResult {
    let elev = data_dir.join("elevation");
    let cache = data_dir.join(format!(
        "graph-cache-chunk-gate-{}-{}",
        if long_trip_enabled { "on" } else { "off" },
        cache_tag(start, end)
    ));
    let _ = std::fs::create_dir_all(&elev);
    let _ = std::fs::create_dir_all(&cache);
    plan_car_route(
        pbf.display().to_string(),
        elev.display().to_string(),
        cache.display().to_string(),
        start.0,
        start.1,
        end.0,
        end.1,
        false,
        TravelProfile::Car,
        false,
        FfiTollPolicy::Allow,
        false,
        false,
        empty_vehicle(),
        false,
        data_dir.display().to_string(),
        String::new(),
        long_trip_enabled,
        None,
        Vec::new(),
    )
}

fn cache_tag(a: (f64, f64), b: (f64, f64)) -> String {
    format!("{:.3}_{:.3}_{:.3}_{:.3}", a.0, a.1, b.0, b.1)
}

fn assert_e6_class(label: &str, km: f64, expected: f64, buggy: f64) {
    let tol = expected * 0.12; // 12% vs OSRM — packs/clock vs live OSRM
    assert!(
        (km - expected).abs() <= tol,
        "{label}: expected ~{expected:.0} km E6-class (tol {tol:.0}), got {km:.1} \
         (buggy densify was ~{buggy:.0} km)"
    );
    assert!(
        km < buggy - 30.0,
        "{label}: still densify-class distance {km:.1} (buggy ~{buggy:.0})"
    );
}

#[test]
fn hamar_dombas_span_exceeds_chunk_threshold() {
    let pts = [HAMAR, DOMBAS];
    let span = driver_break_core::routing::plan_bbox::trip_span_deg(&pts);
    assert!(
        span > driver_break_core::routing::plan_bbox::LONG_TRIP_CHUNK_DEG,
        "fixture OD must be in the densify span band; span={span:.3}"
    );
    let hops = driver_break_core::routing::plan_bbox::densify_route_points(
        &pts,
        driver_break_core::routing::plan_bbox::LONG_TRIP_CHUNK_DEG,
    );
    assert!(
        hops.len() > 2,
        "geometric densify must invent hops for this OD (got {})",
        hops.len()
    );
}

#[test]
#[ignore = "needs Ostlandet PBF/packs under core/target/integration-fixtures or NAVI_OSTLANDET_DATA"]
fn hamar_dombas_long_trip_off_prefers_e6_not_peer_gynt() {
    let Some((pbf, data)) = ostlandet_data() else {
        panic!("missing Ostlandet fixture");
    };
    let off = plan(&pbf, &data, HAMAR, DOMBAS, false);
    assert!(
        off.report.contains("PASS"),
        "plan must PASS:\n{}",
        off.report
    );
    assert!(
        !off.report.contains("long_trip_chunked=true"),
        "long-trip OFF must not densify/chunk:\n{}",
        off.report
    );
    assert!(
        off.report.contains("long_trip_enabled=false"),
        "report must echo long_trip_enabled=false:\n{}",
        off.report
    );
    eprintln!(
        "hamar_dombas OFF: km={:.1} eta={:.0} report_head={}",
        off.distance_km,
        off.eta_minutes,
        off.report.lines().take(8).collect::<Vec<_>>().join(" | ")
    );
    assert_e6_class(
        "Hamar→Dombås long-trip OFF",
        off.distance_km,
        HAMAR_DOMBAS_OSRM_KM,
        HAMAR_DOMBAS_BUGGY_KM,
    );
}

#[test]
#[ignore = "needs Ostlandet PBF/packs under core/target/integration-fixtures or NAVI_OSTLANDET_DATA"]
fn dombas_bolleland_long_trip_off_prefers_e6() {
    let Some((pbf, data)) = ostlandet_data() else {
        panic!("missing Ostlandet fixture");
    };
    let off = plan(&pbf, &data, DOMBAS, BOLLELAND, false);
    assert!(
        off.report.contains("PASS"),
        "plan must PASS:\n{}",
        off.report
    );
    assert!(
        !off.report.contains("long_trip_chunked=true"),
        "long-trip OFF must not densify/chunk:\n{}",
        off.report
    );
    eprintln!(
        "dombas_bolleland OFF: km={:.1} eta={:.0}",
        off.distance_km, off.eta_minutes
    );
    assert_e6_class(
        "Dombås→Bolleland long-trip OFF",
        off.distance_km,
        DOMBAS_BOLLELAND_E6_KM,
        DOMBAS_BOLLELAND_BUGGY_KM,
    );
}

#[test]
#[ignore = "needs Ostlandet PBF/packs under core/target/integration-fixtures or NAVI_OSTLANDET_DATA"]
fn hamar_dombas_long_trip_on_still_chunks() {
    let Some((pbf, data)) = ostlandet_data() else {
        panic!("missing Ostlandet fixture");
    };
    let on = plan(&pbf, &data, HAMAR, DOMBAS, true);
    assert!(
        on.report.contains("PASS"),
        "long-trip ON plan must PASS:\n{}",
        on.report
    );
    assert!(
        on.report.contains("long_trip_chunked=true")
            || on.report.contains("long_trip_enabled=true"),
        "long-trip ON must enable chunk path or at least echo the flag:\n{}",
        on.report
    );
    // Existing densify behaviour for this OD: inflated distance class.
    assert!(
        on.distance_km > HAMAR_DOMBAS_OSRM_KM + 40.0,
        "long-trip ON should still densify this OD into the long corridor class; got {:.1}",
        on.distance_km
    );
    eprintln!(
        "hamar_dombas ON: km={:.1} chunked={}",
        on.distance_km,
        on.report.contains("long_trip_chunked=true")
    );
}

#[test]
#[ignore = "needs Ostlandet PBF/packs under core/target/integration-fixtures or NAVI_OSTLANDET_DATA"]
fn imsroa_fiskevollen_unaffected_either_way() {
    let Some((pbf, data)) = ostlandet_data() else {
        panic!("missing Ostlandet fixture");
    };
    let span = driver_break_core::routing::plan_bbox::trip_span_deg(&[IMSROA, FISKEVOLLEN]);
    assert!(
        span <= driver_break_core::routing::plan_bbox::LONG_TRIP_CHUNK_DEG,
        "gravel OD must stay under chunk threshold; span={span:.3}"
    );
    let off = plan(&pbf, &data, IMSROA, FISKEVOLLEN, false);
    let on = plan(&pbf, &data, IMSROA, FISKEVOLLEN, true);
    assert!(off.report.contains("PASS"), "OFF:\n{}", off.report);
    assert!(on.report.contains("PASS"), "ON:\n{}", on.report);
    assert!(
        !off.report.contains("long_trip_chunked=true")
            && !on.report.contains("long_trip_chunked=true"),
        "short gravel OD must never chunk"
    );
    let delta = (off.distance_km - on.distance_km).abs();
    assert!(
        delta < 5.0,
        "gravel route must be unaffected by long-trip flag; off={:.1} on={:.1}",
        off.distance_km,
        on.distance_km
    );
    eprintln!(
        "imsroa_fiskevollen off={:.1} on={:.1} delta={delta:.2}",
        off.distance_km, on.distance_km
    );
}

#[test]
#[ignore = "needs multi-country long-trip packs (NAVI_LONG_TRIP_DATA) for Tyinkrysset→Bad Bevensen"]
fn tyinkrysset_bad_bevensen_long_trip_on_chunks_and_completes() {
    let Ok(dir) = std::env::var("NAVI_LONG_TRIP_DATA") else {
        eprintln!("skip: set NAVI_LONG_TRIP_DATA to a dir with corridor packs + covering PBF");
        return;
    };
    let data = PathBuf::from(dir);
    let pbf = data
        .join("ostlandet-latest.osm.pbf")
        .canonicalize()
        .or_else(|_| {
            std::fs::read_dir(&data)
                .ok()
                .and_then(|rd| {
                    rd.filter_map(|e| e.ok())
                        .map(|e| e.path())
                        .find(|p| p.extension().and_then(|x| x.to_str()) == Some("pbf"))
                })
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no pbf"))
        })
        .expect("PBF under NAVI_LONG_TRIP_DATA");
    let on = plan(&pbf, &data, TYINKRYSSET, BAD_BEVENSEN, true);
    assert!(
        on.report.contains("PASS"),
        "long corridor must PASS:\n{}",
        on.report
    );
    assert!(
        on.report.contains("long_trip_chunked=true"),
        "long corridor must densify/chunk:\n{}",
        on.report
    );
    assert!(
        on.distance_km > 1400.0 && on.distance_km < 2000.0,
        "expected ~1680 km class, got {:.1}",
        on.distance_km
    );
    // Rest / overnight suggestions: multi-day or break POIs present.
    let has_rest =
        on.break_pois_json.len() > 4 || on.days_json.len() > 4 || on.report.contains("break");
    assert!(
        has_rest,
        "expected rest/overnight suggestions on long corridor; breaks={} days={}",
        on.break_pois_json.len(),
        on.days_json.len()
    );
    eprintln!(
        "tyin_bevensen ON: km={:.1} eta={:.0} breaks_len={} days_len={}",
        on.distance_km,
        on.eta_minutes,
        on.break_pois_json.len(),
        on.days_json.len()
    );
}
