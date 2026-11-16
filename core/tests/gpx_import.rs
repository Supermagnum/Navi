//! Integration tests for the GPX importer: committed fixtures, exporter round trip, and
//! `#[ignore]`d smoke tests against real GPX files on the developer machine.
//!
//! Real-file smoke tests (not part of CI, files are never copied into the repository):
//!
//! ```text
//! cargo test --release -p driver-break-core --test gpx_import -- --ignored --nocapture
//! ```
//!
//! Each test reads its path from an environment variable and falls back to a literal path; it
//! skips cleanly when the file does not exist.

use std::path::{Path, PathBuf};
use std::time::Instant;

use driver_break_core::export::{parse_route_polyline, route_points_from_saved, to_gpx};
use driver_break_core::import::gpx_import::{
    parse_gpx, parse_gpx_bytes, sample_via_points, simplify, simplify_to_max_points, GpxDocument,
    GpxImportError,
};

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("gpx")
        .join(name)
}

fn load(name: &str) -> String {
    std::fs::read_to_string(fixture_path(name)).unwrap_or_else(|e| panic!("fixture {name}: {e}"))
}

fn parse_fixture(name: &str) -> GpxDocument {
    parse_gpx(&load(name)).unwrap_or_else(|e| panic!("fixture {name}: {e}"))
}

#[test]
fn fixture_track_with_two_segments() {
    let doc = parse_fixture("track_two_segments.gpx");
    assert_eq!(doc.name.as_deref(), Some("Two segment hike"));
    assert_eq!(doc.version.as_deref(), Some("1.1"));
    assert_eq!(doc.tracks.len(), 1);
    let t = &doc.tracks[0];
    assert_eq!(t.name.as_deref(), Some("Day 1"));
    assert_eq!(t.segments.len(), 2);
    assert_eq!(t.segments[0].len(), 3);
    assert_eq!(t.segments[1].len(), 2);
    assert_eq!(t.point_count(), 5);
    assert_eq!(doc.total_point_count(), 5);
    assert_eq!(
        t.segments[0][0].time.as_deref(),
        Some("2026-01-01T08:00:00Z")
    );
    assert_eq!(t.segments[1][0].ele, Some(130.0));
    assert_eq!(t.ascent_m(), Some(18.0));
    // Segment 1: ~111 m + ~124 m, segment 2: ~111 m (the gap between segments is not counted).
    let len = t.length_m();
    assert!((330.0..360.0).contains(&len), "length {len}");
    let b = doc.bounds().unwrap();
    assert_eq!(b.as_array(), [60.0, 10.0, 60.011, 10.01]);
    assert!(doc.unsupported.is_empty(), "{:?}", doc.unsupported);
}

#[test]
fn fixture_route_with_rtept() {
    let doc = parse_fixture("route_rtept.gpx");
    assert!(doc.tracks.is_empty());
    assert_eq!(doc.routes.len(), 1);
    let r = &doc.routes[0];
    assert_eq!(r.name.as_deref(), Some("Lillehammer to Tretten"));
    assert_eq!(r.points.len(), 3);
    assert_eq!(r.points[1].lat, 61.2);
    assert_eq!(doc.route_point_count(), 3);
    assert!(r.length_m() > 20_000.0 && r.length_m() < 40_000.0);
    assert!(doc.unsupported.is_empty(), "{:?}", doc.unsupported);
}

#[test]
fn fixture_waypoints_with_symbols_and_unicode() {
    let doc = parse_fixture("waypoints_symbols.gpx");
    assert_eq!(doc.waypoints.len(), 4);
    let w = &doc.waypoints;
    assert_eq!(w[0].name.as_deref(), Some("Svukuriset"));
    assert_eq!(w[0].desc.as_deref(), Some("capacity=43\ntents=yes"));
    assert_eq!(w[0].symbol.as_deref(), Some("Lodging"));
    assert_eq!(w[0].ele, Some(830.0));
    assert_eq!(w[1].name.as_deref(), Some("Sota S\u{e6}ter & Hytte"));
    assert_eq!(w[1].symbol.as_deref(), Some("Campground"));
    assert_eq!(
        w[2].name.as_deref(),
        Some("Gr\u{e5}bendalsvegen \u{2013} \u{c5}stdalsveien")
    );
    assert_eq!(w[2].symbol, None);
    assert_eq!(w[3].name.as_deref(), Some("K\u{f8}benhavn"));
    assert_eq!(w[3].symbol.as_deref(), Some("Flag, Blue"));
    assert!(doc.unsupported.is_empty(), "{:?}", doc.unsupported);
}

