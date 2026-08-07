//! Dangerous barriers that block crow-flies break-stop access:
//! railways, major highways, rivers/canals, cliffs, and glaciers.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use osmpbf::{Element, ElementReader};
use rstar::{RTree, RTreeObject, AABB};

use super::super::graph::RouteGraph;

#[derive(Debug, Clone, Copy)]
struct BarrierSeg {
    /// Endpoints as `[lon, lat]`.
    a: [f64; 2],
    b: [f64; 2],
}

impl RTreeObject for BarrierSeg {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        AABB::from_corners(self.a, self.b)
    }
}

/// Spatial index of dangerous linear barriers + glacier rings.
pub struct DangerBarrierIndex {
    tree: RTree<BarrierSeg>,
    /// Closed glacier rings as `[lon, lat]` rings (first point may equal last).
    glaciers: Vec<Vec<[f64; 2]>>,
}

impl Default for DangerBarrierIndex {
    fn default() -> Self {
        Self {
            tree: RTree::new(),
            glaciers: Vec::new(),
        }
    }
}

impl DangerBarrierIndex {
    pub fn is_empty(&self) -> bool {
        self.tree.size() == 0 && self.glaciers.is_empty()
    }

    /// Number of glacier rings (for tests / diagnostics).
    pub fn glacier_ring_count(&self) -> usize {
        self.glaciers.len()
    }

    /// Sample points for overnight distance checks (ring centroids).
    ///
    /// Reuses glacier geometry already loaded for barrier blocking — avoids a
    /// second PBF pass dedicated to overnight proximity.
    pub fn glacier_sample_points(&self) -> Vec<(f64, f64)> {
        self.glaciers
            .iter()
            .filter_map(|ring| {
                if ring.is_empty() {
                    return None;
                }
                let n = ring.len() as f64;
                let mut sum_lon = 0.0;
                let mut sum_lat = 0.0;
                for p in ring {
                    sum_lon += p[0];
                    sum_lat += p[1];
                }
                Some((sum_lat / n, sum_lon / n))
            })
            .collect()
    }

    /// True when the straight line `(lat0,lon0) → (lat1,lon1)` crosses a
    /// dangerous barrier, or either endpoint sits on a glacier.
    pub fn blocks_access(&self, lat0: f64, lon0: f64, lat1: f64, lon1: f64) -> bool {
        if self.point_in_glacier(lat0, lon0) || self.point_in_glacier(lat1, lon1) {
            return true;
        }
        if self.tree.size() == 0 {
            return false;
        }
        let p0 = [lon0, lat0];
        let p1 = [lon1, lat1];
        let env = AABB::from_corners(p0, p1);
        for seg in self.tree.locate_in_envelope_intersecting(&env) {
            if segments_intersect(p0, p1, seg.a, seg.b) {
                return true;
            }
        }
        false
    }

    fn point_in_glacier(&self, lat: f64, lon: f64) -> bool {
        let p = [lon, lat];
        self.glaciers.iter().any(|ring| point_in_ring(p, ring))
    }

    /// Major highways already present on the routing graph (no extra PBF scan).
    pub fn from_graph(graph: &RouteGraph) -> Self {
        let mut segs = Vec::new();
        for e in &graph.edges {
            let Some(h) = e.highway.as_deref() else {
                continue;
            };
            if !matches!(h, "motorway" | "motorway_link" | "trunk" | "trunk_link") {
                continue;
            }
            let Some(a) = graph.nodes.get(&e.source) else {
                continue;
            };
            let Some(b) = graph.nodes.get(&e.target) else {
                continue;
            };
            segs.push(BarrierSeg {
                a: [a.coord.x, a.coord.y],
                b: [b.coord.x, b.coord.y],
            });
        }
        Self {
            tree: RTree::bulk_load(segs),
            glaciers: Vec::new(),
        }
    }

    /// Merge another index into this one (consumes `other`).
    pub fn merge(&mut self, other: Self) {
        let mut segs: Vec<BarrierSeg> = self.tree.iter().copied().collect();
        segs.extend(other.tree.iter().copied());
        self.tree = RTree::bulk_load(segs);
        self.glaciers.extend(other.glaciers);
    }

