//! Process-wide LRU of **full** materialized tiles (no corridor clip in the key).
//!
//! Tromsø densify hops change corridor clips every leg but reuse the same tile
//! files. Caching by stem+tile path+format lets assemble-from-cache skip
//! mmap/page-in/copy on shared tiles without changing which edges enter the
//! corridor (clips still applied when assembling).
//!
//! **Default: disabled.** On the SM-P613 tablet (~3.5 GiB MemTotal) the fit gate
//! rejects most Ostlandet/Vestlandet tiles (~11% hit rate, dozens of rejects per
//! Tromsø plan) so the LRU does not earn its RSS. Re-enable for experiments with
//! `NAVI_TILE_CACHE=1` (or `true` / `yes` / `on`).

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::routing::graph::{RouteGraph, RoutingProfile};
use crate::routing::indexed::corridor_cache::{
    corridor_cache_max_bytes_from_mem, estimate_graph_bytes, read_mem_available_bytes,
    CORRIDOR_CACHE_CRITICAL_AVAIL_BYTES, CORRIDOR_CACHE_HARD_MAX_BYTES, CORRIDOR_CACHE_MIN_BYTES,
};

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct TileCacheKey {
    pub profile: u8,
    pub path: String,
}

impl TileCacheKey {
    pub fn new(profile: RoutingProfile, path: &Path) -> Self {
        let profile = match profile {
            RoutingProfile::Car => 0,
            RoutingProfile::Bicycle => 1,
            RoutingProfile::Foot => 2,
            RoutingProfile::Truck => 3,
        };
        Self {
            profile,
            path: path.to_string_lossy().into_owned(),
        }
    }
}

struct Entry {
    graph: Arc<RouteGraph>,
    bytes: u64,
}

struct TileLru {
    map: HashMap<TileCacheKey, Entry>,
    order: Vec<TileCacheKey>,
    bytes: u64,
    max_bytes: u64,
    hits: u64,
    misses: u64,
}

impl TileLru {
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

    fn get(&mut self, key: &TileCacheKey) -> Option<Arc<RouteGraph>> {
        if let Some(i) = self.order.iter().position(|k| k == key) {
            let k = self.order.remove(i);
            self.order.push(k);
        }
        match self.map.get(key) {
            Some(e) => {
                self.hits = self.hits.saturating_add(1);
                Some(Arc::clone(&e.graph))
            }
            None => {
                self.misses = self.misses.saturating_add(1);
                None
            }
        }
    }

    fn insert(&mut self, key: TileCacheKey, graph: Arc<RouteGraph>, bytes: u64) {
        if let Some(old) = self.map.remove(&key) {
            self.bytes = self.bytes.saturating_sub(old.bytes);
            self.order.retain(|k| k != &key);
        }
        while self.bytes.saturating_add(bytes) > self.max_bytes && !self.order.is_empty() {
            let victim = self.order.remove(0);
            if let Some(e) = self.map.remove(&victim) {
                self.bytes = self.bytes.saturating_sub(e.bytes);
            }
        }
        if bytes > self.max_bytes {
            // Too large for the cache; do not retain.
            return;
        }
        self.bytes = self.bytes.saturating_add(bytes);
        self.order.push(key.clone());
        self.map.insert(key, Entry { graph, bytes });
    }

    fn reclamp_keep_mru(&mut self, max_bytes: u64, wipe: bool) {
        self.max_bytes = max_bytes.max(CORRIDOR_CACHE_MIN_BYTES);
        if wipe {
            self.map.clear();
            self.order.clear();
            self.bytes = 0;
            return;
        }
        while self.bytes > self.max_bytes && self.order.len() > 1 {
            let victim = self.order.remove(0);
            if let Some(e) = self.map.remove(&victim) {
                self.bytes = self.bytes.saturating_sub(e.bytes);
            }
        }
    }
}

static CACHE: Mutex<Option<TileLru>> = Mutex::new(None);
static ENABLED_CACHED: AtomicBool = AtomicBool::new(false);
static ENABLED_RESOLVED: AtomicBool = AtomicBool::new(false);

/// Whether the full-tile LRU is active. Default off; `NAVI_TILE_CACHE=1` enables.
pub fn tile_cache_enabled() -> bool {
    if ENABLED_RESOLVED.load(Ordering::Relaxed) {
        return ENABLED_CACHED.load(Ordering::Relaxed);
    }
    let on = match std::env::var("NAVI_TILE_CACHE") {
        Ok(v) => {
            let t = v.trim();
            t == "1"
                || t.eq_ignore_ascii_case("true")
                || t.eq_ignore_ascii_case("yes")
                || t.eq_ignore_ascii_case("on")
        }
        Err(_) => false,
    };
    ENABLED_CACHED.store(on, Ordering::Relaxed);
    ENABLED_RESOLVED.store(true, Ordering::Relaxed);
    on
}

fn tile_cache_max_bytes_from_mem() -> u64 {
    // Share the same soft budget as corridor cache, but leave headroom for the
    // assembled corridor Arc that still lives beside cached tiles.
    let half = corridor_cache_max_bytes_from_mem() / 2;
    half.clamp(CORRIDOR_CACHE_MIN_BYTES, CORRIDOR_CACHE_HARD_MAX_BYTES / 2)
}