#[test]
fn fixture_foreign_extension_namespaces() {
    let doc = parse_fixture("foreign_extensions.gpx");
    assert_eq!(doc.name.as_deref(), Some("Extension noise"));
    assert_eq!(doc.tracks[0].name.as_deref(), Some("Track with extensions"));
    assert_eq!(doc.track_point_count(), 3);
    // The foreign <oa:ele> must not be read as elevation.
    assert_eq!(doc.tracks[0].segments[0][2].ele, None);
    assert_eq!(doc.tracks[0].segments[0][0].ele, Some(5.0));
    assert!(doc.unsupported.is_empty(), "{:?}", doc.unsupported);
}

#[test]
fn fixture_invalid_coordinates() {
    let doc = parse_fixture("invalid_coords.gpx");
    assert_eq!(doc.tracks.len(), 1);
    assert_eq!(doc.track_point_count(), 2);
    assert_eq!(doc.waypoint_count(), 1);
    assert_eq!(doc.waypoints[0].name.as_deref(), Some("Good waypoint"));
    let u = doc.unsupported.join(" | ");
    assert!(
        u.contains("2 track points skipped: missing or invalid lat/lon"),
        "{u}"
    );
    assert!(
        u.contains("2 track points skipped: lat/lon out of range"),
        "{u}"
    );
    assert!(
        u.contains("1 waypoints skipped: lat/lon out of range"),
        "{u}"
    );
    assert!(u.contains("track 'No points at all' has no points"), "{u}");
}

#[test]
fn fixture_gpx_10() {
    let doc = parse_fixture("gpx10.gpx");
    assert_eq!(doc.version.as_deref(), Some("1.0"));
    assert_eq!(doc.name.as_deref(), Some("Old school"));
    assert_eq!(doc.tracks.len(), 1);
    assert_eq!(doc.routes.len(), 1);
    assert_eq!(doc.waypoints.len(), 1);
    assert_eq!(doc.waypoints[0].symbol.as_deref(), Some("City"));
    assert_eq!(doc.tracks[0].segments[0][1].ele, Some(20.0));
    assert!(doc.unsupported.is_empty(), "{:?}", doc.unsupported);
}

#[test]
fn fixture_osmand_route_extension_noted_once() {
    let doc = parse_fixture("osmand_route_extension.gpx");
    assert_eq!(doc.track_point_count(), 2);
    let n = doc
        .unsupported
        .iter()
        .filter(|s| s.contains("OsmAnd"))
        .count();
    assert_eq!(n, 1, "{:?}", doc.unsupported);
}

#[test]
fn fixture_not_gpx() {
    assert_eq!(
        parse_gpx(&load("not_gpx.xml")),
        Err(GpxImportError::NotGpx {
            root: "kml".to_string()
        })
    );
}

#[test]
fn fixture_empty_gpx() {
    assert_eq!(parse_gpx(&load("empty.gpx")), Err(GpxImportError::Empty));
}

#[test]
fn bytes_entry_point_handles_bom_and_rejects_archives() {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(load("route_rtept.gpx").as_bytes());
    let doc = parse_gpx_bytes(&bytes).unwrap();
    assert_eq!(doc.routes.len(), 1);
    assert_eq!(
        parse_gpx_bytes(b"PK\x03\x04rest"),
        Err(GpxImportError::Compressed)
    );
    assert_eq!(
        parse_gpx_bytes(&[0x1f, 0x8b, 0x08]),
        Err(GpxImportError::Compressed)
    );
}

#[test]
fn round_trip_with_app_exporter() {
    let via_json = r#"[{"name":"Via A & B","lat":61.2,"lon":10.4},{"name":"Via <C>","lat":61.25,"lon":10.35}]"#;
    let rte = route_points_from_saved(
        61.115,
        10.466,
        Some("Lillehammer"),
        61.315,
        10.300,
        Some("Tretten"),
        via_json,
    );
    let poly = "10.466000,61.115000;10.400000,61.200000;10.350000,61.250000;10.300000,61.315000";
    let track = parse_route_polyline(poly);
    let xml = to_gpx(
        Some("Lillehammer -> Tretten"),
        Some("2026-09-04T08:00:00Z"),
        &rte,
        &track,
    );

    let doc = parse_gpx(&xml).expect("exporter output must re-import");
    assert_eq!(doc.version.as_deref(), Some("1.1"));
    assert_eq!(doc.name.as_deref(), Some("Lillehammer -> Tretten"));
    assert!(doc.unsupported.is_empty(), "{:?}", doc.unsupported);

    assert_eq!(doc.routes.len(), 1);
    let r = &doc.routes[0];
    assert_eq!(r.name.as_deref(), Some("Lillehammer -> Tretten"));
    assert_eq!(r.points.len(), rte.len());
    for (got, want) in r.points.iter().zip(&rte) {
        assert!((got.lat - want.lat).abs() < 1e-6);
        assert!((got.lon - want.lon).abs() < 1e-6);
    }
    assert_eq!(r.points[1].lat, 61.2);

    assert_eq!(doc.tracks.len(), 1);
    assert_eq!(doc.tracks[0].segments.len(), 1);
    let pts = &doc.tracks[0].segments[0];
    assert_eq!(pts.len(), track.len());
    for (got, &(lat, lon)) in pts.iter().zip(&track) {
        assert!((got.lat - lat).abs() < 1e-6);
        assert!((got.lon - lon).abs() < 1e-6);
        assert!(got.ele.is_none());
    }
    assert!(doc.tracks[0].length_m() > 20_000.0);
}

