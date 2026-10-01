//! Repeater import scaffolds (OSM, AnyTone CSV, OpenRepeater, RadioID).
//! RepeaterBook is always disabled.

use crate::repeater::{dmr_dedupe_key, RepeaterDb, RepeaterSite, RepeaterSource};

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

/// Scaffold: JSON array of OSM-shaped sites.
pub fn import_osm_json(db: &RepeaterDb, json: &str) -> anyhow::Result<usize> {
    let value: serde_json::Value = serde_json::from_str(json)?;
    let arr = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("expected JSON array"))?;
    let mut n = 0;
    for (i, item) in arr.iter().enumerate() {
        let callsign = item
            .get("callsign")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
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
            .or_else(|| item.get("shift").and_then(|v| v.as_str()).and_then(parse_mhz))
            .unwrap_or(0.0);
        db.upsert_site(&RepeaterSite {
            id: format!("osm-{i}-{callsign}"),
            callsign,
            lat: item.get("lat").and_then(|v| v.as_f64()),
            lon: item.get("lon").and_then(|v| v.as_f64()),
            freq_out_mhz: freq,
            shift_mhz: shift,
            ctcss_hz: item.get("ctcss_hz").and_then(|v| v.as_f64()),
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
    let cols: Vec<&str> = header.split(',').map(|s| s.trim().trim_matches('"')).collect();
    let idx = |name: &str| cols.iter().position(|c| c.eq_ignore_ascii_case(name));
    let i_name = idx("Channel Name");
    let i_rx = idx("Receive Frequency");
    let i_tx = idx("Transmit Frequency");
    let i_type = idx("Channel Type");
    let i_cc = idx("RX Color Code");
    let i_aprs = idx("APRS RX");

    let mut seen = std::collections::HashSet::new();
    let mut n = 0;
    for (row, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split(',').map(|s| s.trim().trim_matches('"')).collect();
        let get = |i: Option<usize>| i.and_then(|i| fields.get(i)).unwrap_or(&"").to_string();
        let name = get(i_name);
        if name.to_ascii_uppercase().contains("APRS") {
            continue;
        }
        if !get(i_aprs).is_empty() && get(i_aprs) != "None" && get(i_aprs) != "0" {
            continue;
        }
        let rx: f64 = get(i_rx).parse().unwrap_or(0.0);
        let tx: f64 = get(i_tx).parse().unwrap_or(rx);
        if (rx - tx).abs() < 1e-9 {
            continue; // simplex
        }
        let cc: u8 = get(i_cc).parse().unwrap_or(0);
        let key = dmr_dedupe_key(rx, tx - rx, cc);
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
            modulation: get(i_type),
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

pub fn import_openrepeater_json(db: &RepeaterDb, json: &str) -> anyhow::Result<usize> {
    let value: serde_json::Value = serde_json::from_str(json)?;
    let arr = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("expected JSON array"))?;
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
}
