//! Repeater import scaffolds (OSM, AnyTone CSV, OpenRepeater, RadioID).
//! RepeaterBook is always disabled.

use std::collections::HashMap;
use std::path::Path;

use crate::repeater::{dmr_dedupe_key, AccessTone, RepeaterDb, RepeaterSite, RepeaterSource};

/// On-device / host relative directory for user AnyTone CPS CSV drops.
pub const CAT_IMPORT_REL: &str = "cat/import";

/// Normalize European comma decimals (`-0,6 Mhz` -> `-0.6`).
pub fn normalize_mhz_str(s: &str) -> String {
    s.trim()
        .trim_end_matches(|c: char| c.is_ascii_alphabetic() || c == ' ')
        .trim()
        .replace(',', ".")
}

pub fn parse_mhz(s: &str) -> Option<f64> {
    normalize_mhz_str(s).parse().ok()
}

/// Decode CPS export bytes: UTF-8 if valid, else Windows-1252 (byte→U+00xx).
pub fn decode_cps_bytes(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| char::from(b)).collect(),
    }
}

/// Ensure `{files_dir}/cat/import/` exists (Android filesDir or host path).
pub fn ensure_cat_import_dir(files_dir: &Path) -> std::io::Result<std::path::PathBuf> {
    let dir = files_dir.join(CAT_IMPORT_REL);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Scaffold: JSON array of OSM-shaped sites, or `{ "entries": [ ... ] }`
/// (as in `testdata/cat/non_networked.json`).
pub fn import_osm_json(db: &RepeaterDb, json: &str) -> anyhow::Result<usize> {
    let value: serde_json::Value = serde_json::from_str(json)?;
    let arr = if let Some(a) = value.as_array() {
        a.clone()
    } else if let Some(a) = value.get("entries").and_then(|v| v.as_array()) {
        a.clone()
    } else {
        anyhow::bail!("expected JSON array or object with entries[]");
    };
    let mut n = 0;
    for (i, item) in arr.iter().enumerate() {
        let callsign = item
            .get("callsign")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let osm_id = item.get("osm_id").and_then(|v| v.as_u64()).or_else(|| {
            item.get("osm_id")
                .and_then(|v| v.as_i64())
                .map(|v| v as u64)
        });
        let freq = item
            .get("freq_out_mhz")
            .and_then(|v| v.as_f64())
            .or_else(|| {
                item.get("frequency_out")
                    .and_then(|v| v.as_str())
                    .and_then(parse_mhz)
            })
            .unwrap_or(0.0);
        let shift = item.get("shift_mhz").and_then(|v| v.as_f64()).or_else(|| {
            item.get("shift")
                .and_then(|v| v.as_str())
                .and_then(parse_mhz)
        });
        let ctcss_hz = item.get("ctcss_hz").and_then(|v| v.as_f64()).or_else(|| {
            item.get("ctcss")
                .and_then(|v| v.as_str())
                .and_then(|s| s.replace(',', ".").parse().ok())
        });
        let id = match osm_id {
            Some(id) => format!("osm-{id}-{callsign}"),
            None => format!("osm-{i}-{callsign}"),
        };
        db.upsert_site(&RepeaterSite {
            id,
            callsign,
            lat: item.get("lat").and_then(|v| v.as_f64()),
            lon: item.get("lon").and_then(|v| v.as_f64()),
            freq_out_mhz: freq,
            shift_mhz: shift,
            ctcss_hz,
            dcs_code: None,
            color_code: None,
            modulation: item
                .get("modulation")
                .and_then(|v| v.as_str())
                .unwrap_or("NFM")
                .to_string(),
            network_id: item
                .get("network")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            source: RepeaterSource::Osm,
            conflict: false,
            conflict_note: None,
            distance_km: None,
            position_accurate: true,
            exclude_recommended: false,
            attribution: None,
            access: if ctcss_hz.is_some() {
                AccessTone::Ctcss
            } else {
                AccessTone::Unknown
            },
            country: None,
            file_kind: None,
            original_lat: None,
            original_lon: None,
            relocated: false,
            relocation_note: None,
        })?;
        n += 1;
    }
    Ok(n)
}

/// Scaffold: AnyTone `channel.csv` (no coordinates → not distance-autotune).
pub fn import_anytone_channel_csv(db: &RepeaterDb, csv: &str) -> anyhow::Result<usize> {
    let mut lines = csv.lines();
    let header = lines
        .next()
        .ok_or_else(|| anyhow::anyhow!("empty channel.csv"))?;
    let cols: Vec<&str> = header
        .split(',')
        .map(|s| s.trim().trim_matches('"'))
        .collect();
    let idx = |name: &str| cols.iter().position(|c| c.eq_ignore_ascii_case(name));
    let i_name = idx("Channel Name");
    let i_rx = idx("Receive Frequency");
    let i_tx = idx("Transmit Frequency");
    let i_type = idx("Channel Type");
    let i_cc = idx("RX Color Code").or_else(|| idx("Color Code"));
    let i_aprs = idx("APRS RX");

    let mut seen = std::collections::HashSet::new();
    let mut n = 0;
    for (row, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line
            .split(',')
            .map(|s| s.trim().trim_matches('"'))
            .collect();
        let get = |i: Option<usize>| i.and_then(|i| fields.get(i)).unwrap_or(&"").to_string();
        let name = get(i_name);
        if name.to_ascii_uppercase().contains("APRS") {
            continue;
        }
        let aprs = get(i_aprs);
        if !aprs.is_empty()
            && !aprs.eq_ignore_ascii_case("None")
            && aprs != "0"
            && !aprs.eq_ignore_ascii_case("Off")
        {
            continue;
        }
        let rx: f64 = get(i_rx).parse().unwrap_or(0.0);
        let tx: f64 = get(i_tx).parse().unwrap_or(rx);
        if (rx - tx).abs() < 1e-9 {
            continue; // simplex
        }
        let chan_type = get(i_type);
        let cc: u8 = get(i_cc).parse().unwrap_or(0);
        // DMR: many TG/slot rows share one physical repeater. Analog sites may
        // share RX/TX (different networks) — do not collapse those.
        let digital = chan_type.to_ascii_uppercase().contains("DIGITAL");
        let key = if digital {
            dmr_dedupe_key(rx, tx - rx, cc)
        } else {
            format!("analog|{name}|{rx:.5}|{tx:.5}")
        };
        if !seen.insert(key) {
            continue;
        }
        db.upsert_site(&RepeaterSite {
            id: format!("anytone-{row}"),
            callsign: name,
            lat: None,
            lon: None,
            freq_out_mhz: rx,
            shift_mhz: Some(tx - rx),
            ctcss_hz: None,
            dcs_code: None,
            color_code: Some(cc),
            modulation: chan_type,
            network_id: None,
            source: RepeaterSource::AnytoneCsv,
            conflict: false,
            conflict_note: None,
            distance_km: None,
            position_accurate: false,
            exclude_recommended: false,
            attribution: None,
            access: AccessTone::Unknown,
            country: None,
            file_kind: None,
            original_lat: None,
            original_lon: None,
            relocated: false,
            relocation_note: None,
        })?;
        n += 1;
    }
    Ok(n)
}

/// Accept AnyTone `offset.csv` even when the body is empty (header only).
pub fn import_anytone_offset_csv(_db: &RepeaterDb, csv: &str) -> anyhow::Result<usize> {
    let mut lines = csv.lines();
    let header = lines
        .next()
        .ok_or_else(|| anyhow::anyhow!("empty offset.csv"))?;
    if !header.to_ascii_lowercase().contains("offset") {
        anyhow::bail!("offset.csv missing Offset Frequency header");
    }
    let mut n = 0usize;
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        n += 1;
    }
    Ok(n)
}

