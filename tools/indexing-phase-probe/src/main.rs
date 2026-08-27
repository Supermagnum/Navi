//! Investigation probe: wetland/graph phase timings + Navi-vs-ORS keep stats.
use std::collections::{HashMap, HashSet};
use std::env;
use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

mod poi_barrier;

use driver_break_core::download::pbf_priority::for_each_pbf_elements;
use driver_break_core::routing::graph::{RouteGraph, RoutingProfile};
use driver_break_core::routing::wetland::WetlandWayExtract;
use driver_break_core::routing::graph::TiledBuildTimings;
use osmpbf::Element;

fn rss_mb() -> f64 {
    let Ok(s) = std::fs::read_to_string("/proc/self/status") else {
        return 0.0;
    };
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            let kb: f64 = rest
                .split_whitespace()
                .next()
                .and_then(|x| x.parse().ok())
                .unwrap_or(0.0);
            return kb / 1024.0;
        }
    }
    0.0
}

fn tag_map<'a>(tags: impl Iterator<Item = (&'a str, &'a str)>) -> HashMap<String, String> {
    tags.map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

fn wetland_ok(tags: &HashMap<String, String>) -> bool {
    if let Some(w) = tags.get("wetland") {
        let w = w.to_ascii_lowercase();
        if matches!(
            w.as_str(),
            "bog" | "string_bog" | "fen" | "swamp" | "reedbed"
        ) {
            return true;
        }
    }
    tags.get("natural")
        .is_some_and(|v| v.eq_ignore_ascii_case("wetland"))
}

fn barrier_way_ok(tags: &HashMap<String, String>) -> bool {
    if let Some(r) = tags.get("railway") {
        let r = r.to_ascii_lowercase();
        if !matches!(r.as_str(), "abandoned" | "disused" | "razed" | "dismantled") {
            return true;
        }
    }
    if let Some(w) = tags.get("waterway") {
        let w = w.to_ascii_lowercase();
        if matches!(w.as_str(), "river" | "canal") {
            return true;
        }
    }
    if let Some(n) = tags.get("natural") {
        let n = n.to_ascii_lowercase();
        if matches!(n.as_str(), "cliff" | "arete" | "glacier") {
            return true;
        }
    }
    false
}

fn highway_ok(tags: &HashMap<String, String>) -> bool {
    let Some(h) = tags.get("highway") else {
        return false;
    };
    matches!(
        h.as_str(),
        "motorway"
            | "motorway_link"
            | "motorway_junction"
            | "trunk"
            | "trunk_link"
            | "primary"
            | "primary_link"
            | "secondary"
            | "secondary_link"
            | "tertiary"
            | "tertiary_link"
            | "unclassified"
            | "residential"
            | "living_street"
            | "road"
            | "service"
            | "track"
            | "footway"
            | "path"
            | "steps"
            | "pedestrian"
            | "cycleway"
    )
}

fn building_ok(tags: &HashMap<String, String>) -> bool {
    tags.get("building")
        .is_some_and(|v| !v.eq_ignore_ascii_case("no"))
}

