//! Memory-conscious graph build clipped to a WGS84 bbox.
//!
//! Full Ostlandet car graphs are ~300MB on disk and peak far higher in RAM; loading
//! them on a 4GB Automotive AVD kills the process (LMK) before routing starts.
//! Planning clips the same region `.pbf` to the trip bbox so we never materialize
//! the nationwide graph in-process.
//!
//! Tiled convert (`build_tiled_from_pbf`) spills filtered ways to a tempfile and
//! only keeps highway-referenced coordinates in RAM, then builds+writes one tile
//! at a time so the first `.rkyv` appears after two PBF passes — without holding
//! every way's full tag map in-process (that path LMK'd ~4GB tablets before any
//! tile was written).

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use geo_types::Coord;
use osm4routing::{Node, NodeId};
use osmpbf::Element;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use super::builder::{ferry_base_weight_m, GraphEdge, RouteGraph, RoutingProfile};
use super::surface_quality::classify_surface_tags;
use crate::routing::access;
use crate::routing::wetland::tags_map_indicate_boardwalk;

/// Max tiles built in parallel during Step 3. Kept small for 4 GB-class tablets:
/// project docs cite SM-P613 `peak_rss_mb`≈1737 on the old sequential pipeline
/// but do not define a firm process-RSS cap — N=2 targets ~2 GB with headroom.
const TILE_BUILD_CONCURRENCY: usize = 2;

#[derive(Clone)]
struct RawWay {
    id: i64,
    nodes: Vec<i64>,
    tags: HashMap<String, String>,
}

/// Tags needed by [`graph_from_raw_ways`] / access / boardwalk — drop the rest
/// so Pass 1 does not retain every OSM key on every highway.
fn keep_way_tag(key: &str) -> bool {
    matches!(
        key,
        "highway"
            | "oneway"
            | "junction"
            | "maxspeed"
            | "maxspeed:practical"
            | "maxspeed:advisory"
            | "maxspeed:type"
            | "maxspeed:variable"
            | "maxspeed:conditional"
            | "minspeed"
            | "name"
            | "ref"
            | "int_ref"
            | "motorroad"
            | "expressway"
            | "lanes"
            | "maxweight"
            | "maxaxleload"
            | "maxbogieweight"
            | "maxheight"
            | "maxwidth"
            | "maxlength"
            | "toll"
            | "toll:motor_vehicle"
            | "toll:motorcar"
            | "toll:motorcycle"
            | "toll:hgv"
            | "toll:bicycle"
            | "toll:foot"
            | "route"
            | "ferry"
            | "bridge"
            | "tunnel"
            | "man_made"
            | "railway"
            | "surface"
            | "tracktype"
            | "motor_vehicle"
            | "access"
            | "foot"
            | "bicycle"
            | "motor_vehicle:conditional"
            | "access:conditional"
            | "_ferry_approach_promoted"
            | "duration"
            | "interval"
    )
}

fn filter_way_tags(tags: HashMap<String, String>) -> HashMap<String, String> {
    tags.into_iter().filter(|(k, _)| keep_way_tag(k)).collect()
}

fn filter_barrier_tags(tags: HashMap<String, String>) -> HashMap<String, String> {
    tags.into_iter()
        .filter(|(k, _)| {
            matches!(
                k.as_str(),
                "barrier" | "access" | "motor_vehicle" | "foot" | "bicycle"
            )
        })
        .collect()
}

#[derive(Serialize, Deserialize)]
struct SpilledWay {
    id: i64,
    nodes: Vec<i64>,
    tags: Vec<(String, String)>,
}

impl SpilledWay {
    fn from_raw(id: i64, nodes: Vec<i64>, tags: HashMap<String, String>) -> Self {
        Self {
            id,
            nodes,
            tags: tags.into_iter().collect(),
        }
    }

    fn into_raw(self) -> RawWay {
        RawWay {
            id: self.id,
            nodes: self.nodes,
            tags: self.tags.into_iter().collect(),
        }
    }
}

struct TempSpill {
    path: PathBuf,
}

impl TempSpill {
    fn create_unbuffered(dir: &Path, label: &str) -> anyhow::Result<(Self, std::fs::File)> {
        let path = dir.join(format!(
            "navi-{}-{}-{}.bin",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        ));
        let file = std::fs::File::create(&path)?;
        Ok((Self { path }, file))
    }

