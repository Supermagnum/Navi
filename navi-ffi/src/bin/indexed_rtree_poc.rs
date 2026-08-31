//! Phase 1b PoC: preprocess a region `.osm.pbf` into SQLite + R*Tree, then time
//! first-plan load vs cold `build_from_pbf_bbox`.
//!
//! Usage:
//!   indexed-rtree-poc build --pbf REGION.osm.pbf --db out.sqlite
//!   indexed-rtree-poc bench --pbf REGION.osm.pbf --db out.sqlite \
//!       --start-lat .. --start-lon .. --end-lat .. --end-lon ..
//!
//! `build` is offline preprocessing. `bench` compares cold PBF graph build to
//! loading the prebuilt DB for the same trip bbox (no `.navigph` involved).

use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::time::Instant;

use driver_break_core::routing::graph::{
    max_waypoint_snap_m, GraphEdge, RouteGraph, RoutingProfile,
};
use geo_types::Coord;
use osm4routing::{Node, NodeId};
use rusqlite::{params, Connection};

fn usage() -> ! {
    eprintln!(
        "usage:\n  indexed-rtree-poc build --pbf PATH --db PATH [--bbox minLat,minLon,maxLat,maxLon]\n  indexed-rtree-poc bench --pbf PATH --db PATH --start-lat F --start-lon F --end-lat F --end-lon F"
    );
    std::process::exit(2);
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

fn parse_bbox(s: &str) -> [f64; 4] {
    let p: Vec<f64> = s
        .split(',')
        .map(|x| x.trim().parse().expect("bbox float"))
        .collect();
    assert_eq!(p.len(), 4, "bbox needs 4 floats");
    [p[0], p[1], p[2], p[3]]
}

fn trip_bbox(slat: f64, slon: f64, elat: f64, elon: f64) -> [f64; 4] {
    let lat_span = (slat - elat).abs();
    let lon_span = (slon - elon).abs();
    let pad = (lat_span.max(lon_span) * 0.35).clamp(0.35, 2.5);
    [
        slat.min(elat) - pad,
        slon.min(elon) - pad,
        slat.max(elat) + pad,
        slon.max(elon) + pad,
    ]
}

fn open_db(path: &Path) -> Connection {
    let conn = Connection::open(path).expect("open sqlite");
    conn.execute_batch(
        "
        PRAGMA journal_mode=OFF;
        PRAGMA synchronous=OFF;
        PRAGMA temp_store=MEMORY;
        ",
    )
    .ok();
    conn
}

fn init_schema(conn: &Connection) {
    conn.execute_batch(
        "
        DROP TABLE IF EXISTS edge_rtree;
        DROP TABLE IF EXISTS edges;
        DROP TABLE IF EXISTS nodes;
        DROP TABLE IF EXISTS meta;
        CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
        CREATE TABLE nodes(
          id INTEGER PRIMARY KEY,
          lat REAL NOT NULL,
          lon REAL NOT NULL
        );
        CREATE TABLE edges(
          id INTEGER PRIMARY KEY,
          edge_key TEXT NOT NULL,
          source INTEGER NOT NULL,
          target INTEGER NOT NULL,
          length_m REAL NOT NULL,
          base_weight REAL NOT NULL,
          start_lat REAL NOT NULL,
          start_lon REAL NOT NULL,
          end_lat REAL NOT NULL,
          end_lon REAL NOT NULL,
          highway TEXT,
          maxspeed_kmh REAL,
          name TEXT,
          road_ref TEXT,
          is_toll INTEGER NOT NULL,
          is_ferry INTEGER NOT NULL,
          is_roundabout INTEGER NOT NULL
        );
        CREATE VIRTUAL TABLE edge_rtree USING rtree(
          id,
          min_lat, max_lat,
          min_lon, max_lon
        );
        ",
    )
    .expect("schema");
}

fn build_db(pbf: &Path, db: &Path, bbox: Option<[f64; 4]>) {
    let _ = std::fs::remove_file(db);
    if let Some(parent) = db.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    eprintln!("building graph from {} …", pbf.display());
    let t0 = Instant::now();
    let graph = match bbox {
        Some(b) => {
            RouteGraph::build_from_pbf_bbox(pbf, RoutingProfile::Car, b).expect("bbox build")
        }
        None => RouteGraph::build_from_pbf(pbf, RoutingProfile::Car).expect("full build"),
    };
    eprintln!(
        "pbf_build_s={:.2} nodes={} edges={}",
        t0.elapsed().as_secs_f64(),
        graph.nodes.len(),
        graph.edges.len()
    );

    let conn = open_db(db);
    init_schema(&conn);
    conn.execute(
        "INSERT INTO meta(key,value) VALUES (?1,?2)",
        params![
            "pbf_name",
            pbf.file_name().and_then(|s| s.to_str()).unwrap_or("")
        ],
    )
    .ok();
    conn.execute(
        "INSERT INTO meta(key,value) VALUES (?1,?2)",
        params!["profile", "car"],
    )
    .ok();

    let tx = conn.unchecked_transaction().expect("tx");
    {
        let mut ins_n = tx
            .prepare("INSERT INTO nodes(id,lat,lon) VALUES (?1,?2,?3)")
            .unwrap();
        for (id, n) in &graph.nodes {
            ins_n
                .execute(params![id.0, n.coord.y, n.coord.x])
                .expect("ins node");
        }
    }
    {
        let mut ins_e = tx
            .prepare(
                "INSERT INTO edges(id,edge_key,source,target,length_m,base_weight,
                 start_lat,start_lon,end_lat,end_lon,highway,maxspeed_kmh,name,road_ref,
                 is_toll,is_ferry,is_roundabout)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
            )
            .unwrap();
        let mut ins_r = tx
            .prepare(
                "INSERT INTO edge_rtree(id,min_lat,max_lat,min_lon,max_lon) VALUES (?1,?2,?3,?4,?5)",
            )
            .unwrap();
        for (i, e) in graph.edges.iter().enumerate() {
            let id = (i + 1) as i64;
            ins_e
                .execute(params![
                    id,
                    e.id,
                    e.source.0,
                    e.target.0,
                    e.length_m,
                    e.base_weight,
                    e.start_lat,
                    e.start_lon,
                    e.end_lat,
                    e.end_lon,
                    e.highway,
                    e.maxspeed_kmh,
                    e.name,
                    e.road_ref,
                    e.is_toll as i32,
                    e.is_ferry as i32,
                    e.is_roundabout as i32,
                ])
                .expect("ins edge");
            let min_lat = e.start_lat.min(e.end_lat);
            let max_lat = e.start_lat.max(e.end_lat);
            let min_lon = e.start_lon.min(e.end_lon);
            let max_lon = e.start_lon.max(e.end_lon);
            ins_r
                .execute(params![id, min_lat, max_lat, min_lon, max_lon])
                .expect("ins rtree");
        }
    }
    tx.commit().expect("commit");
    let sz = std::fs::metadata(db).map(|m| m.len()).unwrap_or(0);
    eprintln!("wrote {} ({sz} bytes)", db.display());
}

