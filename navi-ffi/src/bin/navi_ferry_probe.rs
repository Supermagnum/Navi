//! Query SH/DK graph packs and ferry overlay sidecars near Puttgarden / Rødby.

use std::path::PathBuf;

use driver_break_core::routing::graph::{GraphEdge, RouteGraph, RoutingProfile};
use driver_break_core::routing::indexed::{
    ferry_sidecar_path, load_graph_pack_clips, manifest_path, NaviManifest,
};

fn haversine_m(alat: f64, alon: f64, blat: f64, blon: f64) -> f64 {
    let r = 6_371_000.0;
    let dlat = (blat - alat).to_radians();
    let dlon = (blon - alon).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + alat.to_radians().cos() * blat.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

fn near_edge(e: &GraphEdge, lat: f64, lon: f64, rad_m: f64) -> bool {
    haversine_m(e.start_lat, e.start_lon, lat, lon) <= rad_m
        || haversine_m(e.end_lat, e.end_lon, lat, lon) <= rad_m
}

fn report_graph(tag: &str, g: &RouteGraph, p: (f64, f64), r: (f64, f64), rad_m: f64) {
    let ferries: Vec<_> = g.edges.iter().filter(|e| e.is_ferry).collect();
    let near_p = ferries
        .iter()
        .filter(|e| near_edge(e, p.0, p.1, rad_m))
        .count();
    let near_r = ferries
        .iter()
        .filter(|e| near_edge(e, r.0, r.1, rad_m))
        .count();
    println!(
        "{tag} nodes={} edges={} ferry={} near_puttgarden={near_p} near_rodby={near_r}",
        g.nodes.len(),
        g.edges.len(),
        ferries.len()
    );
    for e in ferries
        .iter()
        .filter(|e| near_edge(e, p.0, p.1, rad_m) || near_edge(e, r.0, r.1, rad_m))
    {
        let src_roads = g
            .outgoing_edge_indices(e.source)
            .iter()
            .filter(|&&i| !g.edges[i].is_ferry)
            .count();
        let tgt_roads = g
            .outgoing_edge_indices(e.target)
            .iter()
            .filter(|&&i| !g.edges[i].is_ferry)
            .count();
        println!(
            "  ferry {:.5},{:.5} -> {:.5},{:.5} len_m={:.0} src_road_out={src_roads} tgt_road_out={tgt_roads}",
            e.start_lat, e.start_lon, e.end_lat, e.end_lon, e.length_m
        );
    }
}

fn main() {
    let dir = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("usage: navi-ferry-probe PACK_DIR"),
    );
    let puttgarden = (54.50_f64, 11.23_f64);
    let rodby = (54.65_f64, 11.35_f64);
    let clip = [54.35, 11.05, 54.80, 11.55];
    let rad_m = 8_000.0;
    for stem in ["schleswig-holstein-latest", "denmark-latest"] {
        println!("=== stem={stem} dir={} ===", dir.display());
        let man = NaviManifest::load(&manifest_path(&dir, stem));
        match &man {
            Ok(m) => println!(
                "manifest format={} tiles_car={} tiles_truck={}",
                m.graph_format_version,
                m.graph_tiles.get("car").map(|t| t.len()).unwrap_or(0),
                m.graph_tiles.get("truck").map(|t| t.len()).unwrap_or(0)
            ),
            Err(e) => println!("manifest: {e}"),
        }
        for profile in [RoutingProfile::Car, RoutingProfile::Truck] {
            let side = ferry_sidecar_path(&dir, stem, profile);
            println!(
                "overlay {:?} exists={} bytes={}",
                profile,
                side.is_file(),
                side.metadata().map(|m| m.len()).unwrap_or(0)
            );
            if side.is_file() {
                match load_graph_pack_clips(&side, profile, Some(&[clip])) {
                    Ok(g) => report_graph("overlay", &g, puttgarden, rodby, rad_m),
                    Err(e) => println!("overlay load failed: {e}"),
                }
            }
            if let Ok(m) = &man {
                if let Some(tiles) = m.graph_tiles_for(profile) {
                    let mut ferry = 0u64;
                    let mut near_p = 0u64;
                    let mut near_r = 0u64;
                    for t in tiles {
                        let p = dir.join(&t.file);
                        if !p.is_file() {
                            continue;
                        }
                        let Ok(g) = load_graph_pack_clips(&p, profile, Some(&[clip])) else {
                            continue;
                        };
                        for e in &g.edges {
                            if !e.is_ferry {
                                continue;
                            }
                            ferry += 1;
                            if near_edge(e, puttgarden.0, puttgarden.1, rad_m) {
                                near_p += 1;
                            }
                            if near_edge(e, rodby.0, rodby.1, rad_m) {
                                near_r += 1;
                            }
                        }
                    }
                    println!(
                        "pack {:?} ferry_in_clip={ferry} near_puttgarden={near_p} near_rodby={near_r}",
                        profile
                    );
                }
            }
        }
    }
}
