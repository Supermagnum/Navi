//! Process-wide LRU of merged corridor graphs so replan/reroute skips pack load.
//!
//! Tiles are mmapped only while materializing; the owned [`RouteGraph`] is what
//! stays in RSS (~nodes+edges). Caching the **merged** corridor avoids repeating
//! that materialize+merge (~4 s cold) on warm plans with the same tile set.
//!
//! Entries are shared read-only via [`Arc`] — callers must not mutate a cached
//! graph (eco / surface soft costs are per-plan overlays).

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use crate::routing::graph::{RouteGraph, RoutingProfile};

/// Fallback soft bound when `/proc/meminfo` is unavailable.
pub const CORRIDOR_CACHE_MAX_BYTES: u64 = 768 * 1024 * 1024;

/// Floor / ceiling for the memory-derived cache cap.
/// On ~3.5 GiB tablets, keep the cache small so a Bergen-size materialize
/// (~0.8–1.0 GiB owned graph) still fits beside the OS.
pub const CORRIDOR_CACHE_MIN_BYTES: u64 = 64 * 1024 * 1024;
pub const CORRIDOR_CACHE_HARD_MAX_BYTES: u64 = 512 * 1024 * 1024;

/// Only wipe / drop the MRU corridor when MemAvailable is near an LMK risk.
/// The prior 1.1 GiB threshold cleared a just-used Vestlandet corridor on
/// Bergen→Førde eco→non-eco while the device still had ~900 MiB free.
pub const CORRIDOR_CACHE_CRITICAL_AVAIL_BYTES: u64 = 450 * 1024 * 1024;

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct CorridorCacheKey {
    pub profile: u8,
    pub tile_files: Vec<String>,
    pub clips_fp: u64,
}

impl CorridorCacheKey {
    pub fn new(
        profile: RoutingProfile,
        mut tile_files: Vec<String>,
        clips: Option<&[[f64; 4]]>,
    ) -> Self {
        tile_files.sort();
        let profile = match profile {
            RoutingProfile::Car => 0,
            RoutingProfile::Bicycle => 1,
            RoutingProfile::Foot => 2,
            RoutingProfile::Truck => 3,
        };
        let mut h = std::collections::hash_map::DefaultHasher::new();
        if let Some(cs) = clips {
            for c in cs {
                for v in c {
                    v.to_bits().hash(&mut h);
                }
            }
            cs.len().hash(&mut h);
        } else {
            0u8.hash(&mut h);
        }
        Self {
            profile,
            tile_files,
            clips_fp: h.finish(),
        }
    }
}

struct Entry {
    graph: Arc<RouteGraph>,
    bytes: u64,
}

struct CorridorLru {
    map: HashMap<CorridorCacheKey, Entry>,
    order: Vec<CorridorCacheKey>,
    bytes: u64,
    max_bytes: u64,
    hits: u64,
    misses: u64,
}

impl CorridorLru {
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

    fn get(&mut self, key: &CorridorCacheKey) -> Option<Arc<RouteGraph>> {
        if let Some(i) = self.order.iter().position(|k| k == key) {
            let k = self.order.remove(i);
            self.order.push(k);
        }
        match self.map.get(key) {
            Some(entry) => {
                self.hits = self.hits.saturating_add(1);
                Some(Arc::clone(&entry.graph))
            }
            None => {
                self.misses = self.misses.saturating_add(1);
                None
            }
        }
    }

    fn insert(&mut self, key: CorridorCacheKey, graph: Arc<RouteGraph>, bytes: u64) {
        if let Some(old) = self.map.remove(&key) {
            self.bytes = self.bytes.saturating_sub(old.bytes);
            self.order.retain(|k| k != &key);
        }
        // Evict older corridors to fit. Always insert the new MRU even when it
        // alone exceeds the soft cap (eco→non-eco reuse); near-LMK clears via
        // [`reclamp_keep_mru`].
        while !self.order.is_empty() && self.bytes.saturating_add(bytes) > self.max_bytes {
            let evict = self.order.remove(0);
            if let Some(old) = self.map.remove(&evict) {
                self.bytes = self.bytes.saturating_sub(old.bytes);
            }
        }
        self.bytes = self.bytes.saturating_add(bytes);
        self.order.push(key.clone());
        self.map.insert(key, Entry { graph, bytes });
    }