fn with_cache<R>(f: impl FnOnce(&mut TileLru) -> R) -> R {
    let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let max = tile_cache_max_bytes_from_mem();
    let near_lmk = read_mem_available_bytes()
        .map(|a| a > 0 && a < CORRIDOR_CACHE_CRITICAL_AVAIL_BYTES)
        .unwrap_or(false);
    if guard.is_none() {
        crate::routing::plan_perf::note_u64("tile_cache_cap_mb", max / (1024 * 1024));
        *guard = Some(TileLru::new(max));
    } else if let Some(c) = guard.as_mut() {
        if c.max_bytes != max || near_lmk {
            crate::routing::plan_perf::note_u64("tile_cache_cap_mb", max / (1024 * 1024));
            c.reclamp_keep_mru(max, near_lmk);
        }
    }
    f(guard.as_mut().expect("tile cache initialized"))
}

pub fn tile_cache_get(key: &TileCacheKey) -> Option<Arc<RouteGraph>> {
    if !tile_cache_enabled() {
        return None;
    }
    with_cache(|c| c.get(key))
}

pub fn tile_cache_insert(key: TileCacheKey, graph: Arc<RouteGraph>) {
    if !tile_cache_enabled() {
        return;
    }
    let bytes = estimate_graph_bytes(&graph);
    with_cache(|c| c.insert(key, graph, bytes));
}

pub fn tile_cache_stats() -> (u64, u64, u64, usize) {
    if !tile_cache_enabled() {
        return (0, 0, 0, 0);
    }
    with_cache(|c| (c.hits, c.misses, c.bytes, c.map.len()))
}

pub fn tile_cache_clear() {
    if !tile_cache_enabled() {
        return;
    }
    with_cache(|c| {
        c.map.clear();
        c.order.clear();
        c.bytes = 0;
    });
}

#[allow(dead_code)] // used by load heuristics / future pack-stage notes
pub fn tile_cache_max_bytes() -> u64 {
    tile_cache_max_bytes_from_mem()
}

/// True when an on-disk tile is small enough that a full materialize is likely
/// to fit in the tile LRU (avoids paying full-tile copy then discarding).
pub fn tile_likely_fits_cache(path: &Path) -> bool {
    if !tile_cache_enabled() {
        return false;
    }
    let max = tile_cache_max_bytes_from_mem();
    let file_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(u64::MAX);
    // Owned graph ≫ archive. Only cache-full-tile when the rkyv file is a small
    // fraction of the LRU so insert is not immediately discarded.
    file_bytes.saturating_mul(4) < max && file_bytes < 12 * 1024 * 1024
}

pub fn tile_cache_evict_before_load() {
    if !tile_cache_enabled() {
        crate::routing::plan_perf::note("tile_cache", "disabled");
        return;
    }
    let avail = read_mem_available_bytes().unwrap_or(0);
    let near_lmk = avail > 0 && avail < CORRIDOR_CACHE_CRITICAL_AVAIL_BYTES;
    if near_lmk {
        crate::routing::plan_perf::note_u64("tile_cache_evict_avail_mb", avail / (1024 * 1024));
        tile_cache_clear();
        return;
    }
    with_cache(|c| {
        let max = tile_cache_max_bytes_from_mem();
        c.reclamp_keep_mru(max, false);
    });
}

/// Keep edges whose endpoints fall in any clip box (same rule as pack hydrate).
pub fn clip_route_graph(graph: &RouteGraph, clips: Option<&[[f64; 4]]>) -> RouteGraph {
    let Some(clips) = clips.filter(|c| !c.is_empty()) else {
        // Full tile — clone only when the caller needs ownership; prefer Arc share
        // at call site. Here return a structural clone for merge assembly.
        return graph.clone();
    };
    let edge_ok = |e: &crate::routing::graph::GraphEdge| {
        super::graph_pack::clip_keeps_edge(e.start_lat, e.start_lon, e.end_lat, e.end_lon, clips)
    };
    let mut nodes = HashMap::new();
    let mut edges = Vec::new();
    for e in &graph.edges {
        if !edge_ok(e) {
            continue;
        }
        if let Some(n) = graph.nodes.get(&e.source) {
            nodes.insert(e.source, *n);
        }
        if let Some(n) = graph.nodes.get(&e.target) {
            nodes.insert(e.target, *n);
        }
        edges.push(e.clone());
    }
    RouteGraph::from_parts(nodes, edges, graph.profile())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn tiny() -> RouteGraph {
        RouteGraph::from_parts(HashMap::new(), Vec::new(), RoutingProfile::Car)
    }

    #[test]
    fn tile_lru_hit_and_disabled_gate() {
        // Force-enable for unit coverage of the LRU itself.
        ENABLED_CACHED.store(true, Ordering::Relaxed);
        ENABLED_RESOLVED.store(true, Ordering::Relaxed);
        tile_cache_clear();
        let key = TileCacheKey::new(RoutingProfile::Car, Path::new("/tmp/t0.rkyv"));
        assert!(tile_cache_get(&key).is_none());
        tile_cache_insert(key.clone(), Arc::new(tiny()));
        assert!(tile_cache_get(&key).is_some());
        let (hits, misses, _, _) = tile_cache_stats();
        assert!(hits >= 1);
        assert!(misses >= 1);
        tile_cache_clear();

        // Disabled path: insert/get/fit are no-ops (keeps tablet default).
        ENABLED_CACHED.store(false, Ordering::Relaxed);
        ENABLED_RESOLVED.store(true, Ordering::Relaxed);
        let key_off = TileCacheKey::new(RoutingProfile::Car, Path::new("/tmp/t_off.rkyv"));
        tile_cache_insert(key_off.clone(), Arc::new(tiny()));
        assert!(tile_cache_get(&key_off).is_none());
        assert!(!tile_likely_fits_cache(Path::new("/tmp/t_off.rkyv")));
    }
}
