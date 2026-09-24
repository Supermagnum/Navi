//! Soft-rest finalize must find POI packs under long-trip-packs/, not only
//! files/ top-level — same multi-dir layout as corridor graph load (93223129).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use driver_break_core::poi::{PoiCategory, PoiRecord};
use driver_break_core::routing::indexed::{
    try_load_poi_pack_covering_point, try_load_poi_pack_covering_point_with_pack_dirs,
    write_archive_atomic, FlatPoiBarrierPack, FlatWetlandPack, NaviManifest, Preamble,
    GRAPH_FORMAT_VERSION, MAGIC_POI_BARRIER, MAGIC_WETLAND, POI_BARRIER_FORMAT_VERSION,
    SERVER_INSTALL_SUFFIX, WETLAND_FORMAT_VERSION,
};
use rkyv::rancor::Error as RkyvError;
use tempfile::TempDir;

fn write_ready_poi_pack(dir: &Path, stem: &str, lat: f64, lon: f64, name: &str) {
    fs::create_dir_all(dir).unwrap();
    let poi_file = format!("{stem}.navi-poi-barrier.rkyv");
    let wet_file = format!("{stem}.navi-wetland.rkyv");
    let graph_file = format!("{stem}.navi-graph-car.rkyv");

    let rec = PoiRecord {
        osm_id: 42,
        lat,
        lon,
        categories: vec![PoiCategory::RestArea],
        icon_key: "highway-rest_area".into(),
        tags: Default::default(),
        name: Some(name.into()),
    };
    let flat = FlatPoiBarrierPack::from_parts(&[rec], &[], &[], &[]);
    let bytes = rkyv::to_bytes::<RkyvError>(&flat).expect("serialize poi");
    write_archive_atomic(
        &dir.join(&poi_file),
        Preamble::new(MAGIC_POI_BARRIER, POI_BARRIER_FORMAT_VERSION),
        bytes.as_ref(),
    )
    .expect("write poi");

    let wet = FlatWetlandPack::empty();
    let wet_bytes = rkyv::to_bytes::<RkyvError>(&wet).expect("serialize wet");
    write_archive_atomic(
        &dir.join(&wet_file),
        Preamble::new(MAGIC_WETLAND, WETLAND_FORMAT_VERSION),
        wet_bytes.as_ref(),
    )
    .expect("write wet");

    // status_pack_files only checks presence for graphs.
    fs::write(dir.join(&graph_file), b"stub").unwrap();
    fs::write(
        dir.join(format!("{stem}{SERVER_INSTALL_SUFFIX}")),
        br#"{"ok":true}"#,
    )
    .unwrap();

    let mut graph_files = BTreeMap::new();
    graph_files.insert("car".into(), graph_file);
    let man = NaviManifest {
        schema: NaviManifest::SCHEMA,
        stem: stem.into(),
        pbf_filename: format!("{stem}.osm.pbf"),
        pbf_size_bytes: 0,
        pbf_modified_unix_secs: 0,
        graph_files,
        graph_tiles: BTreeMap::new(),
        graph_format_version: GRAPH_FORMAT_VERSION,
        poi_barrier_file: poi_file,
        poi_barrier_format_version: POI_BARRIER_FORMAT_VERSION,
        wetland_file: Some(wet_file),
        wetland_tiles: Vec::new(),
        wetland_format_version: WETLAND_FORMAT_VERSION,
        has_delta_h: false,
        elev_dir: None,
    };
    man.save(&dir.join(format!("{stem}.navi-manifest.json")))
        .expect("save manifest");
}

#[test]
fn covering_point_finds_poi_only_under_long_trip_packs() {
    let root = TempDir::new().unwrap();
    let data_dir = root.path().join("files");
    let pack_dir = data_dir.join("long-trip-packs");
    fs::create_dir_all(&data_dir).unwrap();

    // Skåne RestArea only under LTP (investigation layout).
    write_ready_poi_pack(&pack_dir, "skane-latest", 55.7, 13.2, "Ltp Rest");

    let lat = 55.7;
    let lon = 13.2;

    // Before-fix shape: data_dir alone → Missing.
    assert!(
        try_load_poi_pack_covering_point(&data_dir, lat, lon).is_err(),
        "files/ top-level alone must not see LTP-only POI packs"
    );

    let pack_dirs = vec![pack_dir.clone()];
    let (poi, _) = try_load_poi_pack_covering_point_with_pack_dirs(&data_dir, &pack_dirs, lat, lon)
        .expect("multi-dir must load LTP POI pack");
    let hits = poi.nearest(PoiCategory::RestArea, lat, lon, 5_000.0);
    assert!(
        !hits.is_empty(),
        "raw RestArea candidates must be > 0 after multi-dir load"
    );
    assert_eq!(hits[0].name.as_deref(), Some("Ltp Rest"));
}

#[test]
fn covering_point_files_only_unchanged_when_pack_co_located() {
    let root = TempDir::new().unwrap();
    let data_dir = root.path().join("files");
    // Ostlandet RestArea co-located under files/ (ReuseInternal layout).
    write_ready_poi_pack(&data_dir, "ostlandet-latest", 60.2, 11.0, "Files Rest");

    let lat = 60.2;
    let lon = 11.0;
    let (poi, _) = try_load_poi_pack_covering_point(&data_dir, lat, lon)
        .expect("files/-only must still resolve co-located packs");
    let hits = poi.nearest(PoiCategory::RestArea, lat, lon, 5_000.0);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].name.as_deref(), Some("Files Rest"));

    // Empty pack_dirs must behave the same (additive API).
    let empty: Vec<PathBuf> = Vec::new();
    let (poi2, _) =
        try_load_poi_pack_covering_point_with_pack_dirs(&data_dir, &empty, lat, lon).unwrap();
    assert_eq!(
        poi2.nearest(PoiCategory::RestArea, lat, lon, 5_000.0).len(),
        1
    );
}