fn poi_node_ok(tags: &HashMap<String, String>) -> bool {
    // Mirror classifier.rs keep-set (investigation approximation).
    let amenity = tags.get("amenity").map(|s| s.as_str());
    let tourism = tags.get("tourism").map(|s| s.as_str());
    let natural = tags.get("natural").map(|s| s.as_str());
    let shop = tags.get("shop").map(|s| s.as_str());
    let leisure = tags.get("leisure").map(|s| s.as_str());
    let sport = tags.get("sport").map(|s| s.as_str());
    let craft = tags.get("craft").map(|s| s.as_str());
    let highway = tags.get("highway").map(|s| s.as_str());
    if matches!(
        amenity,
        Some("drinking_water")
            | Some("fountain")
            | Some("water_point")
            | Some("toilets")
            | Some("shelter")
            | Some("camping")
            | Some("cafe")
            | Some("restaurant")
            | Some("fast_food")
            | Some("museum")
            | Some("gallery")
            | Some("zoo")
            | Some("aquarium")
            | Some("viewpoint")
            | Some("picnic_site")
            | Some("parking")
    ) {
        return true;
    }
    if matches!(
        tourism,
        Some("wilderness_hut")
            | Some("alpine_hut")
            | Some("hostel")
            | Some("camp_site")
            | Some("camp_pitch")
            | Some("viewpoint")
            | Some("attraction")
            | Some("museum")
            | Some("hotel")
            | Some("motel")
            | Some("guest_house")
            | Some("apartment")
            | Some("chalet")
    ) {
        return true;
    }
    if matches!(natural, Some("spring")) {
        return true;
    }
    if matches!(shop, Some("alcohol") | Some("fishing")) {
        return true;
    }
    if matches!(leisure, Some("fishing") | Some("fishing_pier")) {
        return true;
    }
    if matches!(sport, Some("fishing")) {
        return true;
    }
    if matches!(craft, Some("brewery")) {
        return true;
    }
    if tags.get("microbrewery").is_some_and(|v| v == "yes") {
        return true;
    }
    if matches!(highway, Some("rest_area") | Some("services")) {
        return true;
    }
    false
}

fn ors_rejects_way(tags: &HashMap<String, String>) -> bool {
    // ORS ComplexElementsFilter bad keys (simplified): if has bad key and no highway/route
    let has_good = tags.contains_key("highway") || tags.contains_key("route");
    if has_good {
        return false;
    }
    for k in [
        "building", "landuse", "boundary", "natural", "place", "waterway", "aeroway",
        "aviation", "military", "power", "communication", "man_made",
    ] {
        if tags.contains_key(k) {
            return true;
        }
    }
    false
}

fn navi_keeps_way(tags: &HashMap<String, String>) -> bool {
    highway_ok(tags) || wetland_ok(tags) || barrier_way_ok(tags) || building_ok(tags)
}

fn navi_keeps_relation(tags: &HashMap<String, String>) -> bool {
    wetland_ok(tags) || tags.contains_key("highway") || tags.contains_key("route")
}

fn cmd_keep_stats(path: &Path) -> anyhow::Result<()> {
    let t0 = Instant::now();
    let mut ways_total = 0u64;
    let mut ways_ors_drop = 0u64;
    let mut ways_navi_keep = 0u64;
    let mut ways_navi_only = 0u64; // kept by Navi but dropped by ORS
    let mut wetland_ways = 0u64;
    let mut barrier_ways = 0u64;
    let mut building_ways = 0u64;
    let mut highway_ways = 0u64;
    let mut rel_total = 0u64;
    let mut rel_wetland = 0u64;
    let mut nodes_poi = 0u64;
    let mut nodes_building = 0u64;
    let mut nodes_barrier = 0u64;

    for_each_pbf_elements(path, |el| match el {
        Element::Way(w) => {
            ways_total += 1;
            let tags = tag_map(w.tags());
            let ors_drop = ors_rejects_way(&tags);
            let navi = navi_keeps_way(&tags);
            if ors_drop {
                ways_ors_drop += 1;
            }
            if navi {
                ways_navi_keep += 1;
                if ors_drop {
                    ways_navi_only += 1;
                }
            }
            if wetland_ok(&tags) {
                wetland_ways += 1;
            }
            if barrier_way_ok(&tags) {
                barrier_ways += 1;
            }
            if building_ok(&tags) {
                building_ways += 1;
            }
            if highway_ok(&tags) {
                highway_ways += 1;
            }
        }
        Element::Relation(r) => {
            rel_total += 1;
            let tags = tag_map(r.tags());
            if wetland_ok(&tags) {
                rel_wetland += 1;
            }
        }
        Element::Node(n) => {
            let tags = tag_map(n.tags());
            if poi_node_ok(&tags) {
                nodes_poi += 1;
            }
            if building_ok(&tags) {
                nodes_building += 1;
            }
            if tags.contains_key("barrier") {
                nodes_barrier += 1;
            }
        }
        Element::DenseNode(n) => {
            let tags = tag_map(n.tags());
            if poi_node_ok(&tags) {
                nodes_poi += 1;
            }
            if building_ok(&tags) {
                nodes_building += 1;
            }
            if tags.contains_key("barrier") {
                nodes_barrier += 1;
            }
        }
    })?;

    println!(
        "KEEP_STATS wall_ms={:.1} peak_rss_mb={:.1}",
        t0.elapsed().as_secs_f64() * 1000.0,
        rss_mb()
    );
    println!("ways_total={ways_total}");
    println!("ways_ors_would_drop={ways_ors_drop}");
    println!("ways_navi_keep={ways_navi_keep}");
    println!("ways_navi_keep_but_ors_drops={ways_navi_only}");
    println!(
        "ways_navi_keep_frac={:.4}",
        ways_navi_keep as f64 / ways_total.max(1) as f64
    );
    println!(
        "ways_ors_drop_frac={:.4}",
        ways_ors_drop as f64 / ways_total.max(1) as f64
    );
    println!("wetland_ways={wetland_ways} barrier_ways={barrier_ways} building_ways={building_ways} highway_ways={highway_ways}");
    println!("rel_total={rel_total} rel_wetland={rel_wetland}");
    println!("nodes_poi={nodes_poi} nodes_building={nodes_building} nodes_barrier={nodes_barrier}");
    Ok(())
}

