//! CLI: convert a region `.osm.pbf` into `.navi-graph-*.rkyv` + `.navi-poi-barrier.rkyv`.

use std::env;
use std::path::PathBuf;

use driver_break_core::routing::graph::RoutingProfile;
use driver_break_core::routing::indexed::{
    convert_region_packs, ensure_ferry_sidecar, ConvertOptions,
};

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut data_dir = None;
    let mut pbf = None;
    let mut elev = None;
    let mut profiles = vec![RoutingProfile::Car, RoutingProfile::Foot];
    let mut ferry_sidecar_only = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--ferry-sidecar-only" => {
                ferry_sidecar_only = true;
                i += 1;
            }
            "--data-dir" => {
                data_dir = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--pbf" => {
                pbf = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--elev-dir" => {
                elev = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--profiles" => {
                profiles = args[i + 1]
                    .split(',')
                    .map(|s| match s.trim() {
                        "car" => RoutingProfile::Car,
                        "truck" => RoutingProfile::Truck,
                        "foot" => RoutingProfile::Foot,
                        "bicycle" => RoutingProfile::Bicycle,
                        other => panic!("unknown profile {other}"),
                    })
                    .collect();
                i += 2;
            }
            other => panic!("unknown arg {other}"),
        }
    }
    let data_dir = data_dir.expect("--data-dir");
    let pbf = pbf.expect("--pbf");
    if ferry_sidecar_only {
        let stem = pbf
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("region.osm.pbf")
            .trim_end_matches(".osm.pbf")
            .trim_end_matches(".pbf")
            .to_string();
        let overlay_profiles = if profiles.is_empty() {
            vec![RoutingProfile::Truck, RoutingProfile::Car]
        } else {
            profiles
        };
        for profile in overlay_profiles {
            match ensure_ferry_sidecar(&data_dir, &stem, profile, &pbf) {
                Ok(wrote) => println!(
                    "PASS ferry_sidecar stem={stem} profile={profile:?} wrote_or_fresh={wrote}"
                ),
                Err(e) => {
                    eprintln!("FAIL ferry_sidecar stem={stem} profile={profile:?}: {e:#}");
                    std::process::exit(1);
                }
            }
        }
        return;
    }
    let mut opts = ConvertOptions::new(&data_dir, &pbf);
    opts.elev_dir = elev;
    opts.profiles = profiles;
    match convert_region_packs(&opts) {
        Ok(r) => {
            println!(
                "PASS stem={} convert_ms={:.1} bbox_scan_ms={:.1} graph_ms={:?} tile_assign_ms={:?} tile_build_ms={:?} poi_ms={:.1} barrier_ms={:.1} overnight_ms={:.1} wetland_ms={:.1} nodes={} edges={} pois={} barrier_segs={} wetland_rings={} graph_tiles={} peak_rss_mb={:.1} delta_h={} graphs={:?} poi={} wetland={} manifest={}",
                r.stem,
                r.convert_ms,
                r.bbox_scan_ms,
                r.graph_ms,
                r.tile_assign_ms,
                r.tile_build_ms,
                r.poi_ms,
                r.barrier_ms,
                r.overnight_ms,
                r.wetland_ms,
                r.nodes,
                r.edges,
                r.pois,
                r.barrier_segs,
                r.wetland_rings,
                r.graph_tiles,
                r.peak_rss_mb,
                r.has_delta_h,
                r.graph_files,
                r.poi_barrier_file,
                r.wetland_file,
                r.manifest_file
            );
        }
        Err(e) => {
            eprintln!("FAIL: {e:#}");
            std::process::exit(1);
        }
    }
}
