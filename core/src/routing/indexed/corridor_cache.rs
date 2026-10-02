//! Process-wide LRU of merged corridor graphs so replan/reroute skips pack load.
//!
//! Tiles are mmapped only while materializing; the owned [`RouteGraph`] is what
//! stays in RSS (~nodes+edges). Caching the **merged** corridor avoids repeating
//! that materialize+merge (~4 s cold) on warm plans with the same tile set.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use crate::routing::graph::{RouteGraph, RoutingProfile};

/// Soft bound on cached corridor graph bytes (estimate). Evict LRU past this.
/// Raufoss→Bergen merged car corridors estimate ~900–1100 MiB owned; keep headroom
/// for one large corridor plus a smaller control route.
pub const CORRIDOR_CACHE_MAX_BYTES: u64 = 1536 * 1024 * 1024;

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

    fn take(&mut self, key: &CorridorCacheKey) -> Option<Entry> {
        if let Some(i) = self.order.iter().position(|k| k == key) {
            self.order.remove(i);
        }
        match self.map.remove(key) {
            Some(entry) => {
                self.bytes = self.bytes.saturating_sub(entry.bytes);
                self.hits = self.hits.saturating_add(1);
                Some(entry)
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
        // Evict LRU until the new entry fits, but always keep capacity for one
        // corridor (even if oversized) so Raufoss→Bergen can warm-cache.
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
}

static CACHE: Mutex<Option<CorridorLru>> = Mutex::new(None);

fn with_cache<R>(f: impl FnOnce(&mut CorridorLru) -> R) -> R {
    let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        *guard = Some(CorridorLru::new(CORRIDOR_CACHE_MAX_BYTES));
    }
    f(guard.as_mut().expect("corridor cache initialized"))
}

/// Estimate owned-graph RSS contribution (nodes + edges + adjacency overhead).
pub fn estimate_graph_bytes(g: &RouteGraph) -> u64 {
    let nodes = g.nodes.len() as u64;
    let edges = g.edges.len() as u64;
    // Empirical ~1.6–2.0 KiB per edge on car packs once strings/shapes land;
    // keep a conservative mid so the LRU stays under device RAM pressure.
    nodes
        .saturating_mul(96)
        .saturating_add(edges.saturating_mul(1800))
}

pub fn corridor_cache_take(key: &CorridorCacheKey) -> Option<RouteGraph> {
    with_cache(|c| {
        let entry = c.take(key)?;
        match Arc::try_unwrap(entry.graph) {
            Ok(g) => Some(g),
            Err(arc) => Some((*arc).clone()),
        }
    })
}

pub fn corridor_cache_insert(key: CorridorCacheKey, graph: Arc<RouteGraph>) {
    let bytes = estimate_graph_bytes(&graph);
    with_cache(|c| c.insert(key, graph, bytes));
}

/// Insert an owned merged corridor (warm put-back after a plan finishes).
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
    fn lru_hit_and_evict() {
        corridor_cache_clear();
        let key = CorridorCacheKey::new(RoutingProfile::Car, vec!["a.rkyv".into()], None);
        assert!(corridor_cache_take(&key).is_none());
        let g = tiny_graph();
        corridor_cache_insert_owned(key.clone(), g);
        assert!(corridor_cache_take(&key).is_some());
        // Second take misses (moved out).
        assert!(corridor_cache_take(&key).is_none());
        let (hits, misses, _, _) = corridor_cache_stats();
        assert!(hits >= 1);
        assert!(misses >= 1);
        with_cache(|c| c.max_bytes = 1);
        let key2 = CorridorCacheKey::new(RoutingProfile::Car, vec!["b.rkyv".into()], None);
        corridor_cache_insert_owned(key2, tiny_graph());
        let _ = NodeId(0);
        corridor_cache_clear();
    }
}