fn cmd_wetland_profile(path: &Path) -> anyhow::Result<()> {
    // Instrument WetlandWayExtract::load by reimplementing timed passes (same logic).
    use osmpbf::{Element, RelMemberType};
    use driver_break_core::routing::wetland::{classify_wetland_value, WetlandClass};

    fn classify_tags(tags: &HashMap<String, String>) -> Option<WetlandClass> {
        if let Some(w) = tags.get("wetland") {
            if let Some(c) = classify_wetland_value(w) {
                return Some(c);
            }
        }
        if tags
            .get("natural")
            .is_some_and(|v| v.eq_ignore_ascii_case("wetland"))
        {
            return Some(WetlandClass::SoftAvoid);
        }
        None
    }

    let mut ways: Vec<(Vec<i64>, WetlandClass)> = Vec::new();
    let mut needed: HashSet<i64> = HashSet::new();
    let mut rel_outers: Vec<(WetlandClass, Vec<i64>)> = Vec::new();
    let mut outer_way_ids: HashSet<i64> = HashSet::new();

    let t_pass1 = Instant::now();
    for_each_pbf_elements(path, |element| match element {
        Element::Way(way) => {
            let tags = tag_map(way.tags());
            let Some(class) = classify_tags(&tags) else {
                return;
            };
            let refs: Vec<i64> = way.refs().collect();
            if refs.len() < 3 {
                return;
            }
            for id in &refs {
                needed.insert(*id);
            }
            ways.push((refs, class));
        }
        Element::Relation(rel) => {
            let tags = tag_map(rel.tags());
            let Some(class) = classify_tags(&tags) else {
                return;
            };
            let mut outers = Vec::new();
            for m in rel.members() {
                let role = m.role().unwrap_or("");
                if m.member_type == RelMemberType::Way && role.eq_ignore_ascii_case("outer") {
                    outers.push(m.member_id);
                    outer_way_ids.insert(m.member_id);
                }
            }
            if !outers.is_empty() {
                rel_outers.push((class, outers));
            }
        }
        _ => {}
    })?;
    let pass1_ms = t_pass1.elapsed().as_secs_f64() * 1000.0;
    let rss1 = rss_mb();

    let mut way_nodes: HashMap<i64, Vec<i64>> = HashMap::new();
    let t_pass2 = Instant::now();
    if !outer_way_ids.is_empty() {
        for_each_pbf_elements(path, |element| {
            let Element::Way(way) = element else {
                return;
            };
            if !outer_way_ids.contains(&way.id()) {
                return;
            }
            let refs: Vec<i64> = way.refs().collect();
            if refs.len() < 3 {
                return;
            }
            for id in &refs {
                needed.insert(*id);
            }
            way_nodes.insert(way.id(), refs);
        })?;
    }
    let pass2_ms = t_pass2.elapsed().as_secs_f64() * 1000.0;
    let rss2 = rss_mb();

    let mut coords: HashMap<i64, (f64, f64)> = HashMap::with_capacity(needed.len());
    let t_pass3 = Instant::now();
    for_each_pbf_elements(path, |element| match element {
        Element::Node(n) => {
            if needed.contains(&n.id()) {
                coords.insert(n.id(), (n.lat(), n.lon()));
            }
        }
        Element::DenseNode(n) => {
            if needed.contains(&n.id()) {
                coords.insert(n.id(), (n.lat(), n.lon()));
            }
        }
        _ => {}
    })?;
    let pass3_ms = t_pass3.elapsed().as_secs_f64() * 1000.0;
    let rss3 = rss_mb();

    // Mirror production tiled convert: tile_grid 1.0 deg over Ostlandet-ish bbox from percentile
    // Use full-world bbox count + timed index_for_bbox over a realistic grid.
    let extract = WetlandWayExtract::load(path)?;
    // Unique rings via full bbox
    let t_idx_full = Instant::now();
    let full = extract.index_for_bbox([-90.0, -180.0, 90.0, 180.0]);
    let full_ms = t_idx_full.elapsed().as_secs_f64() * 1000.0;
    let unique_rings = full.ring_count();

    // Simulate convert tiling: use region from coords extents
    let mut min_lat = f64::INFINITY;
    let mut max_lat = f64::NEG_INFINITY;
    let mut min_lon = f64::INFINITY;
    let mut max_lon = f64::NEG_INFINITY;
    for &(lat, lon) in coords.values() {
        min_lat = min_lat.min(lat);
        max_lat = max_lat.max(lat);
        min_lon = min_lon.min(lon);
        max_lon = max_lon.max(lon);
    }
    let region = [min_lat, min_lon, max_lat, max_lon];
    let cell = 1.0_f64;
    let mut tiles = Vec::new();
    let mut lat0 = region[0];
    let mut row = 0usize;
    while lat0 < region[2] {
        let lat1 = (lat0 + cell).min(region[2]);
        let mut lon0 = region[1];
        let mut col = 0usize;
        while lon0 < region[3] {
            let lon1 = (lon0 + cell).min(region[3]);
            tiles.push((row, col, [lat0, lon0, lat1, lon1]));
            lon0 = lon1;
            col += 1;
        }
        lat0 = lat1;
        row += 1;
    }

    let t_tiles_rewalk = Instant::now();
    let mut rings_sum_rewalk = 0usize;
    let mut nonempty_rewalk = 0usize;
    for (_, _, logical) in &tiles {
        let n = extract.index_for_bbox(*logical).ring_count();
        if n > 0 {
            nonempty_rewalk += 1;
            rings_sum_rewalk += n;
        }
    }
    let tiles_rewalk_ms = t_tiles_rewalk.elapsed().as_secs_f64() * 1000.0;

    let t_tiles_once = Instant::now();
    let once = extract.indexes_for_tiles(&tiles);
    let tiles_once_ms = t_tiles_once.elapsed().as_secs_f64() * 1000.0;
    let rings_sum_once: usize = once.iter().map(|i| i.ring_count()).sum();
    let nonempty_once = once.iter().filter(|i| i.ring_count() > 0).count();
    let counts_match = once
        .iter()
        .zip(tiles.iter())
        .all(|(idx, (_, _, b))| idx.ring_count() == extract.index_for_bbox(*b).ring_count());

    println!("WETLAND_PROFILE");
    println!("pass1_ways_rels_ms={pass1_ms:.1} wetland_ways={} rel_outers={} needed_after_p1={} rss_mb={rss1:.1}", ways.len(), rel_outers.len(), needed.len());
    println!("pass2_outer_ways_ms={pass2_ms:.1} way_nodes={} outer_way_ids={} rss_mb={rss2:.1}", way_nodes.len(), outer_way_ids.len());
    println!("pass3_coords_ms={pass3_ms:.1} coords={} needed={} rss_mb={rss3:.1}", coords.len(), needed.len());
    println!("index_full_bbox_ms={full_ms:.1} unique_rings={unique_rings}");
    println!("index_tiled_rewalk_ms={tiles_rewalk_ms:.1} tiles={} nonempty_tiles={nonempty_rewalk} rings_sum_across_tiles={rings_sum_rewalk} inflation_vs_unique={:.3}", tiles.len(), rings_sum_rewalk as f64 / unique_rings.max(1) as f64);
    println!("index_tiled_once_ms={tiles_once_ms:.1} nonempty_tiles={nonempty_once} rings_sum_across_tiles={rings_sum_once} per_tile_counts_match_rewalk={counts_match}");
    println!("peak_rss_mb={:.1}", rss_mb());
    Ok(())
}

