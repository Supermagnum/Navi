//! Wetland hazard classification shared by on-trail graph weighting and
//! off-trail terrain cost surfaces.
//!
//! Soft-avoid (`bog` / `string_bog` / `fen`): penalize, do not block.
//! Hard-avoid (`swamp` / `reedbed`): exclude by default.
//! Boardwalk carve-out: graph edges tagged `bridge=boardwalk` or `surface=wood`
//! remain usable even when crossing hard wetlands. Terrain cells have no
//! carve-out (no built infrastructure outside the OSM way graph).

use std::collections::{HashMap, HashSet};
use std::path::Path;

use osmpbf::{Element, RelMemberType};

/// Soft-avoid cost multiplier applied to graph edges and terrain cells.
pub const WETLAND_SOFT_COST_MULT: f64 = 5.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WetlandClass {
    SoftAvoid,
    HardAvoid,
}

/// Classify an OSM `wetland=*` value (or equivalent).
pub fn classify_wetland_value(raw: &str) -> Option<WetlandClass> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "bog" | "string_bog" | "fen" => Some(WetlandClass::SoftAvoid),
        "swamp" | "reedbed" => Some(WetlandClass::HardAvoid),
        _ => None,
    }
}

/// True when OSM tags indicate a built boardwalk / wooden crossing.
pub fn tags_indicate_boardwalk(bridge: Option<&str>, surface: Option<&str>) -> bool {
    let bridge_ok = bridge
        .map(|v| v.trim().eq_ignore_ascii_case("boardwalk"))
        .unwrap_or(false);
    let surface_ok = surface
        .map(|v| v.trim().eq_ignore_ascii_case("wood"))
        .unwrap_or(false);
    bridge_ok || surface_ok
}

/// Convenience for tag maps (PBF / osm4routing).
pub fn tags_map_indicate_boardwalk(tags: &HashMap<String, String>) -> bool {
    tags_indicate_boardwalk(
        tags.get("bridge").map(String::as_str),
        tags.get("surface").map(String::as_str),
    )
}

fn classify_tags(tags: &HashMap<String, String>) -> Option<WetlandClass> {
    if let Some(w) = tags.get("wetland") {
        if let Some(c) = classify_wetland_value(w) {
            return Some(c);
        }
    }
    // natural=wetland without subtype: treat as soft-avoid (common bog/mire mapping).
    if tags
        .get("natural")
        .is_some_and(|v| v.eq_ignore_ascii_case("wetland"))
    {
        return Some(WetlandClass::SoftAvoid);
    }
    None
}

#[derive(Debug, Clone)]
struct WetlandRing {
    class: WetlandClass,
    /// Closed ring as `[lon, lat]`.
    ring: Vec<[f64; 2]>,
    min_lon: f64,
    max_lon: f64,
    min_lat: f64,
    max_lat: f64,
}

fn ring_bounds(ring: &[[f64; 2]]) -> (f64, f64, f64, f64) {
    let mut min_lon = f64::INFINITY;
    let mut max_lon = f64::NEG_INFINITY;
    let mut min_lat = f64::INFINITY;
    let mut max_lat = f64::NEG_INFINITY;
    for p in ring {
        min_lon = min_lon.min(p[0]);
        max_lon = max_lon.max(p[0]);
        min_lat = min_lat.min(p[1]);
        max_lat = max_lat.max(p[1]);
    }
    (min_lon, max_lon, min_lat, max_lat)
}

/// Spatial index of wetland polygons for point queries.
#[derive(Debug, Default, Clone)]
pub struct WetlandIndex {
    rings: Vec<WetlandRing>,
}

impl WetlandIndex {
    pub fn is_empty(&self) -> bool {
        self.rings.is_empty()
    }

    pub fn ring_count(&self) -> usize {
        self.rings.len()
    }

