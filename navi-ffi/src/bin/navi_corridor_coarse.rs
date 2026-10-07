//! Stage A coarse search on merged corridor skeletons (directed travel time).
//!
//! usage:
//!   navi-corridor-coarse SKEL_DIR OUT_JSON [PACK_DIR]
//!
//! Reads `*.navi-corridor-skeleton.json` from SKEL_DIR, merges on shared OSM
//! ids, runs directed A* with default RouteOptions (tolls/tunnels allowed),
//! exports Bevensen + forced alternatives + Aga.

use std::path::{Path, PathBuf};
use std::time::Instant;

use driver_break_core::routing::corridor_skeleton::{
    border_osm_from_skeletons, build_coarse_route_report, coarse_shortest_path,
    inter_region_ferry_edges, merge_skeletons_to_route_graph, read_skeleton_file,
    CorridorSkeletonFile, CoarseRouteReport,
};
use driver_break_core::routing::graph::RoutingProfile;
use driver_break_core::routing::indexed::{ferry_sidecar_path, load_graph_pack_clips};
use serde::Serialize;

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

fn load_all_skeletons(dir: &Path) -> Vec<CorridorSkeletonFile> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    let mut paths: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".navi-corridor-skeleton.json"))
        })
        .collect();
    paths.sort();
    for p in paths {
        match read_skeleton_file(&p) {
            Ok(s) => {
                println!(
                    "loaded {} nodes={} edges={} border={}",
                    s.leaf_stem, s.node_count, s.edge_count, s.border_node_count
                );
                out.push(s);
            }
            Err(e) => eprintln!("skip {}: {e}", p.display()),
        }
    }
    out
}

