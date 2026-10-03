//! Process-wide LRU of corridor-clipped POI/barrier indexes.
//!
//! Full Ostlandet packs are ~66 MiB on disk and take 15–17 s to hydrate on
//! emulator SD when every plan reloads the whole region. Plans only need the
//! trip corridor; caching the clipped result makes warm replans near-free.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use crate::poi::PoiIndex;
use crate::routing::safety::DangerBarrierIndex;

/// Soft bound on cached POI/barrier payload estimates.
pub const POI_BARRIER_CACHE_MAX_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct PoiBarrierCacheKey {
    pub paths: Vec<String>,
    pub bbox_fp: u64,
}

impl PoiBarrierCacheKey {
    pub fn new(mut paths: Vec<String>, bbox: Option<[f64; 4]>) -> Self {
        paths.sort();
        let mut h = std::collections::hash_map::DefaultHasher::new();
        if let Some(b) = bbox {
            // Quantize so tiny pad differences still hit (0.05° ≈ 5 km).
            for v in b {
                (((v / 0.05).round() as i64) as u64).hash(&mut h);
            }
        } else {
            0u8.hash(&mut h);
        }
        Self {
            paths,
            bbox_fp: h.finish(),
        }
    }
}

struct Entry {
    poi: Arc<PoiIndex>,
    barriers: Arc<DangerBarrierIndex>,
    bytes: u64,
}

struct PoiLru {
    map: HashMap<PoiBarrierCacheKey, Entry>,
    order: Vec<PoiBarrierCacheKey>,
    bytes: u64,
    max_bytes: u64,
    hits: u64,
    misses: u64,
}

impl PoiLru {
    fn new(max_bytes: u64) -> Self {
        Self {
            map: HashMap::new(),
            order: Vec::new(),
            bytes: 0,
            max_bytes,
            hits: 0,
            misses: 0,
        }
    }

    fn get(
        &mut self,
        key: &PoiBarrierCacheKey,
    ) -> Option<(Arc<PoiIndex>, Arc<DangerBarrierIndex>)> {
        if let Some(i) = self.order.iter().position(|k| k == key) {
            let k = self.order.remove(i);
            self.order.push(k);
        }
        match self.map.get(key) {
            Some(e) => {
                self.hits = self.hits.saturating_add(1);
                Some((Arc::clone(&e.poi), Arc::clone(&e.barriers)))
            }
            None => {
                self.misses = self.misses.saturating_add(1);
                None
            }
        }
    }

    fn insert(
        &mut self,
        key: PoiBarrierCacheKey,
        poi: Arc<PoiIndex>,
        barriers: Arc<DangerBarrierIndex>,
        bytes: u64,
    ) {
        if let Some(old) = self.map.remove(&key) {
            self.bytes = self.bytes.saturating_sub(old.bytes);
            self.order.retain(|k| k != &key);
        }
        while !self.order.is_empty() && self.bytes.saturating_add(bytes) > self.max_bytes {
            let evict = self.order.remove(0);
            if let Some(old) = self.map.remove(&evict) {
                self.bytes = self.bytes.saturating_sub(old.bytes);
            }
        }
        self.bytes = self.bytes.saturating_add(bytes);
        self.order.push(key.clone());
        self.map.insert(
            key,
            Entry {
                poi,
                barriers,
                bytes,
            },
        );
    }
}

static CACHE: Mutex<Option<PoiLru>> = Mutex::new(None);

fn with_cache<R>(f: impl FnOnce(&mut PoiLru) -> R) -> R {
    let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        *guard = Some(PoiLru::new(POI_BARRIER_CACHE_MAX_BYTES));
    }
    f(guard.as_mut().expect("poi barrier cache initialized"))
}

pub fn estimate_poi_barrier_bytes(poi: &PoiIndex, barriers: &DangerBarrierIndex) -> u64 {
    let poi_n = poi.len() as u64;
    let building_n = poi.overnight_buildings().len() as u64;
    let glacier_n = barriers.glacier_ring_count() as u64;
    // Rough: record + rtree entry + tags ≈ 400 B; building point ≈ 16 B; glacier ≈ 2 KiB.
    poi_n
        .saturating_mul(400)
        .saturating_add(building_n.saturating_mul(16))
        .saturating_add(glacier_n.saturating_mul(2048))
        .saturating_add(64 * 1024)
}

pub fn poi_barrier_cache_get(
    key: &PoiBarrierCacheKey,
) -> Option<(Arc<PoiIndex>, Arc<DangerBarrierIndex>)> {
    with_cache(|c| c.get(key))
}

pub fn poi_barrier_cache_insert(
    key: PoiBarrierCacheKey,
    poi: Arc<PoiIndex>,
    barriers: Arc<DangerBarrierIndex>,
) {
    let bytes = estimate_poi_barrier_bytes(&poi, &barriers);
    with_cache(|c| c.insert(key, poi, barriers, bytes));
}

pub fn poi_barrier_cache_stats() -> (u64, u64, u64, usize) {
    with_cache(|c| (c.hits, c.misses, c.bytes, c.map.len()))
}

pub fn poi_barrier_cache_clear() {
    with_cache(|c| {
        c.map.clear();
        c.order.clear();
        c.bytes = 0;
    });
}