fn load_graph_from_rtree(db: &Path, bbox: [f64; 4]) -> RouteGraph {
    let conn = open_db(db);
    let mut edge_ids: Vec<i64> = Vec::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT id FROM edge_rtree
                 WHERE max_lat >= ?1 AND min_lat <= ?2
                   AND max_lon >= ?3 AND min_lon <= ?4",
            )
            .unwrap();
        let rows = stmt
            .query_map(params![bbox[0], bbox[2], bbox[1], bbox[3]], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap();
        for id in rows.flatten() {
            edge_ids.push(id);
        }
    }
    if edge_ids.is_empty() {
        panic!("rtree query returned 0 edges for bbox {bbox:?}");
    }

    let mut edges: Vec<GraphEdge> = Vec::with_capacity(edge_ids.len());
    let mut needed: HashMap<i64, ()> = HashMap::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT edge_key,source,target,length_m,base_weight,
                        start_lat,start_lon,end_lat,end_lon,highway,maxspeed_kmh,name,road_ref,
                        is_toll,is_ferry,is_roundabout
                 FROM edges WHERE id = ?1",
            )
            .unwrap();
        for id in &edge_ids {
            let row = stmt
                .query_row(params![id], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, f64>(3)?,
                        r.get::<_, f64>(4)?,
                        r.get::<_, f64>(5)?,
                        r.get::<_, f64>(6)?,
                        r.get::<_, f64>(7)?,
                        r.get::<_, f64>(8)?,
                        r.get::<_, Option<String>>(9)?,
                        r.get::<_, Option<f64>>(10)?,
                        r.get::<_, Option<String>>(11)?,
                        r.get::<_, Option<String>>(12)?,
                        r.get::<_, i32>(13)?,
                        r.get::<_, i32>(14)?,
                        r.get::<_, i32>(15)?,
                    ))
                })
                .expect("edge row");
            needed.insert(row.1, ());
            needed.insert(row.2, ());
            edges.push(GraphEdge {
                id: row.0,
                source: NodeId(row.1),
                target: NodeId(row.2),
                length_m: row.3,
                base_weight: row.4,
                eco_weight: Some(row.4),
                start_lat: row.5,
                start_lon: row.6,
                end_lat: row.7,
                end_lon: row.8,
                shape: Vec::new(),
                highway: row.9,
                maxspeed_kmh: row.10,
                name: row.11,
                road_ref: row.12,
                is_motorroad: false,
                is_expressway: false,
                is_oneway: false,
                lanes: None,
                maxweight_t: None,
                maxaxleload_t: None,
                maxbogieweight_t: None,
                maxheight_m: None,
                maxwidth_m: None,
                maxlength_m: None,
                is_toll: row.13 != 0,
                is_ferry: row.14 != 0,
                is_boardwalk_crossing: false,
                is_roundabout: row.15 != 0,
                motor_vehicle_conditional: None,
                access_conditional: None,
                maxspeed_conditional: None,
                access_forbidden: false,
                surface_quality: driver_break_core::routing::graph::SurfaceQuality::Good,
            });
        }
    }

    let mut nodes: HashMap<NodeId, Node> = HashMap::with_capacity(needed.len());
    {
        let mut stmt = conn
            .prepare("SELECT lat, lon FROM nodes WHERE id = ?1")
            .unwrap();
        for id in needed.keys() {
            let (lat, lon) = stmt
                .query_row(params![id], |r| {
                    Ok((r.get::<_, f64>(0)?, r.get::<_, f64>(1)?))
                })
                .unwrap_or((0.0, 0.0));
            nodes.insert(
                NodeId(*id),
                Node {
                    id: NodeId(*id),
                    coord: Coord { x: lon, y: lat },
                    uses: 2,
                },
            );
        }
    }
    RouteGraph::from_parts(nodes, edges, RoutingProfile::Car)
}

