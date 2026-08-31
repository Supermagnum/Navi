//! Diagnostic: turn-tier + roundabout maneuvers for two real Innlandet corridors.
//! Run: cargo test -p driver-break-core --test turn_icon_roundabout_audit -- --ignored --nocapture

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use driver_break_core::config::EcoConfig;
use driver_break_core::nav::ManeuverKind;
use driver_break_core::routing::elevation::{ElevationCache, ElevationService};
use driver_break_core::routing::graph::{
    load_or_build_reweighted, load_or_build_reweighted_bbox, RoutingProfile,
};
use driver_break_core::routing::guidance_path::{
    build_maneuvers, parse_maneuver_kind, probe_roundabout_spans,
};
use osm4routing::NodeId;
use osmpbf::{Element, ElementReader};

fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6_378_100.0;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

fn nearest(graph: &driver_break_core::RouteGraph, lat: f64, lon: f64) -> NodeId {
    graph
        .nodes
        .values()
        .min_by(|a, b| {
            let da = haversine_m(lat, lon, a.coord.y, a.coord.x);
            let db = haversine_m(lat, lon, b.coord.y, b.coord.x);
            da.partial_cmp(&db).unwrap()
        })
        .map(|n| n.id)
        .expect("empty graph")
}

fn bearing_deg(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let p1 = lat1.to_radians();
    let p2 = lat2.to_radians();
    let dl = (lon2 - lon1).to_radians();
    let y = dl.sin() * p2.cos();
    let x = p1.cos() * p2.sin() - p1.sin() * p2.cos() * dl.cos();
    let mut brng = y.atan2(x).to_degrees();
    if brng < 0.0 {
        brng += 360.0;
    }
    brng
}

fn turn_delta_deg(in_brng: f64, out_brng: f64) -> f64 {
    let mut d = out_brng - in_brng;
    while d > 180.0 {
        d -= 360.0;
    }
    while d < -180.0 {
        d += 360.0;
    }
    d
}

/// Mirror of Android `RouteManeuver.iconKey()` fallback (when no explicit `icon`).
fn android_icon_key(kind: &str, exit: Option<u8>, explicit: Option<&str>) -> String {
    if let Some(i) = explicit.filter(|s| !s.is_empty()) {
        return i.to_string();
    }
    match kind {
        "slight_left" => "nav_left_1".into(),
        "left" => "nav_left_2".into(),
        "sharp_left" => "nav_left_3".into(),
        "slight_right" => "nav_right_1".into(),
        "right" => "nav_right_2".into(),
        "sharp_right" => "nav_right_3".into(),
        "u_turn" => "nav_turnaround_left".into(),
        "destination" => "nav_destination".into(),
        "roundabout" => match exit {
            Some(1) => "nav_roundabout_r1".into(),
            Some(2) => "nav_roundabout_r2".into(),
            Some(3) => "nav_roundabout_r3".into(),
            Some(4) => "nav_roundabout_r4".into(),
            Some(5) => "nav_roundabout_r5".into(),
            Some(6) => "nav_roundabout_r6".into(),
            Some(7) => "nav_roundabout_r7".into(),
            Some(8) => "nav_roundabout_r8".into(),
            _ => "nav_roundabout_r1".into(),
        },
        "keep_left" => "nav_keep_left".into(),
        "keep_right" => "nav_keep_right".into(),
        "exit_left" => "nav_exit_left".into(),
        "exit_right" => "nav_exit_right".into(),
        "merge_left" => "nav_merge_left".into(),
        "merge_right" => "nav_merge_right".into(),
        _ => "nav_straight".into(),
    }
}

/// What the Navit `_1`/`_2`/`_3` convention *should* map to for geometric turns.
fn expected_tier_icon(kind: &str) -> Option<&'static str> {
    match kind {
        "slight_left" => Some("nav_left_1"),
        "left" => Some("nav_left_2"),
        "sharp_left" => Some("nav_left_3"),
        "slight_right" => Some("nav_right_1"),
        "right" => Some("nav_right_2"),
        "sharp_right" => Some("nav_right_3"),
        _ => None,
    }
}

