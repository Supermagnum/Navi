//! Basemap style resolution for the Linux desktop shell.
//!
//! Mirrors the Android order in `BasemapStyleResolver` (see docs/map-styles.md):
//! prefer a completed local Protomaps PMTiles extract covering the camera, else
//! OpenFreeMap Liberty online. 3D / Mapterhorn hillshade is out of scope here.

use std::path::{Path, PathBuf};

use navi::{pmtiles_list_covering, FfiPmtilesJob};

pub const LIBERTY_URL: &str = "https://tiles.openfreemap.org/styles/liberty";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StyleKind {
    OnlineLiberty,
    OfflineProtomaps,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ResolvedBasemap {
    pub kind: StyleKind,
    /// Liberty style URL, or local HTTP URL for a prepared offline style JSON.
    pub style_url: String,
    pub note: String,
    pub pmtiles_path: Option<String>,
}

/// Resolve basemap for `lat`/`lon`.
///
/// Order:
/// 1. Explicit `--pmtiles` path when the file exists.
/// 2. First completed covering job from the core PMTiles job store (`data_dir`).
/// 3. Heuristic file under `data_dir/pmtiles/` or common fixture paths.
/// 4. OpenFreeMap Liberty (online).
pub fn resolve(
    data_dir: &Path,
    lat: f64,
    lon: f64,
    explicit_pmtiles: Option<&Path>,
    force_online: bool,
    local_origin: &str,
) -> ResolvedBasemap {
    if force_online {
        return liberty("forced online 2D");
    }

    if let Some(p) = explicit_pmtiles {
        if p.is_file() {
            return offline_from_path(p, local_origin, "CLI --pmtiles");
        }
    }

    let covering = pmtiles_list_covering(data_dir.display().to_string(), lat, lon)
        .into_iter()
        .find(|j| Path::new(&j.local_path).is_file());
    if let Some(job) = covering {
        return offline_from_job(&job, local_origin);
    }

    for candidate in heuristic_pmtiles(data_dir) {
        if candidate.is_file() {
            return offline_from_path(
                &candidate,
                local_origin,
                "heuristic local PMTiles file",
            );
        }
    }

    liberty("no local PMTiles covering camera; OpenFreeMap Liberty")
}

fn liberty(note: &str) -> ResolvedBasemap {
    ResolvedBasemap {
        kind: StyleKind::OnlineLiberty,
        style_url: LIBERTY_URL.to_string(),
        note: note.to_string(),
        pmtiles_path: None,
    }
}

fn offline_from_job(job: &FfiPmtilesJob, local_origin: &str) -> ResolvedBasemap {
    offline_from_path(Path::new(&job.local_path), local_origin, "completed covering PMTiles job")
}

fn offline_from_path(path: &Path, local_origin: &str, note: &str) -> ResolvedBasemap {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("region.pmtiles");
    ResolvedBasemap {
        kind: StyleKind::OfflineProtomaps,
        style_url: format!("{local_origin}/styles/offline.json?file={name}"),
        note: format!("{note}: {}", path.display()),
        pmtiles_path: Some(path.display().to_string()),
    }
}

fn heuristic_pmtiles(data_dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    out.push(data_dir.join("pmtiles/europe_norway_ostlandet.pmtiles"));
    out.push(data_dir.join("europe_norway_ostlandet.pmtiles"));
    // Common host fixture from Android/offline bring-up
    out.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../core/target/integration-fixtures/europe_norway_ostlandet.pmtiles"),
    );
    out
}