#[test]
fn round_trip_exporter_with_empty_track_keeps_route() {
    let rte = route_points_from_saved(1.0, 2.0, Some("A"), 3.0, 4.0, Some("B"), "");
    let xml = to_gpx(None, None, &rte, &[]);
    let doc = parse_gpx(&xml).unwrap();
    assert_eq!(doc.routes.len(), 1);
    assert_eq!(doc.routes[0].points.len(), 2);
    assert!(doc.tracks.is_empty());
    assert!(
        doc.unsupported.iter().any(|s| s.contains("has no points")),
        "{:?}",
        doc.unsupported
    );
}

#[test]
fn round_trip_then_via_sampling() {
    let track: Vec<(f64, f64)> = (0..=200)
        .map(|i| (60.0 + i as f64 * 0.001, 10.0 + i as f64 * 0.0005))
        .collect();
    let rte = route_points_from_saved(60.0, 10.0, None, 60.2, 10.1, None, "");
    let doc = parse_gpx(&to_gpx(Some("t"), None, &rte, &track)).unwrap();
    let pts = doc.tracks[0].flatten();
    let vias = sample_via_points(&pts, 5, 1_000.0);
    assert_eq!(vias.len(), 7);
    assert_eq!(vias.first(), pts.first());
    assert_eq!(vias.last(), pts.last());
    // A straight line simplifies to its end points.
    assert_eq!(simplify(&pts, 1.0).len(), 2);
    // Coordinates are rounded to 6 decimals, so tiny deviations may still be kept.
    assert!(simplify_to_max_points(&pts, 50).len() <= 50);
}

// ---------------------------------------------------------------------------------------------
// Real-file smoke tests (ignored by default)
// ---------------------------------------------------------------------------------------------

fn real_path(env_var: &str, default: &str) -> Option<PathBuf> {
    let p = std::env::var_os(env_var)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default));
    if p.is_file() {
        Some(p)
    } else {
        eprintln!(
            "SKIP: {} not found (set {env_var} to override)",
            p.display()
        );
        None
    }
}