fn plan_leg(
    graph: &driver_break_core::RouteGraph,
    a: (f64, f64),
    b: (f64, f64),
) -> (Vec<NodeId>, f64) {
    let s = nearest(graph, a.0, a.1);
    let g = nearest(graph, b.0, b.1);
    let (path, _, _cost) = graph
        .shortest_path(s, g, false)
        .unwrap_or_else(|| panic!("no path {:.5},{:.5} -> {:.5},{:.5}", a.0, a.1, b.0, b.1));
    let mut dist = 0.0;
    for w in path.windows(2) {
        if let Some(idx) = graph.edge_index(w[0], w[1]) {
            dist += graph.edges[idx].length_m;
        }
    }
    (path, dist)
}

fn audit_path(label: &str, graph: &driver_break_core::RouteGraph, path: &[NodeId], offset_m: f64) {
    eprintln!(
        "=== {label} path_nodes={} offset_m={offset_m:.0} ===",
        path.len()
    );
    let mans = build_maneuvers(graph, path);
    eprintln!("maneuver_count={} (incl destination)", mans.len());
    for (i, m) in mans.iter().enumerate() {
        let icon = android_icon_key(&m.kind, m.roundabout_exit, m.icon.as_deref());
        let expect = expected_tier_icon(&m.kind);
        let mismatch = expect.map(|e| e != icon.as_str()).unwrap_or(false);
        eprintln!(
            "  [{i}] kind={:<14} cum_m={:8.1} lat={:.6} lon={:.6} street={:?} exit={:?} icon={icon}{}{}",
            m.kind,
            m.cum_m + offset_m,
            m.lat,
            m.lon,
            m.street,
            m.roundabout_exit,
            if mismatch {
                format!(" EXPECTED={}", expect.unwrap())
            } else {
                String::new()
            },
            if m.kind == "roundabout" {
                " ROUNDABOUT_EMITTED"
            } else {
                ""
            },
        );
    }

    // Recompute geometric deltas at each path vertex for angle audit.
    let mut cum = 0.0;
    for i in 0..path.len().saturating_sub(2) {
        let n0 = path[i];
        let n1 = path[i + 1];
        let n2 = path[i + 2];
        let Some(e_in) = graph.edge_index(n0, n1).map(|idx| &graph.edges[idx]) else {
            continue;
        };
        let Some(e_out) = graph.edge_index(n1, n2).map(|idx| &graph.edges[idx]) else {
            cum += e_in.length_m;
            continue;
        };
        cum += e_in.length_m;
        let in_b = bearing_deg(e_in.start_lat, e_in.start_lon, e_in.end_lat, e_in.end_lon);
        let out_b = bearing_deg(
            e_out.start_lat,
            e_out.start_lon,
            e_out.end_lat,
            e_out.end_lon,
        );
        let delta = turn_delta_deg(in_b, out_b);
        let a = delta.abs();
        if a < 25.0 {
            continue;
        }
        let classified = parse_maneuver_kind(match () {
            _ if a < 45.0 && delta < 0.0 => "slight_left",
            _ if a < 45.0 => "slight_right",
            _ if a < 135.0 && delta < 0.0 => "left",
            _ if a < 135.0 => "right",
            _ if a < 160.0 && delta < 0.0 => "sharp_left",
            _ if a < 160.0 => "sharp_right",
            _ => "u_turn",
        });
        let kind_str = match classified {
            ManeuverKind::SlightLeft => "slight_left",
            ManeuverKind::SlightRight => "slight_right",
            ManeuverKind::Left => "left",
            ManeuverKind::Right => "right",
            ManeuverKind::SharpLeft => "sharp_left",
            ManeuverKind::SharpRight => "sharp_right",
            ManeuverKind::UTurn => "u_turn",
            _ => "other",
        };
        let node = &graph.nodes[&n1];
        eprintln!(
            "  angle@cum={:.0} lat={:.6} lon={:.6} delta={delta:+.1}° class={kind_str} hwy_in={:?} hwy_out={:?} name_out={:?}",
            cum + offset_m,
            node.coord.y,
            node.coord.x,
            e_in.highway,
            e_out.highway,
            e_out.name,
        );
    }
}

type RoundaboutHit = (i64, String, Vec<(f64, f64)>);

