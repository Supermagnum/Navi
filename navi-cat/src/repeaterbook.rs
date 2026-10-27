//! User-downloaded RepeaterBook KML/KMZ/CSV. The app never contacts repeaterbook.com.
//!
//! Real RepeaterBook KML exports (checked against the country files) store, per placemark:
//! callsign in `<name>`, location / output frequency / optional tone / on-air in the
//! HTML description, country as `state_id` in the details link, and a `<Point>` whose
//! coordinates are `lon,lat[,alt]`. Those files do not carry offset, separate TX/RX
//! tones, mode, or a network name.
//!
//! Duplicate border rows (same callsign, same output frequency, positions within
//! [`DEDUP_DISTANCE_KM`]) are dropped. CSV latitude/longitude columns are used only
//! when every value parses, is in range, is not 0,0, and has at least
//! [`COORD_MIN_DECIMALS`] digits after the decimal point.

use std::io::{Cursor, Read};
use std::path::Path;

use roxmltree::Document;

use crate::importers::decode_cps_bytes;
use crate::repeater::{RepeaterDb, RepeaterSite, RepeaterSource};

/// Same callsign + output frequency inside this distance is one repeater.
pub const DEDUP_DISTANCE_KM: f64 = 1.0;
/// Minimum fractional digits before a CSV coordinate column is trusted (~111 m).
pub const COORD_MIN_DECIMALS: usize = 3;
/// Default radius when the position is outside every installed map pack.
pub const DEFAULT_REGION_RADIUS_KM: f64 = 100.0;

const FREQ_MATCH_MHZ: f64 = 0.0125;

#[derive(Debug, Clone)]
pub struct RbRecord {
    pub callsign: String,
    pub location_name: String,
    pub freq_out_mhz: f64,
    pub shift_mhz: Option<f64>,
    pub ctcss_hz: Option<f64>,
    pub mode: String,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub country: Option<String>,
    pub on_air: bool,
    pub network: Option<String>,
    pub file_name: String,
    pub kind: String,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct FileReport {
    pub file_name: String,
    pub kind: String,
    pub country: Option<String>,
    pub read: usize,
    pub imported: usize,
    pub merged_osm: usize,
    pub overrides: Vec<String>,
    pub added: usize,
    pub duplicates_dropped: usize,
    pub rejected: Vec<String>,
    pub csv_without_position: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ImportBatch {
    pub ok: bool,
    pub kept_last_good: bool,
    pub files: Vec<FileReport>,
    pub error: Option<String>,
}

pub fn default_repeater_drop_dir(app_data_dir: &Path) -> std::path::PathBuf {
    app_data_dir.join("repeaters")
}

/// Online mirror sources. RepeaterBook is not one of them.
pub fn online_mirror_source(name: &str) -> bool {
    let n = name.trim();
    !n.eq_ignore_ascii_case("repeaterbook") && !n.eq_ignore_ascii_case("repeater_book")
}

pub fn parse_kml_bytes(file_name: &str, bytes: &[u8]) -> Result<Vec<RbRecord>, String> {
    let text = decode_cps_bytes(bytes);
    parse_kml_str(file_name, &text)
}

pub fn parse_kmz_bytes(file_name: &str, bytes: &[u8]) -> Result<Vec<RbRecord>, String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        if !name.to_ascii_lowercase().ends_with(".kml") {
            continue;
        }
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf).map_err(|e| e.to_string())?;
        return parse_kml_bytes(file_name, &buf);
    }
    Err("kmz contains no .kml entry".into())
}

fn parse_kml_str(file_name: &str, text: &str) -> Result<Vec<RbRecord>, String> {
    let doc = Document::parse(text).map_err(|e| format!("kml: {e}"))?;
    let mut out = Vec::new();
    for node in doc.descendants() {
        if node.tag_name().name() != "Placemark" {
            continue;
        }
        let callsign = child_text(node, "name");
        let description = child_text(node, "description");
        let coords = node
            .descendants()
            .find(|n| n.tag_name().name() == "coordinates")
            .map(|n| n.text().unwrap_or("").trim().to_string())
            .unwrap_or_default();
        match placemark_record(file_name, &callsign, &description, &coords) {
            Ok(rec) => out.push(rec),
            Err(reason) => {
                // A single bad placemark does not fail the file. The caller
                // records `reason` only when the whole document fails to parse.
                let _ = reason;
                out.push(RbRecord {
                    callsign,
                    location_name: String::new(),
                    freq_out_mhz: f64::NAN,
                    shift_mhz: None,
                    ctcss_hz: None,
                    mode: "FM".into(),
                    lat: None,
                    lon: None,
                    country: None,
                    on_air: true,
                    network: None,
                    file_name: file_name.to_string(),
                    kind: "kml".into(),
                });
            }
        }
    }
    if out.is_empty()
        && doc
            .root()
            .descendants()
            .all(|n| n.tag_name().name() != "Placemark")
    {
        return Err("kml: no Placemark".into());
    }
    Ok(out)
}

