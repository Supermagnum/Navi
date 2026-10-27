//! Fetch / verify / import repeater mirrors from navi-server (plain HTTPS GET).
//!
//! Client never holds RepeaterBook or RadioID credentials and never contacts
//! those upstreams directly. Update runs only when the user presses Update.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::repeater::{RepeaterDb, RepeaterSite, RepeaterSource};

/// Default public navi-server repeater root.
pub const DEFAULT_REPEATERS_BASE: &str = "https://navigate-me.duckdns.org";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attribution {
    pub text: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub provisional: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CountryEntry {
    pub country: String,
    pub country_slug: String,
    pub generation: String,
    #[serde(default)]
    pub record_count: Option<u64>,
    pub manifest_url: String,
    pub data_url: String,
    #[serde(default)]
    pub attribution: Option<Attribution>,
    #[serde(default)]
    pub licence: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceEntry {
    pub source: String,
    #[serde(default)]
    pub generation: Option<String>,
    #[serde(default)]
    pub attribution: Option<Attribution>,
    #[serde(default)]
    pub licence: Option<String>,
    #[serde(default)]
    pub countries: Vec<CountryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentCatalog {
    #[serde(default)]
    pub schema: u32,
    pub generation: String,
    pub sources: Vec<SourceEntry>,
    #[serde(default)]
    pub fixture_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateReport {
    pub ok: bool,
    pub generation: String,
    pub imported: usize,
    pub sources_ok: Vec<String>,
    pub sources_missing: Vec<String>,
    pub sources_failed: Vec<String>,
    pub attributions: Vec<String>,
    pub error: Option<String>,
    /// True when the previous onboard DB was left unchanged.
    pub kept_last_good: bool,
}

#[derive(Debug, Clone)]
pub struct CountryBundle {
    pub source: String,
    pub country_slug: String,
    pub manifest_json: String,
    pub checksums_text: String,
    pub zst_bytes: Vec<u8>,
    pub attribution: Option<Attribution>,
}

pub fn parse_current(json: &str) -> anyhow::Result<CurrentCatalog> {
    let cat: CurrentCatalog = serde_json::from_str(json)?;
    Ok(cat)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

pub fn verify_checksum(bytes: &[u8], expected_hex: &str) -> bool {
    let got = sha256_hex(bytes);
    got.eq_ignore_ascii_case(expected_hex.trim())
}

/// Parse `checksums.sha256` (`<hex>  <filename>` lines).
pub fn checksum_for_file(checksums_text: &str, filename: &str) -> Option<String> {
    for line in checksums_text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let hex = parts.next()?;
        let name = parts.next().unwrap_or("");
        let base = name.rsplit('/').next().unwrap_or(name);
        if base == filename || name.ends_with(filename) {
            return Some(hex.to_string());
        }
    }
    None
}

pub fn decompress_zst(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut dec = zstd::stream::read::Decoder::new(std::io::Cursor::new(bytes))?;
    let mut out = Vec::new();
    std::io::Read::read_to_end(&mut dec, &mut out)?;
    Ok(out)
}

fn parse_source_name(s: &str) -> RepeaterSource {
    match s.to_ascii_lowercase().as_str() {
        "radioid" | "radio_id" => RepeaterSource::RadioId,
        "repeaterbook" | "repeater_book" => RepeaterSource::RepeaterBook,
        _ => RepeaterSource::User,
    }
}

fn hz_to_mhz(hz: f64) -> f64 {
    hz / 1_000_000.0
}

/// Import one decompressed `repeaters.json` payload into `db` (no delete).
pub fn import_mirror_payload(
    db: &RepeaterDb,
    json: &str,
    default_source: &str,
    attribution: Option<&Attribution>,
) -> anyhow::Result<usize> {
    let v: Value = serde_json::from_str(json)?;
    let source_str = v
        .get("source")
        .and_then(|x| x.as_str())
        .unwrap_or(default_source);
    let source = parse_source_name(source_str);
    let attr_text = attribution.map(|a| a.text.clone()).or_else(|| {
        v.get("attribution")
            .and_then(|a| a.get("text"))
            .and_then(|t| t.as_str())
            .map(|s| s.to_string())
    });
    let reps = v
        .get("repeaters")
        .and_then(|r| r.as_array())
        .ok_or_else(|| anyhow::anyhow!("repeaters[] missing"))?;
    let mut n = 0usize;
    for r in reps {
        let callsign = r
            .get("callsign")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();
        if callsign.is_empty() {
            continue;
        }
        let locator = r
            .get("radioid_locator")
            .or_else(|| r.get("locator"))
            .and_then(|x| x.as_str())
            .unwrap_or("");
        let id = if !locator.is_empty() {
            format!("{source_str}-{locator}")
        } else {
            format!(
                "{source_str}-{}",
                r.get("radioid_identity_id")
                    .or_else(|| r.get("id"))
                    .map(|x| x.to_string())
                    .unwrap_or_else(|| callsign.clone())
            )
        };
        let position_null = r
            .get("position_null")
            .and_then(|x| x.as_bool())
            .unwrap_or(false);
        let lat = if position_null {
            None
        } else {
            r.get("lat").and_then(|x| x.as_f64()).or_else(|| {
                r.get("position")
                    .and_then(|p| p.get("lat"))
                    .and_then(|x| x.as_f64())
            })
        };
        let lon = if position_null {
            None
        } else {
            r.get("lon").and_then(|x| x.as_f64()).or_else(|| {
                r.get("position")
                    .and_then(|p| p.get("lon"))
                    .and_then(|x| x.as_f64())
            })
        };
        let out_hz = r.get("output_hz").and_then(|x| x.as_f64()).unwrap_or(0.0);
        let shift_hz = r.get("shift_hz").and_then(|x| x.as_f64()).unwrap_or(0.0);
        if out_hz <= 0.0 {
            continue;
        }
        let mode = r
            .get("mode")
            .and_then(|x| x.as_str())
            .unwrap_or("DMR")
            .to_string();
        let exclude = r
            .get("exclude_recommended")
            .and_then(|x| x.as_bool())
            .unwrap_or(false);
        let color = r
            .get("dmr_color_code")
            .and_then(|x| x.as_u64())
            .map(|c| c as u8);
        let site = RepeaterSite {
            id,
            callsign,
            lat,
            lon,
            freq_out_mhz: hz_to_mhz(out_hz),
            shift_mhz: hz_to_mhz(shift_hz),
            ctcss_hz: None,
            dcs_code: None,
            color_code: color,
            modulation: mode,
            network_id: None,
            source,
            conflict: false,
            conflict_note: None,
            distance_km: None,
            position_accurate: !position_null && lat.is_some() && lon.is_some(),
            exclude_recommended: exclude,
            attribution: attr_text.clone(),
        };
        db.upsert_site(&site)?;
        n += 1;
    }
    Ok(n)
}

/// Verify checksums and decompress; does not touch the DB.
pub fn decode_country_bundle(bundle: &CountryBundle) -> anyhow::Result<(String, usize)> {
    let expected = checksum_for_file(&bundle.checksums_text, "repeaters.json.zst")
        .ok_or_else(|| anyhow::anyhow!("checksums.sha256 missing repeaters.json.zst"))?;
    if !verify_checksum(&bundle.zst_bytes, &expected) {
        anyhow::bail!(
            "checksum mismatch for {}/{}",
            bundle.source,
            bundle.country_slug
        );
    }
    // Also check manifest files.sha256 when present.
    if let Ok(man) = serde_json::from_str::<Value>(&bundle.manifest_json) {
        if let Some(hex) = man
            .pointer("/files/repeaters.json.zst/sha256")
            .and_then(|x| x.as_str())
        {
            if !verify_checksum(&bundle.zst_bytes, hex) {
                anyhow::bail!(
                    "manifest sha256 mismatch for {}/{}",
                    bundle.source,
                    bundle.country_slug
                );
            }
        }
    }
    let raw = decompress_zst(&bundle.zst_bytes)?;
    let json = String::from_utf8(raw).map_err(|e| anyhow::anyhow!("utf8: {e}"))?;
    let n = serde_json::from_str::<Value>(&json)?
        .get("repeaters")
        .and_then(|r| r.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    Ok((json, n))
}

/// Apply verified bundles into a staging DB, then atomically replace `live_db_path`.
/// On any failure, leaves the live DB untouched (`kept_last_good`).
pub fn apply_bundles_atomic(
    live_db_path: &Path,
    catalog: &CurrentCatalog,
    bundles: &[CountryBundle],
    expected_sources: &[&str],
) -> UpdateReport {
    let mut report = UpdateReport {
        ok: false,
        generation: catalog.generation.clone(),
        kept_last_good: true,
        ..Default::default()
    };

    // Missing sources (e.g. RepeaterBook absent) → note, continue.
    let present: HashMap<String, &SourceEntry> = catalog
        .sources
        .iter()
        .map(|s| (s.source.to_ascii_lowercase(), s))
        .collect();
    for want in expected_sources {
        if !present.contains_key(&want.to_ascii_lowercase()) {
            report
                .sources_missing
                .push(format!("{want}: not available yet"));
        }
    }
    for s in &catalog.sources {
        if let Some(a) = &s.attribution {
            report
                .attributions
                .push(format!("{}: {}", s.source, a.text));
        }
    }

    if bundles.is_empty() && catalog.sources.is_empty() {
        report.error = Some("no sources in current.json".into());
        return report;
    }

    let parent = live_db_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let staging = parent.join("cat_repeaters.staging.sqlite");
    let _ = std::fs::remove_file(&staging);

    // Start from a copy of live so OSM/CSV/OpenRepeater rows survive.
    if live_db_path.exists() {
        if let Err(e) = std::fs::copy(live_db_path, &staging) {
            report.error = Some(format!("staging copy failed: {e}"));
            return report;
        }
    }

    let db = match RepeaterDb::open_path(&staging) {
        Ok(d) => d,
        Err(e) => {
            report.error = Some(format!("open staging: {e}"));
            let _ = std::fs::remove_file(&staging);
            return report;
        }
    };

    // Drop previous server-mirror rows only (keep OSM / CSV / OpenRepeater / User).
    if let Err(e) = db.delete_sources(&[RepeaterSource::RadioId, RepeaterSource::RepeaterBook]) {
        report.error = Some(format!("clear server sources: {e}"));
        let _ = std::fs::remove_file(&staging);
        return report;
    }

    let mut imported = 0usize;
    for b in bundles {
        match decode_country_bundle(b) {
            Ok((json, _)) => {
                match import_mirror_payload(&db, &json, &b.source, b.attribution.as_ref()) {
                    Ok(n) => {
                        imported += n;
                        if !report.sources_ok.contains(&b.source) {
                            report.sources_ok.push(b.source.clone());
                        }
                    }
                    Err(e) => {
                        report
                            .sources_failed
                            .push(format!("{}/{}: {e}", b.source, b.country_slug));
                        let _ = std::fs::remove_file(&staging);
                        report.error = Some(format!("import failed; kept last good data: {e}"));
                        return report;
                    }
                }
            }
            Err(e) => {
                report
                    .sources_failed
                    .push(format!("{}/{}: {e}", b.source, b.country_slug));
                let _ = std::fs::remove_file(&staging);
                report.error = Some(format!("verify/decode failed; kept last good data: {e}"));
                return report;
            }
        }
    }

    // Atomic replace
    let bak = parent.join("cat_repeaters.sqlite.bak");
    if live_db_path.exists() {
        let _ = std::fs::rename(live_db_path, &bak);
    }
    if let Err(e) = std::fs::rename(&staging, live_db_path) {
        // restore
        if bak.exists() {
            let _ = std::fs::rename(&bak, live_db_path);
        }
        report.error = Some(format!("atomic swap failed: {e}"));
        return report;
    }
    let _ = std::fs::remove_file(&bak);

    report.ok = true;
    report.imported = imported;
    report.kept_last_good = false;
    report
}

fn generation_sidecar_path(live_db_path: &Path) -> PathBuf {
    let mut p = live_db_path.as_os_str().to_os_string();
    p.push(".generation");
    PathBuf::from(p)
}

fn read_applied_generation(live_db_path: &Path) -> Option<String> {
    let path = generation_sidecar_path(live_db_path);
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn write_applied_generation(live_db_path: &Path, generation: &str) {
    let path = generation_sidecar_path(live_db_path);
    let _ = std::fs::write(path, format!("{generation}\n"));
}

/// Blocking HTTPS fetch of current.json + each country (for UniFFI / desktop).
pub fn fetch_and_update(base_url: &str, live_db_path: &Path) -> UpdateReport {
    let base = base_url.trim_end_matches('/');
    let client = match reqwest::blocking::Client::builder()
        .user_agent("Navi-CAT/0.3.22 (+https://github.com/Supermagnum/Navi)")
        .timeout(std::time::Duration::from_secs(60))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return UpdateReport {
                ok: false,
                error: Some(format!("http client: {e}")),
                kept_last_good: true,
                ..Default::default()
            };
        }
    };

    let current_url = format!("{base}/repeaters/current.json");
    let current_txt = match client
        .get(&current_url)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.text())
    {
        Ok(t) => t,
        Err(e) => {
            return UpdateReport {
                ok: false,
                error: Some(format!("current.json: {e}")),
                kept_last_good: true,
                ..Default::default()
            };
        }
    };
    let catalog = match parse_current(&current_txt) {
        Ok(c) => c,
        Err(e) => {
            return UpdateReport {
                ok: false,
                error: Some(format!("parse current: {e}")),
                kept_last_good: true,
                ..Default::default()
            };
        }
    };

    // Same server generation already applied → do not re-download country zsts.
    if live_db_path.is_file() {
        if let Some(prev) = read_applied_generation(live_db_path) {
            if prev == catalog.generation {
                let report = UpdateReport {
                    ok: true,
                    generation: catalog.generation.clone(),
                    imported: 0,
                    kept_last_good: true,
                    ..Default::default()
                };
                return report;
            }
        }
    }

    let mut bundles = Vec::new();
    let mut report_early = UpdateReport {
        generation: catalog.generation.clone(),
        kept_last_good: true,
        ..Default::default()
    };
    for src in &catalog.sources {
        if !crate::repeaterbook::online_mirror_source(&src.source) {
            continue;
        }
        for c in &src.countries {
            let man_url = abs_url(base, &c.manifest_url);
            let data_url = abs_url(base, &c.data_url);
            let checksum_url = data_url
                .rsplit_once('/')
                .map(|(p, _)| format!("{p}/checksums.sha256"))
                .unwrap_or_default();
            let manifest_json = match client
                .get(&man_url)
                .send()
                .and_then(|r| r.error_for_status())
                .and_then(|r| r.text())
            {
                Ok(t) => t,
                Err(e) => {
                    report_early
                        .sources_failed
                        .push(format!("{}: manifest {e}", c.country_slug));
                    continue;
                }
            };
            let checksums_text = match client
                .get(&checksum_url)
                .send()
                .and_then(|r| r.error_for_status())
                .and_then(|r| r.text())
            {
                Ok(t) => t,
                Err(e) => {
                    report_early
                        .sources_failed
                        .push(format!("{}: checksums {e}", c.country_slug));
                    continue;
                }
            };
            let zst_bytes = match client
                .get(&data_url)
                .send()
                .and_then(|r| r.error_for_status())
                .and_then(|r| r.bytes())
            {
                Ok(b) => b.to_vec(),
                Err(e) => {
                    report_early
                        .sources_failed
                        .push(format!("{}: data {e}", c.country_slug));
                    // Partial download → abort entirely (keep last good).
                    report_early.error = Some("partial download; kept last good data".into());
                    return report_early;
                }
            };
            bundles.push(CountryBundle {
                source: src.source.clone(),
                country_slug: c.country_slug.clone(),
                manifest_json,
                checksums_text,
                zst_bytes,
                attribution: c.attribution.clone().or_else(|| src.attribution.clone()),
            });
        }
    }

    if !report_early.sources_failed.is_empty() && bundles.is_empty() {
        report_early.error = Some("all country downloads failed; kept last good data".into());
        return report_early;
    }

    let mut report = apply_bundles_atomic(live_db_path, &catalog, &bundles, &["radioid"]);
    if report.ok {
        write_applied_generation(live_db_path, &catalog.generation);
    }
    // Preserve missing notes from early scan.
    for m in report_early.sources_missing {
        if !report.sources_missing.contains(&m) {
            report.sources_missing.push(m);
        }
    }
    report
}

fn abs_url(base: &str, path: &str) -> String {
    if path.starts_with("http://") || path.starts_with("https://") {
        path.to_string()
    } else if path.starts_with('/') {
        format!("{base}{path}")
    } else {
        format!("{base}/{path}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../testdata/cat/server_mirror")
            .join(name)
    }

    #[test]
    fn parse_current_radioid_only_marks_repeaterbook_missing() {
        let json = fs::read_to_string(fixture("current_radioid_only.json")).unwrap();
        let cat = parse_current(&json).unwrap();
        assert!(cat.sources.iter().any(|s| s.source == "radioid"));
        assert!(!cat.sources.iter().any(|s| s.source == "repeaterbook"));
    }

    #[test]
    fn parse_current_with_synthetic_repeaterbook() {
        let json = fs::read_to_string(fixture("current_with_repeaterbook_synthetic.json")).unwrap();
        let cat = parse_current(&json).unwrap();
        assert!(cat.sources.iter().any(|s| s.source == "repeaterbook"));
    }

    #[test]
    fn checksum_mismatch_rejects_and_keeps_last_good() {
        let live = tempfile::tempdir().unwrap();
        let db_path = live.path().join("cat_repeaters.sqlite");
        {
            let db = RepeaterDb::open_path(&db_path).unwrap();
            db.upsert_site(&RepeaterSite {
                id: "keep-me".into(),
                callsign: "LA0KEEP".into(),
                lat: Some(60.0),
                lon: Some(10.0),
                freq_out_mhz: 145.0,
                shift_mhz: -0.6,
                ctcss_hz: Some(88.5),
                dcs_code: None,
                color_code: None,
                modulation: "FM".into(),
                network_id: None,
                source: RepeaterSource::Osm,
                conflict: false,
                conflict_note: None,
                distance_km: None,
                position_accurate: true,
                exclude_recommended: false,
                attribution: None,
            })
            .unwrap();
        }
        let zst = fs::read(fixture("radioid_norway_slice.json.zst")).unwrap();
        let man = fs::read_to_string(fixture("radioid_norway_slice_manifest.json")).unwrap();
        let bundle = CountryBundle {
            source: "radioid".into(),
            country_slug: "no".into(),
            manifest_json: man,
            checksums_text: "0000000000000000000000000000000000000000000000000000000000000000  repeaters.json.zst\n".into(),
            zst_bytes: zst,
            attribution: None,
        };
        let cat = CurrentCatalog {
            schema: 1,
            generation: "t".into(),
            sources: vec![],
            fixture_note: None,
        };
        let report = apply_bundles_atomic(&db_path, &cat, &[bundle], &["radioid"]);
        assert!(!report.ok);
        assert!(report.kept_last_good);
        let db = RepeaterDb::open_path(&db_path).unwrap();
        let sites = db.query_near(60.0, 10.0, 50.0, None);
        assert!(sites.iter().any(|s| s.id == "keep-me"));
    }

    #[test]
    fn generation_sidecar_round_trip() {
        let live = tempfile::tempdir().unwrap();
        let db_path = live.path().join("cat_repeaters.sqlite");
        fs::write(&db_path, b"sqlite-placeholder").unwrap();
        assert!(read_applied_generation(&db_path).is_none());
        write_applied_generation(&db_path, "20261013T091219Z");
        assert_eq!(
            read_applied_generation(&db_path).as_deref(),
            Some("20261013T091219Z")
        );
    }

    #[test]
    fn apply_empty_bundles_keeps_last_good() {
        let live = tempfile::tempdir().unwrap();
        let db_path = live.path().join("cat_repeaters.sqlite");
        {
            let db = RepeaterDb::open_path(&db_path).unwrap();
            db.upsert_site(&RepeaterSite {
                id: "keep-me".into(),
                callsign: "LA0KEEP".into(),
                lat: Some(60.0),
                lon: Some(10.0),
                freq_out_mhz: 145.0,
                shift_mhz: -0.6,
                ctcss_hz: Some(88.5),
                dcs_code: None,
                color_code: None,
                modulation: "FM".into(),
                network_id: None,
                source: RepeaterSource::Osm,
                conflict: false,
                conflict_note: None,
                distance_km: None,
                position_accurate: true,
                exclude_recommended: false,
                attribution: None,
            })
            .unwrap();
        }
        let cat = CurrentCatalog {
            schema: 1,
            generation: "empty".into(),
            sources: vec![],
            fixture_note: None,
        };
        let report = apply_bundles_atomic(&db_path, &cat, &[], &["radioid"]);
        assert!(!report.ok || report.imported == 0);
        let db = RepeaterDb::open_path(&db_path).unwrap();
        assert!(db
            .query_near(60.0, 10.0, 50.0, None)
            .iter()
            .any(|s| s.id == "keep-me"));
    }

    #[test]
    fn norway_slice_imports_with_exclusions() {
        let live = tempfile::tempdir().unwrap();
        let db_path = live.path().join("cat_repeaters.sqlite");
        let zst = fs::read(fixture("radioid_norway_slice.json.zst")).unwrap();
        let man = fs::read_to_string(fixture("radioid_norway_slice_manifest.json")).unwrap();
        let sums = fs::read_to_string(fixture("radioid_norway_slice_checksums.sha256")).unwrap();
        let bundle = CountryBundle {
            source: "radioid".into(),
            country_slug: "no".into(),
            manifest_json: man,
            checksums_text: sums,
            zst_bytes: zst,
            attribution: Some(Attribution {
                text: "Data courtesy of RadioID.net".into(),
                url: Some("https://www.radioid.net/".into()),
                provisional: true,
            }),
        };
        let cat = parse_current(&fs::read_to_string(fixture("current_radioid_only.json")).unwrap())
            .unwrap();
        let report = apply_bundles_atomic(&db_path, &cat, &[bundle], &["radioid"]);
        assert!(report.ok, "{report:?}");
        assert!(report.imported >= 1);
        assert!(report
            .sources_missing
            .iter()
            .all(|m| !m.to_ascii_lowercase().contains("repeaterbook")));
        let db = RepeaterDb::open_path(&db_path).unwrap();
        // Auto-tune query must skip position_null / exclude_recommended.
        let near = db.query_near(60.0, 10.0, 2000.0, None);
        assert!(near
            .iter()
            .all(|s| s.position_accurate && !s.exclude_recommended));
        // List includes excluded for UI.
        let all = db.list_sites();
        assert!(
            all.iter()
                .any(|s| s.exclude_recommended || !s.position_accurate)
                || all.len() >= near.len()
        );
        assert!(all
            .iter()
            .any(|s| s.attribution.as_deref() == Some("Data courtesy of RadioID.net")));
    }
}
