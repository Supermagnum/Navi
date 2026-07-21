use crate::routing::elevation::cache::ElevationCache;

/// High-level elevation lookup service backed by on-disk tiles.
#[derive(Clone)]
pub struct ElevationService {
    cache: ElevationCache,
}

impl ElevationService {
    pub fn new(cache: ElevationCache) -> Self {
        Self { cache }
    }

    pub fn cache(&self) -> &ElevationCache {
        &self.cache
    }

    pub fn get_elevation(&self, lat: f64, lon: f64) -> Option<f64> {
        self.cache.get_elevation(lat, lon)
    }

    pub fn warm_bbox(&self, bbox: [f64; 4]) -> anyhow::Result<usize> {
        self.cache.warm_bbox(bbox)
    }

    pub fn tile_exists(&self, lat: f64, lon: f64) -> bool {
        let tile = crate::routing::elevation::tile_id::HgtTileId::from_lat_lon(lat, lon);
        self.cache.tile_exists(tile)
    }
}
