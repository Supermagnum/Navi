//! Soft preference for official hiking / cycling route-network ways.
//!
//! Preferring network membership is a **cost multiplier**, not a hard filter:
//! when a relation does not fully connect A→B, A* still falls back through
//! ordinary foot/cycle ways (same discipline as the DNT integration preference).
//!
//! Matching is tag-generic (`type=route` + `route=*` + `network=*`), not a
//! hardcoded list of named trails. Superroute membership is resolved one level
//! deep when straightforward; recursive resolution and Benelux-style node
//! networks (`network:type=node_network`) are out of scope for this pass.
//! Tier weighting (e.g. preferring `nwn` over `lwn`) is a possible future
//! refinement — all listed tiers are treated equally here.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use osmpbf::{Element, ElementReader, RelMemberType};
use rayon::prelude::*;

use super::builder::RouteGraph;

/// Soft penalty applied to edges that are not members of a matching official network.
pub const NON_NETWORK_PENALTY: f64 = 2.5;

const HIKING_NETWORKS: &[&str] = &["iwn", "nwn", "rwn", "lwn"];
const CYCLING_NETWORKS: &[&str] = &["icn", "ncn", "rcn", "lcn"];

/// Which official-network family to prefer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfficialNetworkKind {
    Hiking,
    Cycling,
}

impl OfficialNetworkKind {
    fn route_values(self) -> &'static [&'static str] {
        match self {
            Self::Hiking => &["hiking", "foot"],
            Self::Cycling => &["bicycle", "mtb"],
        }
    }

    fn network_values(self) -> &'static [&'static str] {
        match self {
            Self::Hiking => HIKING_NETWORKS,
            Self::Cycling => CYCLING_NETWORKS,
        }
    }
}

fn tag_eq(tags: &HashMap<String, String>, key: &str, want: &str) -> bool {
    tags.get(key).is_some_and(|v| v.eq_ignore_ascii_case(want))
}

fn tag_in(tags: &HashMap<String, String>, key: &str, allowed: &[&str]) -> bool {
    tags.get(key)
        .is_some_and(|v| allowed.iter().any(|a| v.eq_ignore_ascii_case(a)))
}

/// True when relation tags match an official route network for `kind`.
pub fn is_official_route_relation(
    tags: &HashMap<String, String>,
    kind: OfficialNetworkKind,
) -> bool {
    if !tag_eq(tags, "type", "route") {
        return false;
    }
    tag_in(tags, "route", kind.route_values()) && tag_in(tags, "network", kind.network_values())
}

fn is_superroute(tags: &HashMap<String, String>) -> bool {
    tag_eq(tags, "type", "superroute")
}

fn edge_way_id(edge_id: &str) -> Option<i64> {
    edge_id
        .strip_suffix("-rev")
        .unwrap_or(edge_id)
        .split('-')
        .next()
        .and_then(|s| s.parse().ok())
}

/// Way IDs that belong to matching official route relations (direct members,
/// plus one level of `type=superroute` → child route → ways).
pub fn load_official_network_way_ids(
    path: impl AsRef<Path>,
    kind: OfficialNetworkKind,
) -> anyhow::Result<HashSet<i64>> {
    let file = std::fs::File::open(path.as_ref())?;
    let reader = ElementReader::new(file);

    // First pass: collect matching route relations (ways + child relation ids)
    // and superroute → child relation ids.
    let mut route_way_ids: HashSet<i64> = HashSet::new();
    let mut route_rel_ids: HashSet<i64> = HashSet::new();
    let mut super_child_rel_ids: HashSet<i64> = HashSet::new();
    // All route relations' way members keyed by relation id (for superroute expand).
    let mut rel_to_ways: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut rel_tags: HashMap<i64, HashMap<String, String>> = HashMap::new();

    reader.for_each(|element| {
        if let Element::Relation(rel) = element {
            let id = rel.id();
            let tags: HashMap<String, String> = rel
                .tags()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            let mut ways = Vec::new();
            let mut child_rels = Vec::new();
            for member in rel.members() {
                match member.member_type {
                    RelMemberType::Way => ways.push(member.member_id),
                    RelMemberType::Relation => child_rels.push(member.member_id),
                    RelMemberType::Node => {}
                }
            }
            rel_to_ways.insert(id, ways.clone());
            rel_tags.insert(id, tags.clone());

            if is_official_route_relation(&tags, kind) {
                route_rel_ids.insert(id);
                for w in ways {
                    route_way_ids.insert(w);
                }
            }
            if is_superroute(&tags) {
                for c in child_rels {
                    super_child_rel_ids.insert(c);
                }
            }
        }
    })?;

    // One level: if a superroute points at a matching child route, include its ways.
    for child_id in super_child_rel_ids {
        let Some(tags) = rel_tags.get(&child_id) else {
            continue;
        };
        if is_official_route_relation(tags, kind) || route_rel_ids.contains(&child_id) {
            if let Some(ways) = rel_to_ways.get(&child_id) {
                for &w in ways {
                    route_way_ids.insert(w);
                }
            }
        }
    }

    Ok(route_way_ids)
}