fn roundabouts_near_bbox(pbf: &Path, bbox: [f64; 4]) -> Vec<RoundaboutHit> {
    let mut out = Vec::new();
    // Pass 1: collect node coords in expanded bbox (sparse + dense PBF nodes).
    let mut nodes: std::collections::HashMap<i64, (f64, f64)> = std::collections::HashMap::new();
    {
        let file = std::fs::File::open(pbf).expect("pbf");
        let reader = ElementReader::new(file);
        reader
            .for_each(|el| match el {
                Element::Node(n) => {
                    let lat = n.lat();
                    let lon = n.lon();
                    if lat >= bbox[0] - 0.01
                        && lat <= bbox[2] + 0.01
                        && lon >= bbox[1] - 0.01
                        && lon <= bbox[3] + 0.01
                    {
                        nodes.insert(n.id(), (lat, lon));
                    }
                }
                Element::DenseNode(n) => {
                    let lat = n.lat();
                    let lon = n.lon();
                    if lat >= bbox[0] - 0.01
                        && lat <= bbox[2] + 0.01
                        && lon >= bbox[1] - 0.01
                        && lon <= bbox[3] + 0.01
                    {
                        nodes.insert(n.id(), (lat, lon));
                    }
                }
                _ => {}
            })
            .ok();
    }
    let file = std::fs::File::open(pbf).expect("pbf");
    let reader = ElementReader::new(file);
    reader
        .for_each(|el| {
            if let Element::Way(w) = el {
                let is_ra = w.tags().any(|(k, v)| k == "junction" && v == "roundabout");
                if !is_ra {
                    return;
                }
                let refs: Vec<i64> = w.refs().collect();
                let coords: Vec<(f64, f64)> = refs
                    .iter()
                    .filter_map(|id| nodes.get(id).copied())
                    .collect();
                if coords.is_empty() {
                    return;
                }
                let lat_avg = coords.iter().map(|c| c.0).sum::<f64>() / coords.len() as f64;
                let lon_avg = coords.iter().map(|c| c.1).sum::<f64>() / coords.len() as f64;
                if lat_avg < bbox[0] || lat_avg > bbox[2] || lon_avg < bbox[1] || lon_avg > bbox[3]
                {
                    return;
                }
                let name = w
                    .tags()
                    .find(|(k, _)| *k == "name")
                    .map(|(_, v)| v.to_string())
                    .unwrap_or_default();
                out.push((w.id(), name, coords));
            }
        })
        .ok();
    out
}

fn path_near_roundabout(
    graph: &driver_break_core::RouteGraph,
    path: &[NodeId],
    ra_coords: &[(f64, f64)],
    threshold_m: f64,
) -> bool {
    for id in path {
        let n = &graph.nodes[id];
        for &(lat, lon) in ra_coords {
            if haversine_m(n.coord.y, n.coord.x, lat, lon) <= threshold_m {
                return true;
            }
        }
    }
    false
}

