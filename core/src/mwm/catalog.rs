//! `navi-mwm-catalog/v2` parser.

use std::collections::BTreeMap;

use serde::Deserialize;
use thiserror::Error;

/// Normative schema id.
pub const SCHEMA_V2: &str = "navi-mwm-catalog/v2";

/// Live release `261105` ships 224 country groups with 1159 routable leaves
/// (World + WorldCoasts are non-routable overlays).
pub const COUNTRY_GROUPS_ROUTABLE_TARGET: usize = 1159;

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("catalog json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported catalog schema {0:?}; update the app")]
    UnsupportedSchema(String),
    #[error("catalog missing required field: {0}")]
    Missing(&'static str),
}

#[derive(Debug, Clone, Deserialize)]
pub struct MwmCatalog {
    pub schema: String,
    pub mode: String,
    pub version: String,
    pub comaps: ComapsMeta,
    #[serde(default)]
    pub generator: Option<serde_json::Value>,
    #[serde(default)]
    pub limitation: Option<String>,
    pub leaves: Vec<CatalogLeaf>,
    #[serde(default)]
    pub region_to_leaves: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub shared_leaves: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub country_groups: Vec<CountryGroup>,
    #[serde(default)]
    pub country_groups_url: Option<String>,
    #[serde(default)]
    pub display_name_overrides: DisplayNameOverrides,
    #[serde(default)]
    pub region_migration_url: Option<String>,
    #[serde(default)]
    pub world_leaf_count: Option<u64>,
    #[serde(default)]
    pub elevation_policy_version: Option<u32>,
    #[serde(default)]
    pub non_routable_leaves: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ComapsMeta {
    pub map_series: String,
    pub release_version: u64,
    pub countries_txt_sha256: String,
    pub countries_txt_bytes: u64,
    #[serde(default)]
    pub countries_txt_sig_verified: Option<bool>,
    #[serde(default)]
    pub countries_txt_sig_sha256: Option<String>,
    pub countries_txt_pubkey_hex: String,
    pub metaserver_url: String,
    #[serde(default)]
    pub maps_json_url: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CatalogLeaf {
    pub id: String,
    #[serde(default)]
    pub sidecar_url: Option<String>,
    #[serde(default)]
    pub sidecar_bytes: Option<u64>,
    #[serde(default)]
    pub sidecar_sha256: Option<String>,
    #[serde(default)]
    pub comaps_file: Option<String>,
    #[serde(default)]
    pub comaps_bytes: Option<u64>,
    #[serde(default)]
    pub comaps_sha1_base64: Option<String>,
    #[serde(default)]
    pub mwm_bytes: Option<u64>,
    #[serde(default)]
    pub mwm_sha256: Option<String>,
    #[serde(default)]
    pub mwm_fingerprint: Option<MwmFingerprint>,
    #[serde(default)]
    pub shared: bool,
    #[serde(default)]
    pub shared_with_regions: Vec<String>,
    #[serde(default)]
    pub no_sidecar: bool,
    #[serde(default)]
    pub elevation_policy_version: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MwmFingerprint {
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct DisplayNameOverrides {
    #[serde(default)]
    pub leaves: BTreeMap<String, DisplayNameOverride>,
    #[serde(default)]
    pub likely_outdated_elsewhere: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DisplayNameOverride {
    pub display_name: String,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CountryGroup {
    pub id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub parent_groups: Vec<String>,
    #[serde(default)]
    pub leaf_count: Option<u64>,
    #[serde(default)]
    pub leaves: Vec<CountryGroupLeaf>,
    #[serde(default)]
    pub subgroups: Vec<CountryGroup>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CountryGroupLeaf {
    pub id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub comaps_display_name: Option<String>,
    #[serde(default)]
    pub display_name_overridden: Option<bool>,
    #[serde(default)]
    pub comaps_bytes: Option<u64>,
    #[serde(default)]
    pub mwm_bytes: Option<u64>,
    #[serde(default)]
    pub sidecar_bytes: Option<u64>,
    #[serde(default)]
    pub total_bytes: Option<u64>,
}

impl CatalogLeaf {
    pub fn expects_sidecar(&self) -> bool {
        !self.no_sidecar
    }

    pub fn fingerprint(&self) -> Option<MwmFingerprint> {
        if let Some(fp) = &self.mwm_fingerprint {
            return Some(fp.clone());
        }
        match (self.mwm_bytes, self.mwm_sha256.as_ref()) {
            (Some(size), Some(sha256)) => Some(MwmFingerprint {
                size,
                sha256: sha256.clone(),
            }),
            _ => None,
        }
    }

    pub fn comaps_file_name(&self) -> String {
        self.comaps_file
            .clone()
            .unwrap_or_else(|| format!("{}.mwm", self.id))
    }
}

impl MwmCatalog {
    pub fn validate_schema(&self) -> Result<(), CatalogError> {
        if self.schema != SCHEMA_V2 {
            return Err(CatalogError::UnsupportedSchema(self.schema.clone()));
        }
        if self.version.is_empty() {
            return Err(CatalogError::Missing("version"));
        }
        if self.comaps.metaserver_url.is_empty() {
            return Err(CatalogError::Missing("comaps.metaserver_url"));
        }
        Ok(())
    }

    pub fn leaf(&self, id: &str) -> Option<&CatalogLeaf> {
        self.leaves.iter().find(|l| l.id == id)
    }

    pub fn leaves_for_bake_id(&self, bake_id: &str) -> Option<&[String]> {
        self.region_to_leaves.get(bake_id).map(|v| v.as_slice())
    }

    /// Unique leaf ids listed under `country_groups` (routable UI leaves).
    pub fn country_group_leaf_ids(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        fn walk(
            groups: &[CountryGroup],
            out: &mut Vec<String>,
            seen: &mut std::collections::BTreeSet<String>,
        ) {
            for g in groups {
                for leaf in &g.leaves {
                    if seen.insert(leaf.id.clone()) {
                        out.push(leaf.id.clone());
                    }
                }
                walk(&g.subgroups, out, seen);
            }
        }
        walk(&self.country_groups, &mut out, &mut seen);
        out
    }
}

/// Parse and schema-check a catalog JSON body.
pub fn parse_catalog_json(text: &str) -> Result<MwmCatalog, CatalogError> {
    let catalog: MwmCatalog = serde_json::from_str(text)?;
    catalog.validate_schema()?;
    Ok(catalog)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example_json() -> &'static str {
        include_str!("../../tests/fixtures/mwm/catalog-v2.example.json")
    }

    #[test]
    fn parses_docs_example() {
        let c = parse_catalog_json(example_json()).expect("example catalog");
        assert_eq!(c.schema, SCHEMA_V2);
        assert_eq!(c.version, "261105");
        assert_eq!(c.mode, "comaps-release");
        assert_eq!(c.comaps.release_version, 261105);
        assert_eq!(c.leaves.len(), 2);
        let hedmark = c.leaf("Norway_Hedmark").expect("Hedmark");
        assert!(!hedmark.no_sidecar);
        assert_eq!(hedmark.sidecar_bytes, Some(261797));
        let fp = hedmark.fingerprint().expect("fp");
        assert_eq!(fp.size, 102194292);
        assert!(c.region_to_leaves.contains_key("europe_norway_ostlandet"));
        assert_eq!(
            c.shared_leaves.get("Norway_Southern").map(|v| v.len()),
            Some(2)
        );
        assert_eq!(c.country_groups.len(), 1);
        assert_eq!(c.country_group_leaf_ids().len(), 2);
        assert!(c
            .display_name_overrides
            .leaves
            .contains_key("Norway_Hedmark"));
        assert_eq!(
            c.non_routable_leaves,
            vec!["World".to_string(), "WorldCoasts".to_string()]
        );
    }

    #[test]
    fn rejects_unknown_schema() {
        let bad = example_json().replacen(SCHEMA_V2, "navi-mwm-catalog/v999", 1);
        let err = parse_catalog_json(&bad).unwrap_err();
        assert!(matches!(err, CatalogError::UnsupportedSchema(_)));
    }
}
