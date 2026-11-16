//! `region-migration.json` — map existing `.rkyv` regions to CoMaps leaf sets.

use std::collections::BTreeMap;

use serde::Deserialize;
use thiserror::Error;

pub const SCHEMA_REGION_MIGRATION_V1: &str = "navi-mwm-region-migration/v1";

#[derive(Debug, Error)]
pub enum MigrationError {
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported region-migration schema {0:?}")]
    UnsupportedSchema(String),
}

#[derive(Debug, Clone, Deserialize)]
pub struct RegionMigration {
    pub schema: String,
    #[serde(default)]
    pub comaps_release: Option<String>,
    /// Keyed by Navi bake_id (`europe_norway_ostlandet`).
    pub regions: BTreeMap<String, RegionMigrationEntry>,
    #[serde(default)]
    pub shared_leaves: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RegionMigrationEntry {
    pub region_id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub union_coverage_pct: Option<f64>,
    #[serde(default)]
    pub uncovered_pct: Option<f64>,
    #[serde(default)]
    pub comaps_leaves: Vec<MigrationLeaf>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MigrationLeaf {
    pub leaf: String,
    #[serde(default)]
    pub coverage_of_region_pct: Option<f64>,
    #[serde(default)]
    pub overspill_pct: Option<f64>,
    #[serde(default)]
    pub include_reason: Option<String>,
    #[serde(default)]
    pub shared_with_navi_regions: Vec<String>,
}

impl RegionMigration {
    pub fn validate_schema(&self) -> Result<(), MigrationError> {
        if self.schema != SCHEMA_REGION_MIGRATION_V1 {
            return Err(MigrationError::UnsupportedSchema(self.schema.clone()));
        }
        Ok(())
    }

    pub fn by_bake_id(&self, bake_id: &str) -> Option<&RegionMigrationEntry> {
        self.regions.get(bake_id)
    }

    pub fn by_region_id(&self, region_id: &str) -> Option<(&str, &RegionMigrationEntry)> {
        let want = region_id.trim().trim_matches('/');
        self.regions.iter().find_map(|(bake, e)| {
            if e.region_id.trim().trim_matches('/') == want {
                Some((bake.as_str(), e))
            } else {
                None
            }
        })
    }
}

pub fn parse_region_migration_json(text: &str) -> Result<RegionMigration, MigrationError> {
    let m: RegionMigration = serde_json::from_str(text)?;
    m.validate_schema()?;
    Ok(m)
}

pub fn leaf_set_for_bake_id(migration: &RegionMigration, bake_id: &str) -> Option<Vec<String>> {
    migration.by_bake_id(bake_id).map(|e| {
        e.comaps_leaves
            .iter()
            .map(|l| l.leaf.clone())
            .collect::<Vec<_>>()
    })
}

pub fn leaf_set_for_region_id(migration: &RegionMigration, region_id: &str) -> Option<Vec<String>> {
    migration.by_region_id(region_id).map(|(_, e)| {
        e.comaps_leaves
            .iter()
            .map(|l| l.leaf.clone())
            .collect::<Vec<_>>()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> RegionMigration {
        parse_region_migration_json(include_str!(
            "../../tests/fixtures/mwm/region-migration.example.json"
        ))
        .unwrap()
    }

    #[test]
    fn ostlandet_leaf_set() {
        let m = fixture();
        let leaves = leaf_set_for_bake_id(&m, "europe_norway_ostlandet").unwrap();
        assert!(leaves.contains(&"Norway_Hedmark".to_string()));
        assert!(leaves.contains(&"Norway_Southern".to_string()));
        assert!(leaves.contains(&"Norway_Oppland".to_string()));
        let by_path = leaf_set_for_region_id(&m, "europe/norway/ostlandet").unwrap();
        assert_eq!(leaves, by_path);
        assert_eq!(
            m.by_bake_id("europe_norway_ostlandet")
                .unwrap()
                .display_name
                .as_deref(),
            Some("Ostlandet")
        );
    }

    #[test]
    fn sorlandet_shares_southern() {
        let m = fixture();
        let sor = leaf_set_for_bake_id(&m, "europe_norway_sorlandet").unwrap();
        assert!(sor.contains(&"Norway_Southern".to_string()));
        let shared = m.shared_leaves.get("Norway_Southern").unwrap();
        assert!(shared.contains(&"europe_norway_ostlandet".to_string()));
        assert!(shared.contains(&"europe_norway_sorlandet".to_string()));
    }
}