#[test]
#[ignore = "needs ostlandet PBF under integration-fixtures"]
fn audit_route1_and_route2_turn_icons() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/integration-fixtures");
    let pbf = root.join("ostlandet-latest.osm.pbf");
    assert!(pbf.is_file(), "missing {}", pbf.display());
    let elev = root.join("elevation");
    let cache = root.join("graph-cache-turn-audit");
    let _ = std::fs::create_dir_all(&elev);
    let _ = std::fs::create_dir_all(&cache);

    let eco = EcoConfig::default();
    let elevation = ElevationService::new(ElevationCache::new(&elev));
    let (graph, hit) =
        load_or_build_reweighted(&pbf, &cache, RoutingProfile::Car, &elevation, &eco)
            .expect("graph");
    eprintln!(
        "cache_hit={hit} nodes={} edges={}",
        graph.nodes.len(),
        graph.edges.len()
    );

    // ---- Route 1 (Raufoss area) ----
    let r1_start: (f64, f64) = (60.720_250_0, 10.613_109_0);
    let r1_via: (f64, f64) = (60.726_010_3, 10.613_349_8);
    let r1_end: (f64, f64) = (60.725_109_0, 10.620_231_0);
    let bbox1 = [
        r1_start.0.min(r1_via.0).min(r1_end.0) - 0.005,
        r1_start.1.min(r1_via.1).min(r1_end.1) - 0.005,
        r1_start.0.max(r1_via.0).max(r1_end.0) + 0.005,
        r1_start.1.max(r1_via.1).max(r1_end.1) + 0.005,
    ];
    eprintln!("\n######## ROUTE 1 bbox={bbox1:?} ########");
    let ras1 = roundabouts_near_bbox(&pbf, bbox1);
    eprintln!("OSM roundabouts in bbox: {}", ras1.len());
    assert!(
        !ras1.is_empty(),
        "dense-node scanner must find roundabout(s) in Route 1 bbox"
    );
    for (id, name, coords) in &ras1 {
        let lat = coords.iter().map(|c| c.0).sum::<f64>() / coords.len() as f64;
        let lon = coords.iter().map(|c| c.1).sum::<f64>() / coords.len() as f64;
        eprintln!(
            "  way {id} name='{name}' center={lat:.6},{lon:.6} nodes={}",
            coords.len()
        );
    }

    let (leg1a, d1a) = plan_leg(&graph, r1_start, r1_via);
    let (leg1b, d1b) = plan_leg(&graph, r1_via, r1_end);
    eprintln!(
        "leg1a={:.0}m nodes={} leg1b={:.0}m nodes={}",
        d1a,
        leg1a.len(),
        d1b,
        leg1b.len()
    );

    let mut path_touches_ra = false;
    for (id, name, coords) in &ras1 {
        let near_a = path_near_roundabout(&graph, &leg1a, coords, 40.0);
        let near_b = path_near_roundabout(&graph, &leg1b, coords, 40.0);
        if near_a || near_b {
            path_touches_ra = true;
            eprintln!("PATH TOUCHES roundabout way {id} '{name}'");
        }
    }
    eprintln!("route1_path_touches_any_roundabout={path_touches_ra}");

    audit_path("R1 start→via", &graph, &leg1a, 0.0);
    audit_path("R1 via→end", &graph, &leg1b, d1a);

    let all_kinds: HashSet<String> = build_maneuvers(&graph, &leg1a)
        .into_iter()
        .chain(build_maneuvers(&graph, &leg1b))
        .map(|m| m.kind)
        .collect();
    eprintln!("R1 kinds present: {all_kinds:?}");
    eprintln!(
        "R1 has roundabout kind emitted: {}",
        all_kinds.contains("roundabout")
    );

    // ---- Route 2 ----
    let r2_start: (f64, f64) = (60.657_095_0, 11.206_865_0);
    let r2_via: (f64, f64) = (60.648_532_6, 11.187_735_9);
    // Vardebergvegen 225 — resolve from place_index (60.6430505, 11.1905096).
    let r2_end = resolve_vardebergvegen_225(&root).unwrap_or((60.643_050_5, 11.190_509_6));
    eprintln!(
        "\n######## ROUTE 2 end≈{:.6},{:.6} ########",
        r2_end.0, r2_end.1
    );
    let bbox2 = [
        r2_start.0.min(r2_via.0).min(r2_end.0) - 0.008,
        r2_start.1.min(r2_via.1).min(r2_end.1) - 0.008,
        r2_start.0.max(r2_via.0).max(r2_end.0) + 0.008,
        r2_start.1.max(r2_via.1).max(r2_end.1) + 0.008,
    ];
    let ras2 = roundabouts_near_bbox(&pbf, bbox2);
    eprintln!("OSM roundabouts in bbox: {}", ras2.len());
    for (id, name, coords) in &ras2 {
        let lat = coords.iter().map(|c| c.0).sum::<f64>() / coords.len() as f64;
        let lon = coords.iter().map(|c| c.1).sum::<f64>() / coords.len() as f64;
        eprintln!(
            "  way {id} name='{name}' center={lat:.6},{lon:.6} nodes={}",
            coords.len()
        );
    }

    let (leg2a, d2a) = plan_leg(&graph, r2_start, r2_via);
    let (leg2b, d2b) = plan_leg(&graph, r2_via, r2_end);
    eprintln!(
        "leg2a={:.0}m nodes={} leg2b={:.0}m nodes={}",
        d2a,
        leg2a.len(),
        d2b,
        leg2b.len()
    );

    let mut path_touches_ra2 = false;
    for (id, name, coords) in &ras2 {
        let near_a = path_near_roundabout(&graph, &leg2a, coords, 40.0);
        let near_b = path_near_roundabout(&graph, &leg2b, coords, 40.0);
        if near_a || near_b {
            path_touches_ra2 = true;
            eprintln!("PATH TOUCHES roundabout way {id} '{name}'");
        }
    }
    eprintln!("route2_path_touches_any_roundabout={path_touches_ra2}");

    audit_path("R2 start→via", &graph, &leg2a, 0.0);
    audit_path("R2 via→end", &graph, &leg2b, d2a);

    let all_kinds2: HashSet<String> = build_maneuvers(&graph, &leg2a)
        .into_iter()
        .chain(build_maneuvers(&graph, &leg2b))
        .map(|m| m.kind)
        .collect();
    eprintln!("R2 kinds present: {all_kinds2:?}");
    eprintln!(
        "R2 has roundabout kind emitted: {}",
        all_kinds2.contains("roundabout")
    );

    // Second real roundabout in the same bbox (way 266162214) — different geometry.
    let ra2_center = (60.729_350_0, 10.616_226_0);
    let ra2_start = (60.728_200_0, 10.616_200_0);
    let ra2_end = (60.730_500_0, 10.617_500_0);
    let (leg_ra2, d_ra2) = plan_leg(&graph, ra2_start, ra2_end);
    eprintln!(
        "\n######## SECOND RA near {:.6},{:.6} path={:.0}m nodes={} ########",
        ra2_center.0,
        ra2_center.1,
        d_ra2,
        leg_ra2.len()
    );
    let near_second = ras1.iter().any(|(id, _, coords)| {
        *id == 266162214 && path_near_roundabout(&graph, &leg_ra2, coords, 40.0)
    });
    eprintln!("second_path_touches_266162214={near_second}");
    audit_path("second RA probe", &graph, &leg_ra2, 0.0);
    let second_kinds: HashSet<String> = build_maneuvers(&graph, &leg_ra2)
        .into_iter()
        .map(|m| m.kind)
        .collect();
    if near_second {
        assert!(
            second_kinds.contains("roundabout"),
            "path through 266162214 must emit roundabout; kinds={second_kinds:?}"
        );
        let exit = build_maneuvers(&graph, &leg_ra2)
            .into_iter()
            .find(|m| m.kind == "roundabout")
            .and_then(|m| m.roundabout_exit);
        eprintln!("second RA exit={exit:?} (Route1 was exit 2 — compare for generalization)");
        assert!(exit.is_some_and(|e| (1..=8).contains(&e)));
    } else {
        eprintln!("WARN: probe path missed 266162214 — skip second-RA assert");
    }

    // ---- Primary multi-roundabout route (Navit port verification) ----
    let p_start: (f64, f64) = (60.799_849_9, 10.694_432_8);
    let p_via: (f64, f64) = (60.783_961_5, 10.694_175_9);
    let p_end: (f64, f64) = (60.772_902_7, 10.713_608_9);
    let bbox_p = [
        p_start.0.min(p_via.0).min(p_end.0) - 0.008,
        p_start.1.min(p_via.1).min(p_end.1) - 0.008,
        p_start.0.max(p_via.0).max(p_end.0) + 0.008,
        p_start.1.max(p_via.1).max(p_end.1) + 0.008,
    ];
    eprintln!("\n######## PRIMARY multi-RA bbox={bbox_p:?} ########");
    let ras_p = roundabouts_near_bbox(&pbf, bbox_p);
    eprintln!("OSM roundabouts in primary bbox: {}", ras_p.len());
    for (id, name, coords) in &ras_p {
        let lat = coords.iter().map(|c| c.0).sum::<f64>() / coords.len() as f64;
        let lon = coords.iter().map(|c| c.1).sum::<f64>() / coords.len() as f64;
        eprintln!(
            "  way {id} name='{name}' center={lat:.6},{lon:.6} nodes={}",
            coords.len()
        );
    }
    let (leg_pa, d_pa) = plan_leg(&graph, p_start, p_via);
    let (leg_pb, d_pb) = plan_leg(&graph, p_via, p_end);
    eprintln!(
        "primary legA={:.0}m nodes={} legB={:.0}m nodes={}",
        d_pa,
        leg_pa.len(),
        d_pb,
        leg_pb.len()
    );
    let mut primary_touches = 0usize;
    for (id, name, coords) in &ras_p {
        let near = path_near_roundabout(&graph, &leg_pa, coords, 40.0)
            || path_near_roundabout(&graph, &leg_pb, coords, 40.0);
        if near {
            primary_touches += 1;
            eprintln!("PRIMARY PATH TOUCHES roundabout way {id} '{name}'");
        }
    }
    eprintln!("primary_path_touches_roundabout_count={primary_touches}");
    audit_path("PRIMARY start→via", &graph, &leg_pa, 0.0);
    audit_path("PRIMARY via→end", &graph, &leg_pb, d_pa);
    let primary_mans: Vec<_> = build_maneuvers(&graph, &leg_pa)
        .into_iter()
        .chain(build_maneuvers(&graph, &leg_pb))
        .collect();
    let primary_ra: Vec<_> = primary_mans
        .iter()
        .filter(|m| m.kind == "roundabout")
        .collect();
    eprintln!(
        "PRIMARY roundabouts emitted={} (OSM touches={primary_touches})",
        primary_ra.len()
    );
    // Probe bearings/sectors on each primary leg (host vs device boundary audits).
    let probes: Vec<_> = probe_roundabout_spans(&graph, &leg_pa)
        .into_iter()
        .chain(probe_roundabout_spans(&graph, &leg_pb))
        .collect();
    assert_eq!(
        probes.len(),
        primary_ra.len(),
        "probe span count vs emitted roundabouts"
    );
    for (i, (m, (span, probe))) in primary_ra.iter().zip(probes.iter()).enumerate() {
        let icon = android_icon_key(&m.kind, m.roundabout_exit, m.icon.as_deref());
        eprintln!(
            "  PRIMARY RA[{i}] exit={:?} icon={icon} street={:?} lat={:.6} lon={:.6} entry_brng={:.4} leave_brng={:.4} delta2={:.4} roundabout_delta={:.4} delta_i={} sector={} leave_delta={:.4} dtsir={:.4} probe_icon={}",
            m.roundabout_exit,
            m.street,
            m.lat,
            m.lon,
            probe.entry_in_brng,
            probe.leave_out_brng,
            probe.delta2,
            probe.roundabout_delta,
            probe.roundabout_delta.round() as i32,
            probe.sector,
            probe.leave_delta,
            probe.dtsir,
            probe.icon,
        );
        assert_eq!(icon, probe.icon, "emitted icon vs probe icon for RA[{i}]");
        assert_eq!(span.icon_key, probe.icon);
        assert!(
            m.roundabout_exit.is_some_and(|e| (1..=8).contains(&e)),
            "primary RA exit out of range"
        );
        assert!(
            icon.starts_with("nav_roundabout_"),
            "primary RA icon must be Navit clock-face stem, got {icon}"
        );
        let stem = icon.strip_prefix("nav_roundabout_").unwrap_or("");
        assert!(
            (stem.starts_with('r') || stem.starts_with('l'))
                && stem.len() == 2
                && stem.chars().nth(1).unwrap().is_ascii_digit(),
            "unexpected roundabout icon stem {icon}"
        );
        // Primary RA[4] (Minnesundvegen, ~60.785785/10.693751): lock the r5/r6
        // sector-boundary case that previously disagreed between host traces.
        if (m.lat - 60.785785).abs() < 1e-4 && (m.lon - 10.693751).abs() < 1e-4 {
            eprintln!(
                "  PRIMARY RA[4] BOUNDARY CASE roundabout_delta={:.6} delta_i={} sector={}",
                probe.roundabout_delta,
                probe.roundabout_delta.round() as i32,
                probe.sector
            );
            // Full-country graph often leaves via a different ring node than the
            // product bbox planner (see primary_ra4_bbox_matches_plan_car_route).
            // Record the delta; do not treat full-graph r6 as the product lock.
            eprintln!(
                "  FULL-GRAPH RA[4] note: icon={icon} delta={:.6} (product bbox path locks r5)",
                probe.roundabout_delta
            );
        }
    }
    assert!(
        primary_touches >= 2,
        "primary route expected several OSM roundabouts on path; touches={primary_touches}"
    );
    assert!(
        primary_ra.len() >= 2,
        "primary route must emit several roundabout maneuvers; got {}",
        primary_ra.len()
    );

    // Route 1 must emit a roundabout (way 839797309 on corridor); Route 2 must not.
    assert!(
        path_touches_ra,
        "Route 1 path should touch OSM roundabout 839797309"
    );
    assert!(
        all_kinds.contains("roundabout"),
        "Route 1 must emit kind=roundabout after generation fix; kinds={all_kinds:?}"
    );
    assert!(
        !all_kinds2.contains("roundabout"),
        "Route 2 has no OSM roundabout — unexpected kind; kinds={all_kinds2:?}"
    );
    let r1_ra: Vec<_> = build_maneuvers(&graph, &leg1a)
        .into_iter()
        .chain(build_maneuvers(&graph, &leg1b))
        .filter(|m| m.kind == "roundabout")
        .collect();
    assert!(!r1_ra.is_empty());
    for m in &r1_ra {
        assert!(
            m.roundabout_exit.is_some_and(|e| (1..=8).contains(&e)),
            "roundabout_exit out of icon range: {:?}",
            m.roundabout_exit
        );
        let icon = android_icon_key("roundabout", m.roundabout_exit, m.icon.as_deref());
        eprintln!(
            "R1 roundabout exit={:?} icon={icon} street={:?} explicit={:?}",
            m.roundabout_exit, m.street, m.icon
        );
    }
    let left_or_right = all_kinds
        .iter()
        .chain(all_kinds2.iter())
        .any(|k| k == "left" || k == "right");
    if left_or_right {
        for kind in ["left", "right"] {
            let icon = android_icon_key(kind, None, None);
            let expect = expected_tier_icon(kind).expect("tier");
            assert_eq!(
                icon, expect,
                "android_icon_key mirror out of sync for {kind}"
            );
        }
        eprintln!("OK: normal left/right map to nav_*_2 via android_icon_key mirror");
    }
}

