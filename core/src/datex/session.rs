//! Sticky host session + on-disk snapshot cache for DATEX network economy.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::pack_server::PackDataSource;

use super::fetch::DatexSourceMeta;

const CACHE_META_FILE: &str = "datex-cache.json";
const CACHE_XML_FILE: &str = "datex-GetSituation.xml";

/// Process-wide DATEX session (sticky host + last poll / fingerprint).
#[derive(Debug, Default)]
pub struct DatexSession {
    /// Known-good host for this process; cleared when that hop fails.
    pub sticky: Option<(PackDataSource, String)>,
    pub last_fetch_unix: Option<i64>,
    /// Fingerprint of last successful `source.json` body (skip XML if unchanged).
    pub source_fingerprint: Option<String>,
    pub cached_xml: Option<String>,
    pub cached_meta: Option<DatexSourceMeta>,
    /// Test/diagnostics: how many times the full discovery chain ran.
    pub chain_probe_count: u32,
    /// Test/diagnostics: how many times sticky host was reused without chain.
    pub sticky_reuse_count: u32,
}

impl DatexSession {
    pub fn clear_sticky(&mut self) {
        self.sticky = None;
    }

    pub fn remember_host(&mut self, source: PackDataSource, base: String) {
        self.sticky = Some((source, base));
    }
}

static SESSION: Mutex<DatexSession> = Mutex::new(DatexSession {
    sticky: None,
    last_fetch_unix: None,
    source_fingerprint: None,
    cached_xml: None,
    cached_meta: None,
    chain_probe_count: 0,
    sticky_reuse_count: 0,
});

pub fn with_session<R>(f: impl FnOnce(&mut DatexSession) -> R) -> R {
    let mut guard = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut guard)
}

