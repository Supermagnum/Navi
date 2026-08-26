//! Soft preference for official hiking / cycling route-network ways, and for
//! pilgrim routes (optional, same soft-penalty architecture).
//!
//! Preferring network membership is a **cost multiplier**, not a hard filter:
//! when a relation does not fully connect A→B, A* still falls back through
//! ordinary foot/cycle ways (same discipline as the DNT integration preference).
//!
//! Official matching is tag-generic (`type=route` + `route=*` + `network=*`).
//! Pilgrim matching uses `route=pilgrimage` and name/operator heuristics on
//! `route=hiking`/`foot` relations. Superroute membership is resolved one level
//! deep when straightforward.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use osmpbf::{Element, ElementReader, RelMemberType};
use rayon::prelude::*;

use super::builder::{GraphEdge, RouteGraph};

/// Soft penalty applied to edges that are not members of a matching official network.
pub const NON_NETWORK_PENALTY: f64 = 2.5;

/// Posted speed at/above which hiking/cycling routes get a soft cost penalty.
pub const HIGH_SPEED_ROAD_KMH: f64 = 80.0;

/// Same multiplier discipline as official-network soft preference.
pub const HIGH_SPEED_ROAD_PENALTY: f64 = NON_NETWORK_PENALTY;

/// Lighter penalty for 60–79 km/h tagged roads.
pub const MODERATE_SPEED_ROAD_PENALTY: f64 = 1.35;

/// When maxspeed is absent, infer penalty from highway class.
pub const UNTAGGED_HIGH_CLASS_PENALTY: f64 = 1.6;

fn is_high_class_highway(hw: &str) -> bool {
    matches!(
        hw,
        "primary" | "primary_link" | "trunk" | "trunk_link" | "secondary" | "secondary_link"
    )
}

fn is_primary_or_trunk(hw: &str) -> bool {
    matches!(hw, "primary" | "primary_link" | "trunk" | "trunk_link")
}

/// Cost multiplier for one edge under hiking/cycling slow-road preference.
pub fn slow_road_edge_multiplier(edge: &GraphEdge) -> f64 {
    let mut mult = 1.0;
    if let Some(ms) = edge.maxspeed_kmh {
        if ms >= HIGH_SPEED_ROAD_KMH {
            mult *= HIGH_SPEED_ROAD_PENALTY;
        } else if ms >= 60.0 {
            mult *= MODERATE_SPEED_ROAD_PENALTY;
        }
    } else if edge.highway.as_deref().is_some_and(is_high_class_highway) {
        mult *= UNTAGGED_HIGH_CLASS_PENALTY;
    }
    if edge.highway.as_deref().is_some_and(is_primary_or_trunk) {
        mult *= 1.15;
    }
    mult
}

/// Soft preference for hiking/cycling: penalize high maxspeed / high highway class.
/// Fallback only — never excludes edges.
pub fn apply_slow_road_preference(graph: &mut RouteGraph) {
    graph.edges.par_iter_mut().for_each(|edge| {
        let mult = slow_road_edge_multiplier(edge);
        if mult > 1.0 + 1e-9 {
            edge.base_weight *= mult;
            if let Some(ref mut eco) = edge.eco_weight {
                *eco *= mult;
            }
        }
    });
}

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

/// Name / operator substrings that mark a hiking relation as a pilgrim route
/// when `route=pilgrimage` is missing (common on Pilegrimsleden, Camino, etc.).
const PILGRIM_NAME_HINTS: &[&str] = &[
    "pilegrimsled",
    "pilegrim",
    "pilgrim",
    "camino",
    "way of st. james",
    "way of saint james",
    "via francigena",
    "jakobswege",
    "jakobsweg",
    "st olav",
    "st. olav",
    "olavsleden",
];

fn text_has_pilgrim_hint(s: &str) -> bool {
    let lower = s.to_lowercase();
    PILGRIM_NAME_HINTS.iter().any(|h| lower.contains(h))
}