/// Multiply non-network edge weights by [`NON_NETWORK_PENALTY`]. Soft preference only.
pub fn apply_official_network_preference(graph: &mut RouteGraph, network_way_ids: &HashSet<i64>) {
    if network_way_ids.is_empty() {
        return;
    }
    graph.edges.par_iter_mut().for_each(|edge| {
        let on_network = edge_way_id(&edge.id).is_some_and(|id| network_way_ids.contains(&id));
        if !on_network {
            edge.base_weight *= NON_NETWORK_PENALTY;
            if let Some(ref mut eco) = edge.eco_weight {
                *eco *= NON_NETWORK_PENALTY;
            }
        }
    });
}

/// Difficulty / suitability tags surfaced as informational metadata (not filters).
const DIFFICULTY_KEYS: &[&str] = &[
    "sac_scale",
    "cai_scale",
    "mtb:scale",
    "mtb:scale:uphill",
    "trail_visibility",
    "surface",
    "smoothness",
    "incline",
];

/// Load way-level difficulty tags for edges that appear on `path`.
pub fn load_way_difficulty_tags(
    path: impl AsRef<Path>,
    way_ids: &HashSet<i64>,
) -> anyhow::Result<HashMap<i64, HashMap<String, String>>> {
    if way_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let file = std::fs::File::open(path.as_ref())?;
    let reader = ElementReader::new(file);
    let mut out: HashMap<i64, HashMap<String, String>> = HashMap::new();
    reader.for_each(|element| {
        if let Element::Way(way) = element {
            let id = way.id();
            if !way_ids.contains(&id) {
                return;
            }
            let mut tags = HashMap::new();
            for (k, v) in way.tags() {
                if DIFFICULTY_KEYS.contains(&k) {
                    tags.insert(k.to_string(), v.to_string());
                }
            }
            if !tags.is_empty() {
                out.insert(id, tags);
            }
        }
    })?;
    Ok(out)
}

/// Human-readable difficulty notes for edges on a planned path (deduplicated).
pub fn difficulty_notes_for_path(
    graph: &RouteGraph,
    path_nodes: &[osm4routing::NodeId],
    way_tags: &HashMap<i64, HashMap<String, String>>,
) -> Vec<String> {
    let mut notes: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for w in path_nodes.windows(2) {
        let Some(idx) = graph.edge_index(w[0], w[1]) else {
            continue;
        };
        let Some(wid) = edge_way_id(&graph.edges[idx].id) else {
            continue;
        };
        let Some(tags) = way_tags.get(&wid) else {
            continue;
        };
        if let Some(sac) = tags.get("sac_scale") {
            let note = format!("includes SAC {sac} terrain");
            if seen.insert(note.clone()) {
                notes.push(note);
            }
        }
        if let Some(cai) = tags.get("cai_scale") {
            let note = format!("includes CAI {cai} terrain");
            if seen.insert(note.clone()) {
                notes.push(note);
            }
        }
        if let Some(mtb) = tags.get("mtb:scale") {
            let note = format!("includes MTB scale {mtb}");
            if seen.insert(note.clone()) {
                notes.push(note);
            }
        }
        if let Some(vis) = tags.get("trail_visibility") {
            let note = format!("trail visibility: {vis}");
            if seen.insert(note.clone()) {
                notes.push(note);
            }
        }
        if let Some(surface) = tags.get("surface") {
            let note = format!("surface: {surface}");
            if seen.insert(note.clone()) {
                notes.push(note);
            }
        }
        if let Some(smooth) = tags.get("smoothness") {
            let note = format!("smoothness: {smooth}");
            if seen.insert(note.clone()) {
                notes.push(note);
            }
        }
        if let Some(incline) = tags.get("incline") {
            let note = format!("incline: {incline}");
            if seen.insert(note.clone()) {
                notes.push(note);
            }
        }
    }
    notes
}