fn haversine_m(alat: f64, alon: f64, blat: f64, blon: f64) -> f64 {
    let r = 6_371_000.0_f64;
    let dlat = (blat - alat).to_radians();
    let dlon = (blon - alon).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + alat.to_radians().cos() * blat.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

fn run_named(
    name: &str,
    graph: &mut driver_break_core::routing::graph::RouteGraph,
    waypoints: &[(f64, f64)],
    border_osm: &std::collections::HashSet<i64>,
    note: &str,
) -> Option<CoarseRouteReport> {
    let t0 = Instant::now();
    let (path, edges, _cost) = coarse_shortest_path(graph, waypoints, 35_000.0)?;
    let search_ms = t0.elapsed().as_millis() as u64;
    let vias = &waypoints[1..waypoints.len().saturating_sub(1)];
    Some(build_coarse_route_report(
        name,
        graph,
        &path,
        &edges,
        border_osm,
        vias,
        search_ms,
        peak_rss_mb(),
        note,
    ))
}

fn connectivity_checks(skels: &[CorridorSkeletonFile]) {
    let sh = skels
        .iter()
        .find(|s| s.leaf_stem.contains("schleswig-holstein"));
    let dk = skels.iter().find(|s| s.leaf_stem.contains("denmark"));
    let (Some(sh), Some(dk)) = (sh, dk) else {
        println!("connectivity: missing SH or DK skeleton");
        return;
    };
    let sh_ids: std::collections::HashSet<i64> = sh.node_ids.iter().copied().collect();
    let dk_ids: std::collections::HashSet<i64> = dk.node_ids.iter().copied().collect();
    let common: Vec<_> = sh_ids.intersection(&dk_ids).copied().collect();
    let padborg = common.iter().filter(|&&oid| {
        let i = sh.node_ids.iter().position(|&x| x == oid).unwrap();
        let la = sh.node_lats[i];
        let lo = sh.node_lons[i];
        (54.75..=54.90).contains(&la) && (9.20..=9.50).contains(&lo)
    });
    let padborg_n = padborg.count();
    let mut fehmarn = false;
    for i in 0..sh.edge_src.len() {
        if sh.edge_is_ferry.get(i).copied().unwrap_or(0) == 0 {
            continue;
        }
        let s = sh.edge_src[i] as usize;
        let t = sh.edge_tgt[i] as usize;
        let sla = sh.node_lats[s];
        let slo = sh.node_lons[s];
        let tla = sh.node_lats[t];
        let tlo = sh.node_lons[t];
        let putt = haversine_m(sla, slo, 54.5028, 11.2282) < 3000.0
            || haversine_m(tla, tlo, 54.5028, 11.2282) < 3000.0;
        let rod = haversine_m(sla, slo, 54.6543, 11.3508) < 3000.0
            || haversine_m(tla, tlo, 54.6543, 11.3508) < 3000.0;
        if putt && rod {
            let s_osm = sh.node_ids[s];
            let t_osm = sh.node_ids[t];
            fehmarn = dk_ids.contains(&s_osm) || dk_ids.contains(&t_osm);
            println!(
                "connectivity: Puttgarden-Rodby ferry in SH skeleton len_m={:.0} \
                 weight={:.0} endpoints_in_DK={} osm={s_osm}->{t_osm}",
                sh.edge_length_m[i],
                sh.edge_base_weight[i],
                dk_ids.contains(&s_osm) && dk_ids.contains(&t_osm)
            );
            break;
        }
    }
    println!(
        "connectivity: SH-DK shared_osm={} padborg_shared≈{padborg_n} fehmarn_join={fehmarn}",
        common.len()
    );
}

fn probe_aga_ferries(pack_dir: Option<&Path>, skels: &[CorridorSkeletonFile]) {
    let Some(vest) = skels.iter().find(|s| s.leaf_stem.contains("vestlandet")) else {
        println!("aga: no vestlandet skeleton");
        return;
    };
    println!("aga: vestlandet ferry_edges={}", vest.ferry_edge_count);
    for i in 0..vest.edge_src.len() {
        if vest.edge_is_ferry.get(i).copied().unwrap_or(0) == 0 {
            continue;
        }
        let s = vest.edge_src[i] as usize;
        let t = vest.edge_tgt[i] as usize;
        let sla = vest.node_lats[s];
        let slo = vest.node_lons[s];
        let tla = vest.node_lats[t];
        let tlo = vest.node_lons[t];
        let near_utne = haversine_m(tla, tlo, 60.4241, 6.6218) < 4000.0
            || haversine_m(sla, slo, 60.4241, 6.6218) < 4000.0;
        if !near_utne {
            continue;
        }
        let drive = driver_break_core::routing::graph::ferry_drive_equiv_m_per_s();
        let board_m =
            driver_break_core::routing::graph::FERRY_CAR_BOARDING_PENALTY_MIN * 60.0 * drive;
        let cross_min = ((vest.edge_base_weight[i] - board_m).max(0.0) / drive / 60.0).max(0.0);
        let name = vest
            .edge_name
            .get(i)
            .map(|s| s.as_str())
            .unwrap_or("");
        println!(
            "aga skeleton ferry near Utne: {:.5},{:.5} -> {:.5},{:.5} len_m={:.0} \
             crossing_min≈{cross_min:.1} boarding=10 name={name:?}",
            sla, slo, tla, tlo, vest.edge_length_m[i]
        );
    }
    let Some(dir) = pack_dir else {
        return;
    };
    let side = ferry_sidecar_path(dir, "vestlandet-latest", RoutingProfile::Car);
    if !side.is_file() {
        println!("aga: overlay missing {}", side.display());
        return;
    }
    match load_graph_pack_clips(&side, RoutingProfile::Car, None) {
        Ok(g) => {
            let mut kin = 0u32;
            let mut kvan = 0u32;
            for e in g.edges.iter().filter(|e| e.is_ferry) {
                let u = haversine_m(e.start_lat, e.start_lon, 60.4241, 6.6218) < 4000.0
                    || haversine_m(e.end_lat, e.end_lon, 60.4241, 6.6218) < 4000.0;
                if !u {
                    continue;
                }
                let kins = haversine_m(e.start_lat, e.start_lon, 60.3750, 6.7200) < 4000.0
                    || haversine_m(e.end_lat, e.end_lon, 60.3750, 6.7200) < 4000.0;
                let kv = haversine_m(e.start_lat, e.start_lon, 60.4718, 6.6124) < 4000.0
                    || haversine_m(e.end_lat, e.end_lon, 60.4718, 6.6124) < 4000.0;
                if kins {
                    kin += 1;
                    println!(
                        "aga overlay Kinsarvik-Utne candidate: {:.5},{:.5}->{:.5},{:.5} \
                         len_m={:.0} weight={:.0} name={:?}",
                        e.start_lat,
                        e.start_lon,
                        e.end_lat,
                        e.end_lon,
                        e.length_m,
                        e.base_weight,
                        e.name
                    );
                }
                if kv {
                    kvan += 1;
                    println!(
                        "aga overlay Kvanndal-Utne candidate: {:.5},{:.5}->{:.5},{:.5} \
                         len_m={:.0} weight={:.0} name={:?}",
                        e.start_lat,
                        e.start_lon,
                        e.end_lat,
                        e.end_lon,
                        e.length_m,
                        e.base_weight,
                        e.name
                    );
                }
            }
            println!("aga overlay near Utne: kinsarvik_hits={kin} kvanndal_hits={kvan}");
            drop(g);
        }
        Err(e) => eprintln!("aga overlay load failed: {e}"),
    }
}

#[derive(Serialize)]
struct OutFile {
    note: String,
    peak_rss_mb: u64,
    inter_region_ferries_sample: usize,
    bevensen_chosen: Option<CoarseRouteReport>,
    bevensen_forced_fehmarn_a1: Option<CoarseRouteReport>,
    bevensen_forced_jutland_oresund: Option<CoarseRouteReport>,
    bevensen_forced_jutland_hh: Option<CoarseRouteReport>,
    aga: Option<CoarseRouteReport>,
    aga_force_kvanndal_utne: Option<CoarseRouteReport>,
    aga_force_kinsarvik_utne: Option<CoarseRouteReport>,
}

fn main() {
    let mut args = std::env::args().skip(1);
    let skel_dir = PathBuf::from(
        args.next()
            .expect("usage: navi-corridor-coarse SKEL_DIR OUT_JSON [PACK_DIR]"),
    );
    let out_json = PathBuf::from(args.next().expect("OUT_JSON required"));
    let pack_dir = args.next().map(PathBuf::from);

    let skels = load_all_skeletons(&skel_dir);
    assert!(!skels.is_empty(), "no skeletons in {}", skel_dir.display());
    connectivity_checks(&skels);
    let inter = inter_region_ferry_edges(&skels);
    println!("inter_region_ferry_edge_records={}", inter.len());
    for row in inter.iter().take(12) {
        println!(
            "  inter_ferry region={} {:.5},{:.5}->{:.5},{:.5} len_m={:.0}",
            row.0, row.2, row.3, row.4, row.5, row.6
        );
    }
    probe_aga_ferries(pack_dir.as_deref(), &skels);

    let border_osm = border_osm_from_skeletons(&skels);
    println!(
        "border_osm_ids={} merging directed skeletons…",
        border_osm.len()
    );
    let t_merge = Instant::now();
    let mut graph = merge_skeletons_to_route_graph(&skels, RoutingProfile::Car);
    println!(
        "merged nodes={} edges={} merge_ms={} peak_rss_mb={}",
        graph.nodes.len(),
        graph.edges.len(),
        t_merge.elapsed().as_millis(),
        peak_rss_mb()
    );

    // Bevensen → Vågåvegen 80 → Dalsøren
    let bevensen = (53.07969, 10.5872);
    let vaga = (61.875, 9.096);
    let dalsoren = (61.44338, 7.4614);

    let chosen = run_named(
        "bevensen_vaga_dalsoren",
        &mut graph,
        &[bevensen, vaga, dalsoren],
        &border_osm,
        "directed travel-time coarse on persistent skeletons; joints=border+ferry+via",
    );

    // Forced: A1 corridor + Puttgarden–Rødby. Use exact ferry terminal
    // coordinates (known connected by the Fehmarn edge at cost 73333).
    let a1_near_lubeck = (53.87, 10.69);
    let puttgarden = (54.5028164, 11.2282207);
    let rodby = (54.6543072, 11.3508124);
    let forced_fehmarn = run_named(
        "forced_a1_puttgarden_rodby",
        &mut graph,
        &[bevensen, a1_near_lubeck, puttgarden, rodby, vaga, dalsoren],
        &border_osm,
        "forced via A1/Lübeck and Puttgarden–Rødby ferry terminals",
    );

    // Forced: Jutland + Storebælt + Øresund bridge
    let padborg = (54.82, 9.36);
    let storebaelt = (55.34, 10.97);
    let oresund = (55.57, 12.85);
    let forced_oresund = run_named(
        "forced_jutland_storebaelt_oresund",
        &mut graph,
        &[bevensen, padborg, storebaelt, oresund, vaga, dalsoren],
        &border_osm,
        "forced via Padborg, Storebælt, Øresund bridge",
    );

    // Forced: Jutland + Storebælt + Helsingør–Helsingborg
    let helsingor = (56.033, 12.616);
    let forced_hh = run_named(
        "forced_jutland_storebaelt_hh",
        &mut graph,
        &[bevensen, padborg, storebaelt, helsingor, vaga, dalsoren],
        &border_osm,
        "forced via Padborg, Storebælt, Helsingør (HH ferry)",
    );

    // Breneriroa → Aga (FU14 waypoints); ferry naming on Hardanger.
    let breneriroa = (60.82718, 11.30278);
    let aga = (60.2987, 6.60322);
    let utne = (60.4241, 6.6218);
    let kvanndal = (60.4718, 6.6124);
    let kinsarvik = (60.3750, 6.7200);
    let aga_report = run_named(
        "breneriroa_aga",
        &mut graph,
        &[breneriroa, aga],
        &border_osm,
        "Breneriroa→Aga; ferry named by terminals",
    );
    let aga_via_kvan = run_named(
        "aga_force_kvanndal_utne",
        &mut graph,
        &[breneriroa, kvanndal, utne, aga],
        &border_osm,
        "forced Kvanndal–Utne",
    );
    let aga_via_kins = run_named(
        "aga_force_kinsarvik_utne",
        &mut graph,
        &[breneriroa, kinsarvik, utne, aga],
        &border_osm,
        "forced Kinsarvik–Utne",
    );

    let print_report = |label: &str, r: &CoarseRouteReport| {
        println!(
            "{label}: km={:.3} drive_min={:.1} ferry_min={:.1} total_min={:.1} \
             astar_cost_m={:.0} ferries={} joints={}",
            r.total_km,
            r.driving_min,
            r.ferry_min,
            r.total_min,
            r.astar_cost_m,
            r.ferries.len(),
            r.joints.len()
        );
        for f in &r.ferries {
            println!(
                "  ferry {} → {} cross={:.1} board={:.1} km={:.3}",
                f.from_terminal, f.to_terminal, f.crossing_min, f.boarding_min, f.km
            );
        }
        for j in &r.joints {
            println!(
                "  joint {} {:.5},{:.5} osm={}",
                j.joint_type, j.lat, j.lon, j.osm_id
            );
        }
        for c in &r.countries {
            println!(
                "  country {} km={:.2} drive_min={:.1} ferry_min={:.1} roads={}",
                c.iso,
                c.km,
                c.driving_min,
                c.ferry_min,
                c.roads.len()
            );
            for road in c.roads.iter().take(40) {
                println!("    road {road}");
            }
            if c.roads.len() > 40 {
                println!("    … {} more roads", c.roads.len() - 40);
            }
        }
    };
    if let Some(r) = &chosen {
        print_report("CHOSEN", r);
    }
    for (label, r) in [
        ("fehmarn", &forced_fehmarn),
        ("oresund", &forced_oresund),
        ("hh", &forced_hh),
        ("aga", &aga_report),
        ("aga_kvan", &aga_via_kvan),
        ("aga_kins", &aga_via_kins),
    ] {
        if let Some(r) = r {
            print_report(&format!("ALT {label}"), r);
        } else {
            println!("ALT {label}: NO PATH");
        }
    }

    let out = OutFile {
        note: "FU16 Stage A directed travel-time coarse; Stage B not started".into(),
        peak_rss_mb: peak_rss_mb(),
        inter_region_ferries_sample: inter.len(),
        bevensen_chosen: chosen,
        bevensen_forced_fehmarn_a1: forced_fehmarn,
        bevensen_forced_jutland_oresund: forced_oresund,
        bevensen_forced_jutland_hh: forced_hh,
        aga: aga_report,
        aga_force_kvanndal_utne: aga_via_kvan,
        aga_force_kinsarvik_utne: aga_via_kins,
    };
    let f = std::fs::File::create(&out_json).expect("create out");
    serde_json::to_writer_pretty(f, &out).expect("write json");
    println!("wrote {} peak_rss_mb={}", out_json.display(), peak_rss_mb());
}
