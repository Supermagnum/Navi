use std::fs::File;
use std::path::Path;

use geo_types::Coord as GeoCoord;
use srtm_reader::{Coord, Tile};

use crate::config::ELEVATION_VOID;
use crate::routing::elevation::tile_id::HgtTileId;

pub struct ElevationReader {
    hgt_cache: std::collections::HashMap<String, Tile>,
    tiff_cache: std::collections::HashMap<String, geotiff::GeoTiff>,
}

impl ElevationReader {
    pub fn new() -> Self {
        Self {
            hgt_cache: std::collections::HashMap::new(),
            tiff_cache: std::collections::HashMap::new(),
        }
    }

    /// Sample if the tile is already loaded. Returns `None` when not cached.
    pub fn try_sample_cached(&self, path: &Path, lat: f64, lon: f64) -> Option<Option<f64>> {
        let key = path.to_string_lossy().to_string();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        if ext == "hgt" || key.to_ascii_lowercase().contains(".hgt") {
            let tile = self.hgt_cache.get(&key)?;
            let coord = Coord { lat, lon };
            return Some(tile.get(coord).copied().and_then(normalize_i16));
        }

        let tiff = self.tiff_cache.get(&key)?;
        let model = GeoCoord { x: lon, y: lat };
        let value = tiff.get_value_at::<f64>(&model, 0);
        Some(value.and_then(normalize_f64))
    }

    pub fn ensure_loaded(&mut self, path: &Path) -> anyhow::Result<()> {
        let key = path.to_string_lossy().to_string();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        if ext == "hgt" || key.to_ascii_lowercase().contains(".hgt") {
            if !self.hgt_cache.contains_key(&key) {
                let tile = Tile::from_file(path)
                    .map_err(|e| anyhow::anyhow!("failed to read hgt tile: {e:?}"))?;
                self.hgt_cache.insert(key, tile);
            }
            return Ok(());
        }

        if !self.tiff_cache.contains_key(&key) {
            let file = File::open(path)?;
            let tiff = geotiff::GeoTiff::read(file)
                .map_err(|e| anyhow::anyhow!("failed to read geotiff: {e:?}"))?;
            self.tiff_cache.insert(key, tiff);
        }
        Ok(())
    }

    pub fn sample(&mut self, path: &Path, lat: f64, lon: f64) -> anyhow::Result<Option<f64>> {
        self.ensure_loaded(path)?;
        Ok(self
            .try_sample_cached(path, lat, lon)
            .expect("tile must be loaded"))
    }

    pub fn invalidate(&mut self, tile: HgtTileId) {
        let stem = tile.to_string();
        self.hgt_cache.retain(|k, _| !k.contains(&stem));
        self.tiff_cache.retain(|k, _| !k.contains(&stem));
    }
}

fn normalize_i16(value: i16) -> Option<f64> {
    if value == ELEVATION_VOID || value == -9999 {
        None
    } else {
        Some(value as f64)
    }
}

fn normalize_f64(value: f64) -> Option<f64> {
    if value.is_nan() || (value as i16) == ELEVATION_VOID {
        None
    } else {
        Some(value)
    }
}

impl Default for ElevationReader {
    fn default() -> Self {
        Self::new()
    }
}
