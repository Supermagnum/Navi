//! On-device MWM + sidecar directory layout.

use std::path::{Path, PathBuf};

/// Root directory name under the app `data_dir`.
pub const MWM_ROOT: &str = "mwm";
pub const ACTIVE_FILE: &str = "active.json";
/// Records which CoMaps release the root `World*.mwm` overlays belong to.
pub const WORLD_RELEASE_FILE: &str = "world-release.json";
pub const WORLD_MWM_FILE: &str = "World.mwm";
pub const WORLD_COASTS_MWM_FILE: &str = "WorldCoasts.mwm";

#[derive(Debug, Clone)]
pub struct MwmPaths {
    pub root: PathBuf,
}

impl MwmPaths {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            root: data_dir.join(MWM_ROOT),
        }
    }

    pub fn active_file(&self) -> PathBuf {
        self.root.join(ACTIVE_FILE)
    }

    pub fn world_release_file(&self) -> PathBuf {
        self.root.join(WORLD_RELEASE_FILE)
    }

    pub fn world_mwm(&self) -> PathBuf {
        self.root.join(WORLD_MWM_FILE)
    }

    pub fn world_coasts_mwm(&self) -> PathBuf {
        self.root.join(WORLD_COASTS_MWM_FILE)
    }

    pub fn releases(&self) -> PathBuf {
        self.root.join("releases")
    }

    pub fn staging(&self) -> PathBuf {
        self.root.join("staging")
    }

    pub fn migration_cache(&self) -> PathBuf {
        self.root.join("migration-cache")
    }

    pub fn release(&self, version: &str) -> PathBuf {
        self.releases().join(version)
    }

    pub fn staging_release(&self, version: &str) -> PathBuf {
        self.staging().join(version)
    }
}

pub fn active_path(data_dir: &Path) -> PathBuf {
    MwmPaths::new(data_dir).active_file()
}

pub fn release_dir(data_dir: &Path, version: &str) -> PathBuf {
    MwmPaths::new(data_dir).release(version)
}

pub fn staging_dir(data_dir: &Path, version: &str) -> PathBuf {
    MwmPaths::new(data_dir).staging_release(version)
}

pub fn leaf_mwm_path(release_or_staging: &Path, leaf_id: &str) -> PathBuf {
    release_or_staging
        .join("leaves")
        .join(format!("{leaf_id}.mwm"))
}

pub fn leaf_sidecar_path(release_or_staging: &Path, leaf_id: &str) -> PathBuf {
    release_or_staging
        .join("sidecars")
        .join(format!("{leaf_id}.navi-sidecar.v1.zst"))
}

pub fn catalog_snapshot_path(release_or_staging: &Path) -> PathBuf {
    release_or_staging.join("catalog.json")
}

pub fn attribution_json_path(release_or_staging: &Path) -> PathBuf {
    release_or_staging.join("attribution.json")
}

pub fn attribution_md_path(release_or_staging: &Path) -> PathBuf {
    release_or_staging.join("ATTRIBUTION.md")
}

pub fn world_release_path(data_dir: &Path) -> PathBuf {
    MwmPaths::new(data_dir).world_release_file()
}

pub fn world_mwm_path(data_dir: &Path) -> PathBuf {
    MwmPaths::new(data_dir).world_mwm()
}

pub fn world_coasts_mwm_path(data_dir: &Path) -> PathBuf {
    MwmPaths::new(data_dir).world_coasts_mwm()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_paths() {
        let root = Path::new("/data");
        let p = MwmPaths::new(root);
        assert_eq!(p.root, PathBuf::from("/data/mwm"));
        assert_eq!(
            leaf_mwm_path(&p.release("261105"), "Norway_Hedmark"),
            PathBuf::from("/data/mwm/releases/261105/leaves/Norway_Hedmark.mwm")
        );
        assert_eq!(
            leaf_sidecar_path(&p.staging_release("261105"), "Norway_Hedmark"),
            PathBuf::from("/data/mwm/staging/261105/sidecars/Norway_Hedmark.navi-sidecar.v1.zst")
        );
        assert_eq!(p.world_mwm(), PathBuf::from("/data/mwm/World.mwm"));
        assert_eq!(
            p.world_coasts_mwm(),
            PathBuf::from("/data/mwm/WorldCoasts.mwm")
        );
        assert_eq!(
            p.world_release_file(),
            PathBuf::from("/data/mwm/world-release.json")
        );
    }
}