    /// Rebuild from flat segment endpoints + glacier rings (indexed-pack load).
    ///
    /// Each segment is `(a_lon, a_lat, b_lon, b_lat)`. Glacier rings are
    /// closed `[lon, lat]` rings.
    pub fn from_segments(
        segments: impl IntoIterator<Item = (f64, f64, f64, f64)>,
        glaciers: Vec<Vec<[f64; 2]>>,
    ) -> Self {
        let segs: Vec<BarrierSeg> = segments
            .into_iter()
            .map(|(a_lon, a_lat, b_lon, b_lat)| BarrierSeg {
                a: [a_lon, a_lat],
                b: [b_lon, b_lat],
            })
            .collect();
        Self {
            tree: RTree::bulk_load(segs),
            glaciers,
        }
    }

    /// Load railway / river / cliff / glacier ways that touch `bbox`.
    ///
    /// Avoids the expensive “index every node in bbox” pass: collects danger
    /// ways first, then only resolves those node coordinates.
    pub fn load_from_pbf_bbox(path: impl AsRef<Path>, bbox: [f64; 4]) -> anyhow::Result<Self> {
        let path = path.as_ref();
        let mut ways: Vec<(Vec<i64>, BarrierKind)> = Vec::new();
        let mut needed: HashSet<i64> = HashSet::new();
        {
            let file = std::fs::File::open(path)?;
            let reader = ElementReader::new(file);
            reader.for_each(|element| {
                let Element::Way(way) = element else {
                    return;
                };
                let tags: HashMap<String, String> = way
                    .tags()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();
                let Some(kind) = classify_pbf_barrier(&tags) else {
                    return;
                };
                let refs: Vec<i64> = way.refs().collect();
                if refs.len() < 2 {
                    return;
                }
                for id in &refs {
                    needed.insert(*id);
                }
                ways.push((refs, kind));
            })?;
        }

        let mut coords: HashMap<i64, (f64, f64)> = HashMap::with_capacity(needed.len());
        {
            let file = std::fs::File::open(path)?;
            let reader = ElementReader::new(file);
            reader.for_each(|element| match element {
                Element::Node(n) => {
                    if needed.contains(&n.id()) {
                        coords.insert(n.id(), (n.lat(), n.lon()));
                    }
                }
                Element::DenseNode(n) => {
                    if needed.contains(&n.id()) {
                        coords.insert(n.id(), (n.lat(), n.lon()));
                    }
                }
                _ => {}
            })?;
        }
        drop(needed);

        let mut segs = Vec::new();
        let mut glaciers = Vec::new();
        for (refs, kind) in ways {
            let mut ring: Vec<[f64; 2]> = Vec::with_capacity(refs.len());
            let mut any_in_bbox = false;
            for id in &refs {
                let Some(&(lat, lon)) = coords.get(id) else {
                    continue;
                };
                if in_bbox(lat, lon, bbox) {
                    any_in_bbox = true;
                }
                ring.push([lon, lat]);
            }
            if !any_in_bbox || ring.len() < 2 {
                continue;
            }
            match kind {
                BarrierKind::Line => {
                    for w in ring.windows(2) {
                        segs.push(BarrierSeg { a: w[0], b: w[1] });
                    }
                }
                BarrierKind::Glacier => {
                    for w in ring.windows(2) {
                        segs.push(BarrierSeg { a: w[0], b: w[1] });
                    }
                    // Close ring if OSM left it open.
                    if ring.len() >= 3 {
                        let first = ring[0];
                        let last = *ring.last().unwrap();
                        if first != last {
                            segs.push(BarrierSeg { a: last, b: first });
                            ring.push(first);
                        }
                        glaciers.push(ring);
                    }
                }
            }
        }

        Ok(Self {
            tree: RTree::bulk_load(segs),
            glaciers,
        })
    }
}

#[derive(Clone, Copy)]
enum BarrierKind {
    Line,
    Glacier,
}

fn in_bbox(lat: f64, lon: f64, bbox: [f64; 4]) -> bool {
    lat >= bbox[0] && lat <= bbox[2] && lon >= bbox[1] && lon <= bbox[3]
}

