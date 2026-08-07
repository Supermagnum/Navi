//! Region pack converter: local PBF (+ optional DEM) → graph + poi/barrier archives.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use osmpbf::{Element, ElementReader};
use rkyv::rancor::Error as RkyvError;

use super::graph_pack::{FlatGraphPack, GRAPH_FORMAT_VERSION, MAGIC_GRAPH};
use super::header::Preamble;
use super::io::{discard_partial, write_archive_atomic};
use super::manifest::{
    graph_pack_filename, manifest_path, pbf_fingerprint, poi_barrier_pack_filename, profile_key,
    wetland_pack_filename, NaviManifest,
};
use super::poi_barrier_pack::{FlatPoiBarrierPack, MAGIC_POI_BARRIER, POI_BARRIER_FORMAT_VERSION};
use super::wetland_pack::{FlatWetlandPack, MAGIC_WETLAND, WETLAND_FORMAT_VERSION};
use crate::download::progress as download_progress;
use crate::download::DownloadControl;
use crate::poi::{classify_tags, osm_icon_key, PoiRecord};
use crate::routing::elevation::{ElevationCache, ElevationService};
use crate::routing::graph::{RouteGraph, RoutingProfile};
use crate::routing::wetland::WetlandIndex;

#[derive(Clone)]
pub struct ConvertOptions {
    pub data_dir: PathBuf,
    pub pbf: PathBuf,
    /// When set, sample per-edge Δh into the graph pack.
    pub elev_dir: Option<PathBuf>,
    /// Profiles to emit. Default: car + foot (covers motor + hiking).
    pub profiles: Vec<RoutingProfile>,
    pub control: DownloadControl,
}

