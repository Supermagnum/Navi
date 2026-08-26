//! Containing municipality and nearby sub-area for place-index rows.
//!
//! Computed once at index-build time from the region PBF:
//! - municipality: point-in-polygon against named `boundary=administrative`
//!   relations/ways at admin_level 6–8 (smallest containing 7/8, else 6)
//! - sub-area: nearest named hamlet / locality / neighbourhood / suburb
//!   (village as fallback) that is not the place itself
//!
//! Country-level `country_iso_at` rings are too coarse for kommune/hamlet
//! disambiguation; this uses the extract's OSM admin polygons instead.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use osmpbf::{Element, RelMemberType};
use rstar::{RTree, RTreeObject, AABB};

use crate::tracks::haversine_km;

/// Schema bump written at the end of a context-aware `load_from_pbf`.
pub const PLACE_INDEX_SCHEMA_VERSION: i32 = 2;

const SUB_AREA_MAX_M: f64 = 4_000.0;
const SUB_AREA_VILLAGE_MAX_M: f64 = 2_000.0;
const CITY_SUB_AREA_MAX_M: f64 = 2_000.0;

#[derive(Debug, Clone)]
pub(crate) struct AdminRing {
    name: String,
    admin_level: u8,
    ring: Vec<[f64; 2]>,
    min_lon: f64,
    max_lon: f64,
    min_lat: f64,
    max_lat: f64,
    area: f64,
}

#[derive(Clone)]
struct AdminBBox {
    idx: usize,
    min_lon: f64,
    max_lon: f64,
    min_lat: f64,
    max_lat: f64,
}

impl RTreeObject for AdminBBox {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        AABB::from_corners([self.min_lon, self.min_lat], [self.max_lon, self.max_lat])
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SubAreaPt {
    osm_id: i64,
    name: String,
    kind: String,
    lat: f64,
    lon: f64,
}

impl RTreeObject for SubAreaPt {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        AABB::from_point([self.lon, self.lat])
    }
}

/// Spatial lookup built from the region extract (admin rings + settlement nodes).
pub(crate) struct ContextResolver {
    rings: Vec<AdminRing>,
    admin_tree: RTree<AdminBBox>,
    sub_tree: RTree<SubAreaPt>,
}

#[derive(Debug, Clone, Default)]
pub struct PlaceContext {
    pub sub_area: String,
    pub municipality: String,
}

/// `Place, Sub-area, Municipality` with empty and duplicate parts omitted.
pub fn format_place_display(name: &str, sub_area: &str, municipality: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    for raw in [name, sub_area, municipality] {
        let t = raw.trim();
        if t.is_empty() {
            continue;
        }
        if parts.iter().any(|p| p.to_lowercase() == t.to_lowercase()) {
            continue;
        }
        parts.push(t.to_string());
    }
    parts.join(", ")
}

impl ContextResolver {
    pub(crate) fn from_admin_and_sub_areas(
        rings: Vec<AdminRing>,
        sub_areas: Vec<SubAreaPt>,
    ) -> Self {
        let admin_tree = RTree::bulk_load(
            rings
                .iter()
                .enumerate()
                .map(|(idx, r)| AdminBBox {
                    idx,
                    min_lon: r.min_lon,
                    max_lon: r.max_lon,
                    min_lat: r.min_lat,
                    max_lat: r.max_lat,
                })
                .collect(),
        );
        let sub_tree = RTree::bulk_load(sub_areas);
        Self {
            rings,
            admin_tree,
            sub_tree,
        }
    }

    pub fn resolve(&self, osm_id: i64, name: &str, kind: &str, lat: f64, lon: f64) -> PlaceContext {
        let municipality = self.municipality_at(lat, lon);
        let sub_area = self.sub_area_at(osm_id, name, kind, lat, lon, &municipality);
        PlaceContext {
            sub_area,
            municipality,
        }
    }