/// PBF-only dangers (highways come from [`DangerBarrierIndex::from_graph`]).
fn classify_pbf_barrier(tags: &HashMap<String, String>) -> Option<BarrierKind> {
    if let Some(r) = tags.get("railway").map(String::as_str) {
        if !matches!(r, "abandoned" | "disused" | "razed" | "dismantled") {
            return Some(BarrierKind::Line);
        }
    }
    if let Some(w) = tags.get("waterway").map(String::as_str) {
        if matches!(w, "river" | "canal" | "tidal_channel" | "fairway") {
            return Some(BarrierKind::Line);
        }
    }
    if let Some(n) = tags.get("natural").map(String::as_str) {
        match n {
            "cliff" | "arete" => return Some(BarrierKind::Line),
            "glacier" => return Some(BarrierKind::Glacier),
            _ => {}
        }
    }
    None
}

/// Ray-casting point-in-polygon. `ring` is `[lon, lat]`.
fn point_in_ring(p: [f64; 2], ring: &[[f64; 2]]) -> bool {
    if ring.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = ring.len() - 1;
    for i in 0..ring.len() {
        let pi = ring[i];
        let pj = ring[j];
        let intersect = ((pi[1] > p[1]) != (pj[1] > p[1]))
            && (p[0] < (pj[0] - pi[0]) * (p[1] - pi[1]) / (pj[1] - pi[1] + f64::EPSILON) + pi[0]);
        if intersect {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Proper segment intersection (shared endpoints count as intersecting).
fn segments_intersect(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let o1 = orient(a, b, c);
    let o2 = orient(a, b, d);
    let o3 = orient(c, d, a);
    let o4 = orient(c, d, b);
    if o1 == 0.0 && on_segment(a, c, b) {
        return true;
    }
    if o2 == 0.0 && on_segment(a, d, b) {
        return true;
    }
    if o3 == 0.0 && on_segment(c, a, d) {
        return true;
    }
    if o4 == 0.0 && on_segment(c, b, d) {
        return true;
    }
    (o1 > 0.0) != (o2 > 0.0) && (o3 > 0.0) != (o4 > 0.0)
}

fn orient(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[1] - a[1]) * (c[0] - b[0]) - (b[0] - a[0]) * (c[1] - b[1])
}

fn on_segment(a: [f64; 2], p: [f64; 2], b: [f64; 2]) -> bool {
    p[0] <= a[0].max(b[0]) + 1e-12
        && p[0] + 1e-12 >= a[0].min(b[0])
        && p[1] <= a[1].max(b[1]) + 1e-12
        && p[1] + 1e-12 >= a[1].min(b[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_crossing_segments() {
        assert!(segments_intersect(
            [-1.0, 0.0],
            [1.0, 0.0],
            [0.0, -1.0],
            [0.0, 1.0]
        ));
        assert!(!segments_intersect(
            [0.0, 0.0],
            [1.0, 0.0],
            [0.0, 1.0],
            [1.0, 1.0]
        ));
    }

    #[test]
    fn blocks_access_across_indexed_barrier() {
        let idx = DangerBarrierIndex {
            tree: RTree::bulk_load(vec![BarrierSeg {
                a: [10.0, 60.0],
                b: [10.1, 60.0],
            }]),
            glaciers: Vec::new(),
        };
        assert!(idx.blocks_access(59.9, 10.05, 60.1, 10.05));
        assert!(!idx.blocks_access(60.05, 10.0, 60.05, 10.1));
    }

    #[test]
    fn glacier_interior_blocks() {
        let ring = vec![
            [10.0, 60.0],
            [10.2, 60.0],
            [10.2, 60.2],
            [10.0, 60.2],
            [10.0, 60.0],
        ];
        let idx = DangerBarrierIndex {
            tree: RTree::new(),
            glaciers: vec![ring],
        };
        assert!(idx.blocks_access(60.1, 10.1, 60.1, 10.1));
        assert!(!idx.blocks_access(60.5, 10.5, 60.5, 10.5));
    }
}