/// Named / ref'd route relations for FTS indexing (synthetic lat/lon from first way node).
#[derive(Debug, Clone)]
pub struct NamedRouteEntry {
    pub osm_id: i64,
    pub name: String,
    pub kind: String,
    pub lat: f64,
    pub lon: f64,
    pub operator: Option<String>,
    pub reference: Option<String>,
}

/// Extract searchable named official route relations (hiking + cycling networks).
///
/// Uses the first node of the first way member as a representative coordinate.
/// Superroutes without their own geometry inherit the first child's first way node
/// when available; otherwise they are skipped (known limitation).
pub fn load_named_route_entries(path: impl AsRef<Path>) -> anyhow::Result<Vec<NamedRouteEntry>> {
    let path = path.as_ref();

    struct RelInfo {
        tags: HashMap<String, String>,
        way_ids: Vec<i64>,
        child_rels: Vec<i64>,
    }

    let mut rels: HashMap<i64, RelInfo> = HashMap::new();
    let mut way_first_node: HashMap<i64, i64> = HashMap::new();
    let mut node_coord: HashMap<i64, (f64, f64)> = HashMap::new();

    // Three lightweight passes (relations → ways → nodes).
    {
        let file = std::fs::File::open(path)?;
        let reader = ElementReader::new(file);
        reader.for_each(|element| {
            if let Element::Relation(rel) = element {
                let tags: HashMap<String, String> = rel
                    .tags()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();
                let mut way_ids = Vec::new();
                let mut child_rels = Vec::new();
                for m in rel.members() {
                    match m.member_type {
                        RelMemberType::Way => way_ids.push(m.member_id),
                        RelMemberType::Relation => child_rels.push(m.member_id),
                        RelMemberType::Node => {}
                    }
                }
                rels.insert(
                    rel.id(),
                    RelInfo {
                        tags,
                        way_ids,
                        child_rels,
                    },
                );
            }
        })?;
    }

    let interesting: HashSet<i64> = rels
        .iter()
        .filter(|(_, info)| {
            is_official_route_relation(&info.tags, OfficialNetworkKind::Hiking)
                || is_official_route_relation(&info.tags, OfficialNetworkKind::Cycling)
                || is_superroute(&info.tags)
        })
        .map(|(id, _)| *id)
        .collect();

    let mut needed_ways: HashSet<i64> = HashSet::new();
    for id in &interesting {
        if let Some(info) = rels.get(id) {
            for &w in &info.way_ids {
                needed_ways.insert(w);
            }
            for &c in &info.child_rels {
                if let Some(child) = rels.get(&c) {
                    for &w in &child.way_ids {
                        needed_ways.insert(w);
                    }
                }
            }
        }
    }

    {
        let file = std::fs::File::open(path)?;
        let reader = ElementReader::new(file);
        reader.for_each(|element| {
            if let Element::Way(way) = element {
                let id = way.id();
                if needed_ways.contains(&id) {
                    if let Some(n) = way.refs().next() {
                        way_first_node.insert(id, n);
                    }
                }
            }
        })?;
    }

    let needed_nodes: HashSet<i64> = way_first_node.values().copied().collect();
    {
        let file = std::fs::File::open(path)?;
        let reader = ElementReader::new(file);
        reader.for_each(|element| match element {
            Element::Node(n) => {
                if needed_nodes.contains(&n.id()) {
                    node_coord.insert(n.id(), (n.lat(), n.lon()));
                }
            }
            Element::DenseNode(n) => {
                if needed_nodes.contains(&n.id) {
                    node_coord.insert(n.id, (n.lat(), n.lon()));
                }
            }
            _ => {}
        })?;
    }

    let mut out = Vec::new();
    for id in interesting {
        let Some(info) = rels.get(&id) else {
            continue;
        };
        let is_hike = is_official_route_relation(&info.tags, OfficialNetworkKind::Hiking);
        let is_cycle = is_official_route_relation(&info.tags, OfficialNetworkKind::Cycling);
        let is_super = is_superroute(&info.tags);
        if !is_hike && !is_cycle && !is_super {
            continue;
        }

        let name = info
            .tags
            .get("name")
            .cloned()
            .or_else(|| info.tags.get("ref").cloned());
        let Some(name) = name else {
            continue;
        };
        let reference = info.tags.get("ref").cloned();
        let operator = info.tags.get("operator").cloned();

        let mut way_id = info.way_ids.first().copied();
        if way_id.is_none() {
            for &c in &info.child_rels {
                if let Some(child) = rels.get(&c) {
                    if let Some(&w) = child.way_ids.first() {
                        way_id = Some(w);
                        break;
                    }
                }
            }
        }
        let Some(wid) = way_id else {
            continue;
        };
        let Some(&nid) = way_first_node.get(&wid) else {
            continue;
        };
        let Some(&(lat, lon)) = node_coord.get(&nid) else {
            continue;
        };

        let kind = if is_hike {
            format!(
                "route:hiking:{}",
                info.tags.get("network").map(String::as_str).unwrap_or("?")
            )
        } else if is_cycle {
            format!(
                "route:bicycle:{}",
                info.tags.get("network").map(String::as_str).unwrap_or("?")
            )
        } else {
            "route:superroute".into()
        };

        // Include operator in searchable name text when present.
        let search_name = match &operator {
            Some(op) if !name.to_lowercase().contains(&op.to_lowercase()) => {
                format!("{name} ({op})")
            }
            _ => name,
        };

        out.push(NamedRouteEntry {
            osm_id: id,
            name: search_name,
            kind,
            lat,
            lon,
            operator,
            reference,
        });
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use osm4routing::NodeId;
    use std::collections::HashMap;

    fn tags(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn hiking_network_tiers_match() {
        for net in ["iwn", "nwn", "rwn", "lwn"] {
            assert!(is_official_route_relation(
                &tags(&[("type", "route"), ("route", "hiking"), ("network", net)]),
                OfficialNetworkKind::Hiking
            ));
        }
        assert!(is_official_route_relation(
            &tags(&[("type", "route"), ("route", "foot"), ("network", "nwn")]),
            OfficialNetworkKind::Hiking
        ));
        assert!(!is_official_route_relation(
            &tags(&[("type", "route"), ("route", "hiking"), ("network", "ncn")]),
            OfficialNetworkKind::Hiking
        ));
    }

    #[test]
    fn cycling_network_tiers_match() {
        for net in ["icn", "ncn", "rcn", "lcn"] {
            assert!(is_official_route_relation(
                &tags(&[("type", "route"), ("route", "bicycle"), ("network", net)]),
                OfficialNetworkKind::Cycling
            ));
        }
        assert!(is_official_route_relation(
            &tags(&[("type", "route"), ("route", "mtb"), ("network", "lcn")]),
            OfficialNetworkKind::Cycling
        ));
        assert!(!is_official_route_relation(
            &tags(&[("type", "route"), ("route", "horse"), ("network", "lwn")]),
            OfficialNetworkKind::Cycling
        ));
    }

    #[test]
    fn soft_preference_raises_non_network_cost() {
        use super::super::builder::{GraphEdge, RoutingProfile};

        let nodes = HashMap::new();
        // Minimal graph via from_parts
        let edges = vec![
            GraphEdge {
                id: "10".into(),
                source: NodeId(1),
                target: NodeId(2),
                length_m: 100.0,
                base_weight: 100.0,
                eco_weight: Some(100.0),
                start_lat: 60.0,
                start_lon: 10.0,
                end_lat: 60.0,
                end_lon: 10.01,
                shape: Vec::new(),
                highway: Some("path".into()),
                maxspeed_kmh: None,
                name: None,
                road_ref: None,
                maxweight_t: None,
                maxaxleload_t: None,
                maxbogieweight_t: None,
                maxheight_m: None,
                maxwidth_m: None,
                maxlength_m: None,
                is_toll: false,
                is_ferry: false,
            },
            GraphEdge {
                id: "20".into(),
                source: NodeId(1),
                target: NodeId(2),
                length_m: 100.0,
                base_weight: 100.0,
                eco_weight: Some(100.0),
                start_lat: 60.0,
                start_lon: 10.0,
                end_lat: 60.001,
                end_lon: 10.01,
                shape: Vec::new(),
                highway: Some("path".into()),
                maxspeed_kmh: None,
                name: None,
                road_ref: None,
                maxweight_t: None,
                maxaxleload_t: None,
                maxbogieweight_t: None,
                maxheight_m: None,
                maxwidth_m: None,
                maxlength_m: None,
                is_toll: false,
                is_ferry: false,
            },
        ];
        let mut graph = RouteGraph::from_parts(nodes, edges, RoutingProfile::Foot);
        let mut net = HashSet::new();
        net.insert(10);
        apply_official_network_preference(&mut graph, &net);
        assert!((graph.edges[0].base_weight - 100.0).abs() < 1e-9);
        assert!((graph.edges[1].base_weight - 100.0 * NON_NETWORK_PENALTY).abs() < 1e-9);
    }
}
