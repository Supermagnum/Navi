//! Per-tile search index sidecar (Phase 2 Part C).
//!
//! Built at pack install/refresh alongside the ferry sidecar. Format is independent
//! of `graph_format_version` (no tile rewrite). Invalidated when the sibling
//! `.rkyv` graph tile changes size or mtime.
//!
//! Contents (rkyv, versioned preamble):
//! - `sorted_node_ids` + parallel `local_idx` (binary-search id → archived node ix)
//! - CSR adjacency referencing **archived edge order** (`adj_off` / `adj_edge`)
//! - `border_node_ids`: nodes also present on a neighbouring tile (duplicate-way
//!   endpoints), sorted

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use memmap2::Mmap;
use rkyv::rancor::Error as RkyvError;
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};

use super::graph_pack::{ArchivedFlatGraphPack, GRAPH_FORMAT_VERSION_V8, MAGIC_GRAPH};
use super::graph_pack_v8::ArchivedFlatGraphPackV8;
use super::header::{read_preamble, Preamble, PREAMBLE_LEN};
use super::io::{archive_payload_offset, write_archive_atomic};
use crate::routing::graph::RoutingProfile;

/// Little-endian ASCII "NVTI" (Navi Tile Index).
pub const MAGIC_TILE_INDEX: u32 = 0x4E_56_54_49;
pub const TILE_INDEX_FORMAT_VERSION: u32 = 1;

#[derive(Archive, RkyvSerialize, RkyvDeserialize, Debug, Clone)]
pub struct FlatTileIndex {
    pub sorted_node_ids: Vec<i64>,
    /// Parallel to `sorted_node_ids`: index into the tile's archived `node_ids`.
    pub local_idx: Vec<u32>,
    /// CSR over archived edge indices (`0..edge_src.len()`).
    pub adj_off: Vec<u32>,
    pub adj_edge: Vec<u32>,
    /// Sorted OSM ids that also appear in another tile of the same stem.
    pub border_node_ids: Vec<i64>,
}

fn profile_key(profile: RoutingProfile) -> &'static str {
    match profile {
        RoutingProfile::Car => "car",
        RoutingProfile::Truck => "truck",
        RoutingProfile::Foot => "foot",
        RoutingProfile::Bicycle => "bicycle",
    }
}

#[inline]
fn arch_i64(v: impl Into<i64>) -> i64 {
    v.into()
}

#[inline]
fn arch_u32(v: impl Into<u32>) -> u32 {
    v.into()
}

/// `{tile}.navi-tile-index.rkyv` next to `{stem}.navi-graph-{profile}.tR_C.rkyv`.
pub fn tile_index_path(graph_tile: &Path) -> PathBuf {
    let name = graph_tile
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("tile.rkyv");
    let stem = name.trim_end_matches(".rkyv");
    graph_tile.with_file_name(format!("{stem}.navi-tile-index.rkyv"))
}

fn tile_index_meta_path(graph_tile: &Path) -> PathBuf {
    let p = tile_index_path(graph_tile);
    let name = p
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("x")
        .trim_end_matches(".rkyv");
    p.with_file_name(format!("{name}.meta"))
}

fn tile_fingerprint(graph_tile: &Path) -> Option<String> {
    let meta = fs::metadata(graph_tile).ok()?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Some(format!(
        "bytes={};mtime={mtime};idx_ver={TILE_INDEX_FORMAT_VERSION}",
        meta.len()
    ))
}

pub fn tile_index_fresh(graph_tile: &Path) -> bool {
    let side = tile_index_path(graph_tile);
    let meta_p = tile_index_meta_path(graph_tile);
    if !side.is_file() || !meta_p.is_file() {
        return false;
    }
    let Ok(want) = fs::read_to_string(&meta_p) else {
        return false;
    };
    let Some(fp) = tile_fingerprint(graph_tile) else {
        return false;
    };
    want.trim() == fp
        && super::io::archive_matches_preamble(&side, MAGIC_TILE_INDEX, TILE_INDEX_FORMAT_VERSION)
}

fn mmap_graph_body(path: &Path) -> anyhow::Result<(Mmap, usize, u32)> {
    let file = File::open(path)?;
    let mmap = unsafe { Mmap::map(&file)? };
    if mmap.len() < PREAMBLE_LEN {
        anyhow::bail!("tile too small");
    }
    let mut f = File::open(path)?;
    let pre = read_preamble(&mut f)?;
    if pre.magic != MAGIC_GRAPH {
        anyhow::bail!("not a graph tile");
    }
    Ok((mmap, archive_payload_offset(), pre.format_version))
}

