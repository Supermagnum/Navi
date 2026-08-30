//! v5 → v6 FlatGraphPack regeneration: planted stale packs must report
//! `VersionMismatch`, and `convert_region_packs` must rebuild v6 with vehicle
//! physical limits populated (the fields missing from the v5 schema).
//!
//! Fixture: Stai bru corridor (`stai-bru-limits.osm.pbf`) — Innlandet way
//! 34106197 with maxheight/maxwidth/maxweight/maxaxleload.

use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use driver_break_core::routing::graph::RoutingProfile;
use driver_break_core::routing::indexed::{
    convert_region_packs, try_load_graph_for_plan, ConvertOptions, PackLoadError, PackStatus,
    GRAPH_FORMAT_VERSION, MAGIC_GRAPH,
};
use driver_break_core::routing::indexed::{manifest_path, NaviManifest};

fn fixture_pbf() -> PathBuf {
    let p =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/stai-bru-limits.osm.pbf");
    assert!(
        p.is_file(),
        "missing {} — cut with scripts/cut-corridor-extract.py",
        p.display()
    );
    p
}

fn plant_v5_graph_version(data_dir: &Path, stem: &str) {
    let man_path = manifest_path(data_dir, stem);
    let mut man = NaviManifest::load(&man_path).expect("manifest");
    assert_eq!(
        man.graph_format_version, GRAPH_FORMAT_VERSION,
        "fresh convert must write current graph format"
    );
    man.graph_format_version = 5;
    man.save(&man_path).expect("save planted v5 manifest");

    // Also downgrade on-disk pack preambles so a bypass of the manifest gate
    // still fails `check_preamble` (same gate `load_graph_pack_bbox` uses).
    for entry in fs::read_dir(data_dir).expect("read data_dir") {
        let entry = entry.expect("dirent");
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !(name.contains(".navi-graph-") && name.ends_with(".rkyv")) {
            continue;
        }
        let path = entry.path();
        let mut f = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .expect("open pack");
        let mut preamble = [0u8; 8];
        f.read_exact(&mut preamble).expect("read preamble");
        let magic = u32::from_le_bytes(preamble[0..4].try_into().unwrap());
        let ver = u32::from_le_bytes(preamble[4..8].try_into().unwrap());
        assert_eq!(magic, MAGIC_GRAPH, "pack magic {name}");
        // Truck packs are hard-linked to car; rewriting one file updates both.
        if ver == 5 {
            continue;
        }
        assert_eq!(
            ver, GRAPH_FORMAT_VERSION,
            "pack version before plant {name}"
        );
        preamble[4..8].copy_from_slice(&5u32.to_le_bytes());
        f.seek(SeekFrom::Start(0)).unwrap();
        f.write_all(&preamble).unwrap();
        f.flush().unwrap();
    }
}

#[test]
fn v5_pack_reports_version_mismatch_then_convert_rebuilds_v6_with_limits() {
    let src = fixture_pbf();
    let work = tempfile::tempdir().expect("tempdir");
    let data_dir = work.path();
    let pbf = data_dir.join("stai-bru-limits.osm.pbf");
    fs::copy(&src, &pbf).expect("copy pbf");

    let mut opts = ConvertOptions::new(data_dir, &pbf);
    opts.profiles = vec![
        RoutingProfile::Car,
        RoutingProfile::Truck,
        RoutingProfile::Foot,
    ];
    let report = convert_region_packs(&opts).expect("initial convert");
    assert!(
        report.edges > 0 && report.nodes > 0,
        "convert must produce a graph: {report:?}"
    );

    let stem = "stai-bru-limits";
    let man = NaviManifest::load(&manifest_path(data_dir, stem)).expect("man");
    assert_eq!(man.graph_format_version, GRAPH_FORMAT_VERSION);
    assert_eq!(
        man.status_for_pbf(data_dir, &pbf),
        PackStatus::Ready,
        "fresh packs must be Ready"
    );

    // Sanity: v6 pack already carries Stai bru vehicle limits.
    let g0 = try_load_graph_for_plan(data_dir, &pbf, RoutingProfile::Truck).expect("load v6");
    let stai0 = g0
        .edges
        .iter()
        .find(|e| e.name.as_deref() == Some("Stai bru"))
        .expect("Stai bru in v6 pack");
    assert_eq!(stai0.maxheight_m, Some(3.7));
    assert_eq!(stai0.maxwidth_m, Some(3.5));
    assert_eq!(stai0.maxweight_t, Some(28.0));
    assert_eq!(stai0.maxaxleload_t, Some(6.0));

    plant_v5_graph_version(data_dir, stem);

    let man_v5 = NaviManifest::load(&manifest_path(data_dir, stem)).expect("planted man");
    assert_eq!(man_v5.graph_format_version, 5);
    assert_eq!(
        man_v5.status_for_pbf(data_dir, &pbf),
        PackStatus::VersionMismatch,
        "planted v5 manifest must surface VersionMismatch (not Missing)"
    );

    let err = match try_load_graph_for_plan(data_dir, &pbf, RoutingProfile::Truck) {
        Err(e) => e,
        Ok(_) => panic!("plan-time pack load must reject v5 packs, got Ok"),
    };
    assert!(
        matches!(err, PackLoadError::VersionMismatch),
        "plan-time pack load must reject v5 packs, got {err}"
    );

    // Real call chain used by UniFFI `ensure_indexed_maps`: status not Ready → convert.
    let report2 = convert_region_packs(&opts).expect("regen convert");
    assert!(report2.edges > 0);

    let man2 = NaviManifest::load(&manifest_path(data_dir, stem)).expect("regen man");
    assert_eq!(
        man2.graph_format_version, GRAPH_FORMAT_VERSION,
        "regenerated manifest must be v{GRAPH_FORMAT_VERSION}"
    );
    assert_eq!(man2.status_for_pbf(data_dir, &pbf), PackStatus::Ready);

    let g = try_load_graph_for_plan(data_dir, &pbf, RoutingProfile::Truck).expect("load regen");
    let stai = g
        .edges
        .iter()
        .find(|e| e.name.as_deref() == Some("Stai bru"))
        .expect("Stai bru must survive regen pack load");
    assert_eq!(stai.maxheight_m, Some(3.7), "maxheight after v6 regen");
    assert_eq!(stai.maxwidth_m, Some(3.5), "maxwidth after v6 regen");
    assert_eq!(stai.maxweight_t, Some(28.0), "maxweight after v6 regen");
    assert_eq!(stai.maxaxleload_t, Some(6.0), "maxaxleload after v6 regen");
}
