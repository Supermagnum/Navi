use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use crate::config::ELEVATION_VOID;
use crate::routing::elevation::reader::ElevationReader;
use crate::routing::elevation::tile_id::HgtTileId;

/// In-memory index of locally cached elevation tiles on disk.
#[derive(Clone)]
pub struct ElevationCache {
    data_dir: PathBuf,
    reader: Arc<RwLock<ElevationReader>>,
    /// Negative cache: tile ids known to be absent on disk.
    missing: Arc<RwLock<HashSet<HgtTileId>>>,
    /// Positive cache: resolved on-disk paths.
    resolved: Arc<RwLock<HashMap<HgtTileId, PathBuf>>>,
}

impl ElevationCache {
    pub fn new(data_dir: impl AsRef<Path>) -> Self {
        Self {
            data_dir: data_dir.as_ref().to_path_buf(),
            reader: Arc::new(RwLock::new(ElevationReader::new())),
            missing: Arc::new(RwLock::new(HashSet::new())),
            resolved: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn tile_path(&self, tile: HgtTileId, source: &str) -> PathBuf {
        self.data_dir.join(source).join(format!("{tile}.hgt"))
    }

    pub fn copernicus_tile_dir(&self, tile: HgtTileId) -> PathBuf {
        self.data_dir.join("copernicus").join(tile.to_string())
    }

    pub fn tile_exists(&self, tile: HgtTileId) -> bool {
        self.resolve_local_path(tile).is_some()
    }

    pub fn resolve_local_path(&self, tile: HgtTileId) -> Option<PathBuf> {
        if let Ok(resolved) = self.resolved.read() {
            if let Some(path) = resolved.get(&tile) {
                return Some(path.clone());
            }
        }
        if let Ok(missing) = self.missing.read() {
            if missing.contains(&tile) {
                return None;
            }
        }

        let found = self.scan_disk_for_tile(tile);
        if let Some(ref path) = found {
            if let Ok(mut resolved) = self.resolved.write() {
                resolved.insert(tile, path.clone());
            }
        } else if let Ok(mut missing) = self.missing.write() {
            missing.insert(tile);
        }
        found
    }

    fn scan_disk_for_tile(&self, tile: HgtTileId) -> Option<PathBuf> {
        for source in ["copernicus", "viewfinder", "srtm"] {
            let hgt = self.tile_path(tile, source);
            if hgt.exists() {
                return Some(hgt);
            }
            let dir = self.data_dir.join(source).join(tile.to_string());
            if dir.is_dir() {
                if let Some(tif) = find_geotiff_in_dir(&dir) {
                    return Some(tif);
                }
            }
        }
        None
    }

    pub fn get_elevation(&self, lat: f64, lon: f64) -> Option<f64> {
        let tile = HgtTileId::from_lat_lon(lat, lon);
        let path = self.resolve_local_path(tile)?;

        if let Ok(reader) = self.reader.read() {
            if let Some(sampled) = reader.try_sample_cached(&path, lat, lon) {
                return sampled.filter(|h| *h != ELEVATION_VOID as f64);
            }
        }

        let mut reader = self.reader.write().expect("elevation reader lock");
        match reader.sample(&path, lat, lon) {
            Ok(Some(h)) if h != ELEVATION_VOID as f64 => Some(h),
            _ => None,
        }
    }

    /// Preload all tiles covering `bbox` ([min_lat, min_lon, max_lat, max_lon]).
    pub fn warm_bbox(&self, bbox: [f64; 4]) -> anyhow::Result<usize> {
        use crate::routing::elevation::tile_id::bbox_to_tiles;
        let tiles = bbox_to_tiles(bbox);
        let mut loaded = 0usize;
        let mut reader = self.reader.write().expect("elevation reader lock");
        for tile in tiles {
            if let Some(path) = self.resolve_local_path(tile) {
                reader.ensure_loaded(&path)?;
                loaded += 1;
            }
        }
        Ok(loaded)
    }

    pub fn invalidate(&self, tile: HgtTileId) {
        if let Ok(mut reader) = self.reader.write() {
            reader.invalidate(tile);
        }
        if let Ok(mut missing) = self.missing.write() {
            missing.remove(&tile);
        }
        if let Ok(mut resolved) = self.resolved.write() {
            resolved.remove(&tile);
        }
    }

    pub fn indexed_tiles(&self) -> HashMap<String, PathBuf> {
        let mut out = HashMap::new();
        for source in ["copernicus", "viewfinder", "srtm"] {
            let dir = self.data_dir.join(source);
            if !dir.is_dir() {
                continue;
            }
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("hgt") {
                        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                            out.insert(stem.to_string(), path);
                        }
                    } else if path.is_dir() {
                        if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                            if let Some(tif) = find_geotiff_in_dir(&path) {
                                out.insert(name.to_string(), tif);
                            }
                        }
                    }
                }
            }
        }
        out
    }
}

fn find_geotiff_in_dir(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            let name = path.file_name()?.to_str()?.to_ascii_lowercase();
            if name.ends_with(".tif") || name.ends_with(".tiff") {
                return Some(path);
            }
        }
    }
    None
}
