//! Minimal GPX 1.1 writer for saved / planned routes.
//!
//! Emits `<metadata>`, an `<rte>` of planning waypoints (start / vias / end), and a
//! `<trk>` of resolved path geometry. Elevation and per-point timestamps are omitted
//! unless the caller supplies them (this writer does not fabricate `<ele>` / `<time>`
//! on track points).

use serde::Deserialize;

/// One route waypoint for `<rtept>` (planning intent).
#[derive(Debug, Clone, PartialEq)]
pub struct GpxWaypoint {
    pub lat: f64,
    pub lon: f64,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ViaJsonPoint {
    #[serde(default)]
    name: Option<String>,
    lat: f64,
    lon: f64,
}

/// Parse Navi corridor polyline (`"lon,lat;lon,lat;…"`) into `(lat, lon)` track points.
pub fn parse_route_polyline(polyline: &str) -> Vec<(f64, f64)> {
    polyline
        .split(';')
        .filter_map(|part| {
            let mut bits = part.split(',');
            let lon: f64 = bits.next()?.trim().parse().ok()?;
            let lat: f64 = bits.next()?.trim().parse().ok()?;
            if !(lat.is_finite() && lon.is_finite()) {
                return None;
            }
            Some((lat, lon))
        })
        .collect()
}

/// Parse `via_json` (`[{"name","lat","lon"}, …]`) into waypoints.
pub fn parse_via_json(via_json: &str) -> Vec<GpxWaypoint> {
    let trimmed = via_json.trim();
    if trimmed.is_empty() || trimmed == "[]" {
        return Vec::new();
    }
    let Ok(raw) = serde_json::from_str::<Vec<ViaJsonPoint>>(trimmed) else {
        return Vec::new();
    };
    raw.into_iter()
        .filter(|p| p.lat.is_finite() && p.lon.is_finite())
        .map(|p| GpxWaypoint {
            lat: p.lat,
            lon: p.lon,
            name: p
                .name
                .map(|n| n.trim().to_string())
                .filter(|n| !n.is_empty()),
        })
        .collect()
}

/// Build a complete `<rte>` waypoint list from saved-route fields.
pub fn route_points_from_saved(
    start_lat: f64,
    start_lon: f64,
    start_name: Option<&str>,
    end_lat: f64,
    end_lon: f64,
    end_name: Option<&str>,
    via_json: &str,
) -> Vec<GpxWaypoint> {
    let mut pts = Vec::with_capacity(2 + 8);
    pts.push(GpxWaypoint {
        lat: start_lat,
        lon: start_lon,
        name: start_name
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
    });
    pts.extend(parse_via_json(via_json));
    pts.push(GpxWaypoint {
        lat: end_lat,
        lon: end_lon,
        name: end_name
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
    });
    pts
}

/// Serialize a route to GPX 1.1 XML.
///
/// * `name` / `time_iso` — optional `<metadata>` fields (`time_iso` should be UTC
///   `YYYY-MM-DDTHH:MM:SSZ` when present).
/// * `route_points` — start / via / end for `<rte>`.
/// * `track_points` — `(lat, lon)` geometry for `<trk>/<trkseg>`.
pub fn to_gpx(
    name: Option<&str>,
    time_iso: Option<&str>,
    route_points: &[GpxWaypoint],
    track_points: &[(f64, f64)],
) -> String {
    let mut out = String::with_capacity(256 + route_points.len() * 64 + track_points.len() * 48);
    out.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    out.push('\n');
    out.push_str(r#"<gpx version="1.1" creator="Navi" xmlns="http://www.topografix.com/GPX/1/1">"#);
    out.push('\n');

    let meta_name = name.map(str::trim).filter(|s| !s.is_empty());
    let meta_time = time_iso.map(str::trim).filter(|s| !s.is_empty());
    if meta_name.is_some() || meta_time.is_some() {
        out.push_str("  <metadata>\n");
        if let Some(n) = meta_name {
            out.push_str("    <name>");
            push_escaped(&mut out, n);
            out.push_str("</name>\n");
        }
        if let Some(t) = meta_time {
            out.push_str("    <time>");
            push_escaped(&mut out, t);
            out.push_str("</time>\n");
        }
        out.push_str("  </metadata>\n");
    }

    out.push_str("  <rte>\n");
    if let Some(n) = meta_name {
        out.push_str("    <name>");
        push_escaped(&mut out, n);
        out.push_str("</name>\n");
    }
    for pt in route_points {
        push_rtept(&mut out, pt);
    }
    out.push_str("  </rte>\n");

    out.push_str("  <trk>\n");
    if let Some(n) = meta_name {
        out.push_str("    <name>");
        push_escaped(&mut out, n);
        out.push_str("</name>\n");
    }
    out.push_str("    <trkseg>\n");
    for &(lat, lon) in track_points {
        out.push_str(&format!(
            "      <trkpt lat=\"{lat:.6}\" lon=\"{lon:.6}\"></trkpt>\n"
        ));
    }
    out.push_str("    </trkseg>\n");
    out.push_str("  </trk>\n");
    out.push_str("</gpx>\n");
    out
}

fn push_rtept(out: &mut String, pt: &GpxWaypoint) {
    out.push_str(&format!(
        "    <rtept lat=\"{:.6}\" lon=\"{:.6}\">",
        pt.lat, pt.lon
    ));
    if let Some(ref n) = pt.name {
        let trimmed = n.trim();
        if !trimmed.is_empty() {
            out.push_str("<name>");
            push_escaped(out, trimmed);
            out.push_str("</name>");
        }
    }
    out.push_str("</rtept>\n");
}

fn push_escaped(out: &mut String, s: &str) {
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count_tag(hay: &str, tag: &str) -> usize {
        hay.matches(&format!("<{tag}")).count()
    }

    #[test]
    fn to_gpx_direct_start_end_no_vias() {
        let rte = route_points_from_saved(
            61.115,
            10.466,
            Some("Lillehammer"),
            61.315,
            10.300,
            Some("Tretten"),
            "[]",
        );
        let track = vec![(61.115, 10.466), (61.200, 10.400), (61.315, 10.300)];
        let xml = to_gpx(
            Some("Lillehammer -> Tretten"),
            Some("2026-09-04T08:00:00Z"),
            &rte,
            &track,
        );

        assert!(xml.contains(r#"version="1.1""#));
        assert!(xml.contains("<metadata>"));
        assert!(xml.contains("Lillehammer -&gt; Tretten"));
        assert_eq!(count_tag(&xml, "rtept"), 2);
        assert_eq!(count_tag(&xml, "trkseg"), 1);
        assert_eq!(count_tag(&xml, "trkpt"), 3);
        assert!(!xml.contains("<ele>"));
        assert!(xml.contains(r#"<rtept lat="61.115000" lon="10.466000">"#));
        assert!(xml.contains(r#"<rtept lat="61.315000" lon="10.300000">"#));
    }

    #[test]
    fn to_gpx_with_vias_and_polyline_roundtrip() {
        let via_json =
            r#"[{"name":"Via A","lat":61.2,"lon":10.4},{"name":"Via B","lat":61.25,"lon":10.35}]"#;
        let rte =
            route_points_from_saved(61.1, 10.5, Some("Start"), 61.3, 10.2, Some("End"), via_json);
        assert_eq!(rte.len(), 4);
        assert_eq!(rte[1].name.as_deref(), Some("Via A"));
        assert!((rte[1].lat - 61.2).abs() < 1e-9);
        assert!((rte[1].lon - 10.4).abs() < 1e-9);

        let poly =
            "10.500000,61.100000;10.400000,61.200000;10.350000,61.250000;10.200000,61.300000";
        let track = parse_route_polyline(poly);
        assert_eq!(track.len(), 4);
        assert!((track[0].0 - 61.1).abs() < 1e-9);
        assert!((track[0].1 - 10.5).abs() < 1e-9);

        let xml = to_gpx(Some("Trip"), None, &rte, &track);
        assert_eq!(count_tag(&xml, "rtept"), 4);
        assert_eq!(count_tag(&xml, "trkpt"), 4);
        assert!(xml.contains("<name>Via A</name>"));
        assert!(xml.contains("<name>Via B</name>"));
        // Round-trip: rtept coords match saved waypoints within float formatting.
        for pt in &rte {
            let needle = format!(r#"lat="{:.6}" lon="{:.6}""#, pt.lat, pt.lon);
            assert!(xml.contains(&needle), "missing {needle}");
        }
    }

    #[test]
    fn empty_track_still_valid_rte() {
        let rte = route_points_from_saved(1.0, 2.0, None, 3.0, 4.0, None, "");
        let xml = to_gpx(None, None, &rte, &[]);
        assert!(xml.contains("<rte>"));
        assert_eq!(count_tag(&xml, "rtept"), 2);
        assert!(xml.contains("<trkseg>"));
        assert_eq!(count_tag(&xml, "trkpt"), 0);
    }
}