fn resolve_vardebergvegen_225(root: &Path) -> Option<(f64, f64)> {
    let candidates = [
        root.join("place_index.db"),
        root.join("place_index_search_check.db"),
        PathBuf::from("/tmp/place_index_search_check.db"),
        PathBuf::from("/data/local/tmp/navi_fixtures/place_index_search_check.db"),
    ];
    for db in candidates {
        if !db.is_file() {
            continue;
        }
        let ok =
            rusqlite::Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                .ok()
                .and_then(|conn| {
                    // Prefer exact housenumber match; FTS may fold spelling.
                    let q = "SELECT name, lat, lon FROM name_entries \
                     WHERE name LIKE 'Vardebergvegen 225%' \
                        OR name LIKE '%Vardebergvegen%225%' \
                     ORDER BY length(name) ASC LIMIT 5";
                    let mut stmt = conn.prepare(q).ok()?;
                    let rows: Vec<(String, f64, f64)> = stmt
                        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
                        .ok()?
                        .filter_map(|r| r.ok())
                        .collect();
                    for (name, lat, lon) in &rows {
                        eprintln!("place_hit '{name}' {lat:.6},{lon:.6}");
                    }
                    rows.first().map(|(_, lat, lon)| (*lat, *lon))
                });
        if let Some(p) = ok {
            eprintln!(
                "resolved Vardebergvegen 225 via {} -> {:.6},{:.6}",
                db.display(),
                p.0,
                p.1
            );
            return Some(p);
        }
    }
    eprintln!("WARN: could not resolve Vardebergvegen 225 from place_index; using fallback coord");
    None
}

