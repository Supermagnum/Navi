//! Ostlandet pilgrim soft-preference checks (prefer + gap fallback + FTS name).
//!
//! Ignored by default; needs `core/target/integration-fixtures/ostlandet-latest.osm.pbf`.

use std::collections::HashSet;
use std::path::PathBuf;

use driver_break_core::config::EcoConfig;
use driver_break_core::routing::elevation::{ElevationCache, ElevationService};
use driver_break_core::routing::graph::{
    apply_official_network_preference, load_or_build_reweighted_bbox, load_pilgrim_route_way_ids,
    RoutingProfile,
};
use driver_break_core::search::NameIndex;
use osm4routing::NodeId;

fn fixture_pbf() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/integration-fixtures/ostlandet-latest.osm.pbf")
}

fn fixture_index() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/integration-fixtures/place_index_search_check.db")
}

fn edge_way_id(edge_id: &str) -> Option<i64> {
    edge_id
        .strip_suffix("-rev")
        .unwrap_or(edge_id)
        .split('-')
        .next()
        .and_then(|s| s.parse().ok())
}

#[test]
#[ignore = "needs ostlandet fixture under core/target/integration-fixtures"]
fn ostlandet_pilgrim_pref_and_gap_and_fts() {
    let pbf = fixture_pbf();
    assert!(pbf.is_file(), "missing {}", pbf.display());

    let pilgrim_ways = load_pilgrim_route_way_ids(&pbf).expect("pilgrim ways");
    assert!(
        !pilgrim_ways.is_empty(),
        "expected pilgrim-tagged ways in Ostlandet"
    );
    eprintln!("pilgrim_ways={}", pilgrim_ways.len());

    // Gudbrandsdalsleden (Lillehammer–Dovre) centroid from named-route scan.
    let bbox = [61.05, 10.35, 61.20, 10.60];
    let cache = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/integration-fixtures/graph-cache-pilgrim-gudbrandsdal");
    let elev_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/integration-fixtures/elevation-empty");
    let _ = std::fs::create_dir_all(&elev_dir);
    let elev = ElevationService::new(ElevationCache::new(&elev_dir));
    let eco = EcoConfig::default();
    let (mut graph, _cached) =
        load_or_build_reweighted_bbox(&pbf, &cache, RoutingProfile::Foot, &elev, &eco, bbox)
            .expect("build foot bbox graph");

    // Collect pilgrim edges in the clipped graph.
    let mut pilgrim_edges: Vec<(usize, NodeId, NodeId)> = Vec::new();
    for (i, e) in graph.edges.iter().enumerate() {
        let Some(wid) = edge_way_id(&e.id) else {
            continue;
        };
        if pilgrim_ways.contains(&wid) {
            pilgrim_edges.push((i, e.source, e.target));
        }
    }
    assert!(
        !pilgrim_edges.is_empty(),
        "bbox must contain pilgrim edges near Gudbrandsdalsleden"
    );
    eprintln!("pilgrim_edges_in_bbox={}", pilgrim_edges.len());

    // Prefer case: start/end on pilgrim edges a short walk apart.
    let (start, goal) = {
        let a = pilgrim_edges[0].1;
        let mut best = None;
        for &(_, s, t) in &pilgrim_edges {
            for n in [s, t] {
                if n == a {
                    continue;
                }
                let Some((path, _, cost)) = graph.shortest_path(a, n, false) else {
                    continue;
                };
                if path.len() < 4 || path.len() > 40 {
                    continue;
                }
                let hops = path.len();
                match best {
                    None => best = Some((n, hops, cost)),
                    Some((_, bh, _)) if hops < bh && hops >= 4 => best = Some((n, hops, cost)),
                    _ => {}
                }
            }
        }
        let (g, hops, _) = best.expect("find mid-length pilgrim-connected pair");
        eprintln!("prefer_pair hops={hops}");
        (a, g)
    };

    let (path_plain, _, cost_plain) = graph
        .shortest_path(start, goal, false)
        .expect("plain route");
    let plain_pilgrim = count_pilgrim_hops(&graph, &path_plain, &pilgrim_ways);

    apply_official_network_preference(&mut graph, &pilgrim_ways);
    let (path_pref, _, cost_pref) = graph
        .shortest_path(start, goal, false)
        .expect("preferred route");
    let pref_pilgrim = count_pilgrim_hops(&graph, &path_pref, &pilgrim_ways);
    eprintln!(
        "plain hops={} pilgrim_hops={} cost={cost_plain:.1}; pref hops={} pilgrim_hops={} cost={cost_pref:.1}",
        path_plain.len(),
        plain_pilgrim,
        path_pref.len(),
        pref_pilgrim
    );
    assert!(
        pref_pilgrim >= plain_pilgrim,
        "pilgrim soft pref should not reduce pilgrim-edge share"
    );
    // When a parallel non-pilgrim shortcut exists, pref should improve share or
    // at least keep an end-to-end path (soft preference never fails).
    assert!(!path_pref.is_empty());

    // Gap fallback: route from a pilgrim node to a nearby ordinary node that is
    // not itself on a pilgrim way — must still succeed under the penalty.
    let ordinary_goal = graph
        .edges
        .iter()
        .find_map(|e| {
            let wid = edge_way_id(&e.id)?;
            if pilgrim_ways.contains(&wid) {
                return None;
            }
            // Prefer a node within a few km of start.
            let (lat, lon) = graph.nodes.get(&e.target).map(|n| (n.coord.y, n.coord.x))?;
            let (slat, slon) = graph.nodes.get(&start).map(|n| (n.coord.y, n.coord.x))?;
            let dlat = (lat - slat).abs();
            let dlon = (lon - slon).abs();
            if dlat + dlon > 0.02 || dlat + dlon < 0.002 {
                return None;
            }
            Some(e.target)
        })
        .expect("ordinary nearby goal");
    let gap = graph
        .shortest_path(start, ordinary_goal, false)
        .expect("soft pref must still route across pilgrim tagging gaps");
    eprintln!("gap_fallback hops={}", gap.0.len());
    assert!(gap.0.len() >= 2);

    // Named pilgrim routes must be FTS-searchable (existing index includes names).
    let db = fixture_index();
    assert!(db.is_file(), "missing place index {}", db.display());
    let idx = NameIndex::open(&db).expect("open place index");
    let hits = idx.search("Pilegrimsleden", 12).expect("fts");
    assert!(
        hits.iter()
            .any(|h| h.name.to_lowercase().contains("pilegrim")),
        "expected Pilegrimsleden (or similar) in FTS hits, got {:?}",
        hits.iter().map(|h| &h.name).collect::<Vec<_>>()
    );
    eprintln!(
        "fts_pilegrimsleden top={}",
        hits.first().map(|h| h.name.as_str()).unwrap_or("?")
    );
}

fn count_pilgrim_hops(
    graph: &driver_break_core::routing::graph::RouteGraph,
    path: &[NodeId],
    pilgrim_ways: &HashSet<i64>,
) -> usize {
    let mut n = 0;
    for w in path.windows(2) {
        let Some(idx) = graph.edge_index(w[0], w[1]) else {
            continue;
        };
        let Some(wid) = edge_way_id(&graph.edges[idx].id) else {
            continue;
        };
        if pilgrim_ways.contains(&wid) {
            n += 1;
        }
    }
    n
}
