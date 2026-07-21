use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use geo_types::Coord;
use osm4routing::{Node, NodeId};
use serde::{Deserialize, Serialize};

use crate::config::EcoConfig;
use crate::routing::elevation::ElevationService;

use super::builder::{GraphEdge, RouteGraph, RoutingProfile};

const CACHE_MAGIC: &[u8; 8] = b"NAVIGPH1";

/// Source PBF and eco inputs used to validate a cached reweighted graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphCacheFingerprint {
    pub pbf_path: String,
    pub pbf_size_bytes: u64,
    pub pbf_modified_unix_secs: u64,
    pub profile: RoutingProfile,
    pub eco_drag_coefficient: Option<f64>,
    pub eco_mass_kg: Option<f64>,
}

impl GraphCacheFingerprint {
    pub fn from_pbf(
        pbf_path: &Path,
        profile: RoutingProfile,
        eco: Option<&EcoConfig>,
    ) -> anyhow::Result<Self> {
        let meta = std::fs::metadata(pbf_path)?;
        let abs = pbf_path
            .canonicalize()
            .unwrap_or_else(|_| pbf_path.to_path_buf());
        let modified = meta.modified()?;
        Ok(Self {
            pbf_path: abs.to_string_lossy().into_owned(),
            pbf_size_bytes: meta.len(),
            pbf_modified_unix_secs: modified
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            profile,
            eco_drag_coefficient: eco.map(|e| e.drag_coefficient),
            eco_mass_kg: eco.map(|e| e.mass_kg),
        })
    }
}

#[derive(Serialize, Deserialize)]
struct CachedNode {
    id: i64,
    lat: f64,
    lon: f64,
}

#[derive(Serialize, Deserialize)]
struct CachedGraphEdge {
    id: String,
    source: i64,
    target: i64,
    length_m: f64,
    base_weight: f64,
    eco_weight: Option<f64>,
    start_lat: f64,
    start_lon: f64,
    end_lat: f64,
    end_lon: f64,
    highway: Option<String>,
    maxweight_t: Option<f64>,
    maxaxleload_t: Option<f64>,
    maxheight_m: Option<f64>,
    maxwidth_m: Option<f64>,
}

#[derive(Serialize, Deserialize)]
struct CachedRouteGraph {
    fingerprint: GraphCacheFingerprint,
    profile: RoutingProfile,
    nodes: Vec<CachedNode>,
    edges: Vec<CachedGraphEdge>,
}

fn profile_tag(profile: RoutingProfile) -> &'static str {
    match profile {
        RoutingProfile::Car => "car",
        RoutingProfile::Truck => "truck",
        RoutingProfile::Foot => "foot",
        RoutingProfile::Bicycle => "bicycle",
    }
}

/// Deterministic on-disk path for a reweighted graph cache entry.
pub fn graph_cache_path(cache_dir: &Path, pbf_path: &Path, profile: RoutingProfile) -> PathBuf {
    let abs = pbf_path
        .canonicalize()
        .unwrap_or_else(|_| pbf_path.to_path_buf());
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    abs.to_string_lossy().hash(&mut hasher);
    profile.hash(&mut hasher);
    let hash = hasher.finish();
    let stem = pbf_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("graph");
    cache_dir.join(format!("{stem}_{}_{hash:016x}.navigph", profile_tag(profile)))
}

pub fn save_reweighted_graph(
    graph: &RouteGraph,
    path: &Path,
    fingerprint: &GraphCacheFingerprint,
) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let payload = CachedRouteGraph {
        fingerprint: fingerprint.clone(),
        profile: graph.profile(),
        nodes: graph
            .nodes
            .values()
            .map(|node| CachedNode {
                id: node.id.0,
                lat: node.coord.y,
                lon: node.coord.x,
            })
            .collect(),
        edges: graph
            .edges
            .iter()
            .map(|edge| CachedGraphEdge {
                id: edge.id.clone(),
                source: edge.source.0,
                target: edge.target.0,
                length_m: edge.length_m,
                base_weight: edge.base_weight,
                eco_weight: edge.eco_weight,
                start_lat: edge.start_lat,
                start_lon: edge.start_lon,
                end_lat: edge.end_lat,
                end_lon: edge.end_lon,
                highway: edge.highway.clone(),
                maxweight_t: edge.maxweight_t,
                maxaxleload_t: edge.maxaxleload_t,
                maxheight_m: edge.maxheight_m,
                maxwidth_m: edge.maxwidth_m,
            })
            .collect(),
    };

    let encoded = bincode::serialize(&payload)?;
    let mut file = std::fs::File::create(path)?;
    file.write_all(CACHE_MAGIC)?;
    file.write_all(&encoded)?;
    Ok(())
}

pub fn load_reweighted_graph(
    path: &Path,
    expected: &GraphCacheFingerprint,
) -> anyhow::Result<Option<RouteGraph>> {
    let data = match std::fs::read(path) {
        Ok(data) => data,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
    };

    if data.len() < CACHE_MAGIC.len() || &data[..CACHE_MAGIC.len()] != CACHE_MAGIC {
        return Ok(None);
    }

    let payload: CachedRouteGraph = match bincode::deserialize(&data[CACHE_MAGIC.len()..]) {
        Ok(payload) => payload,
        Err(_) => return Ok(None),
    };

    if payload.fingerprint != *expected || payload.profile != expected.profile {
        return Ok(None);
    }

    Ok(Some(reconstruct_graph(payload)))
}