#[test]
#[ignore = "needs ostlandet PBF under integration-fixtures"]
fn probe_merge_exit_sharp_on_nearby_corridors() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/integration-fixtures");
    let pbf = root.join("ostlandet-latest.osm.pbf");
    assert!(pbf.is_file(), "missing {}", pbf.display());
    let elev = root.join("elevation");
    let cache = root.join("graph-cache-turn-audit");
    let eco = EcoConfig::default();
    let elevation = ElevationService::new(ElevationCache::new(&elev));
    let (graph, _) = load_or_build_reweighted(&pbf, &cache, RoutingProfile::Car, &elevation, &eco)
        .expect("graph");

    type Corridor = (&'static str, (f64, f64), (f64, f64));
    let corridors: &[Corridor] = &[
        (
            "E6_Moelv_southboundish",
            (60.9200, 10.7000),
            (60.8500, 10.7000),
        ),
        ("Rv4_Gjovik_trunk", (60.8000, 10.6900), (60.7700, 10.6900)),
    ];
    let mut saw_merge_or_exit = false;
    let mut saw_sharp_or_uturn = false;
    let mut saw_keep = false;
    for (label, a, b) in corridors {
        let (path, dist) = plan_leg(&graph, *a, *b);
        let mans = build_maneuvers(&graph, &path);
        eprintln!("\n### probe {label} dist={dist:.0}m mans={}", mans.len());
        for (i, m) in mans.iter().enumerate() {
            if m.kind == "destination" {
                continue;
            }
            let icon = android_icon_key(&m.kind, m.roundabout_exit, m.icon.as_deref());
            eprintln!(
                "  [{i}] kind={:<14} icon={icon} street={:?} exit={:?}",
                m.kind, m.street, m.roundabout_exit
            );
            if m.kind.contains("merge") || m.kind.contains("exit_") {
                saw_merge_or_exit = true;
            }
            if m.kind.contains("sharp") || m.kind == "u_turn" {
                saw_sharp_or_uturn = true;
            }
            if m.kind.contains("keep") {
                saw_keep = true;
            }
        }
    }
    eprintln!(
        "SUMMARY saw_merge_or_exit={saw_merge_or_exit} saw_sharp_or_uturn={saw_sharp_or_uturn} saw_keep={saw_keep}"
    );
    assert!(saw_merge_or_exit, "expected merge or exit on E6 corridor");
    assert!(saw_sharp_or_uturn, "expected sharp turn on Gjøvik corridor");
    assert!(saw_keep, "expected keep left/right on probed corridors");
}