fn child_text(node: roxmltree::Node<'_, '_>, name: &str) -> String {
    node.children()
        .find(|n| n.tag_name().name() == name)
        .map(|n| n.text().unwrap_or("").to_string())
        .unwrap_or_default()
}

fn placemark_record(
    file_name: &str,
    callsign: &str,
    description: &str,
    coords: &str,
) -> Result<RbRecord, String> {
    let callsign = callsign.trim().to_string();
    if callsign.is_empty() {
        return Err("missing callsign".into());
    }
    let (lat, lon) = parse_kml_point(coords)?;
    let fields = parse_description(description);
    let freq = fields
        .freq_mhz
        .ok_or_else(|| "missing frequency".to_string())?;
    Ok(RbRecord {
        callsign,
        location_name: fields.location,
        freq_out_mhz: freq,
        shift_mhz: None,
        ctcss_hz: fields.ctcss_hz,
        mode: fields.mode,
        lat: Some(lat),
        lon: Some(lon),
        country: fields.country,
        on_air: fields.on_air,
        network: None,
        file_name: file_name.to_string(),
        kind: "kml".into(),
    })
}

/// KML `<coordinates>` are longitude, latitude, optional altitude.
pub fn parse_kml_point(coords: &str) -> Result<(f64, f64), String> {
    let mut parts = coords.split(',').map(str::trim).filter(|s| !s.is_empty());
    let lon: f64 = parts
        .next()
        .ok_or_else(|| "missing longitude".to_string())?
        .parse()
        .map_err(|_| "bad longitude".to_string())?;
    let lat: f64 = parts
        .next()
        .ok_or_else(|| "missing latitude".to_string())?
        .parse()
        .map_err(|_| "bad latitude".to_string())?;
    validate_lat_lon(lat, lon)?;
    Ok((lat, lon))
}

fn validate_lat_lon(lat: f64, lon: f64) -> Result<(), String> {
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return Err("coordinates out of range".into());
    }
    if lat.abs() < 1e-7 && lon.abs() < 1e-7 {
        return Err("coordinates are 0,0".into());
    }
    Ok(())
}

struct DescFields {
    location: String,
    freq_mhz: Option<f64>,
    ctcss_hz: Option<f64>,
    on_air: bool,
    country: Option<String>,
    mode: String,
}

fn parse_description(raw: &str) -> DescFields {
    let country = raw
        .split("state_id=")
        .nth(1)
        .and_then(|s| s.get(..2))
        .filter(|s| s.chars().all(|c| c.is_ascii_alphabetic()))
        .map(|s| s.to_ascii_uppercase());
    let plain = html_to_lines(raw);
    let mut location = String::new();
    let mut freq_mhz = None;
    let mut ctcss_hz = None;
    let mut on_air = true;
    let mut mode = "FM".to_string();
    for (i, line) in plain.iter().enumerate() {
        let lower = line.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("on-air:") {
            on_air = rest.trim().starts_with("yes");
            continue;
        }
        if lower.contains("dmr") {
            mode = "DMR".into();
        } else if lower.contains("d-star") || lower.contains("dstar") {
            mode = "D-STAR".into();
        } else if lower.contains("c4fm") {
            mode = "C4FM".into();
        }
        if freq_mhz.is_none() {
            if let Some((f, tone)) = parse_freq_tone(line) {
                freq_mhz = Some(f);
                ctcss_hz = tone;
                continue;
            }
        }
        if i == 0 && location.is_empty() && !line.to_ascii_lowercase().contains("click for") {
            location = line.clone();
        }
    }
    DescFields {
        location,
        freq_mhz,
        ctcss_hz,
        on_air,
        country,
        mode,
    }
}

