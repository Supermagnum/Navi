//! Real-world Innlandet vehicle-restriction regressions via the **indexed pack**
//! path (`convert_region_packs` → `try_load_graph_for_plan_bbox`).
//!
//! Locations confirmed via Overpass (`[out:json]`, osm_base ~2026-08-30) against
//! Innlandet (admin_level=4). Victoria-undergangen excluded (Bane NOR rebuild).
//!
//! | Restriction | OSM way | Name | Tag value | Fixture |
//! |---|---|---|---|---|
//! | maxheight | 31776674 | Fokholgutua | 2.7 m | fokholgutua-maxheight.osm.pbf |
//! | maxlength | 323447948 | Atna hengebru | 12.40 m | atna-hengebru-limits.osm.pbf |
//! | maxweight | 323447948 | Atna hengebru | 7.5 t | atna-hengebru-limits.osm.pbf |
//! | maxwidth | 34106197 | Stai bru | 3.5 m | stai-bru-limits.osm.pbf |
//! | maxaxleload | 34106197 | Stai bru | 6 t | stai-bru-limits.osm.pbf |
//! | maxbogieweight | 118689240 | Liabrue (Lom) | 7.5 t | liabrue-bogie-limits.osm.pbf |
//!
//! Stai bru also carries maxheight=3.7 / maxweight=28 (multi-tag timber/truss
//! bridge). Liabrue is the only `maxbogieweight` way inside Innlandet; blocking
//! it severs the local network (no alternate river crossing in the extract), so
//! the oversized-vehicle case may return `None`. Each of those tests includes an
//! under-limit positive control that must succeed and use the bridge — proving
//! the fixture is connected, so a restricted `None` means rejection not a
//! broken extract.

use std::fs;
use std::path::{Path, PathBuf};

use driver_break_core::config::VehicleLimits;
use driver_break_core::routing::graph::{RouteOptions, RoutingProfile};
use driver_break_core::routing::indexed::{
    convert_region_packs, try_load_graph_for_plan_bbox, ConvertOptions,
};
use osm4routing::NodeId;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn require_fixture(name: &str) -> PathBuf {
    let p = fixtures_dir().join(name);
    assert!(
        p.is_file(),
        "missing {} — regenerate with scripts/cut-corridor-extract.py (see fixtures/README.md)",
        p.display()
    );
    p
}

fn convert_truck_packs(work: &Path, src_pbf: &Path) -> PathBuf {
    let name = src_pbf.file_name().unwrap();
    let pbf = work.join(name);
    fs::copy(src_pbf, &pbf).expect("copy pbf into work dir");
    let mut opts = ConvertOptions::new(work, &pbf);
    opts.profiles = vec![
        RoutingProfile::Car,
        RoutingProfile::Truck,
        RoutingProfile::Foot,
    ];
    convert_region_packs(&opts).expect("convert_region_packs");
    pbf
}

fn path_uses_named(
    edges: &[driver_break_core::routing::graph::GraphEdge],
    path: &[NodeId],
    name: &str,
) -> bool {
    path.windows(2).any(|w| {
        edges
            .iter()
            .any(|e| e.source == w[0] && e.target == w[1] && e.name.as_deref() == Some(name))
    })
}

fn path_uses_bogie_span(
    edges: &[driver_break_core::routing::graph::GraphEdge],
    path: &[NodeId],
) -> bool {
    path.windows(2).any(|w| {
        edges.iter().any(|e| {
            e.source == w[0]
                && e.target == w[1]
                && e.name.as_deref() == Some("Liabrue")
                && e.maxbogieweight_t == Some(7.5)
        })
    })
}

fn path_uses_maxheight(
    edges: &[driver_break_core::routing::graph::GraphEdge],
    path: &[NodeId],
    h: f64,
) -> bool {
    path.windows(2).any(|w| {
        edges
            .iter()
            .any(|e| e.source == w[0] && e.target == w[1] && e.maxheight_m == Some(h))
    })
}