impl ConvertOptions {
    pub fn new(data_dir: impl Into<PathBuf>, pbf: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            pbf: pbf.into(),
            elev_dir: None,
            profiles: vec![RoutingProfile::Car, RoutingProfile::Foot],
            control: DownloadControl::default(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ConvertReport {
    pub stem: String,
    pub graph_files: Vec<String>,
    pub poi_barrier_file: String,
    pub wetland_file: String,
    pub manifest_file: String,
    pub nodes: usize,
    pub edges: usize,
    pub pois: usize,
    pub barrier_segs: usize,
    pub wetland_rings: usize,
    pub convert_ms: f64,
    pub has_delta_h: bool,
}

fn stem_of(pbf: &Path) -> String {
    let name = pbf
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("region.osm.pbf");
    let stem = name
        .strip_suffix(".osm.pbf")
        .or_else(|| name.strip_suffix(".pbf"))
        .unwrap_or(name);
    stem.to_string()
}

/// Scan PBF node extents so we can use the memory-safe bbox builder.
fn pbf_node_bbox(pbf: &Path) -> anyhow::Result<[f64; 4]> {
    let mut min_lat = f64::INFINITY;
    let mut min_lon = f64::INFINITY;
    let mut max_lat = f64::NEG_INFINITY;
    let mut max_lon = f64::NEG_INFINITY;
    let mut any = false;
    let file = std::fs::File::open(pbf)?;
    let reader = ElementReader::new(file);
    reader.for_each(|element| {
        let (lat, lon) = match element {
            Element::Node(n) => (n.lat(), n.lon()),
            Element::DenseNode(n) => (n.lat(), n.lon()),
            _ => return,
        };
        any = true;
        min_lat = min_lat.min(lat);
        min_lon = min_lon.min(lon);
        max_lat = max_lat.max(lat);
        max_lon = max_lon.max(lon);
    })?;
    if !any {
        anyhow::bail!("PBF has no nodes: {}", pbf.display());
    }
    // Small pad so boundary ways are not clipped away.
    Ok([
        min_lat - 0.01,
        min_lon - 0.01,
        max_lat + 0.01,
        max_lon + 0.01,
    ])
}

fn collect_poi_records(pbf: &Path) -> anyhow::Result<Vec<PoiRecord>> {
    let mut out = Vec::new();
    let file = std::fs::File::open(pbf)?;
    let reader = ElementReader::new(file);
    reader.for_each(|element| {
        let (id, lat, lon, tags) = match element {
            Element::Node(n) => (
                n.id(),
                n.lat(),
                n.lon(),
                n.tags()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect::<HashMap<_, _>>(),
            ),
            Element::DenseNode(n) => (
                n.id,
                n.lat(),
                n.lon(),
                n.tags()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect::<HashMap<_, _>>(),
            ),
            _ => return,
        };
        let categories = classify_tags(&tags);
        if categories.is_empty() {
            return;
        }
        out.push(PoiRecord {
            osm_id: id,
            lat,
            lon,
            categories,
            icon_key: osm_icon_key(&tags),
            name: tags.get("name").cloned(),
            tags,
        });
    })?;
    Ok(out)
}

type BarrierLineBboxes = Vec<(f64, f64, f64, f64)>;
type BarrierPolylines = Vec<Vec<[f64; 2]>>;

fn extract_pbf_barrier_geometry(
    pbf: &Path,
) -> anyhow::Result<(BarrierLineBboxes, BarrierPolylines)> {
    #[derive(Clone, Copy)]
    enum Kind {
        Line,
        Glacier,
    }

    let mut ways: Vec<(Vec<i64>, Kind)> = Vec::new();
    let mut needed: HashSet<i64> = HashSet::new();
    {
        let file = std::fs::File::open(pbf)?;
        let reader = ElementReader::new(file);
        reader.for_each(|element| {
            let Element::Way(way) = element else {
                return;
            };
            let tags: HashMap<String, String> = way
                .tags()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            let railway = tags.get("railway").map(String::as_str);
            let waterway = tags.get("waterway").map(String::as_str);
            let natural = tags.get("natural").map(String::as_str);
            let kind = if matches!(
                railway,
                Some(r) if !matches!(r, "abandoned" | "disused" | "razed" | "dismantled")
            ) || matches!(waterway, Some("river" | "canal"))
                || matches!(natural, Some("cliff" | "arete"))
            {
                Some(Kind::Line)
            } else if natural == Some("glacier") {
                Some(Kind::Glacier)
            } else {
                None
            };
            let Some(kind) = kind else {
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
        let file = std::fs::File::open(pbf)?;
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

    let mut segs = Vec::new();
    let mut glaciers = Vec::new();
    for (refs, kind) in ways {
        let mut ring: Vec<[f64; 2]> = Vec::with_capacity(refs.len());
        for id in &refs {
            let Some(&(lat, lon)) = coords.get(id) else {
                continue;
            };
            ring.push([lon, lat]);
        }
        if ring.len() < 2 {
            continue;
        }
        match kind {
            Kind::Line => {
                for w in ring.windows(2) {
                    segs.push((w[0][0], w[0][1], w[1][0], w[1][1]));
                }
            }
            Kind::Glacier => {
                for w in ring.windows(2) {
                    segs.push((w[0][0], w[0][1], w[1][0], w[1][1]));
                }
                if ring.len() >= 3 {
                    let first = ring[0];
                    let last = *ring.last().unwrap();
                    if first != last {
                        segs.push((last[0], last[1], first[0], first[1]));
                        ring.push(first);
                    }
                    glaciers.push(ring);
                }
            }
        }
    }
    Ok((segs, glaciers))
}

fn highway_barrier_segs(graph: &RouteGraph) -> Vec<(f64, f64, f64, f64)> {
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
        segs.push((a.coord.x, a.coord.y, b.coord.x, b.coord.y));
    }
    segs
}

/// Convert a region PBF into indexed packs under `data_dir`.
///
/// Cancelling via [`crate::download::DownloadControl`] mid-write deletes the
/// current `.partial` and leaves prior good archives untouched.
pub fn convert_region_packs(opts: &ConvertOptions) -> anyhow::Result<ConvertReport> {
    let t0 = Instant::now();
    let stem = stem_of(&opts.pbf);
    let (pbf_sz, pbf_mtime) = pbf_fingerprint(&opts.pbf)?;
    let pbf_filename = opts
        .pbf
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("region.osm.pbf")
        .to_string();

    download_progress::set(0, Some(5), "Building indexed maps: scanning bounds…");
    let region_bbox = pbf_node_bbox(&opts.pbf)?;

    let elev_svc;
    let elev_ref = if let Some(dir) = &opts.elev_dir {
        elev_svc = ElevationService::new(ElevationCache::new(dir.clone()));
        let _ = elev_svc.warm_bbox(region_bbox);
        Some(&elev_svc)
    } else {
        None
    };

    let mut graph_files: BTreeMap<String, String> = BTreeMap::new();
    let mut nodes = 0usize;
    let mut edges = 0usize;
    let mut car_graph: Option<RouteGraph> = None;

    let profiles = if opts.profiles.is_empty() {
        vec![RoutingProfile::Car, RoutingProfile::Foot]
    } else {
        opts.profiles.clone()
    };

    for profile in &profiles {
        if opts.control.is_cancelled() {
            anyhow::bail!("cancelled");
        }
        let key = profile_key(*profile).to_string();
        download_progress::set(
            graph_files.len() as u64 + 1,
            Some((profiles.len() + 3) as u64),
            &format!("Building indexed maps: graph ({key})…"),
        );
        // Use bbox builder (not osm4routing full read): avoids Missing-node panics
        // on some extracts and matches the 4GB-safe path documented in bbox_build.
        let graph = RouteGraph::build_from_pbf_bbox(&opts.pbf, *profile, region_bbox)?;
        nodes = graph.nodes.len().max(nodes);
        edges = graph.edges.len().max(edges);
        let pack = FlatGraphPack::from_route_graph(&graph, elev_ref);
        let payload = rkyv::to_bytes::<RkyvError>(&pack)
            .map_err(|e| anyhow::anyhow!("rkyv graph serialize: {e}"))?;
        let name = graph_pack_filename(&stem, &key);
        let path = opts.data_dir.join(&name);
        discard_partial(&path);
        if opts.control.is_cancelled() {
            discard_partial(&path);
            anyhow::bail!("cancelled");
        }
        write_archive_atomic(
            &path,
            Preamble::new(MAGIC_GRAPH, GRAPH_FORMAT_VERSION),
            payload.as_ref(),
        )?;
        graph_files.insert(key.clone(), name);
        if *profile == RoutingProfile::Car {
            car_graph = Some(graph);
        }
    }

    download_progress::set(
        (profiles.len() + 1) as u64,
        Some((profiles.len() + 3) as u64),
        "Building indexed maps: POI + barriers…",
    );

    let records = collect_poi_records(&opts.pbf)?;
    let (mut segs, glaciers) = extract_pbf_barrier_geometry(&opts.pbf)?;
    if let Some(g) = &car_graph {
        segs.extend(highway_barrier_segs(g));
    } else if let Some(first) = profiles.first() {
        let g = RouteGraph::build_from_pbf_bbox(&opts.pbf, *first, region_bbox)?;
        segs.extend(highway_barrier_segs(&g));
    }
    let poi_pack = FlatPoiBarrierPack::from_parts(&records, &segs, &glaciers);
    let poi_payload = rkyv::to_bytes::<RkyvError>(&poi_pack)
        .map_err(|e| anyhow::anyhow!("rkyv poi serialize: {e}"))?;
    let poi_name = poi_barrier_pack_filename(&stem);
    let poi_path = opts.data_dir.join(&poi_name);
    discard_partial(&poi_path);
    if opts.control.is_cancelled() {
        discard_partial(&poi_path);
        anyhow::bail!("cancelled");
    }
    write_archive_atomic(
        &poi_path,
        Preamble::new(MAGIC_POI_BARRIER, POI_BARRIER_FORMAT_VERSION),
        poi_payload.as_ref(),
    )?;

    download_progress::set(
        (profiles.len() + 2) as u64,
        Some((profiles.len() + 3) as u64),
        "Building indexed maps: wetlands…",
    );
    let wetlands = WetlandIndex::load_from_pbf(&opts.pbf)?;
    let wetland_rings = wetlands.ring_count();
    let wet_pack = FlatWetlandPack::from_wetland_index(&wetlands);
    let wet_payload = rkyv::to_bytes::<RkyvError>(&wet_pack)
        .map_err(|e| anyhow::anyhow!("rkyv wetland serialize: {e}"))?;
    let wet_name = wetland_pack_filename(&stem);
    let wet_path = opts.data_dir.join(&wet_name);
    discard_partial(&wet_path);
    if opts.control.is_cancelled() {
        discard_partial(&wet_path);
        anyhow::bail!("cancelled");
    }
    write_archive_atomic(
        &wet_path,
        Preamble::new(MAGIC_WETLAND, WETLAND_FORMAT_VERSION),
        wet_payload.as_ref(),
    )?;

    let manifest = NaviManifest {
        schema: NaviManifest::SCHEMA,
        stem: stem.clone(),
        pbf_filename,
        pbf_size_bytes: pbf_sz,
        pbf_modified_unix_secs: pbf_mtime,
        graph_files: graph_files.clone(),
        graph_format_version: GRAPH_FORMAT_VERSION,
        poi_barrier_file: poi_name.clone(),
        poi_barrier_format_version: POI_BARRIER_FORMAT_VERSION,
        wetland_file: Some(wet_name.clone()),
        wetland_format_version: WETLAND_FORMAT_VERSION,
        has_delta_h: elev_ref.is_some(),
        elev_dir: opts
            .elev_dir
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned()),
    };
    let man_path = manifest_path(&opts.data_dir, &stem);
    manifest.save(&man_path)?;

    download_progress::set(
        (profiles.len() + 3) as u64,
        Some((profiles.len() + 3) as u64),
        "Indexed maps ready",
    );

    Ok(ConvertReport {
        stem,
        graph_files: graph_files.values().cloned().collect(),
        poi_barrier_file: poi_name,
        wetland_file: wet_name,
        manifest_file: man_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("manifest")
            .to_string(),
        nodes,
        edges,
        pois: records.len(),
        barrier_segs: segs.len(),
        wetland_rings,
        convert_ms: t0.elapsed().as_secs_f64() * 1000.0,
        has_delta_h: elev_ref.is_some(),
    })
}