fn html_to_lines(raw: &str) -> Vec<String> {
    let unescaped = unescape_basic(raw);
    let with_breaks = unescaped
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("<BR>", "\n");
    let lower_breaks = {
        let mut s = String::new();
        let bytes = with_breaks.as_str();
        let mut rest = bytes;
        while let Some(idx) = rest.to_ascii_lowercase().find("<br") {
            s.push_str(&rest[..idx]);
            s.push('\n');
            if let Some(end) = rest[idx..].find('>') {
                rest = &rest[idx + end + 1..];
            } else {
                rest = &rest[idx + 3..];
            }
        }
        s.push_str(rest);
        s
    };
    let mut stripped = String::new();
    let mut in_tag = false;
    for c in lower_breaks.chars() {
        if c == '<' {
            in_tag = true;
            continue;
        }
        if c == '>' {
            in_tag = false;
            continue;
        }
        if !in_tag {
            stripped.push(c);
        }
    }
    stripped
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

fn unescape_basic(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

/// CTCSS is the second number when it is at most 260 Hz. 1750 Hz is a tone burst,
/// not a CTCSS tone, and is not stored as `ctcss_hz`.
fn parse_freq_tone(line: &str) -> Option<(f64, Option<f64>)> {
    let mut nums = line.split_whitespace();
    let freq: f64 = nums.next()?.parse().ok()?;
    if !(1.0..=1300.0).contains(&freq) {
        return None;
    }
    let tone = nums
        .next()
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|t| (60.0..=260.0).contains(t));
    Some((freq, tone))
}

/// Recognised coordinate headers (ASCII, case-insensitive):
/// latitude: `latitude`, `lat`.
/// longitude: `longitude`, `lon`, `lng`, `long`.
pub fn csv_coord_headers_recognised() -> &'static [&'static str] {
    &["latitude", "lat", "longitude", "lon", "lng", "long"]
}

pub fn parse_csv_bytes(file_name: &str, bytes: &[u8]) -> Result<Vec<RbRecord>, String> {
    let text = decode_cps_bytes(bytes);
    parse_csv_str(file_name, &text)
}

fn parse_csv_str(file_name: &str, text: &str) -> Result<Vec<RbRecord>, String> {
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let header = lines.next().ok_or_else(|| "empty csv".to_string())?;
    let cols: Vec<String> = split_csv(header)
        .into_iter()
        .map(|s| s.trim().trim_matches('"').to_string())
        .collect();
    let idx = |names: &[&str]| {
        cols.iter()
            .position(|c| names.iter().any(|n| c.eq_ignore_ascii_case(n)))
    };
    let i_call = idx(&["callsign", "call", "station call"])
        .ok_or_else(|| "csv missing callsign column".to_string())?;
    let i_freq = idx(&[
        "frequency",
        "freq",
        "output",
        "output frequency",
        "downlink",
    ])
    .ok_or_else(|| "csv missing frequency column".to_string())?;
    let i_lat = idx(&["latitude", "lat"]);
    let i_lon = idx(&["longitude", "lon", "lng", "long"]);
    let i_shift = idx(&["offset", "shift"]);
    let i_ctcss = idx(&["ctcss", "tone", "ctcss tx", "ctcss rx"]);
    let i_mode = idx(&["mode"]);
    let i_net = idx(&["network", "system"]);
    let i_loc = idx(&["location", "city", "nearest city"]);
    let i_status = idx(&["status", "on-air", "on air"]);

    let rows: Vec<Vec<String>> = lines.map(split_csv).collect();
    let coords_ok = match (i_lat, i_lon) {
        (Some(la), Some(lo)) => rows.iter().all(|r| row_coords_ok(r, la, lo)),
        _ => false,
    };

    let mut out = Vec::new();
    for row in rows {
        let callsign = row
            .get(i_call)
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        if callsign.is_empty() {
            continue;
        }
        let freq: f64 = row
            .get(i_freq)
            .and_then(|s| s.trim().replace(',', ".").parse().ok())
            .unwrap_or(f64::NAN);
        let (lat, lon) = if coords_ok {
            let lat = row[i_lat.unwrap()].trim().parse().unwrap();
            let lon = row[i_lon.unwrap()].trim().parse().unwrap();
            (Some(lat), Some(lon))
        } else {
            (None, None)
        };
        let mode = i_mode
            .and_then(|i| row.get(i))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "FM".into());
        let on_air = i_status
            .and_then(|i| row.get(i))
            .map(|s| {
                let t = s.trim().to_ascii_lowercase();
                t.is_empty() || t.starts_with('y') || t == "on" || t == "on-air"
            })
            .unwrap_or(true);
        out.push(RbRecord {
            callsign,
            location_name: i_loc
                .and_then(|i| row.get(i))
                .map(|s| s.trim().to_string())
                .unwrap_or_default(),
            freq_out_mhz: freq,
            shift_mhz: i_shift
                .and_then(|i| row.get(i))
                .and_then(|s| s.trim().parse().ok()),
            ctcss_hz: i_ctcss
                .and_then(|i| row.get(i))
                .and_then(|s| s.trim().parse().ok()),
            mode,
            lat,
            lon,
            country: None,
            on_air,
            network: i_net
                .and_then(|i| row.get(i))
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            file_name: file_name.to_string(),
            kind: "csv".into(),
        });
    }
    Ok(out)
}