fn tile_grid(region: [f64; 4], max_cell_deg: f64) -> Vec<(usize, usize, [f64; 4])> {
    let mut out = Vec::new();
    let mut lat0 = region[0];
    let mut row = 0usize;
    while lat0 < region[2] - 1e-12 {
        let lat1 = (lat0 + max_cell_deg).min(region[2]);
        let mut lon0 = region[1];
        let mut col = 0usize;
        while lon0 < region[3] - 1e-12 {
            let lon1 = (lon0 + max_cell_deg).min(region[3]);
            out.push((row, col, [lat0, lon0, lat1, lon1]));
            lon0 = lon1;
            col += 1;
        }
        lat0 = lat1;
        row += 1;
    }
    out
}

fn cmd_graph_profile(path: &Path, profile: RoutingProfile, spill: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(spill)?;
    let rss0 = rss_mb();

    let t0 = Instant::now();
    let bbox = driver_break_core::download::pbf_priority::pbf_latlon_percentile_bounds(path, 0.005, 0.995)?;
    let bbox_ms = t0.elapsed().as_secs_f64() * 1000.0;
    println!("GRAPH_BBOX_SCAN ms={bbox_ms:.1} bbox={bbox:?} rss={:.1}", rss_mb());

    let tiles = tile_grid(bbox, 1.0);
    println!("GRAPH_TILES count={}", tiles.len());
    let pad = 0.05_f64;
    let written = Mutex::new(0usize);
    let t_build = Instant::now();
    let (way_touching, timings) = RouteGraph::build_tiled_from_pbf(
        path,
        profile,
        &tiles,
        pad,
        spill,
        |_r, _c, _b, g| {
            *written.lock().unwrap() += 1;
            let _ = (g.nodes.len(), g.edges.len());
            Ok(())
        },
    )?;
    let build_ms = t_build.elapsed().as_secs_f64() * 1000.0;
    println!(
        "GRAPH_TILED_BUILD profile={:?} total_ms={build_ms:.1} tile_assign_ms={:.1} tile_build_ms={:.1} ways={way_touching} tiles_written={} peak_rss_mb={:.1} rss0={rss0:.1}",
        profile,
        timings.tile_assign_ms,
        timings.tile_build_ms,
        *written.lock().unwrap(),
        rss_mb(),
    );
    // residual = pass1+pass2 roughly
    let residual = build_ms - timings.tile_assign_ms - timings.tile_build_ms;
    println!("GRAPH_PASS1_PASS2_approx_ms={residual:.1} (total - assign - tile_build)");
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let mut args = env::args().skip(1);
    let cmd = args.next().expect(
        "cmd: keep-stats | wetland-profile | graph-profile | poi-barrier-profile",
    );
    let path = args.next().expect("pbf path");
    let path = Path::new(&path);
    match cmd.as_str() {
        "keep-stats" => cmd_keep_stats(path)?,
        "wetland-profile" => cmd_wetland_profile(path)?,
        "poi-barrier-profile" => poi_barrier::cmd_poi_barrier_profile(path)?,
        "graph-profile" => {
            let spill = args
                .next()
                .unwrap_or_else(|| "/tmp/navi-graph-probe-spill".into());
            let prof = args.next().unwrap_or_else(|| "car".into());
            let profile = match prof.as_str() {
                "foot" => RoutingProfile::Foot,
                _ => RoutingProfile::Car,
            };
            cmd_graph_profile(path, profile, Path::new(&spill))?;
        }
        other => anyhow::bail!("unknown cmd {other}"),
    }
    Ok(())
}
