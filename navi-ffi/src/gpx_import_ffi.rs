//! UniFFI surface for GPX import. The result is a JSON string (the Kotlin side parses it with
//! `org.json`); parsing is CPU bound and a large file can take a noticeable moment, so call it
//! off the main thread.

use driver_break_core::import::gpx_import::{
    parse_gpx, GpxDocument, GpxImportError, GpxPoint, GpxRoute, GpxTrack,
};
use serde_json::{json, Value};

/// Default point budget per line when the caller passes `0`.
const DEFAULT_MAX_POINTS_PER_LINE: usize = 2_000;

/// Parse GPX text and return JSON:
///
/// ```text
/// {ok, error, name, version,
///  tracks:    [{name, points:[[lat,lon],...], original_point_count, length_m, ascent_m?}],
///  routes:    [{name, points:[[lat,lon],...], original_point_count, length_m, ascent_m?}],
///  waypoints: [{name, desc, lat, lon, sym}],
///  unsupported: [string],
///  bounds: [minlat, minlon, maxlat, maxlon] | null}
/// ```
///
/// Track and route lines are reduced to at most `max_points_per_line` points with Douglas-Peucker
/// using an automatic tolerance (`0` selects a default of 2000). `length_m` and `ascent_m` are
/// computed from the full-resolution data, `ascent_m` only when the file carries elevation.
/// Segments of one track are joined into a single line. On failure the result is
/// `{"ok": false, "error": "<readable message>"}`.
#[uniffi::export]
pub fn gpx_parse_json(text: String, max_points_per_line: u32) -> String {
    let max_points = match max_points_per_line {
        0 => DEFAULT_MAX_POINTS_PER_LINE,
        n => n as usize,
    };
    match parse_gpx(&text) {
        Ok(doc) => gpx_document_json(&doc, max_points).to_string(),
        Err(e) => gpx_error_json(&e).to_string(),
    }
}

fn gpx_error_json(e: &GpxImportError) -> Value {
    json!({"ok": false, "error": e.to_string()})
}

fn line_json(
    name: Option<&str>,
    points: &[GpxPoint],
    length_m: f64,
    ascent_m: Option<f64>,
    max_points: usize,
) -> Value {
    let simplified =
        driver_break_core::import::gpx_import::simplify_to_max_points(points, max_points);
    let coords: Vec<[f64; 2]> = simplified.iter().map(|p| [p.lat, p.lon]).collect();
    let mut v = json!({
        "name": name,
        "points": coords,
        "original_point_count": points.len(),
        "length_m": length_m,
    });
    if let Some(a) = ascent_m {
        v["ascent_m"] = json!(a);
    }
    v
}

fn track_json(t: &GpxTrack, max_points: usize) -> Value {
    line_json(
        t.name.as_deref(),
        &t.flatten(),
        t.length_m(),
        t.ascent_m(),
        max_points,
    )
}

fn route_json(r: &GpxRoute, max_points: usize) -> Value {
    line_json(
        r.name.as_deref(),
        &r.points,
        r.length_m(),
        r.ascent_m(),
        max_points,
    )
}

fn gpx_document_json(doc: &GpxDocument, max_points: usize) -> Value {
    let name = doc
        .name
        .clone()
        .or_else(|| doc.tracks.iter().find_map(|t| t.name.clone()))
        .or_else(|| doc.routes.iter().find_map(|r| r.name.clone()));
    json!({
        "ok": true,
        "error": Value::Null,
        "name": name,
        "version": doc.version,
        "tracks": doc.tracks.iter().map(|t| track_json(t, max_points)).collect::<Vec<_>>(),
        "routes": doc.routes.iter().map(|r| route_json(r, max_points)).collect::<Vec<_>>(),
        "waypoints": doc.waypoints.iter().map(|w| json!({
            "name": w.name,
            "desc": w.desc,
            "lat": w.lat,
            "lon": w.lon,
            "sym": w.symbol,
        })).collect::<Vec<_>>(),
        "unsupported": doc.unsupported,
        "bounds": doc.bounds().map(|b| b.as_array()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(n: usize) -> String {
        let mut s = String::from(
            r#"<?xml version="1.0"?><gpx version="1.1" xmlns="http://www.topografix.com/GPX/1/1">
            <wpt lat="60.5" lon="10.5"><name>W &amp; Z</name><desc>d</desc><sym>Flag</sym></wpt>
            <trk><name>T</name><trkseg>"#,
        );
        for i in 0..n {
            s.push_str(&format!(
                r#"<trkpt lat="{:.6}" lon="{:.6}"><ele>{}</ele></trkpt>"#,
                60.0 + i as f64 * 1e-4,
                10.0 + (i as f64 * 0.01).sin() * 1e-3,
                100 + (i / 10) * 5
            ));
        }
        s.push_str("</trkseg></trk></gpx>");
        s
    }

    #[test]
    fn json_success_shape_and_simplification() {
        let v: Value = serde_json::from_str(&gpx_parse_json(sample(2_000), 100)).unwrap();
        assert_eq!(v["ok"], true);
        assert!(v["error"].is_null());
        assert_eq!(v["name"], "T");
        let t = &v["tracks"][0];
        assert_eq!(t["original_point_count"], 2_000);
        let pts = t["points"].as_array().unwrap();
        assert!(pts.len() <= 100 && pts.len() > 2);
        assert_eq!(pts[0].as_array().unwrap().len(), 2);
        assert!(t["length_m"].as_f64().unwrap() > 1_000.0);
        assert!(t["ascent_m"].as_f64().unwrap() > 100.0);
        assert_eq!(v["waypoints"][0]["name"], "W & Z");
        assert_eq!(v["waypoints"][0]["sym"], "Flag");
        assert!(v["unsupported"].as_array().unwrap().is_empty());
        let b = v["bounds"].as_array().unwrap();
        assert_eq!(b.len(), 4);
        assert_eq!(b[0].as_f64().unwrap(), 60.0);
    }

    #[test]
    fn json_zero_means_default_budget() {
        let v: Value = serde_json::from_str(&gpx_parse_json(sample(50), 0)).unwrap();
        assert_eq!(v["tracks"][0]["points"].as_array().unwrap().len(), 50);
    }

    #[test]
    fn json_errors_are_typed_messages() {
        for (input, needle) in [
            ("", "no usable"),
            ("<kml/>", "not a GPX file"),
            ("<gpx", "invalid XML"),
            ("PK\u{3}\u{4}", "compressed"),
        ] {
            let v: Value = serde_json::from_str(&gpx_parse_json(input.to_string(), 100)).unwrap();
            assert_eq!(v["ok"], false, "{input}");
            let msg = v["error"].as_str().unwrap();
            assert!(msg.contains(needle), "{input}: {msg}");
        }
    }

    #[test]
    fn route_only_file_has_routes() {
        let text = r#"<gpx version="1.1" xmlns="http://www.topografix.com/GPX/1/1">
            <rte><name>R</name><rtept lat="60" lon="10"/><rtept lat="60.1" lon="10.1"/></rte></gpx>"#;
        let v: Value = serde_json::from_str(&gpx_parse_json(text.to_string(), 10)).unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(v["name"], "R");
        assert_eq!(v["routes"][0]["points"].as_array().unwrap().len(), 2);
        assert!(v["routes"][0].get("ascent_m").is_none());
    }
}
