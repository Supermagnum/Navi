//! Stage A: build corridor skeletons from a pack directory (memory-bounded).
//!
//! Two passes:
//! 1. Per stem, collect major-road/ferry OSM node ids (tile-at-a-time).
//! 2. Shared OSM ids across stems become border crossings; rebuild each stem
//!    with those borders so secondary approaches to borders stay in the skeleton.
//!
//! Peak RSS is bounded by processing one tile (then one region accumulator) at a
//! time and dropping the full RouteGraph before the next tile.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use driver_break_core::routing::corridor_skeleton::{
    build_skeleton_from_pack, major_node_osm_ids, merge_skeleton_files, shared_osm_ids_across_regions,
    skeleton_path, write_skeleton_file, CorridorSkeletonFile,
};
use driver_break_core::routing::graph::RoutingProfile;
use driver_break_core::routing::indexed::{
    ferry_sidecar_path, load_graph_pack_clips, manifest_path, FlatGraphPack, NaviManifest,
};

fn leaf_to_region(stem: &str) -> String {
    let s = stem.trim_end_matches("-latest");
    match s {
        "denmark" => "europe/denmark".into(),
        "finland" => "europe/finland".into(),
        "sweden" => "europe/sweden".into(),
        "hamburg" => "europe/germany/hamburg".into(),
        "niedersachsen" => "europe/germany/niedersachsen".into(),
        "schleswig-holstein" => "europe/germany/schleswig-holstein".into(),
        "mecklenburg-vorpommern" => "europe/germany/mecklenburg-vorpommern".into(),
        "ostlandet" => "europe/norway/ostlandet".into(),
        "vestlandet" => "europe/norway/vestlandet".into(),
        "sorlandet" => "europe/norway/sorlandet".into(),
        "nord-norge" => "europe/norway/nord-norge".into(),
        "skane" => "europe/sweden/skane".into(),
        "halland" => "europe/sweden/halland".into(),
        "vastra_gotaland" => "europe/sweden/vastra_gotaland".into(),
        other => format!("europe/{other}"),
    }
}

fn peak_rss_mb() -> u64 {
    let Ok(s) = std::fs::read_to_string("/proc/self/status") else {
        return 0;
    };
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            let kb: u64 = rest
                .split_whitespace()
                .next()
                .and_then(|x| x.parse().ok())
                .unwrap_or(0);
            return kb / 1024;
        }
    }
    0
}

fn trim_rss() {
    // Best-effort: drop large locals and let the allocator reuse; avoid a hard
    // libc dependency in this bin. Peak is bounded by one-tile residency.
}

fn tile_paths(dir: &Path, stem: &str, profile: RoutingProfile) -> Vec<PathBuf> {
    let Ok(man) = NaviManifest::load(&manifest_path(dir, stem)) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(tiles) = man.graph_tiles_for(profile) {
        for rel in tiles {
            out.push(dir.join(&rel.file));
        }
    }
    if out.is_empty() {
        let key = match profile {
            RoutingProfile::Truck => "truck",
            RoutingProfile::Foot => "foot",
            _ => "car",
        };
        if let Some(rel) = man.graph_files.get(key) {
            out.push(dir.join(rel));
        }
    }
    out
}

/// Load one pack file, convert to flat, drop the RouteGraph immediately.
fn load_flat_tile(path: &Path, profile: RoutingProfile) -> Option<FlatGraphPack> {
    let g = load_graph_pack_clips(path, profile, None).ok()?;
    let flat = FlatGraphPack::from_route_graph(&g, None);
    drop(g);
    Some(flat)
}

fn collect_major_ids(pack_dir: &Path, stem: &str, profile: RoutingProfile) -> HashSet<i64> {
    let mut ids = HashSet::new();
    for p in tile_paths(pack_dir, stem, profile) {
        let Some(flat) = load_flat_tile(&p, profile) else {
            eprintln!("  skip major-id {}: load failed", p.display());
            continue;
        };
        ids.extend(major_node_osm_ids(&flat));
        drop(flat);
        trim_rss();
    }
    ids
}

