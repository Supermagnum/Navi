//! Repeater import scaffolds (OSM, AnyTone CSV, OpenRepeater, RadioID).
//! RepeaterBook is always disabled.

use std::path::Path;

use crate::repeater::{dmr_dedupe_key, RepeaterDb, RepeaterSite, RepeaterSource};

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
        let shift = item
            .get("shift_mhz")
            .and_then(|v| v.as_f64())
            .or_else(|| {
                item.get("shift")
                    .and_then(|v| v.as_str())
                    .and_then(parse_mhz)
            })
            .unwrap_or(0.0);
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
            shift_mhz: tx - rx,
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
            shift_mhz: tx - rx,
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
            shift_mhz: tx - rx,
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
        })?;
        n += 1;
    }
    Ok(n)
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