fn build_index_from_node_edge_arrays(
    node_ids: impl Fn(usize) -> i64,
    n_nodes: usize,
    edge_src: impl Fn(usize) -> u32,
    n_edges: usize,
    border: &HashSet<i64>,
) -> FlatTileIndex {
    let mut pairs: Vec<(i64, u32)> = Vec::with_capacity(n_nodes);
    for i in 0..n_nodes {
        pairs.push((node_ids(i), i as u32));
    }
    pairs.sort_unstable_by_key(|(id, _)| *id);
    let mut sorted_node_ids = Vec::with_capacity(n_nodes);
    let mut local_idx = Vec::with_capacity(n_nodes);
    for (id, ix) in pairs {
        sorted_node_ids.push(id);
        local_idx.push(ix);
    }

    let mut degrees = vec![0u32; n_nodes];
    for i in 0..n_edges {
        let s = edge_src(i) as usize;
        if s < n_nodes {
            degrees[s] += 1;
        }
    }
    let mut adj_off = vec![0u32; n_nodes + 1];
    let mut sum = 0u32;
    for (i, d) in degrees.iter().enumerate() {
        adj_off[i] = sum;
        sum = sum.saturating_add(*d);
    }
    adj_off[n_nodes] = sum;
    let mut adj_edge = vec![0u32; sum as usize];
    let mut cursor = adj_off.clone();
    cursor.truncate(n_nodes);
    for i in 0..n_edges {
        let s = edge_src(i) as usize;
        if s >= n_nodes {
            continue;
        }
        let slot = cursor[s] as usize;
        adj_edge[slot] = i as u32;
        cursor[s] += 1;
    }

    let mut border_node_ids: Vec<i64> = sorted_node_ids
        .iter()
        .copied()
        .filter(|id| border.contains(id))
        .collect();
    border_node_ids.sort_unstable();
    border_node_ids.dedup();

    FlatTileIndex {
        sorted_node_ids,
        local_idx,
        adj_off,
        adj_edge,
        border_node_ids,
    }
}

fn build_index_from_archived_v9(
    arch: &ArchivedFlatGraphPack,
    border: &HashSet<i64>,
) -> FlatTileIndex {
    build_index_from_node_edge_arrays(
        |i| arch_i64(arch.node_ids[i]),
        arch.node_ids.len(),
        |i| arch_u32(arch.edge_src[i]),
        arch.edge_src.len(),
        border,
    )
}

fn build_index_from_archived_v8(
    arch: &ArchivedFlatGraphPackV8,
    border: &HashSet<i64>,
) -> FlatTileIndex {
    build_index_from_node_edge_arrays(
        |i| arch_i64(arch.node_ids[i]),
        arch.node_ids.len(),
        |i| arch_u32(arch.edge_src[i]),
        arch.edge_src.len(),
        border,
    )
}

/// Build one tile-index sidecar. `border` = OSM ids known to appear in ≥2 tiles.
pub fn ensure_tile_index(graph_tile: &Path, border: &HashSet<i64>) -> anyhow::Result<(u64, u64)> {
    if tile_index_fresh(graph_tile) {
        let bytes = fs::metadata(tile_index_path(graph_tile))
            .map(|m| m.len())
            .unwrap_or(0);
        return Ok((0, bytes));
    }
    let t0 = Instant::now();
    let (mmap, off, ver) = mmap_graph_body(graph_tile)?;
    let body = &mmap[off..];
    let index = if ver == GRAPH_FORMAT_VERSION_V8 {
        let archived = rkyv::access::<ArchivedFlatGraphPackV8, RkyvError>(body)
            .map_err(|e| anyhow::anyhow!("rkyv access v8: {e}"))?;
        build_index_from_archived_v8(archived, border)
    } else {
        let archived = rkyv::access::<ArchivedFlatGraphPack, RkyvError>(body)
            .map_err(|e| anyhow::anyhow!("rkyv access: {e}"))?;
        build_index_from_archived_v9(archived, border)
    };
    let payload =
        rkyv::to_bytes::<RkyvError>(&index).map_err(|e| anyhow::anyhow!("rkyv encode: {e}"))?;
    let side = tile_index_path(graph_tile);
    write_archive_atomic(
        &side,
        Preamble {
            magic: MAGIC_TILE_INDEX,
            format_version: TILE_INDEX_FORMAT_VERSION,
        },
        payload.as_ref(),
    )?;
    if let Some(fp) = tile_fingerprint(graph_tile) {
        let mut f = File::create(tile_index_meta_path(graph_tile))?;
        f.write_all(fp.as_bytes())?;
        f.flush()?;
    }
    let ms = t0.elapsed().as_millis() as u64;
    let bytes = fs::metadata(&side).map(|m| m.len()).unwrap_or(0);
    crate::routing::plan_perf::note(
        "tile_index_build",
        format!(
            "path={};ms={ms};bytes={bytes};nodes={};edges={};border={}",
            graph_tile.display(),
            index.sorted_node_ids.len(),
            index.adj_edge.len(),
            index.border_node_ids.len()
        ),
    );
    Ok((ms, bytes))
}