    /// Shrink to [max_bytes], always keeping the most-recently-used entry
    /// unless [force_clear_mru] (near-LMK path).
    fn reclamp_keep_mru(&mut self, max_bytes: u64, force_clear_mru: bool) {
        self.max_bytes = max_bytes;
        if force_clear_mru {
            self.map.clear();
            self.order.clear();
            self.bytes = 0;
            return;
        }
        while self.order.len() > 1 && self.bytes > self.max_bytes {
            let evict = self.order.remove(0);
            if let Some(old) = self.map.remove(&evict) {
                self.bytes = self.bytes.saturating_sub(old.bytes);
            }
        }
    }
}

static CACHE: Mutex<Option<CorridorLru>> = Mutex::new(None);

/// Derive cache cap from MemAvailable: leave headroom for the live plan / OS,
/// clamp to [64 MiB, 512 MiB]. Falls back to [`CORRIDOR_CACHE_MAX_BYTES`].
pub fn corridor_cache_max_bytes_from_mem() -> u64 {
    let avail = read_mem_available_bytes().unwrap_or(CORRIDOR_CACHE_MAX_BYTES);
    // Keep cache small relative to device RAM (3.5 GiB tablets); live plan needs
    // another ~same-sized corridor for A*.
    let derived = avail.saturating_mul(15) / 100;
    let leave_headroom = avail.saturating_sub(900 * 1024 * 1024);
    derived
        .clamp(CORRIDOR_CACHE_MIN_BYTES, CORRIDOR_CACHE_HARD_MAX_BYTES)
        .min(leave_headroom.max(CORRIDOR_CACHE_MIN_BYTES))
}

pub fn read_mem_available_bytes() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/meminfo").ok()?;
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("MemAvailable:") else {
            continue;
        };
        let kib: u64 = rest.split_whitespace().next()?.parse().ok()?;
        return Some(kib.saturating_mul(1024));
    }
    None
}

fn mem_near_lmk() -> bool {
    match read_mem_available_bytes() {
        Some(avail) if avail > 0 => avail < CORRIDOR_CACHE_CRITICAL_AVAIL_BYTES,
        _ => false,
    }
}

fn with_cache<R>(f: impl FnOnce(&mut CorridorLru) -> R) -> R {
    let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let max = corridor_cache_max_bytes_from_mem();
    let near_lmk = mem_near_lmk();
    if guard.is_none() {
        crate::routing::plan_perf::note_u64("corridor_cache_cap_mb", max / (1024 * 1024));
        *guard = Some(CorridorLru::new(max));
    } else if let Some(c) = guard.as_mut() {
        // Re-size from current MemAvailable, but keep the MRU corridor unless
        // MemAvailable is near an LMK risk. A soft 1.1 GiB wipe previously
        // dropped Bergen→Førde right after eco so non-eco rematerialized (~5 s).
        if c.max_bytes != max || near_lmk {
            crate::routing::plan_perf::note_u64("corridor_cache_cap_mb", max / (1024 * 1024));
            c.reclamp_keep_mru(max, near_lmk);
        }
    }
    f(guard.as_mut().expect("corridor cache initialized"))
}

/// Estimate owned-graph RSS contribution (nodes + edges + adjacency overhead).
pub fn estimate_graph_bytes(g: &RouteGraph) -> u64 {
    let nodes = g.nodes.len() as u64;
    let edges = g.edges.len() as u64;
    nodes
        .saturating_mul(96)
        .saturating_add(edges.saturating_mul(1800))
}

/// Shared read-only borrow of a cached corridor (Arc clone; no graph clone).
pub fn corridor_cache_get(key: &CorridorCacheKey) -> Option<Arc<RouteGraph>> {
    with_cache(|c| c.get(key))
}

