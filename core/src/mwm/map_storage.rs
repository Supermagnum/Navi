//! "Map storage" data: what is installed per country group, what it costs on disk, and the
//! per-leaf delete. Reads the disk (and, when available, a catalog for group / display names);
//! never touches the network and never touches `.rkyv` files.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::catalog::{CountryGroup, MwmCatalog};
use super::install::installed_leaf_ids;
use super::migrate::{read_state, MigrationStatus};
use super::release::read_active_version;
use super::storage::{
    leaf_mwm_path, leaf_sidecar_path, release_dir, world_coasts_mwm_path, world_mwm_path, MwmPaths,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StorageLeaf {
    pub id: String,
    pub display_name: String,
    pub mwm_bytes: u64,
    pub sidecar_bytes: u64,
    pub has_sidecar: bool,
}

impl StorageLeaf {
    pub fn total_bytes(&self) -> u64 {
        self.mwm_bytes + self.sidecar_bytes
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StorageGroup {
    pub id: String,
    pub display_name: String,
    pub leaves: Vec<StorageLeaf>,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StorageSummary {
    /// Active CoMaps release (e.g. `261105`).
    pub version: Option<String>,
    pub groups: Vec<StorageGroup>,
    pub leaf_count: usize,
    pub leaves_bytes: u64,
    /// `World.mwm` + `WorldCoasts.mwm` at the storage root.
    pub world_bytes: u64,
    /// Downloads that are not switched in yet (staging).
    pub staging_bytes: u64,
    /// Release trees other than the active one (older CoMaps releases).
    pub other_release_bytes: u64,
    pub total_bytes: u64,
}

pub fn dir_size(path: &Path) -> u64 {
    let Ok(rd) = fs::read_dir(path) else {
        return fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    };
    rd.flatten()
        .map(|e| {
            let p = e.path();
            if p.is_dir() {
                dir_size(&p)
            } else {
                e.metadata().map(|m| m.len()).unwrap_or(0)
            }
        })
        .sum()
}

fn file_len(p: &Path) -> u64 {
    fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

/// leaf id -> (top-level group id, group display name, leaf display name)
fn group_index(catalog: &MwmCatalog) -> BTreeMap<String, (String, String, String)> {
    let overrides = &catalog.display_name_overrides.leaves;
    let mut out = BTreeMap::new();
    fn walk(
        groups: &[CountryGroup],
        top: Option<(&str, &str)>,
        overrides: &BTreeMap<String, super::catalog::DisplayNameOverride>,
        out: &mut BTreeMap<String, (String, String, String)>,
    ) {
        for g in groups {
            let gname = g.display_name.as_deref().unwrap_or(&g.id);
            let t = top.unwrap_or((g.id.as_str(), gname));
            for l in &g.leaves {
                let name = overrides
                    .get(&l.id)
                    .map(|o| o.display_name.clone())
                    .or_else(|| l.display_name.clone())
                    .unwrap_or_else(|| l.id.replace('_', " "));
                out.entry(l.id.clone())
                    .or_insert((t.0.to_string(), t.1.to_string(), name));
            }
            walk(&g.subgroups, Some(t), overrides, out);
        }
    }
    walk(&catalog.country_groups, None, overrides, &mut out);
    out
}

pub fn storage_summary(data_dir: &Path, catalog: Option<&MwmCatalog>) -> StorageSummary {
    let version = read_active_version(data_dir)
        .ok()
        .flatten()
        .map(|a| a.version);
    let index = catalog.map(group_index).unwrap_or_default();
    let mut groups: BTreeMap<String, StorageGroup> = BTreeMap::new();
    let mut leaves_bytes = 0u64;
    let mut leaf_count = 0usize;
    if let Some(v) = version.as_deref() {
        let release = release_dir(data_dir, v);
        for id in installed_leaf_ids(data_dir, v) {
            let mwm = file_len(&leaf_mwm_path(&release, &id));
            let sc_path = leaf_sidecar_path(&release, &id);
            let sc = file_len(&sc_path);
            let (gid, gname, lname) = index.get(&id).cloned().unwrap_or_else(|| {
                (
                    "_other".to_string(),
                    "Other maps".to_string(),
                    id.replace('_', " "),
                )
            });
            let leaf = StorageLeaf {
                id,
                display_name: lname,
                mwm_bytes: mwm,
                sidecar_bytes: sc,
                has_sidecar: sc_path.is_file(),
            };
            leaves_bytes += leaf.total_bytes();
            leaf_count += 1;
            let g = groups.entry(gid.clone()).or_insert_with(|| StorageGroup {
                id: gid,
                display_name: gname,
                leaves: Vec::new(),
                total_bytes: 0,
            });
            g.total_bytes += leaf.total_bytes();
            g.leaves.push(leaf);
        }
    }
    let mut groups: Vec<StorageGroup> = groups.into_values().collect();
    for g in &mut groups {
        g.leaves.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    }
    groups.sort_by(|a, b| a.display_name.cmp(&b.display_name));

    let world_bytes =
        file_len(&world_mwm_path(data_dir)) + file_len(&world_coasts_mwm_path(data_dir));
    let paths = MwmPaths::new(data_dir);
    let staging_bytes = dir_size(&paths.staging());
    let mut other_release_bytes = 0u64;
    if let Ok(rd) = fs::read_dir(paths.releases()) {
        for e in rd.flatten() {
            if Some(e.file_name().to_string_lossy().as_ref()) != version.as_deref() {
                other_release_bytes += dir_size(&e.path());
            }
        }
    }
    StorageSummary {
        version,
        groups,
        leaf_count,
        leaves_bytes,
        world_bytes,
        staging_bytes,
        other_release_bytes,
        total_bytes: leaves_bytes + world_bytes + staging_bytes + other_release_bytes,
    }
}

/// Why a leaf cannot be deleted right now.
pub fn delete_block_reason(data_dir: &Path, leaf_id: &str) -> Option<String> {
    if leaf_id == "World" || leaf_id == "WorldCoasts" {
        return Some("the world overview is required by every map".into());
    }
    if let Some(st) = read_state(data_dir) {
        if !matches!(st.status, MigrationStatus::Verified | MigrationStatus::Done) {
            return Some("a map migration is in progress".into());
        }
    }
    None
}

/// Delete one installed leaf (MWM + sidecar). Returns the bytes freed.
pub fn delete_leaf(data_dir: &Path, leaf_id: &str) -> Result<u64, String> {
    if let Some(why) = delete_block_reason(data_dir, leaf_id) {
        return Err(why);
    }
    let version = read_active_version(data_dir)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "no active release".to_string())?
        .version;
    let release = release_dir(data_dir, &version);
    let mwm = leaf_mwm_path(&release, leaf_id);
    if !mwm.is_file() {
        return Err(format!("{leaf_id} is not installed"));
    }
    let mut freed = file_len(&mwm);
    fs::remove_file(&mwm).map_err(|e| e.to_string())?;
    let sc = leaf_sidecar_path(&release, leaf_id);
    if sc.is_file() {
        freed += file_len(&sc);
        let _ = fs::remove_file(&sc);
    }
    Ok(freed)
}

/// Remove release trees other than the active one (older CoMaps releases left by an update).
pub fn delete_old_releases(data_dir: &Path) -> Result<u64, String> {
    let active = read_active_version(data_dir)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "no active release".to_string())?
        .version;
    let before = MwmPaths::new(data_dir);
    let size_before = dir_size(&before.releases());
    super::release::cleanup_old_releases(data_dir, &active).map_err(|e| e.to_string())?;
    Ok(size_before.saturating_sub(dir_size(&before.releases())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mwm::catalog::parse_catalog_json;
    use crate::mwm::migrate::{set_status, write_state, MigrationState, OldMapsDecision};
    use crate::mwm::release::write_active_version;
    use tempfile::tempdir;

    fn setup(dir: &Path) {
        write_active_version(dir, "261105").unwrap();
        let rel = release_dir(dir, "261105");
        fs::create_dir_all(rel.join("leaves")).unwrap();
        fs::create_dir_all(rel.join("sidecars")).unwrap();
        fs::write(rel.join("leaves/Norway_Hedmark.mwm"), vec![0u8; 1000]).unwrap();
        fs::write(
            rel.join("sidecars/Norway_Hedmark.navi-sidecar.v1.zst"),
            vec![0u8; 10],
        )
        .unwrap();
        fs::write(rel.join("leaves/Sweden_Stockholm.mwm"), vec![0u8; 500]).unwrap();
        fs::write(dir.join("mwm/World.mwm"), vec![0u8; 70]).unwrap();
        fs::write(dir.join("mwm/WorldCoasts.mwm"), vec![0u8; 30]).unwrap();
        let old = release_dir(dir, "261005");
        fs::create_dir_all(old.join("leaves")).unwrap();
        fs::write(old.join("leaves/Old.mwm"), vec![0u8; 250]).unwrap();
        fs::create_dir_all(dir.join("mwm/staging/261106/leaves")).unwrap();
        fs::write(
            dir.join("mwm/staging/261106/leaves/X.mwm.partial"),
            vec![0u8; 40],
        )
        .unwrap();
    }

    #[test]
    fn summary_groups_leaves_and_counts_everything() {
        let dir = tempdir().unwrap();
        setup(dir.path());
        let cat = parse_catalog_json(include_str!(
            "../../tests/fixtures/mwm/catalog-v2.example.json"
        ))
        .unwrap();
        let s = storage_summary(dir.path(), Some(&cat));
        assert_eq!(s.version.as_deref(), Some("261105"));
        assert_eq!(s.leaf_count, 2);
        let norway = s.groups.iter().find(|g| g.id == "Norway").unwrap();
        assert_eq!(norway.leaves[0].display_name, "Innlandet (Hedmark)");
        assert_eq!(norway.total_bytes, 1010);
        // Leaf unknown to the catalog lands in "Other maps".
        assert!(s.groups.iter().any(|g| g.id == "_other"));
        assert_eq!(s.world_bytes, 100);
        assert_eq!(s.staging_bytes, 40);
        assert_eq!(s.other_release_bytes, 250);
        assert_eq!(s.total_bytes, 1010 + 500 + 100 + 40 + 250);
        // Without a catalog everything is "Other maps".
        let s2 = storage_summary(dir.path(), None);
        assert_eq!(s2.groups.len(), 1);
    }

    #[test]
    fn delete_leaf_frees_bytes_and_protects_world() {
        let dir = tempdir().unwrap();
        setup(dir.path());
        assert!(delete_leaf(dir.path(), "World").is_err());
        assert_eq!(delete_leaf(dir.path(), "Norway_Hedmark").unwrap(), 1010);
        assert!(delete_leaf(dir.path(), "Norway_Hedmark").is_err());
        assert_eq!(delete_old_releases(dir.path()).unwrap(), 250);
    }

    #[test]
    fn delete_blocked_during_migration() {
        let dir = tempdir().unwrap();
        setup(dir.path());
        let now = 1;
        let st = MigrationState {
            schema: 1,
            version: "261105".into(),
            status: MigrationStatus::Running,
            selected_region_ids: vec![],
            leaves: vec![],
            old_regions: vec![],
            wifi_only: false,
            download_bytes: 0,
            created_ms: now,
            updated_ms: now,
            switched_ms: None,
            error: None,
            old_maps_decision: OldMapsDecision::Pending,
            bytes_freed: 0,
        };
        write_state(dir.path(), &st).unwrap();
        assert!(delete_leaf(dir.path(), "Norway_Hedmark").is_err());
        set_status(dir.path(), MigrationStatus::Verified, None).unwrap();
        assert!(delete_leaf(dir.path(), "Norway_Hedmark").is_ok());
    }
}
