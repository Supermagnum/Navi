//! Offline attribution store (OSM ODbL, CoMaps, Mapterhorn, Navi, …).

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use thiserror::Error;

use super::storage::{attribution_json_path, attribution_md_path};

#[derive(Debug, Error)]
pub enum AttributionError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone)]
pub struct AttributionStore {
    pub json_path: PathBuf,
    pub md_path: PathBuf,
    pub json: Value,
    pub markdown: String,
}

impl AttributionStore {
    pub fn load_from_release(release_dir: &Path) -> Result<Self, AttributionError> {
        let json_path = attribution_json_path(release_dir);
        let md_path = attribution_md_path(release_dir);
        let json_text = fs::read_to_string(&json_path)?;
        let json: Value = serde_json::from_str(&json_text)?;
        let markdown = if md_path.is_file() {
            fs::read_to_string(&md_path)?
        } else {
            String::new()
        };
        Ok(Self {
            json_path,
            md_path,
            json,
            markdown,
        })
    }

    /// Credits suitable for an About / Licences screen.
    pub fn licence_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(osm) = self.json.get("osm") {
            if let Some(a) = osm.get("attribution").and_then(|v| v.as_str()) {
                lines.push(a.to_string());
            }
            if let Some(lic) = osm.get("license").and_then(|v| v.as_str()) {
                lines.push(format!("OpenStreetMap licence: {lic}"));
            }
        }
        if let Some(comaps) = self.json.get("comaps") {
            if let Some(arr) = comaps.get("copyright").and_then(|v| v.as_array()) {
                for c in arr {
                    if let Some(s) = c.as_str() {
                        lines.push(s.to_string());
                    }
                }
            }
        }
        if let Some(mh) = self.json.get("mapterhorn") {
            if let Some(label) = mh.get("label").and_then(|v| v.as_str()) {
                lines.push(format!("{label} elevation sources (see attribution.json)"));
            }
        }
        if let Some(navi) = self.json.get("navi") {
            if let Some(label) = navi.get("label").and_then(|v| v.as_str()) {
                let note = navi
                    .get("note")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Navi sidecars");
                lines.push(format!("{label}: {note}"));
            }
        }
        // DATEX / Statens vegvesen may appear under navi extensions or companion
        // packs; surface any top-level string notes that mention them.
        if let Some(datex) = self.json.get("datex") {
            lines.push(format!("DATEX: {datex}"));
        }
        if lines.is_empty() && !self.markdown.is_empty() {
            lines.push(self.markdown.lines().take(8).collect::<Vec<_>>().join("\n"));
        }
        lines
    }
}

/// Write attribution payloads into a release or staging directory.
pub fn store_attribution_files(
    release_or_staging: &Path,
    attribution_json: &str,
    attribution_md: Option<&str>,
) -> Result<(), AttributionError> {
    fs::create_dir_all(release_or_staging)?;
    // Validate JSON before writing.
    let _: Value = serde_json::from_str(attribution_json)?;
    fs::write(attribution_json_path(release_or_staging), attribution_json)?;
    if let Some(md) = attribution_md {
        fs::write(attribution_md_path(release_or_staging), md)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn stores_and_exposes_licences() {
        let dir = tempdir().unwrap();
        let json = r#"{
          "schema":"navi-mwm-attribution/v1",
          "osm":{"license":"ODbL","attribution":"© OpenStreetMap contributors"},
          "comaps":{"copyright":["Map packaging by CoMaps"]},
          "mapterhorn":{"label":"Mapterhorn"},
          "navi":{"label":"Navi","note":"sidecars"}
        }"#;
        store_attribution_files(dir.path(), json, Some("# Attribution\n")).unwrap();
        let store = AttributionStore::load_from_release(dir.path()).unwrap();
        let lines = store.licence_lines();
        assert!(lines.iter().any(|l| l.contains("OpenStreetMap")));
        assert!(lines.iter().any(|l| l.contains("CoMaps")));
        assert!(lines.iter().any(|l| l.contains("Mapterhorn")));
        assert!(lines.iter().any(|l| l.contains("Navi")));
    }
}