    fn municipality_at(&self, lat: f64, lon: f64) -> String {
        if self.rings.is_empty() {
            return String::new();
        }
        let env = AABB::from_point([lon, lat]);
        let p = [lon, lat];
        let mut best_78: Option<&AdminRing> = None;
        let mut best_6: Option<&AdminRing> = None;
        for bb in self.admin_tree.locate_in_envelope_intersecting(&env) {
            let ring = &self.rings[bb.idx];
            if !point_in_ring(p, &ring.ring) {
                continue;
            }
            if ring.admin_level == 7 || ring.admin_level == 8 {
                if best_78.map(|b| ring.area < b.area).unwrap_or(true) {
                    best_78 = Some(ring);
                }
            } else if ring.admin_level == 6 && best_6.map(|b| ring.area < b.area).unwrap_or(true) {
                best_6 = Some(ring);
            }
        }
        best_78
            .or(best_6)
            .map(|r| r.name.clone())
            .unwrap_or_default()
    }

    fn sub_area_at(
        &self,
        osm_id: i64,
        name: &str,
        kind: &str,
        lat: f64,
        lon: f64,
        municipality: &str,
    ) -> String {
        let place = kind.strip_prefix("place:").unwrap_or("");
        if matches!(
            place,
            "hamlet"
                | "village"
                | "suburb"
                | "neighbourhood"
                | "quarter"
                | "city"
                | "town"
                | "municipality"
        ) {
            if matches!(place, "city" | "town" | "municipality") {
                return self.nearest_sub_area(
                    osm_id,
                    name,
                    lat,
                    lon,
                    municipality,
                    CITY_SUB_AREA_MAX_M,
                    &["neighbourhood", "quarter", "suburb"],
                    &[],
                );
            }
            return String::new();
        }
        self.nearest_sub_area(
            osm_id,
            name,
            lat,
            lon,
            municipality,
            SUB_AREA_MAX_M,
            &["hamlet", "locality", "neighbourhood", "quarter", "suburb"],
            &["village"],
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn nearest_sub_area(
        &self,
        osm_id: i64,
        name: &str,
        lat: f64,
        lon: f64,
        municipality: &str,
        max_m: f64,
        primary: &[&str],
        fallback: &[&str],
    ) -> String {
        let pad_lat = max_m / 111_320.0;
        let pad_lon = max_m / (111_320.0 * lat.to_radians().cos().max(0.2));
        let env = AABB::from_corners(
            [lon - pad_lon, lat - pad_lat],
            [lon + pad_lon, lat + pad_lat],
        );
        let mut best_primary: Option<(f64, String)> = None;
        let mut best_fallback: Option<(f64, String)> = None;
        for cand in self.sub_tree.locate_in_envelope_intersecting(&env) {
            if cand.osm_id == osm_id {
                continue;
            }
            if cand.name.to_lowercase() == name.to_lowercase() {
                continue;
            }
            if !municipality.is_empty() && cand.name.to_lowercase() == municipality.to_lowercase() {
                continue;
            }
            let d_m = haversine_km(lat, lon, cand.lat, cand.lon) * 1000.0;
            let k = cand.kind.as_str();
            if primary.contains(&k) && d_m <= max_m {
                if best_primary.as_ref().map(|(d, _)| d_m < *d).unwrap_or(true) {
                    best_primary = Some((d_m, cand.name.clone()));
                }
            } else if fallback.contains(&k)
                && d_m <= SUB_AREA_VILLAGE_MAX_M
                && best_fallback
                    .as_ref()
                    .map(|(d, _)| d_m < *d)
                    .unwrap_or(true)
            {
                best_fallback = Some((d_m, cand.name.clone()));
            }
        }
        best_primary
            .or(best_fallback)
            .map(|(_, n)| n)
            .unwrap_or_default()
    }
}

pub(crate) fn sub_area_pt(
    osm_id: i64,
    name: String,
    kind: &str,
    lat: f64,
    lon: f64,
) -> Option<SubAreaPt> {
    let k = kind.strip_prefix("place:")?;
    if !matches!(
        k,
        "hamlet" | "locality" | "neighbourhood" | "quarter" | "suburb" | "village"
    ) {
        return None;
    }
    Some(SubAreaPt {
        osm_id,
        name,
        kind: k.to_string(),
        lat,
        lon,
    })
}

/// Load named admin_level 6–8 polygons from a region extract.
pub(crate) fn load_admin_from_pbf(path: impl AsRef<Path>) -> anyhow::Result<Vec<AdminRing>> {
    let path = path.as_ref();
    let mut rels: Vec<(String, u8, Vec<i64>)> = Vec::new();
    let mut needed_ways: HashSet<i64> = HashSet::new();
    {
        crate::download::pbf_priority::for_each_pbf_elements(path, |element| {
            let Element::Relation(rel) = element else {
                return;
            };
            let tags: HashMap<String, String> = rel
                .tags()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            let Some((name, level)) = admin_name_level(&tags) else {
                return;
            };
            let mut outers = Vec::new();
            for m in rel.members() {
                if m.member_type != RelMemberType::Way {
                    continue;
                }
                let role = m.role().unwrap_or("");
                if role.eq_ignore_ascii_case("inner") {
                    continue;
                }
                if role.is_empty()
                    || role.eq_ignore_ascii_case("outer")
                    || role.eq_ignore_ascii_case("part")
                {
                    outers.push(m.member_id);
                    needed_ways.insert(m.member_id);
                }
            }
            if !outers.is_empty() {
                rels.push((name, level, outers));
            }
        })?;
    }

    let mut way_nodes: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut standalone: Vec<(String, u8, Vec<i64>)> = Vec::new();
    {
        crate::download::pbf_priority::for_each_pbf_elements(path, |element| {
            let Element::Way(way) = element else {
                return;
            };
            let id = way.id();
            if needed_ways.contains(&id) {
                way_nodes.insert(id, way.refs().collect());
            }
            let tags: HashMap<String, String> = way
                .tags()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            if let Some((name, level)) = admin_name_level(&tags) {
                let refs: Vec<i64> = way.refs().collect();
                if refs.len() >= 3 {
                    standalone.push((name, level, refs));
                }
            }
        })?;
    }

    let mut needed_nodes: HashSet<i64> = HashSet::new();
    for refs in way_nodes.values() {
        needed_nodes.extend(refs.iter().copied());
    }
    for (_, _, refs) in &standalone {
        needed_nodes.extend(refs.iter().copied());
    }

    let mut coords: HashMap<i64, (f64, f64)> = HashMap::with_capacity(needed_nodes.len());
    {
        crate::download::pbf_priority::for_each_pbf_elements(path, |element| match element {
            Element::Node(n) => {
                if needed_nodes.contains(&n.id()) {
                    coords.insert(n.id(), (n.lat(), n.lon()));
                }
            }
            Element::DenseNode(n) => {
                if needed_nodes.contains(&n.id) {
                    coords.insert(n.id, (n.lat(), n.lon()));
                }
            }
            _ => {}
        })?;
    }

    let mut rings = Vec::new();
    for (name, level, outers) in rels {
        let mut ways: Vec<Vec<i64>> = Vec::new();
        for wid in outers {
            if let Some(refs) = way_nodes.get(&wid) {
                if refs.len() >= 2 {
                    ways.push(refs.clone());
                }
            }
        }
        for node_ring in stitch_closed_rings(&ways) {
            if let Some(ring) = ring_from_node_ids(&node_ring, &coords) {
                rings.push(admin_ring(name.clone(), level, ring));
            }
        }
    }
    for (name, level, refs) in standalone {
        if let Some(ring) = ring_from_node_ids(&refs, &coords) {
            rings.push(admin_ring(name, level, ring));
        }
    }
    Ok(rings)
}

fn admin_name_level(tags: &HashMap<String, String>) -> Option<(String, u8)> {
    let boundary = tags.get("boundary").map(|s| s.as_str()).unwrap_or("");
    if boundary != "administrative" {
        return None;
    }
    let name = tags.get("name")?.trim();
    if name.is_empty() {
        return None;
    }
    let level = tags.get("admin_level")?.parse::<u8>().ok()?;
    if !(6..=8).contains(&level) {
        return None;
    }
    Some((name.to_string(), level))
}

fn admin_ring(name: String, admin_level: u8, ring: Vec<[f64; 2]>) -> AdminRing {
    let (min_lon, max_lon, min_lat, max_lat) = ring_bounds(&ring);
    let area = ring_area_abs(&ring);
    AdminRing {
        name,
        admin_level,
        ring,
        min_lon,
        max_lon,
        min_lat,
        max_lat,
        area,
    }
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

fn ring_area_abs(ring: &[[f64; 2]]) -> f64 {
    if ring.len() < 3 {
        return f64::INFINITY;
    }
    let mut s = 0.0;
    for i in 0..ring.len() - 1 {
        s += ring[i][0] * ring[i + 1][1] - ring[i + 1][0] * ring[i][1];
    }
    (s * 0.5).abs()
}

fn ring_from_node_ids(refs: &[i64], coords: &HashMap<i64, (f64, f64)>) -> Option<Vec<[f64; 2]>> {
    if refs.len() < 3 {
        return None;
    }
    let mut ring: Vec<[f64; 2]> = Vec::with_capacity(refs.len());
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
    if ring.len() < 4 {
        return None;
    }
    Some(ring)
}

/// Join way node-id polylines into closed rings by matching endpoints.
pub(crate) fn stitch_closed_rings(ways: &[Vec<i64>]) -> Vec<Vec<i64>> {
    let mut unused: Vec<Vec<i64>> = ways.iter().filter(|w| w.len() >= 2).cloned().collect();
    let mut rings = Vec::new();
    while !unused.is_empty() {
        let mut chain = unused.swap_remove(unused.len() - 1);
        let mut grew = true;
        while grew {
            grew = extend_chain(&mut chain, &mut unused);
        }
        let start = chain.first().copied();
        let end = chain.last().copied();
        if start.zip(end).map(|(a, b)| a == b).unwrap_or(false) && chain.len() >= 4 {
            rings.push(chain);
        }
    }
    rings
}

fn extend_chain(chain: &mut Vec<i64>, unused: &mut Vec<Vec<i64>>) -> bool {
    if chain.len() < 2 {
        return false;
    }
    let start = chain[0];
    let end = *chain.last().unwrap();
    if start == end && chain.len() >= 4 {
        return false;
    }
    for i in 0..unused.len() {
        let w = &unused[i];
        if w.len() < 2 {
            continue;
        }
        let a = w[0];
        let b = *w.last().unwrap();
        if a == end {
            let extra = unused.swap_remove(i);
            chain.extend(extra.into_iter().skip(1));
            return true;
        }
        if b == end {
            let mut extra = unused.swap_remove(i);
            extra.reverse();
            chain.extend(extra.into_iter().skip(1));
            return true;
        }
        if b == start {
            let extra = unused.swap_remove(i);
            let mut new_chain = extra;
            new_chain.extend(chain.iter().copied().skip(1));
            *chain = new_chain;
            return true;
        }
        if a == start {
            let mut extra = unused.swap_remove(i);
            extra.reverse();
            extra.extend(chain.iter().copied().skip(1));
            *chain = extra;
            return true;
        }
    }
    false
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

    fn closed_square(min_lon: f64, min_lat: f64, max_lon: f64, max_lat: f64) -> Vec<[f64; 2]> {
        vec![
            [min_lon, min_lat],
            [max_lon, min_lat],
            [max_lon, max_lat],
            [min_lon, max_lat],
            [min_lon, min_lat],
        ]
    }

    #[test]
    fn format_skips_empty_and_duplicate_parts() {
        assert_eq!(
            format_place_display("Båberg", "Brattberg", "Gjøvik"),
            "Båberg, Brattberg, Gjøvik"
        );
        assert_eq!(format_place_display("Espa", "", "Stange"), "Espa, Stange");
        assert_eq!(format_place_display("Gjøvik", "", "Gjøvik"), "Gjøvik");
        assert_eq!(
            format_place_display("Løken", "Løken", "Ringsaker"),
            "Løken, Ringsaker"
        );
        assert_eq!(format_place_display("Tangen", "", ""), "Tangen");
    }

    #[test]
    fn stitch_two_ways_into_closed_ring() {
        let ways = vec![vec![1, 2, 3], vec![3, 4, 1]];
        let rings = stitch_closed_rings(&ways);
        assert_eq!(rings.len(), 1);
        assert_eq!(rings[0].first(), rings[0].last());
        assert!(rings[0].len() >= 4);
    }

    #[test]
    fn baberg_farms_get_distinct_hamlet_and_kommune() {
        let gjovik = admin_ring(
            "Gjøvik".into(),
            7,
            closed_square(10.40, 60.90, 10.70, 61.10),
        );
        let ringsaker = admin_ring(
            "Ringsaker".into(),
            7,
            closed_square(10.70, 60.80, 11.00, 61.00),
        );
        let hamlets = vec![
            SubAreaPt {
                osm_id: 10,
                name: "Brattberg".into(),
                kind: "hamlet".into(),
                lat: 60.969,
                lon: 10.550,
            },
            SubAreaPt {
                osm_id: 11,
                name: "Løken".into(),
                kind: "hamlet".into(),
                lat: 60.925,
                lon: 10.837,
            },
        ];
        let resolver = ContextResolver::from_admin_and_sub_areas(vec![gjovik, ringsaker], hamlets);

        let a = resolver.resolve(1, "Båberg", "place:farm", 60.9684907, 10.5482071);
        assert_eq!(a.sub_area, "Brattberg");
        assert_eq!(a.municipality, "Gjøvik");
        assert_eq!(
            format_place_display("Båberg", &a.sub_area, &a.municipality),
            "Båberg, Brattberg, Gjøvik"
        );

        let b = resolver.resolve(2, "Båberg", "place:farm", 60.9241628, 10.8363604);
        assert_eq!(b.sub_area, "Løken");
        assert_eq!(b.municipality, "Ringsaker");
        assert_eq!(
            format_place_display("Båberg", &b.sub_area, &b.municipality),
            "Båberg, Løken, Ringsaker"
        );
    }

    #[test]
    fn unique_settlement_keeps_municipality_without_inventing_sub_area() {
        let stange = admin_ring(
            "Stange".into(),
            7,
            closed_square(11.10, 60.50, 11.40, 60.70),
        );
        let resolver = ContextResolver::from_admin_and_sub_areas(
            vec![stange],
            vec![SubAreaPt {
                osm_id: 99,
                name: "Espa".into(),
                kind: "village".into(),
                lat: 60.5778,
                lon: 11.2712,
            }],
        );
        let hit = resolver.resolve(99, "Espa", "place:village", 60.5778, 11.2712);
        assert!(
            hit.sub_area.is_empty(),
            "village is the sub-area, got {:?}",
            hit.sub_area
        );
        assert_eq!(hit.municipality, "Stange");
        assert_eq!(
            format_place_display("Espa", &hit.sub_area, &hit.municipality),
            "Espa, Stange"
        );
    }

    #[test]
    fn prefers_smaller_admin_level_eight_over_seven() {
        let large = admin_ring("County".into(), 7, closed_square(10.0, 60.0, 12.0, 62.0));
        let small = admin_ring("Town".into(), 8, closed_square(10.4, 60.4, 10.6, 60.6));
        let resolver = ContextResolver::from_admin_and_sub_areas(vec![large, small], Vec::new());
        let hit = resolver.resolve(1, "Farm", "place:farm", 60.5, 10.5);
        assert_eq!(hit.municipality, "Town");
    }
}