fn row_coords_ok(row: &[String], i_lat: usize, i_lon: usize) -> bool {
    let Some(lat_s) = row.get(i_lat) else {
        return false;
    };
    let Some(lon_s) = row.get(i_lon) else {
        return false;
    };
    if decimal_places(lat_s.trim()) < COORD_MIN_DECIMALS
        || decimal_places(lon_s.trim()) < COORD_MIN_DECIMALS
    {
        return false;
    }
    let Ok(lat) = lat_s.trim().parse::<f64>() else {
        return false;
    };
    let Ok(lon) = lon_s.trim().parse::<f64>() else {
        return false;
    };
    validate_lat_lon(lat, lon).is_ok()
}

fn decimal_places(s: &str) -> usize {
    s.split_once('.').map(|(_, frac)| frac.len()).unwrap_or(0)
}

fn split_csv(line: &str) -> Vec<String> {
    line.split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .collect()
}

/// Drop later rows that repeat callsign + output frequency within [`DEDUP_DISTANCE_KM`].
pub fn dedup_records(records: &mut Vec<RbRecord>) -> usize {
    let mut kept: Vec<RbRecord> = Vec::new();
    let mut dropped = 0usize;
    for rec in records.drain(..) {
        if !rec.freq_out_mhz.is_finite() {
            kept.push(rec);
            continue;
        }
        let dup = kept.iter().any(|prev| {
            prev.callsign.eq_ignore_ascii_case(&rec.callsign)
                && (prev.freq_out_mhz - rec.freq_out_mhz).abs() <= FREQ_MATCH_MHZ
                && match (prev.lat, prev.lon, rec.lat, rec.lon) {
                    (Some(a), Some(b), Some(c), Some(d)) => {
                        haversine_km(a, b, c, d) <= DEDUP_DISTANCE_KM
                    }
                    _ => true,
                }
        });
        if dup {
            dropped += 1;
        } else {
            kept.push(rec);
        }
    }
    *records = kept;
    dropped
}

fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6371.0;
    let p1 = lat1.to_radians();
    let p2 = lat2.to_radians();
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

pub fn merge_records(db: &RepeaterDb, records: &[RbRecord]) -> FileReport {
    let mut report = FileReport {
        read: records.len(),
        ..FileReport::default()
    };
    let mut countries = std::collections::BTreeMap::<String, usize>::new();
    for rec in records {
        if let Some(c) = &rec.country {
            *countries.entry(c.clone()).or_default() += 1;
        }
        if !rec.freq_out_mhz.is_finite() {
            report
                .rejected
                .push(format!("{}: missing frequency", rec.callsign));
            continue;
        }
        match apply_one(db, rec) {
            Apply::Merged => report.merged_osm += 1,
            Apply::Override(note) => report.overrides.push(note),
            Apply::Added => report.added += 1,
            Apply::Unplaced => report.csv_without_position += 1,
        }
        report.imported += 1;
    }
    report.country = countries
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .map(|(c, _)| c);
    report
}

enum Apply {
    Merged,
    Override(String),
    Added,
    Unplaced,
}