/// Fokholgutua (way 31776674, maxheight=2.7): height 2.8 m must detour.
#[test]
fn pack_path_fokholgutua_maxheight_finds_detour() {
    // Regenerate if way 31776674 is re-tagged/split/removed:
    //   Overpass: [out:json][timeout:120]; way(31776674); out tags center;
    //   Discover: way["maxheight"](60.5,9.5,62.6,12.8); out tags center;
    //   Cut: scripts/cut-corridor-extract.py --src …/hedmark-latest.osm.pbf \
    //        --dst core/tests/fixtures/fokholgutua-maxheight.osm.pbf \
    //        --bbox 11.150,60.710,11.220,60.750
    let src = require_fixture("fokholgutua-maxheight.osm.pbf");
    let work = tempfile::tempdir().unwrap();
    let pbf = convert_truck_packs(work.path(), &src);
    let bbox = [60.710, 11.150, 60.750, 11.220];
    let g = try_load_graph_for_plan_bbox(work.path(), &pbf, RoutingProfile::Truck, Some(bbox))
        .expect("pack load");

    let bridge = g
        .edges
        .iter()
        .find(|e| e.name.as_deref() == Some("Fokholgutua") && e.maxheight_m.is_some())
        .expect("Fokholgutua maxheight segment in pack");
    assert_eq!(
        bridge.maxheight_m,
        Some(2.7),
        "maxheight must survive FlatGraphPack"
    );

    let (s, _) = g.nearest_routable(60.7320, 11.1950).expect("start");
    let (t, _) = g.nearest_routable(60.7228810, 11.1530380).expect("end");

    let open = g
        .shortest_path_with_options(s, t, false, &RouteOptions::default())
        .expect("unrestricted path");
    assert!(
        path_uses_maxheight(&g.edges, &open.0, 2.7),
        "unrestricted truck route should prefer the maxheight=2.7 Fokholgutua span"
    );

    let limited = g
        .shortest_path_with_options(
            s,
            t,
            false,
            &RouteOptions {
                vehicle: Some(VehicleLimits {
                    height_m: Some(2.8),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .expect("height-limited path must find a detour");
    assert!(
        !path_uses_maxheight(&g.edges, &limited.0, 2.7),
        "2.8 m vehicle must avoid maxheight=2.7 Fokholgutua"
    );
    assert_ne!(open.0, limited.0);
    assert!(
        limited.1 > open.1,
        "detour should be longer: open={:.0} lim={:.0}",
        open.1,
        limited.1
    );
}

/// Atna hengebru (way 323447948, maxlength=12.40): length 14 m must detour.
#[test]
fn pack_path_atna_maxlength_finds_detour() {
    // Regenerate if way 323447948 is re-tagged/split/removed:
    //   Overpass: [out:json][timeout:120]; way(323447948); out tags center;
    //   Discover: way["maxlength"](60.5,9.5,62.6,12.8); out tags center;
    //   Cut: scripts/cut-corridor-extract.py --src …/hedmark-latest.osm.pbf \
    //        --dst core/tests/fixtures/atna-hengebru-limits.osm.pbf \
    //        --bbox 10.800,61.710,10.860,61.750
    let src = require_fixture("atna-hengebru-limits.osm.pbf");
    let work = tempfile::tempdir().unwrap();
    let pbf = convert_truck_packs(work.path(), &src);
    let bbox = [61.710, 10.800, 61.750, 10.860];
    let g = try_load_graph_for_plan_bbox(work.path(), &pbf, RoutingProfile::Truck, Some(bbox))
        .expect("pack load");

    let bridge = g
        .edges
        .iter()
        .find(|e| e.name.as_deref() == Some("Atna hengebru"))
        .expect("Atna hengebru in pack");
    assert_eq!(bridge.maxlength_m, Some(12.4));
    assert_eq!(bridge.maxweight_t, Some(7.5));

    let (s, _) = g.nearest_routable(61.7285, 10.8300).expect("start");
    let (t, _) = g.nearest_routable(61.7292, 10.8220).expect("end");

    let open = g
        .shortest_path_with_options(s, t, false, &RouteOptions::default())
        .expect("unrestricted");
    assert!(path_uses_named(&g.edges, &open.0, "Atna hengebru"));

    let limited = g
        .shortest_path_with_options(
            s,
            t,
            false,
            &RouteOptions {
                vehicle: Some(VehicleLimits {
                    length_m: Some(14.0),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .expect("length-limited detour");
    assert!(!path_uses_named(&g.edges, &limited.0, "Atna hengebru"));
    assert!(limited.1 > open.1);
}

/// Atna hengebru maxweight=7.5 t: 10 t vehicle must detour.
#[test]
fn pack_path_atna_maxweight_finds_detour() {
    // Regenerate if way 323447948 is re-tagged/split/removed:
    //   Overpass: [out:json][timeout:120]; way(323447948); out tags center;
    //   Discover: way["maxweight"](60.5,9.5,62.6,12.8); out tags center;
    //   Cut: scripts/cut-corridor-extract.py --src …/hedmark-latest.osm.pbf \
    //        --dst core/tests/fixtures/atna-hengebru-limits.osm.pbf \
    //        --bbox 10.800,61.710,10.860,61.750
    let src = require_fixture("atna-hengebru-limits.osm.pbf");
    let work = tempfile::tempdir().unwrap();
    let pbf = convert_truck_packs(work.path(), &src);
    let bbox = [61.710, 10.800, 61.750, 10.860];
    let g = try_load_graph_for_plan_bbox(work.path(), &pbf, RoutingProfile::Truck, Some(bbox))
        .expect("pack load");

    let (s, _) = g.nearest_routable(61.7285, 10.8300).unwrap();
    let (t, _) = g.nearest_routable(61.7292, 10.8220).unwrap();

    let open = g
        .shortest_path_with_options(s, t, false, &RouteOptions::default())
        .unwrap();
    assert!(path_uses_named(&g.edges, &open.0, "Atna hengebru"));

    let limited = g
        .shortest_path_with_options(
            s,
            t,
            false,
            &RouteOptions {
                vehicle: Some(VehicleLimits {
                    total_weight_kg: Some(10_000.0),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .expect("weight-limited detour");
    assert!(!path_uses_named(&g.edges, &limited.0, "Atna hengebru"));
    assert!(limited.1 > open.1);
}

/// Stai bru (way 34106197): maxwidth=3.5 / maxaxleload=6 survive pack load and
/// block exceeding vehicles. Under-limit positive control proves the fixture is
/// connected through the bridge; oversized width/axle must not use it (detour
/// or `None` when this is the only Glomma crossing for the endpoints).
#[test]
fn pack_path_stai_maxwidth_and_maxaxleload_enforced() {
    // Regenerate if way 34106197 is re-tagged/split/removed:
    //   Overpass: [out:json][timeout:120]; way(34106197); out tags center;
    //   Discover: way["maxwidth"](60.5,9.5,62.6,12.8); out tags center;
    //     (also tagged maxheight/maxweight/maxaxleload — multi-tag Stai bru)
    //   Cut: scripts/cut-corridor-extract.py --src …/hedmark-latest.osm.pbf \
    //        --dst core/tests/fixtures/stai-bru-limits.osm.pbf \
    //        --bbox 11.000,61.460,11.120,61.540
    let src = require_fixture("stai-bru-limits.osm.pbf");
    let work = tempfile::tempdir().unwrap();
    let pbf = convert_truck_packs(work.path(), &src);
    let bbox = [61.460, 11.000, 61.540, 11.120];
    let g = try_load_graph_for_plan_bbox(work.path(), &pbf, RoutingProfile::Truck, Some(bbox))
        .expect("pack load");

    let stai = g
        .edges
        .iter()
        .find(|e| e.name.as_deref() == Some("Stai bru"))
        .expect("Stai bru in pack");
    assert_eq!(stai.maxwidth_m, Some(3.5));
    assert_eq!(stai.maxaxleload_t, Some(6.0));
    assert_eq!(stai.maxheight_m, Some(3.7));
    assert_eq!(stai.maxweight_t, Some(28.0));

    let (s, _) = g.nearest_routable(61.4952, 11.0620).unwrap();
    let (t, _) = g.nearest_routable(61.4958, 11.0500).unwrap();

    // Positive control: ordinary car comfortably under all four tagged limits.
    // If this fails, the extract/bbox is disconnected — not a routing-limit bug.
    let under = g
        .shortest_path_with_options(
            s,
            t,
            false,
            &RouteOptions {
                vehicle: Some(VehicleLimits {
                    width_m: Some(2.0),
                    height_m: Some(2.0),
                    total_weight_kg: Some(2_000.0),
                    axle_weight_kg: Some(1_500.0),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .expect(
            "under-limit car must route through Stai bru — fixture connectivity failure if None",
        );
    assert!(
        path_uses_named(&g.edges, &under.0, "Stai bru"),
        "under-limit positive control must use Stai bru (way 34106197)"
    );

    let wide = g.shortest_path_with_options(
        s,
        t,
        false,
        &RouteOptions {
            vehicle: Some(VehicleLimits {
                width_m: Some(4.0),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    assert!(
        wide.as_ref()
            .map(|(p, _)| !path_uses_named(&g.edges, p, "Stai bru"))
            .unwrap_or(true),
        "width 4.0 m must not use maxwidth=3.5 Stai bru (got {wide:?})"
    );

    let axle = g.shortest_path_with_options(
        s,
        t,
        false,
        &RouteOptions {
            vehicle: Some(VehicleLimits {
                axle_weight_kg: Some(8_000.0),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    assert!(
        axle.as_ref()
            .map(|(p, _)| !path_uses_named(&g.edges, p, "Stai bru"))
            .unwrap_or(true),
        "axle 8 t must not use maxaxleload=6 Stai bru (got {axle:?})"
    );
}

/// Liabrue (way 118689240, Lom, Innlandet): maxbogieweight=7.5 t.
/// Only Innlandet way with this tag; no alternate crossing in-extract.
#[test]
fn pack_path_liabrue_maxbogieweight_enforced() {
    // Regenerate if way 118689240 is re-tagged/split/removed:
    //   Overpass: [out:json][timeout:120]; way(118689240); out tags center;
    //   Discover: way["maxbogieweight"](60.5,8.0,62.6,12.8); out tags center;
    //     (Innlandet west of 9.5°E — Lom; prior 9.5°E east-only bbox missed it)
    //   Cut: scripts/cut-corridor-extract.py --src …/oppland-latest.osm.pbf \
    //        --dst core/tests/fixtures/liabrue-bogie-limits.osm.pbf \
    //        --bbox 8.620,61.820,8.760,61.900
    let src = require_fixture("liabrue-bogie-limits.osm.pbf");
    let work = tempfile::tempdir().unwrap();
    let pbf = convert_truck_packs(work.path(), &src);
    let bbox = [61.820, 8.620, 61.900, 8.760];
    let g = try_load_graph_for_plan_bbox(work.path(), &pbf, RoutingProfile::Truck, Some(bbox))
        .expect("pack load");

    let bridge = g
        .edges
        .iter()
        .find(|e| e.name.as_deref() == Some("Liabrue") && e.maxbogieweight_t.is_some())
        .expect("Liabrue bridge segment with bogie limit in pack");
    assert_eq!(bridge.maxbogieweight_t, Some(7.5));
    assert_eq!(bridge.maxaxleload_t, Some(5.0));
    assert_eq!(bridge.maxwidth_m, Some(3.5));

    let (s, _) = g.nearest_routable(61.8570, 8.6860).unwrap();
    let (t, _) = g.nearest_routable(61.8620, 8.6800).unwrap();

    // Positive control: bogie under 7.5 t (and under co-tagged axle/width).
    // If this fails, the extract/bbox is disconnected — not a routing-limit bug.
    let under = g
        .shortest_path_with_options(
            s,
            t,
            false,
            &RouteOptions {
                vehicle: Some(VehicleLimits {
                    bogie_weight_kg: Some(5_000.0),
                    axle_weight_kg: Some(4_000.0),
                    width_m: Some(2.5),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .expect(
            "under-limit vehicle must route through Liabrue — fixture connectivity failure if None",
        );
    assert!(
        path_uses_bogie_span(&g.edges, &under.0),
        "under-limit positive control must use Liabrue bogie span (way 118689240)"
    );

    let limited = g.shortest_path_with_options(
        s,
        t,
        false,
        &RouteOptions {
            vehicle: Some(VehicleLimits {
                bogie_weight_kg: Some(10_000.0),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    assert!(
        limited
            .as_ref()
            .map(|(p, _)| !path_uses_bogie_span(&g.edges, p))
            .unwrap_or(true),
        "bogie 10 t must not use maxbogieweight=7.5 Liabrue (got {limited:?})"
    );
}