pub fn load_or_build_reweighted(
    pbf: &Path,
    cache_dir: &Path,
    profile: RoutingProfile,
    elevation: &ElevationService,
    eco: &EcoConfig,
) -> anyhow::Result<(RouteGraph, bool)> {
    std::fs::create_dir_all(cache_dir)?;
    let expected = GraphCacheFingerprint::from_pbf(pbf, profile, Some(eco))?;
    let cache_path = graph_cache_path(cache_dir, pbf, profile);

    if let Some(graph) = load_reweighted_graph(&cache_path, &expected)? {
        return Ok((graph, true));
    }

    let mut graph = RouteGraph::build_from_pbf(pbf, profile)?;
    graph.apply_eco_reweighting(elevation, eco);
    save_reweighted_graph(&graph, &cache_path, &expected)?;
    Ok((graph, false))
}

fn reconstruct_graph(payload: CachedRouteGraph) -> RouteGraph {
    let nodes: HashMap<NodeId, Node> = payload
        .nodes
        .into_iter()
        .map(|node| {
            (
                NodeId(node.id),
                Node {
                    id: NodeId(node.id),
                    coord: Coord {
                        x: node.lon,
                        y: node.lat,
                    },
                    uses: 0,
                },
            )
        })
        .collect();

    let edges: Vec<GraphEdge> = payload
        .edges
        .into_iter()
        .map(|edge| GraphEdge {
            id: edge.id,
            source: NodeId(edge.source),
            target: NodeId(edge.target),
            length_m: edge.length_m,
            base_weight: edge.base_weight,
            eco_weight: edge.eco_weight,
            start_lat: edge.start_lat,
            start_lon: edge.start_lon,
            end_lat: edge.end_lat,
            end_lon: edge.end_lon,
            highway: edge.highway,
            maxweight_t: edge.maxweight_t,
            maxaxleload_t: edge.maxaxleload_t,
            maxheight_m: edge.maxheight_m,
            maxwidth_m: edge.maxwidth_m,
        })
        .collect();

    RouteGraph::from_parts(nodes, edges, payload.profile)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn synthetic_graph() -> RouteGraph {
        let n1 = Node {
            id: NodeId(1),
            coord: Coord {
                x: 10.0,
                y: 60.0,
            },
            uses: 0,
        };
        let n2 = Node {
            id: NodeId(2),
            coord: Coord {
                x: 10.01,
                y: 60.01,
            },
            uses: 0,
        };
        let mut nodes = HashMap::new();
        nodes.insert(n1.id, n1);
        nodes.insert(n2.id, n2);
        let edges = vec![GraphEdge {
            id: "e1".to_string(),
            source: NodeId(1),
            target: NodeId(2),
            length_m: 1234.0,
            base_weight: 1234.0,
            eco_weight: Some(1500.0),
            start_lat: 60.0,
            start_lon: 10.0,
            end_lat: 60.01,
            end_lon: 10.01,
            highway: Some("primary".to_string()),
            maxweight_t: None,
            maxaxleload_t: None,
            maxheight_m: None,
            maxwidth_m: None,
        }];
        RouteGraph::from_parts(nodes, edges, RoutingProfile::Car)
    }

    #[test]
    fn roundtrip_reweighted_graph_cache() {
        let dir = tempdir().expect("tempdir");
        let graph = synthetic_graph();
        let fingerprint = GraphCacheFingerprint {
            pbf_path: "/tmp/test.osm.pbf".to_string(),
            pbf_size_bytes: 42,
            pbf_modified_unix_secs: 1_700_000_000,
            profile: RoutingProfile::Car,
            eco_drag_coefficient: Some(0.28),
            eco_mass_kg: Some(1500.0),
        };
        let cache_path = dir.path().join("test.navigph");

        save_reweighted_graph(&graph, &cache_path, &fingerprint).expect("save");
        let loaded = load_reweighted_graph(&cache_path, &fingerprint)
            .expect("load")
            .expect("cache hit");

        assert_eq!(loaded.profile(), RoutingProfile::Car);
        assert_eq!(loaded.nodes.len(), graph.nodes.len());
        assert_eq!(loaded.edges.len(), graph.edges.len());
        assert_eq!(loaded.edges[0].eco_weight, Some(1500.0));
        assert!(loaded.edge_index(NodeId(1), NodeId(2)).is_some());
    }

    #[test]
    fn fingerprint_mismatch_returns_none() {
        let dir = tempdir().expect("tempdir");
        let graph = synthetic_graph();
        let fingerprint = GraphCacheFingerprint {
            pbf_path: "/tmp/test.osm.pbf".to_string(),
            pbf_size_bytes: 42,
            pbf_modified_unix_secs: 1_700_000_000,
            profile: RoutingProfile::Car,
            eco_drag_coefficient: Some(0.28),
            eco_mass_kg: Some(1500.0),
        };
        let cache_path = dir.path().join("test.navigph");
        save_reweighted_graph(&graph, &cache_path, &fingerprint).expect("save");

        let mut stale = fingerprint.clone();
        stale.pbf_size_bytes += 1;
        assert!(load_reweighted_graph(&cache_path, &stale)
            .expect("load")
            .is_none());
    }

    #[test]
    fn graph_cache_path_is_deterministic() {
        let dir = Path::new("/tmp/cache");
        let pbf = Path::new("/data/norway.osm.pbf");
        let a = graph_cache_path(dir, pbf, RoutingProfile::Car);
        let b = graph_cache_path(dir, pbf, RoutingProfile::Car);
        assert_eq!(a, b);
        assert!(a.starts_with(dir));
        assert!(a.to_string_lossy().contains("car"));
    }
}