/// True when relation tags describe a pilgrim route (soft preference only).
pub fn is_pilgrim_route_relation(tags: &HashMap<String, String>) -> bool {
    if !tag_eq(tags, "type", "route") {
        return false;
    }
    if tag_eq(tags, "route", "pilgrimage") {
        return true;
    }
    if !tag_in(tags, "route", &["hiking", "foot"]) {
        return false;
    }
    for key in ["name", "name:en", "name:nb", "name:no", "operator", "ref"] {
        if tags.get(key).is_some_and(|v| text_has_pilgrim_hint(v)) {
            return true;
        }
    }
    false
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

/// Way IDs on pilgrim route relations (`route=pilgrimage` or pilgrim-named hiking).
pub fn load_pilgrim_route_way_ids(path: impl AsRef<Path>) -> anyhow::Result<HashSet<i64>> {
    let file = std::fs::File::open(path.as_ref())?;
    let reader = ElementReader::new(file);

    let mut route_way_ids: HashSet<i64> = HashSet::new();
    let mut route_rel_ids: HashSet<i64> = HashSet::new();
    let mut super_child_rel_ids: HashSet<i64> = HashSet::new();
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

            if is_pilgrim_route_relation(&tags) {
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

    for child_id in super_child_rel_ids {
        let Some(tags) = rel_tags.get(&child_id) else {
            continue;
        };
        if is_pilgrim_route_relation(tags) || route_rel_ids.contains(&child_id) {
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
                || is_pilgrim_route_relation(&info.tags)
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
        let is_pilgrim = is_pilgrim_route_relation(&info.tags);
        let is_super = is_superroute(&info.tags);
        if !is_hike && !is_cycle && !is_pilgrim && !is_super {
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

        let kind = if is_pilgrim {
            "route:pilgrimage".into()
        } else if is_hike {
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
    fn pilgrim_route_tag_and_name_hints_match() {
        assert!(is_pilgrim_route_relation(&tags(&[
            ("type", "route"),
            ("route", "pilgrimage"),
            ("name", "Somewhere"),
        ])));
        assert!(is_pilgrim_route_relation(&tags(&[
            ("type", "route"),
            ("route", "hiking"),
            ("name", "Pilegrimsleden"),
            ("network", "nwn"),
        ])));
        assert!(is_pilgrim_route_relation(&tags(&[
            ("type", "route"),
            ("route", "hiking"),
            ("name", "Camino Frances"),
        ])));
        assert!(!is_pilgrim_route_relation(&tags(&[
            ("type", "route"),
            ("route", "hiking"),
            ("name", "Ordinary local path"),
            ("network", "lwn"),
        ])));
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
                is_boardwalk_crossing: false,
                is_roundabout: false,
                motor_vehicle_conditional: None,
                access_conditional: None,
                maxspeed_conditional: None,
                access_forbidden: false,
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
                is_boardwalk_crossing: false,
                is_roundabout: false,
                motor_vehicle_conditional: None,
                access_conditional: None,
                maxspeed_conditional: None,
                access_forbidden: false,
            },
        ];
        let mut graph = RouteGraph::from_parts(nodes, edges, RoutingProfile::Foot);
        let mut net = HashSet::new();
        net.insert(10);
        apply_official_network_preference(&mut graph, &net);
        assert!((graph.edges[0].base_weight - 100.0).abs() < 1e-9);
        assert!((graph.edges[1].base_weight - 100.0 * NON_NETWORK_PENALTY).abs() < 1e-9);
    }

    #[test]
    #[ignore = "needs ostlandet fixture under core/target/integration-fixtures"]
    fn ostlandet_has_pilgrim_routes_when_fixture_present() {
        let pbf = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/integration-fixtures/ostlandet-latest.osm.pbf");
        assert!(pbf.is_file(), "missing {}", pbf.display());
        let ways = load_pilgrim_route_way_ids(&pbf).expect("pilgrim ways");
        let named = load_named_route_entries(&pbf).expect("named");
        let pilgrim_named: Vec<_> = named
            .iter()
            .filter(|n| n.kind == "route:pilgrimage")
            .collect();
        eprintln!(
            "PILGRIM_OSTLANDET ways={} named_pilgrim={}",
            ways.len(),
            pilgrim_named.len()
        );
        for n in pilgrim_named.iter().take(8) {
            eprintln!("  named {} kind={} @ {},{}", n.name, n.kind, n.lat, n.lon);
        }
        assert!(
            !ways.is_empty() || !pilgrim_named.is_empty(),
            "expected pilgrim tagging in Ostlandet extract"
        );
    }

    #[test]
    fn pilgrim_soft_pref_still_allows_non_network_path() {
        use super::super::builder::{GraphEdge, RoutingProfile};
        use osm4routing::NodeId;

        // 1 --pilgrim--> 2 --gap ordinary--> 3 ; parallel long pilgrim-only detour absent.
        // Soft pref raises cost on ordinary edge but path 1→2→3 must still exist.
        let nodes = HashMap::new();
        // nodes unused by from_parts routing if edges carry coords — keep empty like sibling test.
        let edges = vec![
            GraphEdge {
                id: "100".into(),
                source: NodeId(1),
                target: NodeId(2),
                length_m: 100.0,
                base_weight: 100.0,
                eco_weight: Some(100.0),
                start_lat: 60.0,
                start_lon: 10.0,
                end_lat: 60.001,
                end_lon: 10.0,
                shape: Vec::new(),
                highway: Some("path".into()),
                maxspeed_kmh: None,
                name: Some("Pilegrimsleden".into()),
                road_ref: None,
                maxweight_t: None,
                maxaxleload_t: None,
                maxbogieweight_t: None,
                maxheight_m: None,
                maxwidth_m: None,
                maxlength_m: None,
                is_toll: false,
                is_ferry: false,
                is_boardwalk_crossing: false,
                is_roundabout: false,
                motor_vehicle_conditional: None,
                access_conditional: None,
                maxspeed_conditional: None,
                access_forbidden: false,
            },
            GraphEdge {
                id: "200".into(),
                source: NodeId(2),
                target: NodeId(3),
                length_m: 100.0,
                base_weight: 100.0,
                eco_weight: Some(100.0),
                start_lat: 60.001,
                start_lon: 10.0,
                end_lat: 60.002,
                end_lon: 10.0,
                shape: Vec::new(),
                highway: Some("path".into()),
                maxspeed_kmh: None,
                name: Some("ordinary".into()),
                road_ref: None,
                maxweight_t: None,
                maxaxleload_t: None,
                maxbogieweight_t: None,
                maxheight_m: None,
                maxwidth_m: None,
                maxlength_m: None,
                is_toll: false,
                is_ferry: false,
                is_boardwalk_crossing: false,
                is_roundabout: false,
                motor_vehicle_conditional: None,
                access_conditional: None,
                maxspeed_conditional: None,
                access_forbidden: false,
            },
        ];
        let mut graph = RouteGraph::from_parts(nodes, edges, RoutingProfile::Foot);
        let mut pilgrim = HashSet::new();
        pilgrim.insert(100);
        apply_official_network_preference(&mut graph, &pilgrim);
        assert!((graph.edges[0].base_weight - 100.0).abs() < 1e-9);
        assert!(graph.edges[1].base_weight > 100.0);
        let (path, _cost) = graph
            .shortest_path(NodeId(1), NodeId(3), false)
            .expect("soft pref must not block gap on ordinary path");
        assert!(path.len() >= 2);
    }

    #[test]
    fn pilgrim_pref_chooses_pilgrim_over_shorter_parallel() {
        use super::super::builder::{GraphEdge, RoutingProfile};
        use osm4routing::NodeId;

        // Short ordinary 1→3 vs longer pilgrim 1→2→3. Without pref A* takes ordinary;
        // with soft pref the ordinary edge is ×NON_NETWORK_PENALTY and pilgrim wins.
        let edges = vec![
            GraphEdge {
                id: "10".into(),
                source: NodeId(1),
                target: NodeId(3),
                length_m: 100.0,
                base_weight: 100.0,
                eco_weight: Some(100.0),
                start_lat: 60.0,
                start_lon: 10.0,
                end_lat: 60.002,
                end_lon: 10.0,
                shape: Vec::new(),
                highway: Some("path".into()),
                maxspeed_kmh: None,
                name: Some("shortcut".into()),
                road_ref: None,
                maxweight_t: None,
                maxaxleload_t: None,
                maxbogieweight_t: None,
                maxheight_m: None,
                maxwidth_m: None,
                maxlength_m: None,
                is_toll: false,
                is_ferry: false,
                is_boardwalk_crossing: false,
                is_roundabout: false,
                motor_vehicle_conditional: None,
                access_conditional: None,
                maxspeed_conditional: None,
                access_forbidden: false,
            },
            GraphEdge {
                id: "20".into(),
                source: NodeId(1),
                target: NodeId(2),
                length_m: 120.0,
                base_weight: 120.0,
                eco_weight: Some(120.0),
                start_lat: 60.0,
                start_lon: 10.0,
                end_lat: 60.001,
                end_lon: 10.001,
                shape: Vec::new(),
                highway: Some("path".into()),
                maxspeed_kmh: None,
                name: Some("Pilegrimsleden".into()),
                road_ref: None,
                maxweight_t: None,
                maxaxleload_t: None,
                maxbogieweight_t: None,
                maxheight_m: None,
                maxwidth_m: None,
                maxlength_m: None,
                is_toll: false,
                is_ferry: false,
                is_boardwalk_crossing: false,
                is_roundabout: false,
                motor_vehicle_conditional: None,
                access_conditional: None,
                maxspeed_conditional: None,
                access_forbidden: false,
            },
            GraphEdge {
                id: "21".into(),
                source: NodeId(2),
                target: NodeId(3),
                length_m: 120.0,
                base_weight: 120.0,
                eco_weight: Some(120.0),
                start_lat: 60.001,
                start_lon: 10.001,
                end_lat: 60.002,
                end_lon: 10.0,
                shape: Vec::new(),
                highway: Some("path".into()),
                maxspeed_kmh: None,
                name: Some("Pilegrimsleden".into()),
                road_ref: None,
                maxweight_t: None,
                maxaxleload_t: None,
                maxbogieweight_t: None,
                maxheight_m: None,
                maxwidth_m: None,
                maxlength_m: None,
                is_toll: false,
                is_ferry: false,
                is_boardwalk_crossing: false,
                is_roundabout: false,
                motor_vehicle_conditional: None,
                access_conditional: None,
                maxspeed_conditional: None,
                access_forbidden: false,
            },
        ];
        let plain = RouteGraph::from_parts(HashMap::new(), edges.clone(), RoutingProfile::Foot);
        let (path_plain, _) = plain
            .shortest_path(NodeId(1), NodeId(3), false)
            .expect("plain");
        assert_eq!(path_plain, vec![NodeId(1), NodeId(3)]);

        let mut preferred = RouteGraph::from_parts(HashMap::new(), edges, RoutingProfile::Foot);
        let mut pilgrim = HashSet::new();
        pilgrim.insert(20);
        pilgrim.insert(21);
        apply_official_network_preference(&mut preferred, &pilgrim);
        let (path_pref, _) = preferred
            .shortest_path(NodeId(1), NodeId(3), false)
            .expect("preferred");
        assert_eq!(path_pref, vec![NodeId(1), NodeId(2), NodeId(3)]);
    }

    fn rv3_vs_237_edges() -> Vec<GraphEdge> {
        vec![
            GraphEdge {
                id: "rv3".into(),
                source: NodeId(1),
                target: NodeId(3),
                length_m: 1000.0,
                base_weight: 1000.0,
                eco_weight: Some(1000.0),
                start_lat: 61.89,
                start_lon: 11.55,
                end_lat: 61.91,
                end_lon: 11.58,
                shape: Vec::new(),
                highway: Some("primary".into()),
                maxspeed_kmh: Some(80.0),
                name: Some("Rv 3".into()),
                road_ref: Some("3".into()),
                maxweight_t: None,
                maxaxleload_t: None,
                maxbogieweight_t: None,
                maxheight_m: None,
                maxwidth_m: None,
                maxlength_m: None,
                is_toll: false,
                is_ferry: false,
                is_boardwalk_crossing: false,
                is_roundabout: false,
                motor_vehicle_conditional: None,
                access_conditional: None,
                maxspeed_conditional: None,
                access_forbidden: false,
            },
            GraphEdge {
                id: "237a".into(),
                source: NodeId(1),
                target: NodeId(2),
                length_m: 600.0,
                base_weight: 600.0,
                eco_weight: Some(600.0),
                start_lat: 61.89,
                start_lon: 11.55,
                end_lat: 61.90,
                end_lon: 11.56,
                shape: Vec::new(),
                highway: Some("tertiary".into()),
                maxspeed_kmh: Some(50.0),
                name: Some("Fv 237".into()),
                road_ref: Some("237".into()),
                maxweight_t: None,
                maxaxleload_t: None,
                maxbogieweight_t: None,
                maxheight_m: None,
                maxwidth_m: None,
                maxlength_m: None,
                is_toll: false,
                is_ferry: false,
                is_boardwalk_crossing: false,
                is_roundabout: false,
                motor_vehicle_conditional: None,
                access_conditional: None,
                maxspeed_conditional: None,
                access_forbidden: false,
            },
            GraphEdge {
                id: "237b".into(),
                source: NodeId(2),
                target: NodeId(3),
                length_m: 600.0,
                base_weight: 600.0,
                eco_weight: Some(600.0),
                start_lat: 61.90,
                start_lon: 11.56,
                end_lat: 61.91,
                end_lon: 11.58,
                shape: Vec::new(),
                highway: Some("tertiary".into()),
                maxspeed_kmh: Some(50.0),
                name: Some("Fv 237".into()),
                road_ref: Some("237".into()),
                maxweight_t: None,
                maxaxleload_t: None,
                maxbogieweight_t: None,
                maxheight_m: None,
                maxwidth_m: None,
                maxlength_m: None,
                is_toll: false,
                is_ferry: false,
                is_boardwalk_crossing: false,
                is_roundabout: false,
                motor_vehicle_conditional: None,
                access_conditional: None,
                maxspeed_conditional: None,
                access_forbidden: false,
            },
        ]
    }

    #[test]
    fn slow_road_pref_prefers_lower_speed_parallel() {
        use super::super::builder::RoutingProfile;
        let plain =
            RouteGraph::from_parts(HashMap::new(), rv3_vs_237_edges(), RoutingProfile::Foot);
        let (path_plain, _) = plain
            .shortest_path(NodeId(1), NodeId(3), false)
            .expect("plain");
        assert_eq!(
            path_plain,
            vec![NodeId(1), NodeId(3)],
            "shorter Rv3 wins without pref"
        );

        let mut preferred =
            RouteGraph::from_parts(HashMap::new(), rv3_vs_237_edges(), RoutingProfile::Foot);
        apply_slow_road_preference(&mut preferred);
        let (path_pref, _) = preferred
            .shortest_path(NodeId(1), NodeId(3), false)
            .expect("preferred");
        assert_eq!(
            path_pref,
            vec![NodeId(1), NodeId(2), NodeId(3)],
            "Fv 237 detour wins with slow-road pref"
        );
    }

    #[test]
    fn slow_road_pref_fallback_when_only_high_speed_connects() {
        use super::super::builder::RoutingProfile;
        let edges = vec![rv3_vs_237_edges()[0].clone()];
        let mut graph = RouteGraph::from_parts(HashMap::new(), edges, RoutingProfile::Foot);
        apply_slow_road_preference(&mut graph);
        assert!(graph.edges[0].base_weight > 1000.0);
        let (path, _) = graph
            .shortest_path(NodeId(1), NodeId(3), false)
            .expect("must still route on high-speed when only option");
        assert_eq!(path, vec![NodeId(1), NodeId(3)]);
    }

    #[test]
    fn slow_road_pref_does_not_affect_car_profile_costing() {
        use super::super::builder::RoutingProfile;
        // Car graph without apply_slow_road_preference — caller responsibility.
        let graph = RouteGraph::from_parts(HashMap::new(), rv3_vs_237_edges(), RoutingProfile::Car);
        let (path, _) = graph
            .shortest_path(NodeId(1), NodeId(3), false)
            .expect("car");
        assert_eq!(path, vec![NodeId(1), NodeId(3)]);
    }

    #[test]
    fn high_speed_edge_gets_penalty_multiplier() {
        let edge = rv3_vs_237_edges()[0].clone();
        let mult = slow_road_edge_multiplier(&edge);
        assert!(mult >= HIGH_SPEED_ROAD_PENALTY);
    }
}