pub fn import_openrepeater_json(db: &RepeaterDb, json: &str) -> anyhow::Result<usize> {
    let value: serde_json::Value = serde_json::from_str(json)?;
    let arr = if let Some(a) = value.as_array() {
        a.clone()
    } else if let Some(a) = value.get("repeaters").and_then(|v| v.as_array()) {
        a.clone()
    } else {
        anyhow::bail!("expected JSON array or object with repeaters[]");
    };
    let mut n = 0;
    for (i, item) in arr.iter().enumerate() {
        let callsign = item
            .get("callsign")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let mode = item
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if mode.to_ascii_uppercase().contains("APRS") {
            continue;
        }
        let rx = item
            .get("frequency")
            .or_else(|| item.get("rx"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let tx = item.get("tx").and_then(|v| v.as_f64()).unwrap_or(rx);
        db.upsert_site(&RepeaterSite {
            id: format!("or-{i}-{callsign}"),
            callsign,
            lat: item.get("lat").and_then(|v| v.as_f64()),
            lon: item.get("lon").and_then(|v| v.as_f64()),
            freq_out_mhz: rx,
            shift_mhz: Some(tx - rx),
            ctcss_hz: item.get("ctcss").and_then(|v| v.as_f64()),
            dcs_code: None,
            color_code: item
                .get("color_code")
                .and_then(|v| v.as_u64())
                .map(|v| v as u8),
            modulation: mode,
            network_id: None,
            source: RepeaterSource::OpenRepeater,
            conflict: false,
            conflict_note: None,
            distance_km: None,
            position_accurate: true,
            exclude_recommended: false,
            attribution: None,
            access: AccessTone::Unknown,
            country: None,
            file_kind: None,
            original_lat: None,
            original_lon: None,
            relocated: false,
            relocation_note: None,
        })?;
        n += 1;
    }
    Ok(n)
}

pub fn import_radioid_csv(db: &RepeaterDb, csv: &str) -> anyhow::Result<usize> {
    let mut lines = csv.lines();
    let header = lines
        .next()
        .ok_or_else(|| anyhow::anyhow!("empty radioid csv"))?;
    let cols: Vec<&str> = header.split(',').map(|s| s.trim()).collect();
    let idx = |name: &str| cols.iter().position(|c| c.eq_ignore_ascii_case(name));
    let i_call = idx("callsign");
    let i_rx = idx("rx").or_else(|| idx("frequency"));
    let i_tx = idx("tx");
    let i_cc = idx("color_code").or_else(|| idx("cc"));
    let i_lat = idx("lat");
    let i_lon = idx("lon");

    let mut seen = std::collections::HashSet::new();
    let mut n = 0;
    for (row, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
        let get = |i: Option<usize>| i.and_then(|i| fields.get(i)).unwrap_or(&"").to_string();
        let callsign = get(i_call);
        let rx: f64 = get(i_rx).parse().unwrap_or(0.0);
        let tx: f64 = get(i_tx).parse().unwrap_or(rx);
        let cc: u8 = get(i_cc).parse().unwrap_or(0);
        let key = dmr_dedupe_key(rx, tx - rx, cc);
        if !seen.insert(key) {
            continue;
        }
        db.upsert_site(&RepeaterSite {
            id: format!("radioid-{row}"),
            callsign,
            lat: get(i_lat).parse().ok(),
            lon: get(i_lon).parse().ok(),
            freq_out_mhz: rx,
            shift_mhz: Some(tx - rx),
            ctcss_hz: None,
            dcs_code: None,
            color_code: Some(cc),
            modulation: "DMR".into(),
            network_id: None,
            source: RepeaterSource::RadioId,
            conflict: false,
            conflict_note: None,
            distance_km: None,
            position_accurate: true,
            exclude_recommended: false,
            attribution: None,
            access: AccessTone::Unknown,
            country: None,
            file_kind: None,
            original_lat: None,
            original_lon: None,
            relocated: false,
            relocation_note: None,
        })?;
        n += 1;
    }
    Ok(n)
}

/// Parse an OSM `relation/.../full` XML (LA5MR-style network) into sites.
/// Members missing frequency_out or shift are skipped (never guessed).
/// Network-level CTCSS is used when a member omits its own tone.
pub fn import_osm_relation_xml(db: &RepeaterDb, xml: &str) -> anyhow::Result<usize> {
    let root = match xml_lite_parse(xml) {
        Ok(r) => r,
        Err(e) => anyhow::bail!("OSM relation XML: {e}"),
    };
    let mut nodes: HashMap<String, XmlEl> = HashMap::new();
    let mut ways: HashMap<String, XmlEl> = HashMap::new();
    let mut relation = None;
    for el in root.children {
        match el.name.as_str() {
            "node" => {
                if let Some(id) = el.attrs.get("id") {
                    nodes.insert(id.clone(), el);
                }
            }
            "way" => {
                if let Some(id) = el.attrs.get("id") {
                    ways.insert(id.clone(), el);
                }
            }
            "relation" => relation = Some(el),
            _ => {}
        }
    }
    let relation = relation.ok_or_else(|| anyhow::anyhow!("no relation element"))?;
    let rel_tags = tags_of(&relation);
    let network_id = rel_tags
        .get("name")
        .cloned()
        .or_else(|| rel_tags.get("network").cloned())
        .unwrap_or_else(|| "network".into());
    let default_ctcss = rel_tags
        .get("communication:amateur_radio:repeater:ctcss")
        .and_then(|s| parse_mhz(s.trim_end_matches("hz").trim_end_matches("Hz")));

    let mut n = 0usize;
    for m in relation.children.iter().filter(|c| c.name == "member") {
        let typ = m.attrs.get("type").map(|s| s.as_str()).unwrap_or("");
        let ref_id = m.attrs.get("ref").cloned().unwrap_or_default();
        let (el, id_prefix) = match typ {
            "node" => (nodes.get(&ref_id), "osm-node"),
            "way" => (ways.get(&ref_id), "osm-way"),
            _ => (None, "osm"),
        };
        let Some(el) = el else { continue };
        let tags = tags_of(el);
        let callsign = tags
            .get("callsign")
            .or_else(|| tags.get("communication:amateur_radio:callsign"))
            .cloned()
            .unwrap_or_else(|| ref_id.clone());
        let freq = tags
            .get("communication:amateur_radio:repeater:frequency_out")
            .and_then(|s| parse_mhz(s));
        let shift = tags
            .get("communication:amateur_radio:repeater:shift")
            .and_then(|s| parse_mhz(s));
        let (Some(freq), Some(shift)) = (freq, shift) else {
            continue; // incomplete — skip, never guess
        };
        let ctcss_hz = tags
            .get("communication:amateur_radio:repeater:ctcss")
            .and_then(|s| parse_mhz(s))
            .or(default_ctcss);
        let modulation = tags
            .get("communication:amateur_radio:repeater:modulation")
            .cloned()
            .unwrap_or_else(|| "11K2F3E".into());
        // Prefer FM/NFM analog; skip pure digital-only if no 11K2F3E.
        if !modulation.contains("11K2F3E") && !modulation.to_ascii_uppercase().contains("FM") {
            continue;
        }
        let (lat, lon) = match typ {
            "node" => (
                el.attrs.get("lat").and_then(|s| s.parse().ok()),
                el.attrs.get("lon").and_then(|s| s.parse().ok()),
            ),
            "way" => way_centroid(el, &nodes),
            _ => (None, None),
        };
        db.upsert_site(&RepeaterSite {
            id: format!("{id_prefix}-{ref_id}"),
            callsign,
            lat,
            lon,
            freq_out_mhz: freq,
            shift_mhz: Some(shift),
            ctcss_hz,
            dcs_code: None,
            color_code: None,
            modulation,
            network_id: Some(network_id.clone()),
            source: RepeaterSource::Osm,
            conflict: false,
            conflict_note: None,
            distance_km: None,
            position_accurate: lat.is_some() && lon.is_some(),
            exclude_recommended: false,
            attribution: None,
            access: AccessTone::Unknown,
            country: None,
            file_kind: None,
            original_lat: None,
            original_lon: None,
            relocated: false,
            relocation_note: None,
        })?;
        n += 1;
    }
    Ok(n)
}

#[derive(Debug, Clone)]
struct XmlEl {
    name: String,
    attrs: HashMap<String, String>,
    children: Vec<XmlEl>,
    #[allow(dead_code)]
    text: String,
}

fn tags_of(el: &XmlEl) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for c in &el.children {
        if c.name == "tag" {
            if let (Some(k), Some(v)) = (c.attrs.get("k"), c.attrs.get("v")) {
                m.insert(k.clone(), v.clone());
            }
        }
    }
    m
}

fn way_centroid(way: &XmlEl, nodes: &HashMap<String, XmlEl>) -> (Option<f64>, Option<f64>) {
    let mut lats = Vec::new();
    let mut lons = Vec::new();
    for c in &way.children {
        if c.name != "nd" {
            continue;
        }
        let Some(r) = c.attrs.get("ref") else {
            continue;
        };
        let Some(n) = nodes.get(r) else { continue };
        if let (Some(la), Some(lo)) = (
            n.attrs.get("lat").and_then(|s| s.parse().ok()),
            n.attrs.get("lon").and_then(|s| s.parse().ok()),
        ) {
            lats.push(la);
            lons.push(lo);
        }
    }
    if lats.is_empty() {
        return (None, None);
    }
    (
        Some(lats.iter().sum::<f64>() / lats.len() as f64),
        Some(lons.iter().sum::<f64>() / lons.len() as f64),
    )
}

/// Tiny non-validating XML parser for OSM API 0.6 documents (no namespaces).
fn xml_lite_parse(xml: &str) -> Result<XmlEl, String> {
    let bytes = xml.as_bytes();
    let mut i = 0usize;
    fn skip_ws(bytes: &[u8], i: &mut usize) {
        while *i < bytes.len() && bytes[*i].is_ascii_whitespace() {
            *i += 1;
        }
    }
    fn parse_el(bytes: &[u8], i: &mut usize) -> Result<XmlEl, String> {
        skip_ws(bytes, i);
        if *i >= bytes.len() || bytes[*i] != b'<' {
            return Err("expected <".into());
        }
        *i += 1;
        if *i < bytes.len() && bytes[*i] == b'/' {
            return Err("unexpected close".into());
        }
        if *i < bytes.len() && (bytes[*i] == b'?' || bytes[*i] == b'!') {
            // skip <?xml ...?> or <!-- -->
            while *i < bytes.len() && bytes[*i] != b'>' {
                *i += 1;
            }
            *i += 1;
            return parse_el(bytes, i);
        }
        let start = *i;
        while *i < bytes.len() && bytes[*i].is_ascii_alphanumeric() {
            *i += 1;
        }
        let name = std::str::from_utf8(&bytes[start..*i])
            .map_err(|e| e.to_string())?
            .to_string();
        let mut attrs = HashMap::new();
        loop {
            while *i < bytes.len() && bytes[*i].is_ascii_whitespace() {
                *i += 1;
            }
            if *i >= bytes.len() {
                return Err("truncated tag".into());
            }
            if bytes[*i] == b'/' {
                *i += 1;
                if *i < bytes.len() && bytes[*i] == b'>' {
                    *i += 1;
                    return Ok(XmlEl {
                        name,
                        attrs,
                        children: vec![],
                        text: String::new(),
                    });
                }
            }
            if bytes[*i] == b'>' {
                *i += 1;
                break;
            }
            let k0 = *i;
            while *i < bytes.len() && bytes[*i] != b'=' {
                *i += 1;
            }
            let key = std::str::from_utf8(&bytes[k0..*i])
                .map_err(|e| e.to_string())?
                .trim()
                .to_string();
            *i += 1;
            if *i >= bytes.len() || bytes[*i] != b'"' {
                return Err("attr quote".into());
            }
            *i += 1;
            let v0 = *i;
            while *i < bytes.len() && bytes[*i] != b'"' {
                *i += 1;
            }
            let val = std::str::from_utf8(&bytes[v0..*i])
                .map_err(|e| e.to_string())?
                .to_string();
            *i += 1;
            attrs.insert(key, val);
        }
        let mut children = Vec::new();
        let mut text = String::new();
        loop {
            while *i < bytes.len() && bytes[*i].is_ascii_whitespace() {
                *i += 1;
            }
            if *i + 1 < bytes.len() && bytes[*i] == b'<' && bytes[*i + 1] == b'/' {
                *i += 2;
                while *i < bytes.len() && bytes[*i] != b'>' {
                    *i += 1;
                }
                *i += 1;
                break;
            }
            if *i < bytes.len() && bytes[*i] == b'<' {
                children.push(parse_el(bytes, i)?);
            } else if *i < bytes.len() {
                let t0 = *i;
                while *i < bytes.len() && bytes[*i] != b'<' {
                    *i += 1;
                }
                text.push_str(std::str::from_utf8(&bytes[t0..*i]).unwrap_or(""));
            } else {
                break;
            }
        }
        Ok(XmlEl {
            name,
            attrs,
            children,
            text,
        })
    }
    parse_el(bytes, &mut i)
}

/// Always disabled until written RepeaterBook API permission exists.
pub fn import_repeaterbook(_db: &RepeaterDb, _query: &str) -> anyhow::Result<usize> {
    anyhow::bail!("RepeaterBook sync is disabled (no written API permission)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comma_mhz() {
        assert!((parse_mhz("-0,6 Mhz").unwrap() - (-0.6)).abs() < 1e-9);
    }

    #[test]
    fn repeaterbook_disabled() {
        let db = RepeaterDb::open_memory().unwrap();
        assert!(import_repeaterbook(&db, "x").is_err());
    }

    #[test]
    fn ensure_import_dir_creates() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = ensure_cat_import_dir(tmp.path()).unwrap();
        assert!(dir.is_dir());
        assert!(dir.ends_with("cat/import"));
    }
}
