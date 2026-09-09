//! Host regression: typed POI discovery from a checked-in mini PBF.
//!
//! Mirrors the corridor "POI discovery" check in
//! `OfflineInnlandetScreenshotTest` without needing the ~60 MB Espa corridor
//! extract or an emulator: load [`PoiIndex`] from
//! `tests/fixtures/motor-access-hamar-gjovik.osm.pbf` and assert named
//! amenities near Hamar are found by category via [`PoiIndex::nearest`].
//!
//! Fixture is shared with `motor_access_barrier` (Torggata / Kirkebyskogen cut).

use driver_break_core::poi::{PoiCategory, PoiIndex};
use std::path::PathBuf;

fn fixture_pbf() -> PathBuf {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/motor-access-hamar-gjovik.osm.pbf");
    assert!(
        p.is_file(),
        "missing checked-in fixture {} — regenerate with scripts/cut-corridor-extract.py",
        p.display()
    );
    p
}

/// Central Hamar (near Torggata / Stortorget).
const HAMAR_LAT: f64 = 60.7928;
const HAMAR_LON: f64 = 11.0758;
const RADIUS_M: f64 = 800.0;

#[test]
fn hamar_fixture_indexes_multiple_poi_records() {
    let idx = PoiIndex::load_from_pbf(fixture_pbf()).expect("poi load");
    assert!(
        idx.len() >= 10,
        "expected dense downtown amenities in Hamar cut, got {}",
        idx.len()
    );
}

#[test]
fn discovers_named_general_amenity_near_hamar() {
    let idx = PoiIndex::load_from_pbf(fixture_pbf()).expect("poi load");
    let hits = idx.nearest(PoiCategory::General, HAMAR_LAT, HAMAR_LON, RADIUS_M);
    assert!(
        !hits.is_empty(),
        "expected General POIs (cafe/restaurant/museum) near Hamar"
    );
    let names: Vec<_> = hits.iter().filter_map(|h| h.name.as_deref()).collect();
    assert!(
        names.contains(&"Peppes Pizza")
            || names.contains(&"Kunstbanken")
            || names.contains(&"Café Gravdahl"),
        "expected a known Hamar General POI, got {names:?}"
    );
}

#[test]
fn discovers_hotel_as_lodging_near_hamar() {
    let idx = PoiIndex::load_from_pbf(fixture_pbf()).expect("poi load");
    let hits = idx.nearest(PoiCategory::Lodging, HAMAR_LAT, HAMAR_LON, RADIUS_M);
    assert!(
        hits.iter()
            .any(|h| h.name.as_deref() == Some("Home Hotel Astoria")),
        "expected Home Hotel Astoria as Lodging, got {:?}",
        hits.iter()
            .map(|h| (h.osm_id, h.name.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn discovers_restroom_near_hamar() {
    let idx = PoiIndex::load_from_pbf(fixture_pbf()).expect("poi load");
    let hits = idx.nearest(PoiCategory::Restroom, HAMAR_LAT, HAMAR_LON, RADIUS_M);
    assert!(
        !hits.is_empty(),
        "expected amenity=toilets near Hamar downtown"
    );
    assert!(
        hits.iter().any(|h| h.osm_id == 10159982537),
        "expected known toilets node 10159982537, got {:?}",
        hits.iter().map(|h| h.osm_id).collect::<Vec<_>>()
    );
}

#[test]
fn discovers_water_fountain_near_hamar() {
    let idx = PoiIndex::load_from_pbf(fixture_pbf()).expect("poi load");
    let hits = idx.nearest(PoiCategory::Water, HAMAR_LAT, HAMAR_LON, RADIUS_M);
    assert!(
        hits.iter().any(|h| h.osm_id == 444678868),
        "expected amenity=fountain node 444678868 as Water, got {:?}",
        hits.iter()
            .map(|h| (h.osm_id, h.name.clone(), h.icon_key.clone()))
            .collect::<Vec<_>>()
    );
}