fn plan_on(graph: &RouteGraph, slat: f64, slon: f64, elat: f64, elon: f64) -> (f64, f64, usize) {
    let (s, _) = graph.nearest_routable(slat, slon).expect("snap start");
    let (g, _) = graph.nearest_routable(elat, elon).expect("snap end");
    let (path, _, _cost) = graph.shortest_path(s, g, false).expect("no path");
    let mut dist = 0.0;
    for w in path.windows(2) {
        if let Some(idx) = graph.edge_index(w[0], w[1]) {
            dist += graph.edges[idx].length_m;
        }
    }
    let eta = driver_break_core::routing::motor_path_minutes(graph, &path);
    (dist / 1000.0, eta, path.len())
}

fn bench(pbf: &Path, db: &Path, slat: f64, slon: f64, elat: f64, elon: f64) {
    let bbox = trip_bbox(slat, slon, elat, elon);
    eprintln!(
        "bbox={:.3},{:.3},{:.3},{:.3} snap_max_m={:.0}",
        bbox[0],
        bbox[1],
        bbox[2],
        bbox[3],
        max_waypoint_snap_m(RoutingProfile::Car)
    );

    eprintln!("=== COLD PBF graph_build (build_from_pbf_bbox) ===");
    let t_cold = Instant::now();
    let cold_graph =
        RouteGraph::build_from_pbf_bbox(pbf, RoutingProfile::Car, bbox).expect("cold build");
    let cold_build_ms = t_cold.elapsed().as_secs_f64() * 1000.0;
    let t_ca = Instant::now();
    let (cold_km, cold_eta, cold_nodes) = plan_on(&cold_graph, slat, slon, elat, elon);
    let cold_astar_ms = t_ca.elapsed().as_secs_f64() * 1000.0;
    eprintln!(
        "cold_graph_build_ms={cold_build_ms:.1} cold_astar_ms={cold_astar_ms:.1} nodes={} edges={} dist_km={cold_km:.2} eta_min={cold_eta:.1} path_nodes={cold_nodes}",
        cold_graph.nodes.len(),
        cold_graph.edges.len()
    );

    eprintln!("=== INDEXED SQLite R*Tree load (first plan of this bbox from prebuilt DB) ===");
    let t_idx = Instant::now();
    let idx_graph = load_graph_from_rtree(db, bbox);
    let idx_load_ms = t_idx.elapsed().as_secs_f64() * 1000.0;
    let t_ia = Instant::now();
    let (idx_km, idx_eta, idx_nodes) = plan_on(&idx_graph, slat, slon, elat, elon);
    let idx_astar_ms = t_ia.elapsed().as_secs_f64() * 1000.0;
    eprintln!(
        "indexed_load_ms={idx_load_ms:.1} indexed_astar_ms={idx_astar_ms:.1} nodes={} edges={} dist_km={idx_km:.2} eta_min={idx_eta:.1} path_nodes={idx_nodes}",
        idx_graph.nodes.len(),
        idx_graph.edges.len()
    );

    let speedup = cold_build_ms / idx_load_ms.max(0.001);
    let pass_abs = idx_load_ms <= 2000.0;
    let pass_ratio = speedup >= 10.0;
    eprintln!(
        "RESULT speedup={speedup:.1}x phase0_abs_le_2s={pass_abs} phase0_ge_10x={pass_ratio} PHASE1B={}",
        if pass_abs && pass_ratio { "GO" } else { "NO-GO" }
    );
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        usage();
    }
    match args[1].as_str() {
        "build" => {
            let pbf = PathBuf::from(arg_value(&args, "--pbf").expect("--pbf"));
            let db = PathBuf::from(arg_value(&args, "--db").expect("--db"));
            let bbox = arg_value(&args, "--bbox").map(|s| parse_bbox(&s));
            build_db(&pbf, &db, bbox);
        }
        "bench" => {
            let pbf = PathBuf::from(arg_value(&args, "--pbf").expect("--pbf"));
            let db = PathBuf::from(arg_value(&args, "--db").expect("--db"));
            let slat: f64 = arg_value(&args, "--start-lat")
                .expect("--start-lat")
                .parse()
                .unwrap();
            let slon: f64 = arg_value(&args, "--start-lon")
                .expect("--start-lon")
                .parse()
                .unwrap();
            let elat: f64 = arg_value(&args, "--end-lat")
                .expect("--end-lat")
                .parse()
                .unwrap();
            let elon: f64 = arg_value(&args, "--end-lon")
                .expect("--end-lon")
                .parse()
                .unwrap();
            bench(&pbf, &db, slat, slon, elat, elon);
        }
        _ => usage(),
    }
}