    fn create(dir: &Path, label: &str) -> anyhow::Result<(Self, BufWriter<std::fs::File>)> {
        let path = dir.join(format!(
            "navi-{}-{}-{}.bin",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        ));
        let file = std::fs::File::create(&path)?;
        Ok((Self { path }, BufWriter::new(file)))
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempSpill {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn write_spilled_way(w: &mut impl Write, way: &SpilledWay) -> anyhow::Result<()> {
    let bytes = bincode::serialize(way).map_err(|e| anyhow::anyhow!("spill serialize: {e}"))?;
    let len = u32::try_from(bytes.len()).map_err(|_| anyhow::anyhow!("spill way too large"))?;
    w.write_all(&len.to_le_bytes())?;
    w.write_all(&bytes)?;
    Ok(())
}

fn read_spilled_way(r: &mut impl Read) -> anyhow::Result<Option<SpilledWay>> {
    let mut len_buf = [0u8; 4];
    match r.read_exact(&mut len_buf) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut bytes = vec![0u8; len];
    r.read_exact(&mut bytes)?;
    let way =
        bincode::deserialize(&bytes).map_err(|e| anyhow::anyhow!("spill deserialize: {e}"))?;
    Ok(Some(way))
}

fn append_spill_file(dest: &mut impl Write, src_path: &Path) -> anyhow::Result<u64> {
    let file = std::fs::File::open(src_path)?;
    let mut reader = BufReader::new(file);
    let mut count = 0u64;
    while let Some(way) = read_spilled_way(&mut reader)? {
        write_spilled_way(dest, &way)?;
        count += 1;
    }
    Ok(count)
}

/// Pass 1 for tiled convert: filter profile highways and append length-prefixed
/// [`SpilledWay`] records. Each Rayon worker keeps its own spill file and node-id
/// batch so bincode serialization and writes never contend on a global lock.
fn spill_tiled_highway_ways(
    path: &Path,
    spill_dir: &Path,
    profiles: &[RoutingProfile],
    dest: &mut impl Write,
) -> anyhow::Result<(HashSet<i64>, u64)> {
    struct ThreadPass1 {
        /// Kept so [`TempSpill`]'s drop guard does not delete the file while
        /// `writer` is still appending.
        _spill: TempSpill,
        writer: std::fs::File,
        batch_needed: HashSet<i64>,
    }

    static THREAD_SPILL_ID: AtomicU32 = AtomicU32::new(0);
    static THREAD_PASS_ID: AtomicU32 = AtomicU32::new(0);
    let pass_id = THREAD_PASS_ID.fetch_add(1, Ordering::Relaxed);
    thread_local! {
        static PASS_ID: Cell<u32> = const { Cell::new(u32::MAX) };
        static STATE: RefCell<Option<ThreadPass1>> = const { RefCell::new(None) };
    }

    let spill_dir = spill_dir.to_path_buf();
    let spill_err = Mutex::new(None::<anyhow::Error>);
    let spill_paths: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
    let needed_acc = Arc::new(Mutex::new(HashSet::new()));

    crate::download::pbf_priority::for_each_pbf_data_block(path, |block| {
        if spill_err
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
        {
            return Ok(());
        }
        STATE.with(|state_cell| {
            if PASS_ID.with(|p| p.get()) != pass_id {
                PASS_ID.with(|p| p.set(pass_id));
                *state_cell.borrow_mut() = None;
            }
            if state_cell.borrow().is_none() {
                let id = THREAD_SPILL_ID.fetch_add(1, Ordering::Relaxed);
                let (spill, writer) =
                    TempSpill::create_unbuffered(&spill_dir, &format!("tiled-ways-t{id}"))
                        .expect("thread spill create");
                spill_paths
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(spill.path().to_path_buf());
                *state_cell.borrow_mut() = Some(ThreadPass1 {
                    _spill: spill,
                    writer,
                    batch_needed: HashSet::new(),
                });
            }
            {
                let mut state = state_cell.borrow_mut();
                let Some(state) = state.as_mut() else {
                    return;
                };
                state.batch_needed.clear();
                block.for_each_element(|element| {
                    let Element::Way(way) = element else {
                        return;
                    };
                    let tags = filter_way_tags(
                        way.tags()
                            .map(|(k, v)| (k.to_string(), v.to_string()))
                            .collect(),
                    );
                    if !way_ok_for_any(&tags, profiles) {
                        return;
                    }
                    let refs: Vec<i64> = way.refs().collect();
                    if refs.is_empty() {
                        return;
                    }
                    for id in &refs {
                        state.batch_needed.insert(*id);
                    }
                    let spilled = SpilledWay::from_raw(way.id(), refs, tags);
                    if let Err(e) = write_spilled_way(&mut state.writer, &spilled) {
                        *spill_err.lock().unwrap_or_else(|x| x.into_inner()) = Some(e);
                    }
                });
                if !state.batch_needed.is_empty() {
                    needed_acc
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .extend(state.batch_needed.drain());
                }
            }
        });
        if let Some(e) = spill_err.lock().unwrap_or_else(|e| e.into_inner()).take() {
            return Err(e);
        }
        Ok(())
    })?;

    let mut way_count = 0u64;
    let mut paths = spill_paths.lock().unwrap_or_else(|e| e.into_inner());
    paths.sort();
    for ways_path in paths.drain(..) {
        way_count += append_spill_file(dest, &ways_path)?;
    }
    let needed = Arc::try_unwrap(needed_acc)
        .map_err(|_| anyhow::anyhow!("needed set still shared"))?
        .into_inner()
        .map_err(|_| anyhow::anyhow!("needed set poisoned"))?;
    Ok((needed, way_count))
}

fn in_bbox(lat: f64, lon: f64, bbox: [f64; 4]) -> bool {
    lat >= bbox[0] && lat <= bbox[2] && lon >= bbox[1] && lon <= bbox[3]
}

fn in_chebyshev_vacuum(lat: f64, lon: f64, terminals: &[(f64, f64)], deg: f64) -> bool {
    terminals
        .iter()
        .any(|(tlat, tlon)| (lat - tlat).abs() <= deg && (lon - tlon).abs() <= deg)
}

/// Chebyshev radius (degrees) for pier/footway promotion vacuum around a
/// terminal (~1.7 km).
const TERMINAL_PIER_VACUUM_DEG: f64 = 0.015;

fn car_highway_ok(highway: &str) -> bool {
    matches!(
        highway,
        "motorway"
            | "motorway_link"
            | "motorway_junction"
            | "trunk"
            | "trunk_link"
            | "primary"
            | "primary_link"
            | "secondary"
            | "secondary_link"
            | "tertiary"
            | "tertiary_link"
            | "unclassified"
            | "residential"
            | "living_street"
            | "road"
            | "service"
            | "track"
            // Rare OSM form; most ferries are `route=ferry` without highway (see
            // [`tags_indicate_ferry`]).
            | "ferry"
    )
}

/// OSM car/foot ferry ways: typically `route=ferry` with **no** `highway=*`.
/// Pass 1 used to require a highway class and dropped these entirely, so pack
/// graphs reported `graph_ferry_edges=0` and Fehmarn-style densify hops
/// disconnected across water.
fn tags_indicate_ferry(tags: &HashMap<String, String>) -> bool {
    tags.get("route")
        .is_some_and(|v| v.eq_ignore_ascii_case("ferry"))
        || tags
            .get("highway")
            .is_some_and(|v| v.eq_ignore_ascii_case("ferry"))
        || tags
            .get("ferry")
            .is_some_and(|v| super::builder::is_truthy_tag(v))
}

fn way_ok_for_profile(tags: &HashMap<String, String>, profile: RoutingProfile) -> bool {
    if tags_indicate_ferry(tags) {
        // Keep ferries for every profile; [`access::tags_forbid_mode`] drops
        // vehicle-only / pedestrian-no cases when building edges.
        return true;
    }
    let Some(highway) = tags.get("highway") else {
        return false;
    };
    if crate::routing::graph::is_construction_or_proposed_highway(Some(highway.as_str())) {
        return false;
    }
    highway_ok_for_profile(highway, profile)
}

/// Tags for a ferry-terminal approach way retained in the overlay.
///
/// Real OSM piers / boarding links are often `man_made=pier` or
/// `highway=footway` (movable pier bridges) without a car highway class. Car
/// profile would reject them, so approach hops never leave the ferry terminal
/// and merge-by-OSM-id cannot reach pack land. Promote those stubs to
/// `highway=service` for overlay topology only — A* still chooses ferries under
/// normal costs when `avoid_ferries` is off.
fn ferry_approach_tags(
    mut tags: HashMap<String, String>,
    profile: RoutingProfile,
) -> Option<HashMap<String, String>> {
    if tags_indicate_ferry(&tags) {
        return None;
    }
    let highway = tags.get("highway").map(String::as_str);
    let promote = matches!(
        highway,
        Some("footway") | Some("path") | Some("pedestrian") | Some("steps") | Some("platform")
    ) || tags
        .get("man_made")
        .is_some_and(|v| v.eq_ignore_ascii_case("pier"))
        || tags
            .get("railway")
            .is_some_and(|v| v.eq_ignore_ascii_case("platform"));
    if promote {
        tags.insert("highway".into(), "service".into());
        // Former footway/pier links are pedestrian by implication; force motor
        // access so car/truck overlay edges are materialized for topology merge.
        tags.insert("motor_vehicle".into(), "yes".into());
        tags.insert("foot".into(), "yes".into());
        // Marker so vacuum can prefer pier promotions without swallowing every
        // residential road within the terminal radius.
        tags.insert("_ferry_approach_promoted".into(), "1".into());
    }
    if !tags.contains_key("highway") {
        return None;
    }
    if !way_ok_for_profile(&tags, profile) {
        return None;
    }
    Some(tags)
}

fn way_ok_for_any(tags: &HashMap<String, String>, profiles: &[RoutingProfile]) -> bool {
    profiles.iter().any(|&p| way_ok_for_profile(tags, p))
}

fn non_motorway_car_highway_ok(highway: &str) -> bool {
    car_highway_ok(highway)
        && !matches!(highway, "motorway" | "motorway_link" | "motorway_junction")
}

fn highway_ok_for_profile(highway: &str, profile: RoutingProfile) -> bool {
    match profile {
        RoutingProfile::Car | RoutingProfile::Truck => car_highway_ok(highway),
        RoutingProfile::Foot => {
            matches!(
                highway,
                "footway"
                    | "path"
                    | "steps"
                    | "pedestrian"
                    | "living_street"
                    | "residential"
                    | "service"
                    | "track"
                    | "unclassified"
                    | "tertiary"
                    | "secondary"
                    | "primary"
                    | "cycleway"
            ) || non_motorway_car_highway_ok(highway)
        }
        // Never ingest motorway-class ways into bicycle graphs (illegal / unsuitable).
        RoutingProfile::Bicycle => {
            non_motorway_car_highway_ok(highway)
                || matches!(highway, "cycleway" | "path" | "footway")
        }
    }
}

fn profile_label(profile: RoutingProfile) -> &'static str {
    match profile {
        RoutingProfile::Car => "car",
        RoutingProfile::Truck => "truck",
        RoutingProfile::Foot => "foot",
        RoutingProfile::Bicycle => "bicycle",
    }
}

fn parse_metric(raw: &str) -> Option<f64> {
    let cleaned = raw.trim().to_lowercase().replace(['t', 'm'], "");
    cleaned.trim().parse::<f64>().ok()
}

fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6_378_100.0;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

fn oneway_forward_only(tags: &HashMap<String, String>) -> bool {
    if tags.get("junction").is_some_and(|v| v == "roundabout") {
        return true;
    }
    matches!(
        tags.get("oneway").map(String::as_str),
        Some("yes" | "true" | "1")
    )
}

/// Sub-phase timings from [`RouteGraph::build_tiled_from_pbf`].
#[derive(Debug, Clone, Copy, Default)]
pub struct TiledBuildTimings {
    /// Way-to-tile spill assignment (sequential).
    pub tile_assign_ms: f64,
    /// Per-tile graph build + caller pack/write (batched, up to [`TILE_BUILD_CONCURRENCY`]).
    pub tile_build_ms: f64,
}

impl RouteGraph {
    /// Build a car/truck/foot/bike graph from `path`, keeping only ways that
    /// touch `bbox` `[min_lat, min_lon, max_lat, max_lon]`.
    pub fn build_from_pbf_bbox(
        path: impl AsRef<Path>,
        profile: RoutingProfile,
        bbox: [f64; 4],
    ) -> anyhow::Result<Self> {
        let path = path.as_ref();
        crate::routing::pbf_extract::ensure_pbf_usable_for_graph_build(path)?;
        crate::download::progress::set(0, Some(4), "Planning route: indexing area…");
        // Pass 1: node ids inside bbox (ids only — storing every coord OOMs on large extracts).
        let mut in_bbox_ids: HashSet<i64> = HashSet::new();
        {
            crate::download::plan_cancel::abort_if_cancelled()?;
            crate::download::pbf_priority::for_each_pbf_elements(path, |element| match element {
                Element::Node(n) => {
                    if in_bbox(n.lat(), n.lon(), bbox) {
                        in_bbox_ids.insert(n.id());
                    }
                }
                Element::DenseNode(n) if in_bbox(n.lat(), n.lon(), bbox) => {
                    in_bbox_ids.insert(n.id());
                }
                _ => {}
            })?;
        }

        crate::download::plan_cancel::abort_if_cancelled()?;
        crate::download::progress::set(1, Some(4), "Planning route: reading roads…");
        // Pass 2: highway ways that reference at least one in-bbox node.
        let mut ways: Vec<RawWay> = Vec::new();
        let mut needed: HashSet<i64> = HashSet::new();
        {
            crate::download::pbf_priority::for_each_pbf_elements(path, |element| {
                let Element::Way(way) = element else {
                    return;
                };
                let tags = filter_way_tags(
                    way.tags()
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                        .collect(),
                );
                if !way_ok_for_profile(&tags, profile) {
                    return;
                }
                let refs: Vec<i64> = way.refs().collect();
                if refs.is_empty() {
                    return;
                }
                if !refs.iter().any(|id| in_bbox_ids.contains(id)) {
                    return;
                }
                for id in &refs {
                    needed.insert(*id);
                }
                ways.push(RawWay {
                    id: way.id(),
                    nodes: refs,
                    tags,
                });
            })?;
        }
        drop(in_bbox_ids);

        crate::download::plan_cancel::abort_if_cancelled()?;
        crate::download::progress::set(2, Some(4), "Planning route: loading geometry…");
        // Pass 3: coords + barrier access tags for nodes referenced by kept ways.
        let mut coords: HashMap<i64, (f64, f64)> = HashMap::with_capacity(needed.len());
        let mut barrier_tags: HashMap<i64, HashMap<String, String>> = HashMap::new();
        {
            crate::download::pbf_priority::for_each_pbf_elements(path, |element| match element {
                Element::Node(n) => {
                    if needed.contains(&n.id()) {
                        coords.insert(n.id(), (n.lat(), n.lon()));
                        let tags: HashMap<String, String> = n
                            .tags()
                            .map(|(k, v)| (k.to_string(), v.to_string()))
                            .collect();
                        if tags.contains_key("barrier") {
                            barrier_tags.insert(n.id(), filter_barrier_tags(tags));
                        }
                    }
                }
                Element::DenseNode(n) if needed.contains(&n.id()) => {
                    coords.insert(n.id(), (n.lat(), n.lon()));
                    let tags: HashMap<String, String> = n
                        .tags()
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                        .collect();
                    if tags.contains_key("barrier") {
                        barrier_tags.insert(n.id(), filter_barrier_tags(tags));
                    }
                }
                _ => {}
            })?;
        }
        drop(needed);

        crate::download::plan_cancel::abort_if_cancelled()?;
        crate::download::progress::set(3, Some(4), "Planning route: linking graph…");
        let arcs: Vec<Arc<RawWay>> = ways.into_iter().map(Arc::new).collect();
        let graph = graph_from_raw_ways(&arcs, &coords, profile, &barrier_tags)?;
        if graph.edges.is_empty() {
            anyhow::bail!("bbox graph empty for {bbox:?} from {}", path.display());
        }
        Ok(graph)
    }

    /// Ferry overlay from a region `.osm.pbf`: OSM `route=ferry` ways plus
    /// pier / footway promotions that share terminal nodes.
    ///
    /// Does not invent water crossings or land hops. Overlay only materializes
    /// ferries already present in the extract. Pack convert must tile the
    /// full catalog/PBF extent so those ways are also in loaded graph tiles.
    pub fn build_ferry_overlay_from_pbf(
        path: impl AsRef<Path>,
        profile: RoutingProfile,
        bbox: [f64; 4],
    ) -> anyhow::Result<Self> {
        let path = path.as_ref();
        let t_all = Instant::now();

        // Pass 1 — ferries + pier/footway/platform promotions.
        let t0 = Instant::now();
        let mut ferry_ways: Vec<RawWay> = Vec::new();
        let mut approach_ways: Vec<RawWay> = Vec::new();
        crate::download::pbf_priority::for_each_pbf_elements(path, |element| {
            let Element::Way(way) = element else {
                return;
            };
            let tags = filter_way_tags(
                way.tags()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            );
            let refs: Vec<i64> = way.refs().collect();
            if refs.is_empty() {
                return;
            }
            if tags_indicate_ferry(&tags) {
                if !way_ok_for_profile(&tags, profile) {
                    return;
                }
                ferry_ways.push(RawWay {
                    id: way.id(),
                    nodes: refs,
                    tags,
                });
                return;
            }
            let Some(tags) = ferry_approach_tags(tags, profile) else {
                return;
            };
            if !tags.contains_key("_ferry_approach_promoted") {
                return;
            }
            approach_ways.push(RawWay {
                id: way.id(),
                nodes: refs,
                tags,
            });
        })?;
        let t_ways_ms = t0.elapsed().as_millis() as u64;

        if ferry_ways.is_empty() {
            crate::routing::plan_perf::note(
                "ferry_overlay_build",
                format!(
                    "empty_ferries;ways_ms={t_ways_ms};approaches={};total_ms={}",
                    approach_ways.len(),
                    t_all.elapsed().as_millis()
                ),
            );
            return Ok(Self::from_parts(HashMap::new(), Vec::new(), profile));
        }

        // node_id → indices into approach_ways for hops from ferry terminals.
        let mut node_to_approach: HashMap<i64, Vec<usize>> = HashMap::new();
        for (i, w) in approach_ways.iter().enumerate() {
            for &id in &w.nodes {
                node_to_approach.entry(id).or_default().push(i);
            }
        }

        // Pass 2 — coords for ferry + pier-approach nodes only.
        let mut all_ref_ids: HashSet<i64> = HashSet::new();
        for w in &ferry_ways {
            all_ref_ids.extend(w.nodes.iter().copied());
        }
        for w in &approach_ways {
            all_ref_ids.extend(w.nodes.iter().copied());
        }

        let t_n = Instant::now();
        let mut coords: HashMap<i64, (f64, f64)> =
            HashMap::with_capacity(all_ref_ids.len().min(200_000));
        crate::download::pbf_priority::for_each_pbf_elements(path, |element| {
            let (id, lat, lon) = match element {
                Element::Node(n) => (n.id(), n.lat(), n.lon()),
                Element::DenseNode(n) => (n.id(), n.lat(), n.lon()),
                _ => return,
            };
            if all_ref_ids.contains(&id) {
                coords.insert(id, (lat, lon));
            }
        })?;
        drop(all_ref_ids);
        let t_nodes_ms = t_n.elapsed().as_millis() as u64;

        let ferry_in_bbox = |w: &RawWay| {
            w.nodes.iter().any(|id| {
                coords
                    .get(id)
                    .is_some_and(|&(lat, lon)| in_bbox(lat, lon, bbox))
            })
        };
        let mut ways: Vec<RawWay> = ferry_ways.into_iter().filter(ferry_in_bbox).collect();
        if ways.is_empty() {
            crate::routing::plan_perf::note(
                "ferry_overlay_build",
                format!(
                    "ferries_outside_bbox;ways_ms={t_ways_ms};nodes_ms={t_nodes_ms};total_ms={}",
                    t_all.elapsed().as_millis()
                ),
            );
            return Ok(Self::from_parts(HashMap::new(), Vec::new(), profile));
        }

        let t_sel = Instant::now();
        let mut ferry_nodes: HashSet<i64> = HashSet::new();
        let mut terminal_ids: HashSet<i64> = HashSet::new();
        for w in &ways {
            for &id in &w.nodes {
                ferry_nodes.insert(id);
            }
            if w.nodes.len() >= 2 {
                terminal_ids.insert(w.nodes[0]);
                terminal_ids.insert(*w.nodes.last().unwrap());
            }
        }

        // Up to 4 hops through pier/footway promotions from ferry terminals.
        let mut selected_approach: HashSet<usize> = HashSet::new();
        let mut attach = ferry_nodes;
        for _hop in 0..4 {
            let mut next_attach: HashSet<i64> = HashSet::new();
            for &nid in &attach {
                let Some(idxs) = node_to_approach.get(&nid) else {
                    continue;
                };
                for &i in idxs {
                    if !selected_approach.insert(i) {
                        continue;
                    }
                    for &id in &approach_ways[i].nodes {
                        next_attach.insert(id);
                    }
                }
            }
            attach = next_attach;
            if attach.is_empty() {
                break;
            }
        }

        let mut terminal_coords: Vec<(f64, f64)> = Vec::with_capacity(terminal_ids.len());
        for id in &terminal_ids {
            if let Some(&(lat, lon)) = coords.get(id) {
                terminal_coords.push((lat, lon));
            }
        }
        let mut vacuum_added = 0usize;
        for (i, w) in approach_ways.iter().enumerate() {
            if selected_approach.contains(&i) {
                continue;
            }
            let near = w.nodes.iter().any(|id| {
                coords.get(id).is_some_and(|&(lat, lon)| {
                    in_chebyshev_vacuum(lat, lon, &terminal_coords, TERMINAL_PIER_VACUUM_DEG)
                })
            });
            if near {
                selected_approach.insert(i);
                vacuum_added += 1;
            }
        }
        let t_select_ms = t_sel.elapsed().as_millis() as u64;

        let mut kept_approach = 0usize;
        for i in selected_approach {
            ways.push(approach_ways[i].clone());
            kept_approach += 1;
        }
        let ferry_count = ways.iter().filter(|w| tags_indicate_ferry(&w.tags)).count();

        let t_g = Instant::now();
        let barrier_tags: HashMap<i64, HashMap<String, String>> = HashMap::new();
        let arcs: Vec<Arc<RawWay>> = ways.into_iter().map(Arc::new).collect();
        let graph = graph_from_raw_ways(&arcs, &coords, profile, &barrier_tags)?;
        let t_graph_ms = t_g.elapsed().as_millis() as u64;

        crate::routing::plan_perf::note(
            "ferry_overlay_build",
            format!(
                "ferry_ways={ferry_count};approach_ways={kept_approach};vacuum_added={vacuum_added};\
                 ways_ms={t_ways_ms};select_ms={t_select_ms};nodes_ms={t_nodes_ms};graph_ms={t_graph_ms};\
                 edges={};total_ms={}",
                graph.edges.len(),
                t_all.elapsed().as_millis()
            ),
        );
        Ok(graph)
    }

    /// Build graphs for all spatial tiles with **two PBF passes total** (not
    /// per tile). Filtered ways are spilled; coords are loaded once; then each
    /// tile is built+written and dropped.
    ///
    /// For multiple profiles, Pass 1/2 run once over the highway union; each
    /// profile then filters the shared spill and runs its own tile-assign +
    /// tile-build. Truck must not be included (alias car packs instead).
    pub fn build_tiled_from_pbf(
        path: impl AsRef<Path>,
        profile: RoutingProfile,
        tiles: &[(usize, usize, [f64; 4])],
        pad_deg: f64,
        spill_dir: impl AsRef<Path>,
        on_tile: impl Fn(usize, usize, [f64; 4], Self) -> anyhow::Result<()> + Send + Sync,
    ) -> anyhow::Result<(usize, TiledBuildTimings)> {
        let skip = HashSet::new();
        let results = Self::build_tiled_from_pbf_profiles(
            path,
            &[profile],
            tiles,
            pad_deg,
            spill_dir,
            &skip,
            move |_profile, row, col, logical, g| on_tile(row, col, logical, g),
        )?;
        results
            .into_iter()
            .next()
            .map(|(_, produced, timings)| (produced, timings))
            .ok_or_else(|| anyhow::anyhow!("tiled graph empty for profile {profile:?}"))
    }

    /// Shared Pass 1/2 for all `profiles`, then per-profile tile-assign + build.
    ///
    /// `skip_tiles` is `(profile, row, col)` for archives already on disk from a
    /// crashed convert. Pass 1/2 still run (way spill is not checkpointed); the
    /// matching tile-build jobs are omitted.
    pub fn build_tiled_from_pbf_profiles(
        path: impl AsRef<Path>,
        profiles: &[RoutingProfile],
        tiles: &[(usize, usize, [f64; 4])],
        pad_deg: f64,
        spill_dir: impl AsRef<Path>,
        skip_tiles: &HashSet<(RoutingProfile, usize, usize)>,
        on_tile: impl Fn(RoutingProfile, usize, usize, [f64; 4], Self) -> anyhow::Result<()>
            + Send
            + Sync,
    ) -> anyhow::Result<Vec<(RoutingProfile, usize, TiledBuildTimings)>> {
        let path = path.as_ref();
        let spill_dir = spill_dir.as_ref();
        if profiles.is_empty() {
            anyhow::bail!("no profiles");
        }
        if profiles.contains(&RoutingProfile::Truck) {
            anyhow::bail!("truck must alias car packs; do not build tiled truck graphs");
        }
        if tiles.is_empty() {
            anyhow::bail!("no tiles");
        }
        if tiles.len() > 64 {
            anyhow::bail!("tile grid exceeds u64 bitmask capacity ({})", tiles.len());
        }
        std::fs::create_dir_all(spill_dir)?;
        let expanded: Vec<[f64; 4]> = tiles
            .iter()
            .map(|(_, _, b)| {
                [
                    b[0] - pad_deg,
                    b[1] - pad_deg,
                    b[2] + pad_deg,
                    b[3] + pad_deg,
                ]
            })
            .collect();

        let label = profiles
            .iter()
            .map(|p| profile_label(*p))
            .collect::<Vec<_>>()
            .join("+");
        crate::download::progress::set(
            0,
            Some(4),
            &format!("Indexed maps: pass1 way-spill ({label})…"),
        );
        let (ways_spill, mut ways_writer) = TempSpill::create(spill_dir, "tiled-ways")?;
        let (needed, way_count) =
            spill_tiled_highway_ways(path, spill_dir, profiles, &mut ways_writer)?;
        ways_writer
            .flush()
            .map_err(|e| anyhow::anyhow!("spill flush: {e}"))?;
        drop(ways_writer);
        if way_count == 0 {
            anyhow::bail!("tiled graph empty for profiles {profiles:?} (no ways)");
        }
        log::info!(
            target: "NaviConvert",
            "CONVERT_PHASE pass1 done ({label}) ways={way_count} needed_nodes={}",
            needed.len()
        );

        crate::download::progress::set(
            1,
            Some(4),
            &format!("Indexed maps: pass2 coords ({label})…"),
        );
        let mut coords: HashMap<i64, (f64, f64)> = HashMap::with_capacity(needed.len());
        let mut barrier_tags: HashMap<i64, HashMap<String, String>> = HashMap::new();
        {
            crate::download::pbf_priority::for_each_pbf_elements(path, |element| match element {
                Element::Node(n) => {
                    if needed.contains(&n.id()) {
                        coords.insert(n.id(), (n.lat(), n.lon()));
                        let tags: HashMap<String, String> = n
                            .tags()
                            .map(|(k, v)| (k.to_string(), v.to_string()))
                            .collect();
                        if tags.contains_key("barrier") {
                            barrier_tags.insert(n.id(), filter_barrier_tags(tags));
                        }
                    }
                }
                Element::DenseNode(n) if needed.contains(&n.id()) => {
                    coords.insert(n.id(), (n.lat(), n.lon()));
                    let tags: HashMap<String, String> = n
                        .tags()
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                        .collect();
                    if tags.contains_key("barrier") {
                        barrier_tags.insert(n.id(), filter_barrier_tags(tags));
                    }
                }
                _ => {}
            })?;
        }
        drop(needed);
        log::info!(
            target: "NaviConvert",
            "CONVERT_PHASE pass2 done ({label}) coords={}",
            coords.len()
        );

        let mut out = Vec::with_capacity(profiles.len());
        for (pi, &profile) in profiles.iter().enumerate() {
            let last_profile = pi + 1 == profiles.len();
            let profile_key = profile_label(profile);
            crate::download::progress::set(
                2,
                Some(4),
                &format!("Indexed maps: tile-assign ({profile_key})…"),
            );
            let t_assign = Instant::now();
            let mut tile_spills: Vec<TempSpill> = Vec::with_capacity(tiles.len());
            let mut tile_writers: Vec<BufWriter<std::fs::File>> = Vec::with_capacity(tiles.len());
            for i in 0..tiles.len() {
                let (spill, w) =
                    TempSpill::create(spill_dir, &format!("tiled-{profile_key}-t{i}"))?;
                tile_spills.push(spill);
                tile_writers.push(w);
            }
            let mut tile_way_counts = vec![0u64; tiles.len()];
            let mut tile_node_ids: Vec<HashSet<i64>> =
                (0..tiles.len()).map(|_| HashSet::new()).collect();
            {
                let file = std::fs::File::open(ways_spill.path())?;
                let mut reader = BufReader::new(file);
                while let Some(way) = read_spilled_way(&mut reader)? {
                    let tags_map: HashMap<String, String> = way.tags.iter().cloned().collect();
                    if !way_ok_for_profile(&tags_map, profile) {
                        continue;
                    }
                    let mut mask = 0u64;
                    for id in &way.nodes {
                        let Some(&(lat, lon)) = coords.get(id) else {
                            continue;
                        };
                        for (i, bb) in expanded.iter().enumerate() {
                            if in_bbox(lat, lon, *bb) {
                                mask |= 1u64 << i;
                            }
                        }
                    }
                    if mask == 0 {
                        continue;
                    }
                    for (i, writer) in tile_writers.iter_mut().enumerate() {
                        if mask & (1u64 << i) != 0 {
                            write_spilled_way(writer, &way)?;
                            tile_way_counts[i] += 1;
                            tile_node_ids[i].extend(&way.nodes);
                        }
                    }
                }
            }
            for w in &mut tile_writers {
                w.flush()?;
            }
            // Release assign writers before step3; spills stay until tiles are consumed.
            drop(tile_writers);
            let tile_assign_ms = t_assign.elapsed().as_secs_f64() * 1000.0;
            log::info!(
                target: "NaviConvert",
                "CONVERT_PHASE tile-assign done ({profile_key}) ms={tile_assign_ms:.0}"
            );

            crate::download::progress::set(
                3,
                Some(4),
                &format!("Indexed maps: step3 tile-build ({profile_key})…"),
            );
            let t_build = Instant::now();
            let yield_to_plan = crate::download::pbf_priority::background_indexer_active();
            let mut produced = 0usize;
            let mut pending: Vec<usize> = (0..tiles.len())
                .filter(|&i| tile_way_counts[i] > 0)
                .filter(|&i| {
                    let (row, col, _) = tiles[i];
                    !skip_tiles.contains(&(profile, row, col))
                })
                .collect();
            let pending_total = pending.len();
            let skipped = skip_tiles.iter().filter(|(p, _, _)| *p == profile).count();
            if skipped > 0 {
                log::info!(
                    target: "NaviConvert",
                    "CONVERT_PHASE resume skip tiles ({profile_key}) kept={skipped} remaining={pending_total}"
                );
            }
            // On the last profile, drop shared Pass-2 coords we no longer need
            // before building tiles — peak RSS otherwise stacks full coords with
            // concurrent RouteGraph builds (LMK on ~4 GB devices).
            if last_profile {
                retain_coords_for_pending_tiles(&mut coords, &tile_node_ids, &pending);
                coords.shrink_to_fit();
                log::info!(
                    target: "NaviConvert",
                    "CONVERT_PHASE coords trimmed before last-profile step3 ({profile_key}) remaining_nodes={}",
                    coords.len()
                );
            }
            // Cap concurrency when the shared coord table is still huge (earlier
            // profiles cannot trim coords yet — later tile-assign needs them).
            let concurrency = if !last_profile && coords.len() > 4_000_000 {
                1
            } else {
                TILE_BUILD_CONCURRENCY
            };
            log::info!(
                target: "NaviConvert",
                "CONVERT_PHASE step3 start ({profile_key}) tiles={pending_total} concurrency={concurrency}"
            );
            crate::download::progress::set(
                0,
                Some(pending_total as u64),
                &format!("Indexed maps: step3 ({profile_key}) 0/{pending_total}…"),
            );

            while !pending.is_empty() {
                let batch_len = concurrency.min(pending.len());
                let batch: Vec<usize> = pending.drain(..batch_len).collect();
                let remaining_with_batch = pending.len() + batch.len();
                let done_before = pending_total.saturating_sub(remaining_with_batch);
                log::info!(
                    target: "NaviConvert",
                    "CONVERT_PHASE step3 batch ({profile_key}) remaining={} batch={:?}",
                    remaining_with_batch,
                    batch
                );
                crate::download::progress::set(
                    done_before as u64,
                    Some(pending_total as u64),
                    &format!("Indexed maps: step3 ({profile_key}) {done_before}/{pending_total}…"),
                );

                struct TileWork {
                    row: usize,
                    col: usize,
                    logical: [f64; 4],
                    ways: Vec<Arc<RawWay>>,
                    coords: HashMap<i64, (f64, f64)>,
                }

                let mut works: Vec<TileWork> = Vec::with_capacity(batch.len());
                for i in batch {
                    let (row, col, logical) = tiles[i];
                    let coords_subset = subset_coords(&coords, &tile_node_ids[i]);
                    let mut ways: Vec<Arc<RawWay>> =
                        Vec::with_capacity(tile_way_counts[i] as usize);
                    {
                        let file = std::fs::File::open(tile_spills[i].path())?;
                        let mut reader = BufReader::new(file);
                        while let Some(way) = read_spilled_way(&mut reader)? {
                            ways.push(Arc::new(way.into_raw()));
                        }
                    }
                    let _ = std::fs::remove_file(tile_spills[i].path());
                    // Free per-tile node-id set once its spill is loaded.
                    tile_node_ids[i] = HashSet::new();
                    works.push(TileWork {
                        row,
                        col,
                        logical,
                        ways,
                        coords: coords_subset,
                    });
                }

                let batch_produced = Arc::new(AtomicUsize::new(0));
                // Yield on the convert caller thread before occupying Rayon —
                // never sleep inside `par_iter` workers (starves plan ferry overlay).
                crate::download::pbf_priority::yield_if_background_indexer(yield_to_plan);
                works
                    .par_iter()
                    .try_for_each(|work| -> anyhow::Result<()> {
                        match graph_from_raw_ways(&work.ways, &work.coords, profile, &barrier_tags)
                        {
                            Ok(g) if !g.edges.is_empty() => {
                                batch_produced.fetch_add(1, Ordering::Relaxed);
                                on_tile(profile, work.row, work.col, work.logical, g)
                            }
                            _ => Ok(()),
                        }
                    })?;
                produced += batch_produced.load(Ordering::Relaxed);
                // Explicitly drop tile payloads before shrinking shared coords /
                // starting the next batch (do not leave Arc graphs for GC).
                drop(works);

                // Keep shared coords intact until the last profile finishes
                // tile-assign; during last-profile step3, release nodes for
                // finished tiles after each batch.
                if last_profile {
                    if pending.is_empty() {
                        coords.clear();
                        coords.shrink_to_fit();
                    } else {
                        retain_coords_for_pending_tiles(&mut coords, &tile_node_ids, &pending);
                        coords.shrink_to_fit();
                    }
                }
                let done_after = pending_total.saturating_sub(pending.len());
                crate::download::progress::set(
                    done_after as u64,
                    Some(pending_total as u64),
                    &format!("Indexed maps: step3 ({profile_key}) {done_after}/{pending_total}…"),
                );
            }

            let tile_build_ms = t_build.elapsed().as_secs_f64() * 1000.0;
            log::info!(
                target: "NaviConvert",
                "CONVERT_PHASE step3 done ({profile_key}) produced={produced} ms={tile_build_ms:.0}"
            );
            if produced == 0 {
                anyhow::bail!("tiled graph empty for profile {profile:?}");
            }
            // Drop per-profile spill state before the next profile reallocates.
            drop(tile_spills);
            tile_node_ids.clear();
            tile_node_ids.shrink_to_fit();
            tile_way_counts.clear();
            tile_way_counts.shrink_to_fit();
            out.push((
                profile,
                produced,
                TiledBuildTimings {
                    tile_assign_ms,
                    tile_build_ms,
                },
            ));
            log::info!(
                target: "NaviConvert",
                "CONVERT_PHASE profile released ({profile_key}) next={} shared_coords={}",
                if last_profile {
                    "none".to_string()
                } else {
                    profile_label(profiles[pi + 1]).to_string()
                },
                coords.len()
            );
        }
        // Shared way spill is only needed for per-profile tile-assign; release
        // before returning so TempSpill::drop deletes the file promptly.
        drop(ways_spill);
        drop(barrier_tags);
        coords.clear();
        Ok(out)
    }
}

fn subset_coords(full: &HashMap<i64, (f64, f64)>, ids: &HashSet<i64>) -> HashMap<i64, (f64, f64)> {
    let mut out = HashMap::with_capacity(ids.len());
    for &id in ids {
        if let Some(&coord) = full.get(&id) {
            out.insert(id, coord);
        }
    }
    out
}

fn retain_coords_for_pending_tiles(
    coords: &mut HashMap<i64, (f64, f64)>,
    tile_node_ids: &[HashSet<i64>],
    pending: &[usize],
) {
    if pending.is_empty() {
        coords.clear();
        return;
    }
    let mut keep: HashSet<i64> = HashSet::new();
    for &i in pending {
        keep.extend(&tile_node_ids[i]);
    }
    coords.retain(|id, _| keep.contains(id));
}

fn graph_from_raw_ways(
    ways: &[Arc<RawWay>],
    coords: &HashMap<i64, (f64, f64)>,
    profile: RoutingProfile,
    barrier_tags: &HashMap<i64, HashMap<String, String>>,
) -> anyhow::Result<RouteGraph> {
    let mode = profile.access_mode();
    let mut uses: HashMap<i64, i32> = HashMap::new();
    for way in ways {
        if access::tags_forbid_mode(&way.tags, mode) {
            continue;
        }
        let n = way.nodes.len();
        for (i, id) in way.nodes.iter().enumerate() {
            let add = if i == 0 || i + 1 == n { 2 } else { 1 };
            *uses.entry(*id).or_insert(0) += add;
        }
    }

    // Junction endpoints only (uses > 1). Mid-way OSM nodes stay in edge
    // shape for geometry and edge-based snap; contracting them keeps A* and
    // pack tiles tractable on Automotive-class devices.
    let mut nodes: HashMap<NodeId, Node> = HashMap::new();
    for (&id, &count) in &uses {
        if count <= 1 {
            continue;
        }
        let Some(&(lat, lon)) = coords.get(&id) else {
            continue;
        };
        nodes.insert(
            NodeId(id),
            Node {
                id: NodeId(id),
                coord: Coord { x: lon, y: lat },
                uses: count as i16,
            },
        );
    }

    let mut edges: Vec<GraphEdge> = Vec::new();
    for way in ways {
        if access::tags_forbid_mode(&way.tags, mode) {
            continue;
        }
        // Mandated minimum speed: foot/bicycle cannot legally use the way.
        if matches!(profile, RoutingProfile::Foot | RoutingProfile::Bicycle)
            && way
                .tags
                .get("minspeed")
                .and_then(|v| crate::routing::eta::parse_maxspeed_kmh(v))
                .is_some()
        {
            continue;
        }
        let mut source: Option<i64> = None;
        let mut prev: Option<(i64, f64, f64)> = None;
        let mut length_m = 0.0;
        let mut shape: Vec<(f64, f64)> = Vec::new();
        let mut seg = 0usize;
        let forward_only = oneway_forward_only(&way.tags);
        let highway = way.tags.get("highway").cloned();
        let maxspeed_kmh = way
            .tags
            .get("maxspeed")
            .and_then(|v| crate::routing::eta::parse_maxspeed_kmh(v));
        let maxspeed_practical_kmh = way
            .tags
            .get("maxspeed:practical")
            .and_then(|v| crate::routing::eta::parse_maxspeed_kmh(v));
        let maxspeed_advisory_kmh = way
            .tags
            .get("maxspeed:advisory")
            .and_then(|v| crate::routing::eta::parse_maxspeed_kmh(v));
        let maxspeed_type = way.tags.get("maxspeed:type").cloned();
        let maxspeed_variable = way
            .tags
            .get("maxspeed:variable")
            .is_some_and(|v| super::builder::is_truthy_tag(v));
        let minspeed_kmh = way
            .tags
            .get("minspeed")
            .and_then(|v| crate::routing::eta::parse_maxspeed_kmh(v));
        let name = way.tags.get("name").cloned();
        let road_ref = super::builder::combine_osm_road_refs(
            way.tags.get("ref").cloned(),
            way.tags.get("int_ref").cloned(),
        );
        let is_motorroad = way
            .tags
            .get("motorroad")
            .is_some_and(|v| super::builder::is_truthy_tag(v));
        let is_expressway = way
            .tags
            .get("expressway")
            .is_some_and(|v| super::builder::is_truthy_tag(v));
        let is_oneway = way
            .tags
            .get("oneway")
            .is_some_and(|v| super::builder::is_oneway_yes_tag(v));
        let lanes = way
            .tags
            .get("lanes")
            .and_then(|v| super::builder::parse_lanes_tag(v));
        let maxweight_t = way.tags.get("maxweight").and_then(|v| parse_metric(v));
        let maxaxleload_t = way.tags.get("maxaxleload").and_then(|v| parse_metric(v));
        let maxbogieweight_t = way.tags.get("maxbogieweight").and_then(|v| parse_metric(v));
        let maxheight_m = way.tags.get("maxheight").and_then(|v| parse_metric(v));
        let maxwidth_m = way.tags.get("maxwidth").and_then(|v| parse_metric(v));
        let maxlength_m = way.tags.get("maxlength").and_then(|v| parse_metric(v));
        let is_toll = crate::routing::toll::toll_applies_for_profile(profile, |k| {
            way.tags.get(k).map(String::as_str)
        });
        let is_ferry = tags_indicate_ferry(&way.tags);
        let ferry_duration = if is_ferry {
            way.tags.get("duration").cloned()
        } else {
            None
        };
        let ferry_interval_min = if is_ferry {
            way.tags
                .get("interval")
                .and_then(|v| super::builder::parse_osm_interval_minutes(v))
        } else {
            None
        };
        let is_tunnel = way
            .tags
            .get("tunnel")
            .is_some_and(|v| super::builder::is_tunnel_tag(v));
        let is_boardwalk_crossing = tags_map_indicate_boardwalk(&way.tags);
        let is_roundabout = way.tags.get("junction").is_some_and(|v| v == "roundabout");
        let motor_vehicle_conditional = way.tags.get("motor_vehicle:conditional").cloned();
        let access_conditional = way.tags.get("access:conditional").cloned();
        let maxspeed_conditional = way.tags.get("maxspeed:conditional").cloned();
        let surface_quality = classify_surface_tags(
            highway.as_deref(),
            way.tags.get("surface").map(String::as_str),
            way.tags.get("tracktype").map(String::as_str),
        );

        for id in &way.nodes {
            let Some(&(lat, lon)) = coords.get(id) else {
                continue;
            };
            if let Some((_, plat, plon)) = prev {
                length_m += haversine_m(plat, plon, lat, lon);
            }
            prev = Some((*id, lat, lon));

            let is_end = source.is_some() && uses.get(id).copied().unwrap_or(0) > 1;
            if source.is_none() {
                if uses.get(id).copied().unwrap_or(0) > 1 {
                    source = Some(*id);
                    length_m = 0.0;
                    shape.clear();
                }
                continue;
            }
            if !is_end {
                shape.push((lon, lat));
                continue;
            }
            let src = source.unwrap();
            let tgt = *id;
            if src == tgt || length_m <= 0.0 {
                source = Some(tgt);
                length_m = 0.0;
                shape.clear();
                continue;
            }
            let Some(sn) = nodes.get(&NodeId(src)).copied() else {
                source = Some(tgt);
                length_m = 0.0;
                shape.clear();
                continue;
            };
            let Some(tn) = nodes.get(&NodeId(tgt)).copied() else {
                source = Some(tgt);
                length_m = 0.0;
                shape.clear();
                continue;
            };
            let id_fwd = format!("{}-{}", way.id, seg);
            seg += 1;
            let shape_fwd = shape.clone();
            let mut shape_rev = shape_fwd.clone();
            shape_rev.reverse();
            edges.push(bbox_edge(
                id_fwd.clone(),
                sn.id,
                tn.id,
                sn.coord.y,
                sn.coord.x,
                tn.coord.y,
                tn.coord.x,
                length_m,
                shape_fwd,
                highway.clone(),
                maxspeed_kmh,
                maxspeed_practical_kmh,
                maxspeed_advisory_kmh,
                maxspeed_type.clone(),
                maxspeed_variable,
                minspeed_kmh,
                name.clone(),
                road_ref.clone(),
                is_motorroad,
                is_expressway,
                is_oneway,
                lanes,
                maxweight_t,
                maxaxleload_t,
                maxbogieweight_t,
                maxheight_m,
                maxwidth_m,
                maxlength_m,
                is_toll,
                is_ferry,
                ferry_interval_min,
                ferry_duration.as_deref(),
                is_tunnel,
                is_boardwalk_crossing,
                is_roundabout,
                motor_vehicle_conditional.clone(),
                access_conditional.clone(),
                maxspeed_conditional.clone(),
                false,
                surface_quality,
                profile,
            ));
            if !forward_only {
                edges.push(bbox_edge(
                    format!("{id_fwd}-rev"),
                    tn.id,
                    sn.id,
                    tn.coord.y,
                    tn.coord.x,
                    sn.coord.y,
                    sn.coord.x,
                    length_m,
                    shape_rev,
                    highway.clone(),
                    maxspeed_kmh,
                    maxspeed_practical_kmh,
                    maxspeed_advisory_kmh,
                    maxspeed_type.clone(),
                    maxspeed_variable,
                    minspeed_kmh,
                    name.clone(),
                    road_ref.clone(),
                    is_motorroad,
                    is_expressway,
                    is_oneway,
                    lanes,
                    maxweight_t,
                    maxaxleload_t,
                    maxbogieweight_t,
                    maxheight_m,
                    maxwidth_m,
                    maxlength_m,
                    is_toll,
                    is_ferry,
                    ferry_interval_min,
                    ferry_duration.as_deref(),
                    is_tunnel,
                    is_boardwalk_crossing,
                    is_roundabout,
                    motor_vehicle_conditional.clone(),
                    access_conditional.clone(),
                    maxspeed_conditional.clone(),
                    false,
                    surface_quality,
                    profile,
                ));
            }
            source = Some(tgt);
            length_m = 0.0;
            shape.clear();
        }
    }

    let blocked = access::blocked_barrier_nodes(barrier_tags, mode)
        .into_iter()
        .filter(|id| nodes.contains_key(id))
        .collect();
    Ok(RouteGraph::from_parts_with_blocks(
        nodes, edges, profile, blocked, true,
    ))
}

#[allow(clippy::too_many_arguments)]
fn bbox_edge(
    id: String,
    source: NodeId,
    target: NodeId,
    start_lat: f64,
    start_lon: f64,
    end_lat: f64,
    end_lon: f64,
    length_m: f64,
    shape: Vec<(f64, f64)>,
    highway: Option<String>,
    maxspeed_kmh: Option<f64>,
    maxspeed_practical_kmh: Option<f64>,
    maxspeed_advisory_kmh: Option<f64>,
    maxspeed_type: Option<String>,
    maxspeed_variable: bool,
    minspeed_kmh: Option<f64>,
    name: Option<String>,
    road_ref: Option<String>,
    is_motorroad: bool,
    is_expressway: bool,
    is_oneway: bool,
    lanes: Option<u8>,
    maxweight_t: Option<f64>,
    maxaxleload_t: Option<f64>,
    maxbogieweight_t: Option<f64>,
    maxheight_m: Option<f64>,
    maxwidth_m: Option<f64>,
    maxlength_m: Option<f64>,
    is_toll: bool,
    is_ferry: bool,
    ferry_interval_min: Option<f64>,
    ferry_duration: Option<&str>,
    is_tunnel: bool,
    is_boardwalk_crossing: bool,
    is_roundabout: bool,
    motor_vehicle_conditional: Option<String>,
    access_conditional: Option<String>,
    maxspeed_conditional: Option<String>,
    access_forbidden: bool,
    surface_quality: super::surface_quality::SurfaceQuality,
    profile: RoutingProfile,
) -> GraphEdge {
    let base_weight = if is_ferry {
        ferry_base_weight_m(length_m, ferry_duration, profile)
    } else {
        length_m
    };
    GraphEdge {
        id,
        source,
        target,
        length_m,
        base_weight,
        eco_weight: None,
        start_lat,
        start_lon,
        end_lat,
        end_lon,
        shape,
        highway,
        maxspeed_kmh,
        maxspeed_practical_kmh,
        maxspeed_advisory_kmh,
        maxspeed_type,
        maxspeed_variable,
        minspeed_kmh,
        name,
        road_ref,
        is_motorroad,
        is_expressway,
        is_oneway,
        lanes,
        maxweight_t,
        maxaxleload_t,
        maxbogieweight_t,
        maxheight_m,
        maxwidth_m,
        maxlength_m,
        is_toll,
        is_ferry,
        ferry_interval_min,
        is_tunnel,
        is_boardwalk_crossing,
        is_roundabout,
        motor_vehicle_conditional,
        access_conditional,
        maxspeed_conditional,
        access_forbidden,
        surface_quality,
    }
}

#[cfg(test)]
mod bbox_tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Arc;

    #[test]
    fn keep_way_tag_retains_tunnel_for_avoid_tunnels() {
        assert!(keep_way_tag("tunnel"));
        assert!(keep_way_tag("ferry"));
        assert!(!keep_way_tag("wikipedia"));
        let filtered = filter_way_tags(HashMap::from([
            ("highway".into(), "trunk".into()),
            ("tunnel".into(), "yes".into()),
            ("wikipedia".into(), "no".into()),
        ]));
        assert_eq!(filtered.get("tunnel").map(String::as_str), Some("yes"));
        assert!(!filtered.contains_key("wikipedia"));
    }

    #[test]
    fn ferry_approach_promotes_footway_pier_link_for_car() {
        let foot = HashMap::from([
            ("highway".into(), "footway".into()),
            ("bridge".into(), "movable".into()),
        ]);
        let out = ferry_approach_tags(foot, RoutingProfile::Car).expect("promote");
        assert_eq!(out.get("highway").map(String::as_str), Some("service"));
    }

    #[test]
    fn route_ferry_without_highway_is_kept_for_car() {
        let ferry = HashMap::from([
            ("route".into(), "ferry".into()),
            ("motor_vehicle".into(), "yes".into()),
            ("name".into(), "Puttgarden - Rodby".into()),
        ]);
        assert!(tags_indicate_ferry(&ferry));
        assert!(way_ok_for_profile(&ferry, RoutingProfile::Car));
        assert!(!way_ok_for_profile(
            &HashMap::from([("route".into(), "bus".into())]),
            RoutingProfile::Car
        ));

        // Land — ferry — land sharing terminal nodes (Fehmarn-shaped hop).
        let coords = HashMap::from([
            (1_i64, (54.50, 11.20)),
            (2, (54.503, 11.226)),
            (3, (54.655, 11.352)),
            (4, (54.66, 11.36)),
        ]);
        let ways = vec![
            Arc::new(RawWay {
                id: 10,
                nodes: vec![1, 2],
                tags: HashMap::from([("highway".into(), "primary".into())]),
            }),
            Arc::new(RawWay {
                id: 11,
                nodes: vec![2, 3],
                tags: ferry,
            }),
            Arc::new(RawWay {
                id: 12,
                nodes: vec![3, 4],
                tags: HashMap::from([("highway".into(), "primary".into())]),
            }),
        ];
        let g = graph_from_raw_ways(&ways, &coords, RoutingProfile::Car, &HashMap::new())
            .expect("ferry corridor graph");
        let ferry_n = g.edges.iter().filter(|e| e.is_ferry).count();
        assert!(
            ferry_n > 0,
            "route=ferry without highway must yield ferry edges; edges={}",
            g.edges.len()
        );
        assert!(
            g.nodes.contains_key(&NodeId(2)) && g.nodes.contains_key(&NodeId(3)),
            "ferry terminals must be graph nodes"
        );
        // Bidirectional ferry + two land edges => connectivity across the belt.
        let has_land = g.edges.iter().any(|e| !e.is_ferry);
        assert!(has_land, "land approaches must remain");
    }

    #[test]
    fn bbox_build_gps_atnbrua_from_ostlandet() {
        let pbf = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/integration-fixtures/ostlandet-latest.osm.pbf");
        if !pbf.is_file() {
            eprintln!("skip: missing {pbf:?}");
            return;
        }
        let start_lat: f64 = 60.750_920;
        let start_lon: f64 = 10.960_358;
        let end_lat: f64 = 61.851_250;
        let end_lon: f64 = 10.233_842;
        let pad = 0.35_f64;
        let bbox = [
            start_lat.min(end_lat) - pad,
            start_lon.min(end_lon) - pad,
            start_lat.max(end_lat) + pad,
            start_lon.max(end_lon) + pad,
        ];
        let g =
            RouteGraph::build_from_pbf_bbox(&pbf, RoutingProfile::Car, bbox).expect("bbox build");
        assert!(g.nodes.len() > 1000, "nodes={}", g.nodes.len());
        assert!(g.edges.len() > 1000, "edges={}", g.edges.len());
        eprintln!("bbox graph nodes={} edges={}", g.nodes.len(), g.edges.len());
    }
}