fn apply_one(db: &RepeaterDb, rec: &RbRecord) -> Apply {
    let osm: Vec<RepeaterSite> = db
        .list_sites()
        .into_iter()
        .filter(|s| {
            s.source == RepeaterSource::Osm && s.callsign.eq_ignore_ascii_case(&rec.callsign)
        })
        .collect();
    if let Some(hit) = best_osm_match(&osm, rec) {
        let mut fields = Vec::new();
        let mut updated = hit.clone();
        if (hit.freq_out_mhz - rec.freq_out_mhz).abs() > FREQ_MATCH_MHZ {
            updated.freq_out_mhz = rec.freq_out_mhz;
            fields.push("frequency");
        }
        if let Some(tone) = rec.ctcss_hz {
            if hit
                .ctcss_hz
                .map(|c| (c - tone).abs() > 0.05)
                .unwrap_or(true)
            {
                updated.ctcss_hz = Some(tone);
                fields.push("ctcss");
            }
        }
        if let Some(shift) = rec.shift_mhz {
            if (hit.shift_mhz - shift).abs() > 0.001 {
                updated.shift_mhz = shift;
                fields.push("shift");
            }
        }
        if fields.is_empty() {
            if updated.attribution.is_none() {
                updated.attribution = Some(provenance(rec));
            }
            let _ = db.upsert_site(&updated);
            return Apply::Merged;
        }
        updated.conflict = true;
        updated.conflict_note = Some(format!(
            "RepeaterBook override ({}) from {}",
            fields.join(","),
            rec.file_name
        ));
        updated.attribution = Some(provenance(rec));
        let note = updated.conflict_note.clone().unwrap_or_default();
        let _ = db.upsert_site(&updated);
        return Apply::Override(note);
    }
    let placed = rec.lat.is_some() && rec.lon.is_some();
    let site = RepeaterSite {
        id: format!("rb-{}-{}-{:.5}", rec.kind, rec.callsign, rec.freq_out_mhz),
        callsign: rec.callsign.clone(),
        lat: rec.lat,
        lon: rec.lon,
        freq_out_mhz: rec.freq_out_mhz,
        shift_mhz: rec.shift_mhz.unwrap_or(0.0),
        ctcss_hz: rec.ctcss_hz,
        dcs_code: None,
        color_code: None,
        modulation: rec.mode.clone(),
        network_id: rec.network.clone(),
        source: RepeaterSource::RepeaterBook,
        conflict: false,
        conflict_note: if rec.on_air {
            None
        } else {
            Some("off-air".into())
        },
        distance_km: None,
        position_accurate: placed,
        exclude_recommended: !rec.on_air || !is_fm_mode(&rec.mode),
        attribution: Some(provenance(rec)),
    };
    let _ = db.upsert_site(&site);
    if placed {
        Apply::Added
    } else {
        Apply::Unplaced
    }
}

fn is_fm_mode(mode: &str) -> bool {
    let m = mode.to_ascii_uppercase();
    !(m.contains("DMR") || m.contains("D-STAR") || m.contains("DSTAR") || m.contains("C4FM"))
}

fn provenance(rec: &RbRecord) -> String {
    let kind = if rec.kind == "csv" {
        "RepeaterBook CSV"
    } else {
        "RepeaterBook KML"
    };
    format!("{kind}: {}", rec.file_name)
}

fn best_osm_match(osm: &[RepeaterSite], rec: &RbRecord) -> Option<RepeaterSite> {
    let mut best: Option<(f64, RepeaterSite)> = None;
    for s in osm {
        let freq_d = (s.freq_out_mhz - rec.freq_out_mhz).abs();
        let pos_ok = match (s.lat, s.lon, rec.lat, rec.lon) {
            (Some(a), Some(b), Some(c), Some(d)) => haversine_km(a, b, c, d) <= DEDUP_DISTANCE_KM,
            (None, None, _, _) => true,
            (_, _, None, None) => freq_d <= FREQ_MATCH_MHZ,
            _ => freq_d <= FREQ_MATCH_MHZ,
        };
        if !pos_ok && freq_d > FREQ_MATCH_MHZ {
            continue;
        }
        if !pos_ok {
            continue;
        }
        let score = freq_d;
        if best.as_ref().map(|(d, _)| score < *d).unwrap_or(true) {
            best = Some((score, s.clone()));
        }
    }
    best.map(|(_, s)| s)
}