fn report_real(env_var: &str, default: &str) -> Option<GpxDocument> {
    let path = real_path(env_var, default)?;
    let text = std::fs::read_to_string(&path).expect("read real GPX file");
    let start = Instant::now();
    let doc = parse_gpx(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let parse_ms = start.elapsed().as_secs_f64() * 1000.0;

    println!("=== {} ({} bytes)", path.display(), text.len());
    println!(
        "name={:?} version={:?} tracks={} routes={} waypoints={}",
        doc.name,
        doc.version,
        doc.tracks.len(),
        doc.routes.len(),
        doc.waypoints.len()
    );
    println!(
        "points: track={} route={} waypoint={} total={}",
        doc.track_point_count(),
        doc.route_point_count(),
        doc.waypoint_count(),
        doc.total_point_count()
    );
    for (i, t) in doc.tracks.iter().enumerate() {
        println!(
            "  track {i}: name={:?} segments={} points={} length_m={:.1} ascent_m={:?}",
            t.name,
            t.segments.len(),
            t.point_count(),
            t.length_m(),
            t.ascent_m().map(|a| a.round())
        );
    }
    for (i, r) in doc.routes.iter().enumerate() {
        println!(
            "  route {i}: name={:?} points={} length_m={:.1}",
            r.name,
            r.point_count(),
            r.length_m()
        );
    }
    println!("bounds={:?}", doc.bounds().map(|b| b.as_array()));
    println!("unsupported={:?}", doc.unsupported);
    println!(
        "parse_time_ms={parse_ms:.1} (profile: {})",
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );

    // Downstream helpers on the longest track.
    if let Some(longest) = doc.tracks.iter().max_by_key(|t| t.point_count()) {
        let pts = longest.flatten();
        let t0 = Instant::now();
        let simp = simplify_to_max_points(&pts, 500);
        let simp_ms = t0.elapsed().as_secs_f64() * 1000.0;
        let t1 = Instant::now();
        let tol = simplify(&pts, 5.0);
        let tol_ms = t1.elapsed().as_secs_f64() * 1000.0;
        let vias = sample_via_points(&pts, 20, 500.0);
        println!(
            "longest track: points={} simplify_to_max_points(500)={} ({simp_ms:.1} ms) \
             simplify(5 m)={} ({tol_ms:.1} ms) via_points(20, 500 m)={}",
            pts.len(),
            simp.len(),
            tol.len(),
            vias.len()
        );
        assert!(simp.len() <= 500);
        assert_eq!(simp.first(), pts.first());
        assert_eq!(simp.last(), pts.last());
    }
    Some(doc)
}

#[test]
#[ignore = "real-file smoke test: needs the developer's GPX file (env GPX_REAL_ROMERIKSLEDEN)"]
fn real_romeriksleden_hiking_path() {
    let Some(doc) = report_real(
        "GPX_REAL_ROMERIKSLEDEN",
        "/home/haaken/Nedlastinger/pilgrimsleden-osm/data/by_trail/Romeriksleden/hiking_path.gpx",
    ) else {
        return;
    };
    assert_eq!(doc.track_point_count(), 12521);
}

#[test]
#[ignore = "real-file smoke test: needs the developer's GPX file (env GPX_REAL_OSRM_ROUTE)"]
fn real_osrm_car_route() {
    let Some(doc) = report_real(
        "GPX_REAL_OSRM_ROUTE",
        "/home/haaken/Nedlastinger/route-unofficial.gpx",
    ) else {
        return;
    };
    assert_eq!(doc.track_point_count(), 3259);
}

#[test]
#[ignore = "real-file smoke test: needs the developer's GPX file (env GPX_REAL_DNT_EXPORT)"]
fn real_dnt_export() {
    let Some(doc) = report_real(
        "GPX_REAL_DNT_EXPORT",
        "/home/haaken/Nedlastinger/DNT-export.gpx",
    ) else {
        return;
    };
    assert_eq!(doc.waypoint_count(), 147);
    assert_eq!(doc.tracks.len(), 8);
}

#[test]
#[ignore = "real-file smoke test: needs the developer's GPX file (env GPX_REAL_STOLAVSLEDEN_BIKING)"]
fn real_stolavsleden_biking() {
    let Some(doc) = report_real(
        "GPX_REAL_STOLAVSLEDEN_BIKING",
        "/home/haaken/Nedlastinger/pilgrimsleden-osm/data/stolavsleden/biking.gpx",
    ) else {
        return;
    };
    assert_eq!(doc.track_point_count(), 28678);
}

#[test]
#[ignore = "real-file smoke test: needs the developer's GPX file (env GPX_REAL_EGINOLEDEN)"]
fn real_eginoleden() {
    let _ = report_real(
        "GPX_REAL_EGINOLEDEN",
        "/home/haaken/Nedlastinger/Scandi-pilgrim-poi/data/sweden/official_tracks/Eginoleden-Kiaby-Karlskrona.gpx",
    );
}

#[test]
#[ignore = "real-file smoke test: synthetic 15 MB file, timing is only meaningful with --release"]
fn synthetic_15mb_parse_time() {
    let mut s = String::with_capacity(16 * 1024 * 1024);
    s.push_str(
        r#"<?xml version="1.0"?><gpx version="1.1" xmlns="http://www.topografix.com/GPX/1/1"><trk><trkseg>"#,
    );
    let mut i = 0usize;
    while s.len() < 15 * 1024 * 1024 {
        s.push_str(&format!(
            "<trkpt lat=\"{:.7}\" lon=\"{:.7}\"><ele>{:.1}</ele><time>2026-01-01T00:00:00Z</time></trkpt>\n",
            60.0 + i as f64 * 1e-6,
            10.0 + (i as f64 * 0.001).sin() * 0.01,
            100.0 + (i % 200) as f64
        ));
        i += 1;
    }
    s.push_str("</trkseg></trk></gpx>");
    let start = Instant::now();
    let doc = parse_gpx(&s).unwrap();
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    println!(
        "synthetic: {} bytes, {} points, parse_time_ms={ms:.1} (profile: {})",
        s.len(),
        doc.track_point_count(),
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );
    assert_eq!(doc.track_point_count(), i);
}