fn build_stem_skeleton(
    pack_dir: &Path,
    stem: &str,
    profile: RoutingProfile,
    border_osm: &HashSet<i64>,
) -> Option<CorridorSkeletonFile> {
    let region = leaf_to_region(stem);
    let prof = match profile {
        RoutingProfile::Truck => "truck",
        _ => "car",
    };
    let mut parts = Vec::new();
    for p in tile_paths(pack_dir, stem, profile) {
        let Some(flat) = load_flat_tile(&p, profile) else {
            eprintln!("  skip build {}: load failed", p.display());
            continue;
        };
        let part = build_skeleton_from_pack(&flat, &region, stem, prof, border_osm);
        drop(flat);
        parts.push(part);
        trim_rss();
    }
    // Ferry overlay sidecar carries pier/approach stubs that pack tiles may
    // omit; without them ferry terminals are water-only and Forced Fehmarn
    // cannot enter from land.
    let side = ferry_sidecar_path(pack_dir, stem, profile);
    if side.is_file() {
        match load_flat_tile(&side, profile) {
            Some(flat) => {
                let ferry_n = flat
                    .edge_is_ferry
                    .iter()
                    .filter(|&&v| v != 0)
                    .count();
                println!(
                    "  overlay stem={stem} nodes={} edges={} ferry_edges≈{ferry_n}",
                    flat.node_ids.len(),
                    flat.edge_src.len()
                );
                let part = build_skeleton_from_pack(&flat, &region, stem, prof, border_osm);
                drop(flat);
                parts.push(part);
                trim_rss();
            }
            None => eprintln!("  skip overlay {}: load failed", side.display()),
        }
    } else {
        println!("  overlay stem={stem}: no sidecar at {}", side.display());
    }
    merge_skeleton_files(parts)
}

fn build_one(
    pack_dir: &Path,
    out_dir: &Path,
    stem: &str,
    profile: RoutingProfile,
    border_osm: &HashSet<i64>,
) {
    let t0 = Instant::now();
    let peak_before = peak_rss_mb();
    let Some(skel) = build_stem_skeleton(pack_dir, stem, profile, border_osm) else {
        eprintln!("FAIL build {stem}");
        return;
    };
    let file_bytes = match write_skeleton_file(out_dir, &skel) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("FAIL write {stem}: {e}");
            return;
        }
    };
    let peak = peak_rss_mb();
    println!(
        "skeleton region={} stem={stem} nodes={} edges={} ferry_edges={} secondary={} \
         border_nodes={} ferry_terminals={} build_ms={} wall_ms={} file_bytes={file_bytes} \
         peak_rss_mb={peak} delta_rss_mb={} path={}",
        skel.region_id,
        skel.node_count,
        skel.edge_count,
        skel.ferry_edge_count,
        skel.secondary_edge_count,
        skel.border_node_count,
        skel.ferry_terminal_count,
        skel.build_ms,
        t0.elapsed().as_millis(),
        peak.saturating_sub(peak_before),
        skeleton_path(out_dir, stem).display()
    );
    drop(skel);
    trim_rss();
}

fn main() {
    let mut args = std::env::args().skip(1);
    let pack_dir = PathBuf::from(
        args.next()
            .expect("usage: navi-corridor-skeleton PACK_DIR [OUT_DIR] [stem...]"),
    );
    let out_dir = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| pack_dir.clone());
    let stems: Vec<String> = {
        let rest: Vec<String> = args.collect();
        if rest.is_empty() {
            let mut v = Vec::new();
            if let Ok(rd) = std::fs::read_dir(&pack_dir) {
                for e in rd.flatten() {
                    let n = e.file_name().to_string_lossy().into_owned();
                    if let Some(stem) = n.strip_suffix(".navi-manifest.json") {
                        v.push(stem.to_string());
                    }
                }
            }
            v.sort();
            v
        } else {
            rest
        }
    };
    std::fs::create_dir_all(&out_dir).ok();
    let profile = RoutingProfile::Car;
    println!(
        "pack_dir={} out_dir={} stems={} (pass1: major node ids)",
        pack_dir.display(),
        out_dir.display(),
        stems.len()
    );
    let mut region_sets = Vec::new();
    for stem in &stems {
        let ids = collect_major_ids(&pack_dir, stem, profile);
        println!(
            "  major_ids stem={stem} count={} peak_rss_mb={}",
            ids.len(),
            peak_rss_mb()
        );
        region_sets.push(ids);
    }
    let border_osm = shared_osm_ids_across_regions(&region_sets);
    println!(
        "shared_border_osm_ids={} peak_rss_mb={}",
        border_osm.len(),
        peak_rss_mb()
    );
    drop(region_sets);
    trim_rss();
    println!("pass2: build skeletons with border anchors");
    for stem in &stems {
        build_one(&pack_dir, &out_dir, stem, profile, &border_osm);
    }
    println!("done peak_rss_mb={}", peak_rss_mb());
}