/// Read a drop directory. Unknown extensions are ignored. One bad file does not
/// stop the others. `fail_after` aborts before the atomic swap so the live DB
/// stays as it was.
pub fn import_directory(dir: &Path, live_db: &Path, fail_after: Option<usize>) -> ImportBatch {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) => {
            return ImportBatch {
                ok: false,
                kept_last_good: true,
                error: Some(format!("read dir: {e}")),
                ..ImportBatch::default()
            };
        }
    };
    for ent in rd.flatten() {
        let name = ent.file_name().to_string_lossy().to_string();
        let lower = name.to_ascii_lowercase();
        if !(lower.ends_with(".kml") || lower.ends_with(".kmz") || lower.ends_with(".csv")) {
            continue;
        }
        match std::fs::read(ent.path()) {
            Ok(bytes) => files.push((name, bytes)),
            Err(e) => files.push((format!("{name}\0{e}"), Vec::new())),
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    import_named_bytes(live_db, &files, fail_after)
}

pub fn import_named_bytes(
    live_db: &Path,
    files: &[(String, Vec<u8>)],
    fail_after: Option<usize>,
) -> ImportBatch {
    let parent = live_db.parent().unwrap_or_else(|| Path::new("."));
    let staging = parent.join("cat_repeaters.staging.sqlite");
    let _ = std::fs::remove_file(&staging);
    if live_db.exists() && std::fs::copy(live_db, &staging).is_err() {
        return ImportBatch {
            ok: false,
            kept_last_good: true,
            error: Some("staging copy failed".into()),
            ..ImportBatch::default()
        };
    }
    let db = match RepeaterDb::open_path(&staging) {
        Ok(db) => db,
        Err(e) => {
            let _ = std::fs::remove_file(&staging);
            return ImportBatch {
                ok: false,
                kept_last_good: true,
                error: Some(format!("open staging: {e}")),
                ..ImportBatch::default()
            };
        }
    };
    let _ = db.delete_sources(&[RepeaterSource::RepeaterBook]);
    let mut reports = Vec::new();
    for (i, (name, bytes)) in files.iter().enumerate() {
        if fail_after == Some(i) {
            let _ = std::fs::remove_file(&staging);
            return ImportBatch {
                ok: false,
                kept_last_good: true,
                files: reports,
                error: Some("injected failure; kept last good data".into()),
            };
        }
        if name.contains('\0') {
            reports.push(FileReport {
                file_name: name.split('\0').next().unwrap_or(name).into(),
                kind: "error".into(),
                error: Some("unreadable".into()),
                ..FileReport::default()
            });
            continue;
        }
        let lower = name.to_ascii_lowercase();
        let parsed = if lower.ends_with(".kmz") {
            parse_kmz_bytes(name, bytes)
        } else if lower.ends_with(".kml") {
            parse_kml_bytes(name, bytes)
        } else if lower.ends_with(".csv") {
            parse_csv_bytes(name, bytes)
        } else {
            continue;
        };
        match parsed {
            Ok(mut recs) => {
                let dropped = dedup_records(&mut recs);
                let mut report = merge_records(&db, &recs);
                report.file_name = name.clone();
                report.kind = if lower.ends_with(".csv") {
                    "csv".into()
                } else if lower.ends_with(".kmz") {
                    "kmz".into()
                } else {
                    "kml".into()
                };
                report.duplicates_dropped = dropped;
                reports.push(report);
            }
            Err(e) => reports.push(FileReport {
                file_name: name.clone(),
                kind: "error".into(),
                error: Some(e),
                ..FileReport::default()
            }),
        }
    }
    let bak = parent.join("cat_repeaters.sqlite.bak");
    if live_db.exists() {
        let _ = std::fs::rename(live_db, &bak);
    }
    if std::fs::rename(&staging, live_db).is_err() {
        if bak.exists() {
            let _ = std::fs::rename(&bak, live_db);
        }
        return ImportBatch {
            ok: false,
            kept_last_good: true,
            files: reports,
            error: Some("atomic swap failed".into()),
        };
    }
    let _ = std::fs::remove_file(&bak);
    ImportBatch {
        ok: true,
        kept_last_good: false,
        files: reports,
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::importers::import_osm_json;

    fn hoy_kml() -> String {
        r#"<?xml version="1.0" encoding="UTF-8"?>
<kml xmlns="http://www.opengis.net/kml/2.2"><Document>
<Placemark>
<name>LA5MR</name>
<description><![CDATA[
Hoykorset<br>
145.625000 88.5<br>
On-air: Yes<br>
<a href='https://www.repeaterbook.com/row_repeaters/details.php?state_id=NO&ID=1'>Click for Details</a>
]]></description>
<Point><coordinates>10.6018219,60.4781659,0</coordinates></Point>
</Placemark>
</Document></kml>"#
            .into()
    }

    #[test]
    fn kml_point_is_lon_lat() {
        let (lat, lon) = parse_kml_point("10.6018219,60.4781659,0").unwrap();
        assert!((lon - 10.6018219).abs() < 1e-9);
        assert!((lat - 60.4781659).abs() < 1e-9);
        assert!(parse_kml_point("0,0,0").is_err());
        assert!(parse_kml_point("10,95,0").is_err());
    }

    #[test]
    fn same_osm_data_merges_changed_tone_overrides_missing_is_added() {
        let db = RepeaterDb::open_memory().unwrap();
        import_osm_json(
            &db,
            r#"[{"callsign":"LA5MR","lat":60.4781659,"lon":10.6018219,"freq_out_mhz":145.625,"shift_mhz":-0.6,"ctcss_hz":88.5,"network":"LA5MR","modulation":"11K2F3E"}]"#,
        )
        .unwrap();
        let mut same = parse_kml_str("la5mr.kml", &hoy_kml()).unwrap();
        let report = merge_records(&db, &same);
        assert_eq!(report.merged_osm, 1);
        assert!(report.overrides.is_empty());
        assert_eq!(report.country.as_deref(), Some("NO"));

        let changed = hoy_kml().replace("88.5", "123.0");
        same = parse_kml_str("la5mr.kml", &changed).unwrap();
        let report = merge_records(&db, &same);
        assert_eq!(report.overrides.len(), 1);
        assert!(report.overrides[0].contains("ctcss"));
        let site = db
            .list_sites()
            .into_iter()
            .find(|s| s.callsign == "LA5MR")
            .unwrap();
        assert!((site.ctcss_hz.unwrap() - 123.0).abs() < 0.01);

        let added = r#"<?xml version="1.0"?>
<kml><Document><Placemark><name>LA5ARR</name>
<description><![CDATA[Tretten<br>434.775000 71.9<br>On-air: Yes<br>
<a href='https://www.repeaterbook.com/row_repeaters/details.php?state_id=NO&ID=2'>Click for Details</a>
]]></description>
<Point><coordinates>10.3113599,61.2803118,0</coordinates></Point>
</Placemark></Document></kml>"#;
        let recs = parse_kml_str("add.kml", added).unwrap();
        let report = merge_records(&db, &recs);
        assert_eq!(report.added, 1);
        let added_site = db
            .list_sites()
            .into_iter()
            .find(|s| s.callsign == "LA5ARR")
            .unwrap();
        assert_eq!(added_site.source, RepeaterSource::RepeaterBook);
        let attr = added_site.attribution.unwrap();
        assert!(attr.starts_with("RepeaterBook KML:"));
        assert!(!attr.contains('/'));
    }

    #[test]
    fn border_duplicate_is_dropped() {
        let a = parse_kml_str("no.kml", &hoy_kml()).unwrap();
        let b = parse_kml_str("se.kml", &hoy_kml().replace("state_id=NO", "state_id=SE")).unwrap();
        let mut all = a;
        all.extend(b);
        assert_eq!(dedup_records(&mut all), 1);
        assert_eq!(all.len(), 1);
    }

    #[test]
    fn malformed_kml_is_an_error_and_does_not_wipe_other_files() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("cat_repeaters.sqlite");
        {
            let db = RepeaterDb::open_path(&db_path).unwrap();
            import_osm_json(
                &db,
                r#"[{"callsign":"LA5MR","lat":60.4781659,"lon":10.6018219,"freq_out_mhz":145.625,"shift_mhz":-0.6,"ctcss_hz":88.5}]"#,
            )
            .unwrap();
        }
        let batch = import_named_bytes(
            &db_path,
            &[
                ("bad.kml".into(), b"<kml><Placemark><name>LA5MR".to_vec()),
                ("ok.kml".into(), hoy_kml().into_bytes()),
            ],
            None,
        );
        assert!(batch.ok, "{batch:?}");
        assert!(batch
            .files
            .iter()
            .any(|f| f.file_name == "bad.kml" && f.error.is_some()));
        assert!(batch
            .files
            .iter()
            .any(|f| f.file_name == "ok.kml" && f.error.is_none()));
        let db = RepeaterDb::open_path(&db_path).unwrap();
        assert!(db.list_sites().iter().any(|s| s.callsign == "LA5MR"));
    }

    #[test]
    fn injected_failure_keeps_the_live_db() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("cat_repeaters.sqlite");
        {
            let db = RepeaterDb::open_path(&db_path).unwrap();
            import_osm_json(
                &db,
                r#"[{"callsign":"LA5MR","lat":60.4781659,"lon":10.6018219,"freq_out_mhz":145.625,"shift_mhz":-0.6,"ctcss_hz":88.5}]"#,
            )
            .unwrap();
        }
        let batch = import_named_bytes(
            &db_path,
            &[("ok.kml".into(), hoy_kml().into_bytes())],
            Some(0),
        );
        assert!(!batch.ok);
        assert!(batch.kept_last_good);
        let db = RepeaterDb::open_path(&db_path).unwrap();
        let site = db
            .list_sites()
            .into_iter()
            .find(|s| s.callsign == "LA5MR")
            .unwrap();
        assert_eq!(site.source, RepeaterSource::Osm);
        assert!(site.attribution.is_none());
    }

    #[test]
    fn csv_coords_used_when_valid_and_matched_when_absent() {
        let db = RepeaterDb::open_memory().unwrap();
        import_osm_json(
            &db,
            r#"[{"callsign":"LA6GR","lat":60.5695830,"lon":10.9379810,"freq_out_mhz":434.75,"shift_mhz":-2.0,"ctcss_hz":71.9}]"#,
        )
        .unwrap();
        let with = b"Callsign,Frequency,Latitude,Longitude\nLA5ARR,434.775,61.2803118,10.3113599\n";
        let recs = parse_csv_bytes("with.csv", with).unwrap();
        assert!(recs[0].lat.is_some());
        let report = merge_records(&db, &recs);
        assert_eq!(report.added, 1);

        let without = b"Callsign,Frequency\nLA6GR,434.75\n";
        let recs = parse_csv_bytes("without.csv", without).unwrap();
        assert!(recs[0].lat.is_none());
        let report = merge_records(&db, &recs);
        assert_eq!(report.merged_osm, 1);

        let bad = b"Callsign,Frequency,Latitude,Longitude\nLA6GR,434.75,0.000,0.000\nLA5MR,145.625,91.000,10.000\n";
        let recs = parse_csv_bytes("bad.csv", bad).unwrap();
        assert!(recs.iter().all(|r| r.lat.is_none()));
        let report = merge_records(&db, &recs);
        assert!(report.csv_without_position >= 1);
        assert!(db
            .query_near(60.5, 10.5, 150.0, None)
            .iter()
            .all(|s| s.position_accurate));
    }

    #[test]
    fn innlandet_lists_la5mr_by_distance_and_radius_fallback_drops_it() {
        let xml = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../testdata/cat/osm/la5mr/relation_18780801_full.xml"
        ))
        .unwrap();
        let db = RepeaterDb::open_memory().unwrap();
        crate::importers::import_osm_relation_xml(&db, &xml).unwrap();
        let lillehammer = db.list_networks(61.115, 10.466, 150.0);
        let la = lillehammer
            .iter()
            .find(|n| n.name == "LA5MR")
            .expect("LA5MR");
        assert!(la.members >= 1, "members {}", la.members);
        let mut prev = 0.0;
        for n in &lillehammer {
            assert!(n.nearest_km + 1e-9 >= prev);
            prev = n.nearest_km;
        }
        eprintln!(
            "Innlandet LA5MR members={} nearest_km={:.1}",
            la.members, la.nearest_km
        );
        let outside = db.list_networks(52.0, 10.0, DEFAULT_REGION_RADIUS_KM);
        assert!(outside.iter().all(|n| n.name != "LA5MR"));
    }

    #[test]
    fn committed_fixtures_parse_lon_lat_and_drop_the_border_copy() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../testdata/cat/repeaterbook");
        let kml = std::fs::read(root.join("la5mr-osm.kml")).unwrap();
        let recs = parse_kml_bytes("la5mr-osm.kml", &kml).unwrap();
        let hoy = recs.iter().find(|r| r.callsign == "LA5MR").unwrap();
        assert!((hoy.lon.unwrap() - 10.6018219).abs() < 1e-6);
        assert!((hoy.lat.unwrap() - 60.4781659).abs() < 1e-6);
        let kmz = std::fs::read(root.join("la5mr-osm.kmz")).unwrap();
        let from_kmz = parse_kmz_bytes("la5mr-osm.kmz", &kmz).unwrap();
        assert_eq!(from_kmz.len(), recs.len());
        let no = parse_kml_bytes(
            "border-no.kml",
            &std::fs::read(root.join("border-no.kml")).unwrap(),
        )
        .unwrap();
        let se = parse_kml_bytes(
            "border-se.kml",
            &std::fs::read(root.join("border-se.kml")).unwrap(),
        )
        .unwrap();
        let mut both = no;
        both.extend(se);
        assert_eq!(dedup_records(&mut both), 1);
        assert!(parse_kml_bytes(
            "malformed.kml",
            &std::fs::read(root.join("malformed.kml")).unwrap()
        )
        .is_err());
    }

    #[test]
    fn local_import_does_not_name_an_http_client() {
        assert!(!online_mirror_source("repeaterbook"));
        assert!(!online_mirror_source("repeater_book"));
        assert!(online_mirror_source("radioid"));
    }

    #[test]
    fn real_kml_dir_when_present() {
        let mut candidates = Vec::new();
        if let Ok(p) = std::env::var("NAVI_RB_KML_DIR") {
            candidates.push(std::path::PathBuf::from(p));
        }
        candidates.push(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../klm"));
        let Some(dir) = candidates.into_iter().find(|p| p.is_dir()) else {
            eprintln!("skip real KML dir (set NAVI_RB_KML_DIR)");
            return;
        };
        let tmp = tempfile::tempdir().unwrap();
        let live = tmp.path().join("cat_repeaters.sqlite");
        let batch = import_directory(&dir, &live, None);
        assert!(batch.ok, "{batch:?}");
        for f in &batch.files {
            eprintln!(
                "{} kind={} country={:?} read={} imported={} err={:?}",
                f.file_name, f.kind, f.country, f.read, f.imported, f.error
            );
        }
        assert!(!batch.files.is_empty());
    }
}
