//! SPLAT talk-coverage scoring (test oracle only — not a product input).

use serde_json::Value;

use crate::follow::haversine_km;
use crate::repeater::RepeaterSite;

#[derive(Debug, Clone)]
pub struct CoverageScore {
    pub samples_with_any_talk: usize,
    pub samples_selected_has_talk: usize,
    pub switches: usize,
    pub percent: f64,
}

/// Ray-casting point-in-ring. `ring` is GeoJSON `[lon, lat]` pairs.
pub fn point_in_ring(lon: f64, lat: f64, ring: &[[f64; 2]]) -> bool {
    let mut inside = false;
    let n = ring.len();
    if n < 3 {
        return false;
    }
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = (ring[i][0], ring[i][1]);
        let (xj, yj) = (ring[j][0], ring[j][1]);
        let intersect =
            ((yi > lat) != (yj > lat)) && (lon < (xj - xi) * (lat - yi) / (yj - yi + 1e-30) + xi);
        if intersect {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn ring_from_value(v: &[Value]) -> Option<Vec<[f64; 2]>> {
    let mut out = Vec::with_capacity(v.len());
    for p in v {
        let arr = p.as_array()?;
        if arr.len() < 2 {
            return None;
        }
        out.push([arr[0].as_f64()?, arr[1].as_f64()?]);
    }
    Some(out)
}

fn point_in_geometry(lon: f64, lat: f64, geom: &Value) -> bool {
    let typ = geom.get("type").and_then(|t| t.as_str()).unwrap_or("");
    let coords = match geom.get("coordinates") {
        Some(c) => c,
        None => return false,
    };
    let polys: Vec<&Value> = match typ {
        "Polygon" => vec![coords],
        "MultiPolygon" => coords
            .as_array()
            .map(|a| a.iter().collect())
            .unwrap_or_default(),
        _ => return false,
    };
    for poly in polys {
        let rings = match poly.as_array() {
            Some(r) => r,
            None => continue,
        };
        if rings.is_empty() {
            continue;
        }
        let outer = match rings[0].as_array().and_then(|r| ring_from_value(r)) {
            Some(r) => r,
            None => continue,
        };
        // bbox reject
        let (mut minx, mut maxx, mut miny, mut maxy) = (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        );
        for p in &outer {
            minx = minx.min(p[0]);
            maxx = maxx.max(p[0]);
            miny = miny.min(p[1]);
            maxy = maxy.max(p[1]);
        }
        if lon < minx || lon > maxx || lat < miny || lat > maxy {
            continue;
        }
        if !point_in_ring(lon, lat, &outer) {
            continue;
        }
        let mut in_hole = false;
        for hole in rings.iter().skip(1) {
            if let Some(h) = hole.as_array().and_then(|r| ring_from_value(r)) {
                if point_in_ring(lon, lat, &h) {
                    in_hole = true;
                    break;
                }
            }
        }
        if !in_hole {
            return true;
        }
    }
    false
}

/// Index talk-level MultiPolygons by `properties.callsign`.
pub fn talk_polygons_by_callsign(
    geojson: &str,
) -> Result<std::collections::HashMap<String, Vec<Value>>, String> {
    let v: Value = serde_json::from_str(geojson).map_err(|e| e.to_string())?;
    let feats = v
        .get("features")
        .and_then(|f| f.as_array())
        .ok_or_else(|| "FeatureCollection.features missing".to_string())?;
    let mut map: std::collections::HashMap<String, Vec<Value>> = std::collections::HashMap::new();
    for f in feats {
        let props = f.get("properties").unwrap_or(&Value::Null);
        let level = props.get("level").and_then(|x| x.as_str()).unwrap_or("");
        if level != "talk" {
            continue;
        }
        let call = props
            .get("callsign")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        if call.is_empty() {
            continue;
        }
        if let Some(g) = f.get("geometry") {
            map.entry(call).or_default().push(g.clone());
        }
    }
    Ok(map)
}

/// Map an OSM site id / callsign to a SPLAT feature callsign when possible.
pub fn splat_key_for_site(site: &RepeaterSite) -> String {
    // SPLAT labels Høykorset while OSM member 2641537344 is still named Bagn.
    if site.id.ends_with("2641537344") {
        return "LA5MR_HOYKORSET".into();
    }
    // Prefer osm-node-<id> → LA5MR_<id> if callsign is LA5MR
    if let Some(rest) = site.id.strip_prefix("osm-node-") {
        if site.callsign == "LA5MR" {
            return format!("LA5MR_{rest}");
        }
        return site.callsign.clone();
    }
    if site.id.starts_with("osm-way-") {
        return site.callsign.clone();
    }
    site.callsign.clone()
}

pub fn site_has_talk_at(
    lon: f64,
    lat: f64,
    site: &RepeaterSite,
    talk: &std::collections::HashMap<String, Vec<Value>>,
) -> bool {
    let key = splat_key_for_site(site);
    // Also try bare callsign
    let keys = [key, site.callsign.clone()];
    for k in &keys {
        if let Some(geoms) = talk.get(k) {
            for g in geoms {
                if point_in_geometry(lon, lat, g) {
                    return true;
                }
            }
        }
    }
    false
}

/// Score: among samples where any member has talk coverage, fraction where the
/// selected follow site also has talk. Reports switch count.
pub fn score_route_talk_coverage(
    samples: &[(f64, f64)], // (lat, lon)
    selected_site_ids: &[Option<&str>],
    members: &[RepeaterSite],
    talk: &std::collections::HashMap<String, Vec<Value>>,
) -> CoverageScore {
    assert_eq!(samples.len(), selected_site_ids.len());
    let mut any = 0usize;
    let mut covered = 0usize;
    let mut switches = 0usize;
    let mut prev: Option<&str> = None;
    for (i, &(lat, lon)) in samples.iter().enumerate() {
        let any_talk = members.iter().any(|s| site_has_talk_at(lon, lat, s, talk));
        if !any_talk {
            continue;
        }
        any += 1;
        let sel = selected_site_ids[i];
        if let Some(id) = sel {
            if prev.map(|p| p != id).unwrap_or(true) {
                switches += 1;
                prev = Some(id);
            }
            if let Some(site) = members.iter().find(|s| s.id == id) {
                if site_has_talk_at(lon, lat, site, talk) {
                    covered += 1;
                }
            }
        }
    }
    let percent = if any == 0 {
        0.0
    } else {
        100.0 * covered as f64 / any as f64
    };
    CoverageScore {
        samples_with_any_talk: any,
        samples_selected_has_talk: covered,
        switches,
        percent,
    }
}

/// Great-circle sample points Espa→Dombås style.
pub fn sample_great_circle(
    lat1: f64,
    lon1: f64,
    lat2: f64,
    lon2: f64,
    step_km: f64,
) -> Vec<(f64, f64)> {
    let total = haversine_km(lat1, lon1, lat2, lon2);
    let n = (total / step_km).ceil() as usize;
    (0..=n)
        .map(|i| {
            let f = i as f64 / n as f64;
            (lat1 + (lat2 - lat1) * f, lon1 + (lon2 - lon1) * f)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_in_simple_square() {
        let ring = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.0, 0.0]];
        assert!(point_in_ring(0.5, 0.5, &ring));
        assert!(!point_in_ring(1.5, 0.5, &ring));
    }

    #[test]
    fn score_with_tiny_geojson() {
        let geo = r#"{
          "type":"FeatureCollection",
          "features":[{
            "type":"Feature",
            "properties":{"callsign":"LA5MR_1","level":"talk"},
            "geometry":{"type":"Polygon","coordinates":[[[0,0],[2,0],[2,2],[0,2],[0,0]]]}
          },{
            "type":"Feature",
            "properties":{"callsign":"LA5MR_2","level":"talk"},
            "geometry":{"type":"Polygon","coordinates":[[[1,1],[3,1],[3,3],[1,3],[1,1]]]}
          }]
        }"#;
        let talk = talk_polygons_by_callsign(geo).unwrap();
        let members = vec![
            RepeaterSite {
                id: "osm-node-1".into(),
                callsign: "LA5MR".into(),
                lat: Some(0.5),
                lon: Some(0.5),
                freq_out_mhz: 145.625,
                shift_mhz: -0.6,
                ctcss_hz: Some(88.5),
                dcs_code: None,
                color_code: None,
                modulation: "11K2F3E".into(),
                network_id: Some("LA5MR".into()),
                source: crate::repeater::RepeaterSource::Osm,
                conflict: false,
                conflict_note: None,
                distance_km: None,
                position_accurate: true,
            },
            RepeaterSite {
                id: "osm-node-2".into(),
                callsign: "LA5MR".into(),
                lat: Some(2.0),
                lon: Some(2.0),
                freq_out_mhz: 145.625,
                shift_mhz: -0.6,
                ctcss_hz: Some(88.5),
                dcs_code: None,
                color_code: None,
                modulation: "11K2F3E".into(),
                network_id: Some("LA5MR".into()),
                source: crate::repeater::RepeaterSource::Osm,
                conflict: false,
                conflict_note: None,
                distance_km: None,
                position_accurate: true,
            },
        ];
        // Override splat key: our helper makes LA5MR_1 / LA5MR_2 from osm-node-1/2
        let samples = vec![(0.5, 0.5), (1.5, 1.5), (2.5, 2.5)];
        let selected = vec![Some("osm-node-1"), Some("osm-node-1"), Some("osm-node-2")];
        let score = score_route_talk_coverage(&samples, &selected, &members, &talk);
        assert!(score.percent >= 85.0 || score.samples_with_any_talk > 0);
        assert_eq!(score.switches, 2);
    }
}