/// Scan stem tiles, compute border-node set (ids in ≥2 tiles), build all sidecars.
pub fn ensure_tile_indexes_for_stem(
    pack_dir: &Path,
    stem: &str,
    profile: RoutingProfile,
) -> anyhow::Result<String> {
    let key = profile_key(profile);
    let prefix = format!("{stem}.navi-graph-{key}.t");
    let mut tiles: Vec<PathBuf> = Vec::new();
    let rd = fs::read_dir(pack_dir)?;
    for ent in rd.flatten() {
        let p = ent.path();
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if name.starts_with(&prefix) && name.ends_with(".rkyv") && !name.contains("tile-index") {
            tiles.push(p);
        }
    }
    tiles.sort();
    if tiles.is_empty() {
        return Ok(format!("PASS stem={stem} tiles=0"));
    }

    let mut id_tile_count: HashMap<i64, u8> = HashMap::with_capacity(1 << 20);
    for tile in &tiles {
        let (mmap, off, ver) = mmap_graph_body(tile)?;
        let body = &mmap[off..];
        let mut seen = HashSet::new();
        if ver == GRAPH_FORMAT_VERSION_V8 {
            let archived = rkyv::access::<ArchivedFlatGraphPackV8, RkyvError>(body)
                .map_err(|e| anyhow::anyhow!("{}: {e}", tile.display()))?;
            for i in 0..archived.node_ids.len() {
                let id = arch_i64(archived.node_ids[i]);
                if seen.insert(id) {
                    let e = id_tile_count.entry(id).or_insert(0);
                    *e = (*e).saturating_add(1);
                }
            }
        } else {
            let archived = rkyv::access::<ArchivedFlatGraphPack, RkyvError>(body)
                .map_err(|e| anyhow::anyhow!("{}: {e}", tile.display()))?;
            for i in 0..archived.node_ids.len() {
                let id = arch_i64(archived.node_ids[i]);
                if seen.insert(id) {
                    let e = id_tile_count.entry(id).or_insert(0);
                    *e = (*e).saturating_add(1);
                }
            }
        }
    }
    let border: HashSet<i64> = id_tile_count
        .into_iter()
        .filter(|(_, c)| *c >= 2)
        .map(|(id, _)| id)
        .collect();

    let mut rows = Vec::new();
    let mut total_ms = 0u64;
    let mut total_bytes = 0u64;
    for tile in &tiles {
        let (ms, bytes) = ensure_tile_index(tile, &border)?;
        total_ms = total_ms.saturating_add(ms);
        total_bytes = total_bytes.saturating_add(bytes);
        rows.push(format!(
            "{}\t{ms}\t{bytes}",
            tile.file_name().and_then(|s| s.to_str()).unwrap_or("?")
        ));
    }
    Ok(format!(
        "PASS stem={stem} tiles={} border_ids={} total_ms={total_ms} total_bytes={total_bytes}\n{}",
        tiles.len(),
        border.len(),
        rows.join("\n")
    ))
}

/// Owned tile index loaded from a sidecar (mmap read + deserialize).
pub struct MappedTileIndex {
    pub index: FlatTileIndex,
}

impl MappedTileIndex {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let file = File::open(path)?;
        let mmap = unsafe { Mmap::map(&file)? };
        if mmap.len() < PREAMBLE_LEN {
            anyhow::bail!("index too small");
        }
        let mut f = File::open(path)?;
        let pre = read_preamble(&mut f)?;
        if pre.magic != MAGIC_TILE_INDEX || pre.format_version != TILE_INDEX_FORMAT_VERSION {
            anyhow::bail!("bad tile index preamble");
        }
        let body = &mmap[archive_payload_offset()..];
        let archived = rkyv::access::<ArchivedFlatTileIndex, RkyvError>(body)
            .map_err(|e| anyhow::anyhow!("rkyv: {e}"))?;
        let index: FlatTileIndex = rkyv::deserialize::<FlatTileIndex, RkyvError>(archived)
            .map_err(|e| anyhow::anyhow!("rkyv deserialize: {e}"))?;
        Ok(Self { index })
    }

    /// Direct id → local archived node index (binary search). O(log N).
    pub fn lookup(&self, osm_id: i64) -> Option<u32> {
        self.index
            .sorted_node_ids
            .binary_search(&osm_id)
            .ok()
            .map(|i| self.index.local_idx[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_path_naming() {
        let p = PathBuf::from("/x/ostlandet-latest.navi-graph-car.t2_3.rkyv");
        assert!(tile_index_path(&p)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .ends_with(".navi-tile-index.rkyv"));
    }
}