#[test]
#[ignore = "needs ostlandet PBF under integration-fixtures"]
fn primary_ra4_bbox_matches_plan_car_route() {
    // Mirror navi-ffi plan_car_route bbox pad + per-leg graphs so host icons
    // match the on-device UniFFI planner (not a full-country graph).
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/integration-fixtures");
    let pbf = root.join("ostlandet-latest.osm.pbf");
    assert!(pbf.is_file(), "missing {}", pbf.display());
    let elev = root.join("elevation");
    let cache = root.join("graph-cache-primary-bbox-ra4");
    let _ = std::fs::create_dir_all(&elev);
    let _ = std::fs::remove_dir_all(&cache);
    let _ = std::fs::create_dir_all(&cache);

    let eco = EcoConfig::default();
    let elevation = ElevationService::new(ElevationCache::new(&elev));

    let start = (60.799_849_9, 10.694_432_8);
    let via = (60.783_961_5, 10.694_175_9);
    let end = (60.772_902_7, 10.713_608_9);

    fn trip_bbox(a: (f64, f64), b: (f64, f64)) -> [f64; 4] {
        let lat_span = (a.0 - b.0).abs();
        let lon_span = (a.1 - b.1).abs();
        let pad = (lat_span.max(lon_span) * 0.35).clamp(0.35, 2.5);
        [
            a.0.min(b.0) - pad,
            a.1.min(b.1) - pad,
            a.0.max(b.0) + pad,
            a.1.max(b.1) + pad,
        ]
    }

    let mut all_ra = Vec::new();
    for (label, a, b) in [("start→via", start, via), ("via→end", via, end)] {
        let bbox = trip_bbox(a, b);
        let leg_cache = cache.join(label.replace('→', "_"));
        let (graph, hit) = load_or_build_reweighted_bbox(
            &pbf,
            &leg_cache,
            &leg_cache,
            RoutingProfile::Car,
            &elevation,
            &eco,
            bbox,
        )
        .expect("bbox graph");
        eprintln!(
            "{label} cache_hit={hit} nodes={} edges={}",
            graph.nodes.len(),
            graph.edges.len()
        );
        let (path, _d) = plan_leg(&graph, a, b);
        for (span, probe) in probe_roundabout_spans(&graph, &path) {
            let node = &graph.nodes[&path[span.leave_idx]];
            eprintln!(
                "  {label} RA exit={} icon={} lat={:.6} lon={:.6} delta={:.4} delta_i={} sector={}",
                span.exit_number,
                probe.icon,
                node.coord.y,
                node.coord.x,
                probe.roundabout_delta,
                probe.roundabout_delta.round() as i32,
                probe.sector,
            );
            all_ra.push((
                node.coord.y,
                node.coord.x,
                probe.icon,
                probe.roundabout_delta,
            ));
        }
    }

    assert!(all_ra.len() >= 6, "expected >=6 RAs, got {}", all_ra.len());
    // Leave-node coords differ slightly between full-graph and bbox paths; match by area.
    let ra4 = all_ra
        .iter()
        .find(|(lat, lon, _, _)| (*lat - 60.7856).abs() < 5e-4 && (*lon - 10.6937).abs() < 5e-4)
        .expect("RA[4] Minnesundvegen missing from bbox paths");
    eprintln!(
        "BBOX RA4 lock icon={} delta={:.6} lat={:.6} lon={:.6}",
        ra4.2, ra4.3, ra4.0, ra4.1
    );
    assert_eq!(ra4.2, "nav_roundabout_r5");
    assert!(
        (ra4.3 + 48.19).abs() < 1.0,
        "bbox RA4 roundabout_delta drifted ({:.6})",
        ra4.3
    );
}