    /// Build from owned `(class, closed ring [lon,lat])` parts (indexed-pack path).
    pub fn from_parts(parts: Vec<(WetlandClass, Vec<[f64; 2]>)>) -> Self {
        Self {
            rings: parts
                .into_iter()
                .filter(|(_, ring)| ring.len() >= 3)
                .map(|(class, ring)| {
                    let (min_lon, max_lon, min_lat, max_lat) = ring_bounds(&ring);
                    WetlandRing {
                        class,
                        ring,
                        min_lon,
                        max_lon,
                        min_lat,
                        max_lat,
                    }
                })
                .collect(),
        }
    }

    /// Export rings for archive serialization.
    pub fn rings_as_parts(&self) -> Vec<(WetlandClass, Vec<[f64; 2]>)> {
        self.rings
            .iter()
            .map(|r| (r.class, r.ring.clone()))
            .collect()
    }

    pub fn class_at(&self, lat: f64, lon: f64) -> Option<WetlandClass> {
        let p = [lon, lat];
        let mut best: Option<WetlandClass> = None;
        for r in &self.rings {
            // AABB reject before expensive point-in-polygon.
            if lon < r.min_lon || lon > r.max_lon || lat < r.min_lat || lat > r.max_lat {
                continue;
            }
            if point_in_ring(p, &r.ring) {
                match (best, r.class) {
                    (None, c) => best = Some(c),
                    (Some(WetlandClass::SoftAvoid), WetlandClass::HardAvoid) => {
                        best = Some(WetlandClass::HardAvoid);
                    }
                    _ => {}
                }
            }
        }
        best
    }

    /// Load closed wetland ways (and multipolygon outers) that touch `bbox`.
    pub fn load_from_pbf_bbox(path: impl AsRef<Path>, bbox: [f64; 4]) -> anyhow::Result<Self> {
        let extract = WetlandWayExtract::load(path.as_ref())?;
        Ok(extract.index_for_bbox(bbox))
    }

    /// Region-wide load (full extract extents). Used by the indexed converter.
    pub fn load_from_pbf(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        // Wide bbox: accept every ring that has at least one resolved node.
        Self::load_from_pbf_bbox(path, [-90.0, -180.0, 90.0, 180.0])
    }
}

/// Shared PBF wetland way/coord extract so tiled converts can emit one tile pack
/// at a time without holding a full-region [`WetlandIndex`].
pub struct WetlandWayExtract {
    ways: Vec<(Vec<i64>, WetlandClass)>,
    way_nodes: HashMap<i64, Vec<i64>>,
    rel_outers: Vec<(WetlandClass, Vec<i64>)>,
    coords: HashMap<i64, (f64, f64)>,
}

impl WetlandWayExtract {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let mut ways: Vec<(Vec<i64>, WetlandClass)> = Vec::new();
        let mut needed: HashSet<i64> = HashSet::new();
        let mut rel_outers: Vec<(WetlandClass, Vec<i64>)> = Vec::new();
        let mut outer_way_ids: HashSet<i64> = HashSet::new();

        crate::download::pbf_priority::for_each_pbf_elements(path, |element| match element {
            Element::Way(way) => {
                let tags: HashMap<String, String> = way
                    .tags()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();
                let Some(class) = classify_tags(&tags) else {
                    return;
                };
                let refs: Vec<i64> = way.refs().collect();
                if refs.len() < 3 {
                    return;
                }
                for id in &refs {
                    needed.insert(*id);
                }
                ways.push((refs, class));
            }
            Element::Relation(rel) => {
                let tags: HashMap<String, String> = rel
                    .tags()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();
                let Some(class) = classify_tags(&tags) else {
                    return;
                };
                let mut outers = Vec::new();
                for m in rel.members() {
                    let role = m.role().unwrap_or("");
                    if m.member_type == RelMemberType::Way && role.eq_ignore_ascii_case("outer") {
                        outers.push(m.member_id);
                        outer_way_ids.insert(m.member_id);
                    }
                }
                if !outers.is_empty() {
                    rel_outers.push((class, outers));
                }
            }
            _ => {}
        })?;