/// Reset session state (unit tests).
pub fn reset_session_for_tests() {
    with_session(|s| *s = DatexSession::default());
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiskCacheMeta {
    fetched_unix: i64,
    source_fingerprint: String,
    data_source: String,
    base_url: String,
    attribution: Option<String>,
    source: Option<String>,
}

pub fn cache_paths(dir: &Path) -> (PathBuf, PathBuf) {
    (dir.join(CACHE_META_FILE), dir.join(CACHE_XML_FILE))
}

pub fn load_disk_cache(dir: &Path) -> Option<(DatexSourceMeta, String, i64, String)> {
    let (meta_path, xml_path) = cache_paths(dir);
    let xml = fs::read_to_string(&xml_path).ok()?;
    if xml.trim().is_empty() {
        return None;
    }
    // Campaign ADB injects may write XML (+ apply stamp) without datex-cache.json.
    // Detect navi-synth fixtures from XML so refresh_for_route keeps them.
    let synth_xml =
        xml.contains("navi-synth") || xml.contains("<naviSynthetic>true</naviSynthetic>");
    if let Ok(meta_text) = fs::read_to_string(&meta_path) {
        if let Ok(disk) = serde_json::from_str::<DiskCacheMeta>(&meta_text) {
            let fp = if disk.source_fingerprint.starts_with("navi-synth") {
                disk.source_fingerprint
            } else if synth_xml {
                "navi-synth-disk-xml".into()
            } else {
                disk.source_fingerprint
            };
            let meta = DatexSourceMeta {
                schema: Some(1),
                source: disk.source,
                license: None,
                attribution: disk.attribution,
                endpoints: None,
            };
            return Some((meta, xml, disk.fetched_unix, fp));
        }
    }
    if synth_xml {
        let meta = DatexSourceMeta {
            schema: Some(1),
            source: Some("navi-synth".into()),
            license: None,
            attribution: Some("synthetic".into()),
            endpoints: None,
        };
        return Some((meta, xml, 0, "navi-synth-disk-xml".into()));
    }
    None
}

pub fn is_navi_synth_fingerprint(fp: &str) -> bool {
    fp.starts_with("navi-synth")
}

pub fn save_disk_cache(
    dir: &Path,
    meta: &DatexSourceMeta,
    xml: &str,
    fetched_unix: i64,
    fingerprint: &str,
    data_source: PackDataSource,
    base_url: &str,
) -> Result<(), String> {
    // Live NPRA / duckdns must never wipe a campaign ADB navi-synth fixture.
    if !is_navi_synth_fingerprint(fingerprint) {
        if let Some((_, _, _, existing_fp)) = load_disk_cache(dir) {
            if is_navi_synth_fingerprint(&existing_fp) {
                log::info!(
                    target: "NaviDatex",
                    "refusing disk overwrite of {existing_fp} with live fingerprint {fingerprint}"
                );
                return Ok(());
            }
        }
    }
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let (meta_path, xml_path) = cache_paths(dir);
    let partial_xml = xml_path.with_extension("xml.partial");
    fs::write(&partial_xml, xml).map_err(|e| e.to_string())?;
    fs::rename(&partial_xml, &xml_path).map_err(|e| e.to_string())?;

    let disk = DiskCacheMeta {
        fetched_unix,
        source_fingerprint: fingerprint.to_string(),
        data_source: data_source.as_str().to_string(),
        base_url: base_url.to_string(),
        attribution: meta.attribution.clone(),
        source: meta.source.clone(),
    };
    let meta_json = serde_json::to_string_pretty(&disk).map_err(|e| e.to_string())?;
    let partial_meta = meta_path.with_extension("json.partial");
    fs::write(&partial_meta, meta_json).map_err(|e| e.to_string())?;
    fs::rename(&partial_meta, &meta_path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Simple stable fingerprint of source.json (no crypto dep required).
pub fn fingerprint_source_body(body: &str) -> String {
    // FNV-1a 64-bit over UTF-8 bytes — enough to detect source.json changes.
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in body.as_bytes() {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn navi_synth_xml_without_meta_loads() {
        let dir = tempdir().unwrap();
        let xml = r#"<?xml version="1.0"?><d2LogicalModel><publicationCreator><nationalIdentifier>navi-synth</nationalIdentifier></publicationCreator><naviSynthetic>true</naviSynthetic></d2LogicalModel>"#;
        fs::write(dir.path().join("datex-GetSituation.xml"), xml).unwrap();
        let loaded = load_disk_cache(dir.path()).expect("synth xml");
        assert!(loaded.3.starts_with("navi-synth"), "fp={}", loaded.3);
    }

    #[test]
    fn save_disk_cache_refuses_live_overwrite_of_navi_synth() {
        let dir = tempdir().unwrap();
        let synth_xml = r#"<?xml version="1.0"?><d2LogicalModel><publicationCreator><nationalIdentifier>navi-synth</nationalIdentifier></publicationCreator><naviSynthetic>true</naviSynthetic><keep>synth</keep></d2LogicalModel>"#;
        let synth_meta = DatexSourceMeta {
            schema: Some(1),
            source: Some("navi-synth".into()),
            license: None,
            attribution: Some("synthetic".into()),
            endpoints: None,
        };
        save_disk_cache(
            dir.path(),
            &synth_meta,
            synth_xml,
            1_700_000_000,
            "navi-synth-elsa",
            PackDataSource::ServerDuckdns,
            "https://example.test",
        )
        .unwrap();

        let live_xml = r#"<?xml version="1.0"?><d2LogicalModel><live>npra</live></d2LogicalModel>"#;
        let live_meta = DatexSourceMeta {
            schema: Some(1),
            source: Some("NPRA".into()),
            license: None,
            attribution: Some("NPRA".into()),
            endpoints: None,
        };
        save_disk_cache(
            dir.path(),
            &live_meta,
            live_xml,
            1_700_000_100,
            "abcdef0123456789",
            PackDataSource::ServerDuckdns,
            "https://navigate-me.duckdns.org",
        )
        .unwrap();

        let loaded = load_disk_cache(dir.path()).expect("still synth");
        assert_eq!(loaded.3, "navi-synth-elsa");
        assert!(loaded.1.contains("keep>synth"), "xml wiped by live save");
    }
}