/// Legacy take API: returns an owned graph (clones if another Arc remains).
/// Prefer [`corridor_cache_get`] + overlays for planning.
pub fn corridor_cache_take(key: &CorridorCacheKey) -> Option<RouteGraph> {
    let arc = corridor_cache_get(key)?;
    Some(Arc::try_unwrap(arc).unwrap_or_else(|a| (*a).clone()))
}

pub fn corridor_cache_insert(key: CorridorCacheKey, graph: Arc<RouteGraph>) {
    let bytes = estimate_graph_bytes(&graph);
    with_cache(|c| c.insert(key, graph, bytes));
}

/// Insert an owned merged corridor.
pub fn corridor_cache_insert_owned(key: CorridorCacheKey, graph: RouteGraph) {
    corridor_cache_insert(key, Arc::new(graph));
}

pub fn corridor_cache_stats() -> (u64, u64, u64, usize) {
    with_cache(|c| (c.hits, c.misses, c.bytes, c.map.len()))
}

pub fn corridor_cache_clear() {
    with_cache(|c| {
        c.map.clear();
        c.order.clear();
        c.bytes = 0;
    });
}

/// Evict cached corridors before materializing a new one when MemAvailable is
/// near an LMK risk. Otherwise only drop older LRU entries and **keep the MRU**
/// so eco→non-eco of the same corridor stays a hit.
pub fn corridor_cache_evict_before_load() {
    let avail = read_mem_available_bytes().unwrap_or(0);
    let near_lmk = avail > 0 && avail < CORRIDOR_CACHE_CRITICAL_AVAIL_BYTES;
    if near_lmk {
        crate::routing::plan_perf::note_u64("corridor_cache_evict_avail_mb", avail / (1024 * 1024));
        corridor_cache_clear();
        return;
    }
    with_cache(|c| {
        let max = corridor_cache_max_bytes_from_mem();
        c.reclamp_keep_mru(max, false);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::graph::RouteGraph;
    use osm4routing::NodeId;
    use std::collections::HashMap;

    fn tiny_graph() -> RouteGraph {
        RouteGraph::from_parts(HashMap::new(), Vec::new(), RoutingProfile::Car)
    }

    #[test]
    fn lru_hit_shares_arc() {
        corridor_cache_clear();
        let key = CorridorCacheKey::new(RoutingProfile::Car, vec!["a.rkyv".into()], None);
        assert!(corridor_cache_get(&key).is_none());
        corridor_cache_insert_owned(key.clone(), tiny_graph());
        let a = corridor_cache_get(&key).expect("hit");
        let b = corridor_cache_get(&key).expect("hit again");
        assert!(Arc::ptr_eq(&a, &b));
        let (hits, _, _, _) = corridor_cache_stats();
        assert!(hits >= 2);
        let _ = NodeId(0);
        corridor_cache_clear();
    }

    #[test]
    fn reclamp_keeps_mru_when_over_soft_cap() {
        let mut c = CorridorLru::new(10_000);
        let older = CorridorCacheKey::new(RoutingProfile::Car, vec!["older.rkyv".into()], None);
        let mru = CorridorCacheKey::new(RoutingProfile::Car, vec!["mru.rkyv".into()], None);
        c.insert(older.clone(), Arc::new(tiny_graph()), 6_000);
        c.insert(mru.clone(), Arc::new(tiny_graph()), 6_000);
        assert!(c.map.contains_key(&mru));
        // Soft cap shrink (MemAvailable drift): must keep MRU, may drop older.
        c.reclamp_keep_mru(100, false);
        assert!(
            c.map.contains_key(&mru),
            "MRU must survive soft reclamp (Bergen→Førde eco→non-eco)"
        );
        assert!(!c.map.contains_key(&older) || c.order.last() == Some(&mru));
        // Near-LMK path may clear even the MRU.
        c.reclamp_keep_mru(100, true);
        assert!(c.map.is_empty());
    }
}