        let mut way_nodes: HashMap<i64, Vec<i64>> = HashMap::new();
        if !outer_way_ids.is_empty() {
            crate::download::pbf_priority::for_each_pbf_elements(path, |element| {
                let Element::Way(way) = element else {
                    return;
                };
                if !outer_way_ids.contains(&way.id()) {
                    return;
                }
                let refs: Vec<i64> = way.refs().collect();
                if refs.len() < 3 {
                    return;
                }
                for id in &refs {
                    needed.insert(*id);
                }
                way_nodes.insert(way.id(), refs);
            })?;
        }

        let mut coords: HashMap<i64, (f64, f64)> = HashMap::with_capacity(needed.len());
        crate::download::pbf_priority::for_each_pbf_elements(path, |element| match element {
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

        Ok(Self {
            ways,
            way_nodes,
            rel_outers,
            coords,
        })
    }

    pub fn index_for_bbox(&self, bbox: [f64; 4]) -> WetlandIndex {
        let mut rings = Vec::new();
        self.for_each_resolved_ring(|class, ring| {
            if ring_touches_bbox(ring, bbox) {
                rings.push(wetland_ring_from_parts(class, ring.to_vec()));
            }
        });
        WetlandIndex { rings }
    }

    /// Assign each resolved wetland ring to every tile that contains at least
    /// one of its vertices — the same membership rule as calling
    /// [`Self::index_for_bbox`] once per tile, but with a single walk over ways.
    ///
    /// Output length equals `tiles.len()`; empty tiles get an empty index.
    /// Rings that span tile boundaries are duplicated into each touched tile
    /// (matching the prior per-tile rewalk behavior).
    pub fn indexes_for_tiles(&self, tiles: &[(usize, usize, [f64; 4])]) -> Vec<WetlandIndex> {
        let n = tiles.len();
        let mut per_tile: Vec<Vec<WetlandRing>> = (0..n).map(|_| Vec::new()).collect();
        self.for_each_resolved_ring(|class, ring| {
            for (i, (_, _, bbox)) in tiles.iter().enumerate() {
                if ring_touches_bbox(ring, *bbox) {
                    per_tile[i].push(wetland_ring_from_parts(class, ring.to_vec()));
                }
            }
        });
        per_tile
            .into_iter()
            .map(|rings| WetlandIndex { rings })
            .collect()
    }

    fn for_each_resolved_ring(&self, mut f: impl FnMut(WetlandClass, &[[f64; 2]])) {
        for (refs, class) in &self.ways {
            if let Some(ring) = ring_from_refs_resolved(refs, &self.coords) {
                f(*class, &ring);
            }
        }
        for (class, outers) in &self.rel_outers {
            for wid in outers {
                let Some(refs) = self.way_nodes.get(wid) else {
                    continue;
                };
                if let Some(ring) = ring_from_refs_resolved(refs, &self.coords) {
                    f(*class, &ring);
                }
            }
        }
    }
}

fn wetland_ring_from_parts(class: WetlandClass, ring: Vec<[f64; 2]>) -> WetlandRing {
    let (min_lon, max_lon, min_lat, max_lat) = ring_bounds(&ring);
    WetlandRing {
        class,
        ring,
        min_lon,
        max_lon,
        min_lat,
        max_lat,
    }
}

/// Resolve all node coords and close the ring. Does not apply a bbox filter.
fn ring_from_refs_resolved(
    refs: &[i64],
    coords: &HashMap<i64, (f64, f64)>,
) -> Option<Vec<[f64; 2]>> {
    if refs.len() < 3 {
        return None;
    }
    let mut ring: Vec<[f64; 2]> = Vec::with_capacity(refs.len() + 1);
    for id in refs {
        let &(lat, lon) = coords.get(id)?;
        ring.push([lon, lat]);
    }
    if ring.len() < 3 {
        return None;
    }
    let first = ring[0];
    let last = *ring.last().unwrap();
    if first != last {
        ring.push(first);
    }
    Some(ring)
}

fn ring_touches_bbox(ring: &[[f64; 2]], bbox: [f64; 4]) -> bool {
    ring.iter().any(|p| in_bbox(p[1], p[0], bbox))
}

fn in_bbox(lat: f64, lon: f64, bbox: [f64; 4]) -> bool {
    lat >= bbox[0] && lat <= bbox[2] && lon >= bbox[1] && lon <= bbox[3]
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_soft_and_hard() {
        assert_eq!(classify_wetland_value("bog"), Some(WetlandClass::SoftAvoid));
        assert_eq!(
            classify_wetland_value("string_bog"),
            Some(WetlandClass::SoftAvoid)
        );
        assert_eq!(classify_wetland_value("fen"), Some(WetlandClass::SoftAvoid));
        assert_eq!(
            classify_wetland_value("swamp"),
            Some(WetlandClass::HardAvoid)
        );
        assert_eq!(
            classify_wetland_value("reedbed"),
            Some(WetlandClass::HardAvoid)
        );
        assert_eq!(classify_wetland_value("marsh"), None);
    }

    #[test]
    fn boardwalk_carve_out_tags() {
        assert!(tags_indicate_boardwalk(Some("boardwalk"), None));
        assert!(tags_indicate_boardwalk(None, Some("wood")));
        assert!(tags_indicate_boardwalk(Some("Boardwalk"), Some("asphalt")));
        assert!(!tags_indicate_boardwalk(Some("yes"), None));
        assert!(!tags_indicate_boardwalk(None, Some("gravel")));
    }

    #[test]
    fn point_in_wetland_ring() {
        let idx = WetlandIndex::from_parts(vec![(
            WetlandClass::HardAvoid,
            vec![
                [10.0, 60.0],
                [10.2, 60.0],
                [10.2, 60.2],
                [10.0, 60.2],
                [10.0, 60.0],
            ],
        )]);
        assert_eq!(idx.class_at(60.1, 10.1), Some(WetlandClass::HardAvoid));
        assert_eq!(idx.class_at(60.5, 10.5), None);
    }

    #[test]
    fn indexes_for_tiles_matches_per_tile_rewalk() {
        // Synthetic extract: one ring entirely in tile A, one spanning A|B.
        let mut coords = HashMap::new();
        // Tile A ring: lon 10.1–10.2, lat 60.1–60.2
        for (id, lat, lon) in [
            (1, 60.1, 10.1),
            (2, 60.1, 10.2),
            (3, 60.2, 10.2),
            (4, 60.2, 10.1),
        ] {
            coords.insert(id, (lat, lon));
        }
        // Spanning ring: vertices in both lon bands around 11.0
        for (id, lat, lon) in [
            (10, 60.1, 10.8),
            (11, 60.1, 11.2),
            (12, 60.2, 11.2),
            (13, 60.2, 10.8),
        ] {
            coords.insert(id, (lat, lon));
        }
        let extract = WetlandWayExtract {
            ways: vec![
                (vec![1, 2, 3, 4, 1], WetlandClass::SoftAvoid),
                (vec![10, 11, 12, 13, 10], WetlandClass::HardAvoid),
            ],
            way_nodes: HashMap::new(),
            rel_outers: Vec::new(),
            coords,
        };
        let tiles = vec![
            (0, 0, [60.0, 10.0, 61.0, 11.0]),
            (0, 1, [60.0, 11.0, 61.0, 12.0]),
        ];
        let once = extract.indexes_for_tiles(&tiles);
        let rewalk: Vec<_> = tiles
            .iter()
            .map(|(_, _, b)| extract.index_for_bbox(*b))
            .collect();
        assert_eq!(once.len(), 2);
        assert_eq!(once[0].ring_count(), rewalk[0].ring_count());
        assert_eq!(once[1].ring_count(), rewalk[1].ring_count());
        assert_eq!(once[0].ring_count(), 2); // A-only + spanning
        assert_eq!(once[1].ring_count(), 1); // spanning only
        assert_eq!(
            once[0].class_at(60.15, 10.15),
            Some(WetlandClass::SoftAvoid)
        );
        assert_eq!(once[0].class_at(60.15, 10.9), Some(WetlandClass::HardAvoid));
        assert_eq!(once[1].class_at(60.15, 11.1), Some(WetlandClass::HardAvoid));
    }
}
