//! Region pack manifest (`{stem}.navi-manifest.json`).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use super::graph_pack::GRAPH_FORMAT_VERSION;
use super::poi_barrier_pack::POI_BARRIER_FORMAT_VERSION;
use crate::routing::graph::RoutingProfile;

pub const GRAPH_PROFILE_CAR: &str = "car";
pub const GRAPH_PROFILE_TRUCK: &str = "truck";
pub const GRAPH_PROFILE_FOOT: &str = "foot";
pub const GRAPH_PROFILE_BICYCLE: &str = "bicycle";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NaviManifest {
    pub schema: u32,
    pub stem: String,
    pub pbf_filename: String,
    pub pbf_size_bytes: u64,
    pub pbf_modified_unix_secs: u64,
    /// Profile key → relative graph archive filename.
    pub graph_files: BTreeMap<String, String>,
    pub graph_format_version: u32,
    pub poi_barrier_file: String,
    pub poi_barrier_format_version: u32,
    /// Optional wetland archive (Phase 4b). Absent on pre-4b manifests → hiking
    /// falls back to PBF wetland scan.
    #[serde(default)]
    pub wetland_file: Option<String>,
    #[serde(default)]
    pub wetland_format_version: u32,
    #[serde(default)]
    pub has_delta_h: bool,
    #[serde(default)]
    pub elev_dir: Option<String>,
}

impl NaviManifest {
    pub const SCHEMA: u32 = 1;
}

pub fn manifest_path(data_dir: &Path, stem: &str) -> PathBuf {
    data_dir.join(format!("{stem}.navi-manifest.json"))
}

pub fn graph_pack_filename(stem: &str, profile_key: &str) -> String {
    format!("{stem}.navi-graph-{profile_key}.rkyv")
}

pub fn poi_barrier_pack_filename(stem: &str) -> String {
    format!("{stem}.navi-poi-barrier.rkyv")
}

pub fn wetland_pack_filename(stem: &str) -> String {
    format!("{stem}.navi-wetland.rkyv")
}

pub fn profile_key(profile: RoutingProfile) -> &'static str {
    match profile {
        RoutingProfile::Car => GRAPH_PROFILE_CAR,
        RoutingProfile::Truck => GRAPH_PROFILE_TRUCK,
        RoutingProfile::Foot => GRAPH_PROFILE_FOOT,
        RoutingProfile::Bicycle => GRAPH_PROFILE_BICYCLE,
    }
}

pub fn pbf_fingerprint(pbf: &Path) -> anyhow::Result<(u64, u64)> {
    let meta = fs::metadata(pbf)?;
    let modified = meta
        .modified()
        .unwrap_or(SystemTime::UNIX_EPOCH)
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Ok((meta.len(), modified))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackStatus {
    Ready,
    Missing,
    StalePbf,
    VersionMismatch,
}

impl NaviManifest {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let text = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&text)?)
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self)?;
        let tmp = path.with_extension("json.partial");
        fs::write(&tmp, text)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn status_for_pbf(&self, data_dir: &Path, pbf: &Path) -> PackStatus {
        let Ok((sz, mtime)) = pbf_fingerprint(pbf) else {
            return PackStatus::Missing;
        };
        if sz != self.pbf_size_bytes || mtime != self.pbf_modified_unix_secs {
            return PackStatus::StalePbf;
        }
        if self.graph_format_version != GRAPH_FORMAT_VERSION
            || self.poi_barrier_format_version != POI_BARRIER_FORMAT_VERSION
            || self.schema != Self::SCHEMA
        {
            return PackStatus::VersionMismatch;
        }
        // Wetland is optional for motor plans; hiking checks it separately via
        // `try_load_wetland_for_plan` so a missing wetland file does not block
        // graph/POI pack hits.
        for name in self.graph_files.values() {
            if !data_dir.join(name).is_file() {
                return PackStatus::Missing;
            }
        }
        if !data_dir.join(&self.poi_barrier_file).is_file() {
            return PackStatus::Missing;
        }
        PackStatus::Ready
    }

    pub fn graph_path(&self, data_dir: &Path, profile: RoutingProfile) -> Option<PathBuf> {
        let key = profile_key(profile);
        self.graph_files.get(key).map(|n| data_dir.join(n))
    }

    pub fn poi_barrier_path(&self, data_dir: &Path) -> PathBuf {
        data_dir.join(&self.poi_barrier_file)
    }

    pub fn wetland_path(&self, data_dir: &Path) -> Option<PathBuf> {
        self.wetland_file.as_ref().map(|n| data_dir.join(n))
    }
}
