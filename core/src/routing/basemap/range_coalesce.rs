//! HTTP range coalescing for remote PMTiles extracts (go-pmtiles-style overfetch).
//!
//! The `pmtiles` crate's `DirEntry` fields are `pub(crate)`, so we parse directories
//! ourselves and merge nearby tile byte-ranges into fewer, larger Range GETs.

use anyhow::{anyhow, bail, Context};
use bytes::Bytes;
use flate2::read::GzDecoder;
use futures_util::future::join_all;
use pmtiles::{AsyncBackend, TileCoord, TileId};
use std::collections::{HashMap, HashSet};
use std::io::Read;

use crate::download::progress as download_progress;
use crate::download::DownloadControl;
use crate::routing::basemap::http_backend::Reqwest012Backend;
use crate::storage::{PmtilesJobStatus, PmtilesJobStore, Storage};
use uuid::Uuid;

const HEADER_SIZE: usize = 127;
/// Match go-pmtiles default: allow ~5% extra transfer to collapse nearby ranges.
pub const DEFAULT_OVERFETCH: f32 = 0.05;

#[derive(Clone, Copy, Debug)]
struct HeaderOffsets {
    root_offset: u64,
    root_length: u64,
    leaf_offset: u64,
    data_offset: u64,
    clustered: bool,
    internal_gzip: bool,
    tile_compression: u8,
    tile_type: u8,
    max_zoom: u8,
}

/// Archive tile encoding + tiles fetched via coalesced range GETs.
pub struct CoalescedTiles {
    pub tile_type: pmtiles::TileType,
    pub tile_compression: pmtiles::Compression,
    pub header_max_zoom: u8,
    pub tiles: Vec<(TileCoord, Bytes)>,
}

#[derive(Clone, Debug)]
struct Entry {
    tile_id: u64,
    offset: u64,
    length: u32,
    run_length: u32,
}

impl Entry {
    fn is_leaf(&self) -> bool {
        self.run_length == 0
    }
}

#[derive(Clone, Debug)]
struct SrcRange {
    src_offset: u64,
    length: u64,
}

#[derive(Clone, Debug)]
struct CopyDiscard {
    wanted: u64,
    discard: u64,
}

#[derive(Clone, Debug)]
struct OverfetchRange {
    src_offset: u64,
    length: u64,
    copy_discards: Vec<CopyDiscard>,
}

/// Resolve directory + tile byte ranges for `coords`, coalesce with overfetch,
/// download in parallel, return Hilbert-ordered `(TileCoord, Bytes)` pairs.
///
/// HTTP request counts are recorded by [`Reqwest012Backend`]'s optional counter.
pub async fn fetch_tiles_coalesced(
    backend: &Reqwest012Backend,
    coords: &[TileCoord],
    workers: usize,
    overfetch: f32,
    control: &DownloadControl,
    store: Option<(&Storage, Uuid)>,
    planned_tiles: u64,
) -> anyhow::Result<CoalescedTiles> {
    let wanted: HashSet<u64> = coords.iter().map(|c| TileId::from(*c).value()).collect();
    if wanted.is_empty() {
        return Ok(CoalescedTiles {
            tile_type: pmtiles::TileType::Mvt,
            tile_compression: pmtiles::Compression::Gzip,
            header_max_zoom: 0,
            tiles: Vec::new(),
        });
    }
    let max_tile_id = *wanted.iter().max().unwrap();

    let header_bytes = backend
        .read(0, HEADER_SIZE)
        .await
        .map_err(|e| anyhow!("read header: {e}"))?
        .bytes;
    let header = parse_header(&header_bytes)?;
    if !header.clustered {
        bail!("source archive must be clustered for coalesced extract");
    }
    let tile_type: pmtiles::TileType = header
        .tile_type
        .try_into()
        .map_err(|e| anyhow!("tile type: {e}"))?;
    let tile_compression: pmtiles::Compression = header
        .tile_compression
        .try_into()
        .map_err(|e| anyhow!("tile compression: {e}"))?;

    let root_raw = backend
        .read(header.root_offset as usize, header.root_length as usize)
        .await
        .map_err(|e| anyhow!("read root directory: {e}"))?
        .bytes;
    let root_dir = decode_directory(&root_raw, header.internal_gzip)?;

    let (mut tile_entries, leaves) = relevant_entries(&wanted, max_tile_id, &root_dir);

    let leaf_ranges: Vec<SrcRange> = leaves
        .iter()
        .map(|e| SrcRange {
            src_offset: header.leaf_offset + e.offset,
            length: u64::from(e.length),
        })
        .collect();
    let leaf_chunks = merge_ranges(&leaf_ranges, overfetch);
    for chunk in leaf_chunks {
        let blob = backend
            .read(chunk.src_offset as usize, chunk.length as usize)
            .await
            .map_err(|e| anyhow!("read leaf dirs: {e}"))?
            .bytes;
        let mut cursor = 0usize;
        for cd in &chunk.copy_discards {
            let end = cursor + cd.wanted as usize;
            let slice = blob
                .get(cursor..end)
                .ok_or_else(|| anyhow!("leaf chunk short"))?;
            let dir = decode_directory(slice, header.internal_gzip)?;
            let (more, deeper) = relevant_entries(&wanted, max_tile_id, &dir);
            if !deeper.is_empty() {
                bail!("leaf depth > 1 not supported");
            }
            tile_entries.extend(more);
            cursor = end + cd.discard as usize;
        }
    }

    tile_entries.sort_by_key(|e| e.tile_id);

    // Expand to per-tile blob refs; build unique content ranges sorted by offset.
    let mut tile_blobs: Vec<(u64, u64, u32)> = Vec::new();
    let mut unique: HashMap<u64, u32> = HashMap::new();
    for e in &tile_entries {
        let run = e.run_length.max(1);
        for id in e.tile_id..e.tile_id + u64::from(run) {
            if !wanted.contains(&id) {
                continue;
            }
            tile_blobs.push((id, e.offset, e.length));
            unique.entry(e.offset).or_insert(e.length);
        }
    }

    let mut content_ranges: Vec<SrcRange> = unique
        .iter()
        .map(|(&off, &len)| SrcRange {
            src_offset: off,
            length: u64::from(len),
        })
        .collect();
    content_ranges.sort_by_key(|r| r.src_offset);

    // Collapse already-adjacent contents (zero gap) before overfetch merging.
    let mut adjacent: Vec<SrcRange> = Vec::new();
    for r in content_ranges {
        if let Some(last) = adjacent.last_mut() {
            if last.src_offset + last.length == r.src_offset {
                last.length += r.length;
                continue;
            }
        }
        adjacent.push(r);
    }

    let mut chunk_queue = merge_ranges(&adjacent, overfetch);
    let workers = workers.max(1);
    log::info!(
        target: "NaviDownload",
        "[NaviDownload] pmtiles coalesce plan tiles={} entries={} content_ranges={} http_chunks={} workers={} overfetch={overfetch}",
        wanted.len(),
        tile_entries.len(),
        adjacent.len(),
        chunk_queue.len(),
        workers
    );

    let data_offset = header.data_offset;
    let mut content_by_offset: HashMap<u64, Bytes> = HashMap::new();

    let total_chunks = chunk_queue.len().max(1);
    let mut chunks_done = 0usize;
    while !chunk_queue.is_empty() {
        // Honour pause/cancel between HTTP batches (coalesce phase is the long wait).
        if control.is_cancelled() {
            bail!("cancelled");
        }
        while control.is_paused() {
            if let Some((storage, job_id)) = store {
                let _ = PmtilesJobStore::new(storage).set_status(
                    job_id,
                    PmtilesJobStatus::Paused,
                    true,
                );
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            if control.is_cancelled() {
                bail!("cancelled");
            }
        }
        if let Some((storage, job_id)) = store {
            let _ =
                PmtilesJobStore::new(storage).set_status(job_id, PmtilesJobStatus::Running, false);
        }

        let n = workers.min(chunk_queue.len());
        let batch: Vec<OverfetchRange> = chunk_queue.drain(..n).collect();
        let futs = batch.into_iter().map(|chunk| {
            let backend = backend.clone();
            async move {
                let abs = data_offset + chunk.src_offset;
                let resp = backend
                    .read(abs as usize, chunk.length as usize)
                    .await
                    .map_err(|e| anyhow!("tile chunk read: {e}"))?;
                Ok::<_, anyhow::Error>((chunk, resp.bytes))
            }
        });
        for res in join_all(futs).await {
            let (chunk, blob) = res?;
            chunks_done += 1;
            let approx = ((chunks_done as u64).saturating_mul(planned_tiles)) / total_chunks as u64;
            download_progress::set(
                approx.min(planned_tiles),
                Some(planned_tiles),
                "Downloading map tiles for region…",
            );
            if let Some((storage, job_id)) = store {
                let _ = PmtilesJobStore::new(storage).set_progress(
                    job_id,
                    approx.min(planned_tiles),
                    Some(planned_tiles),
                );
            }
            if chunks_done == total_chunks || chunks_done % workers.max(1) == 0 {
                log::info!(
                    target: "NaviDownload",
                    "[NaviDownload] pmtiles coalesce download chunks={chunks_done}/{total_chunks}"
                );
            }
            let mut cursor = 0usize;
            let mut pos = chunk.src_offset;
            for cd in &chunk.copy_discards {
                let end = cursor + cd.wanted as usize;
                let slice = blob
                    .get(cursor..end)
                    .ok_or_else(|| anyhow!("tile chunk short wanted"))?;
                let span_start = pos;
                let span_end = pos + cd.wanted;
                for &(_tid, off, len) in &tile_blobs {
                    if off >= span_start && off + u64::from(len) <= span_end {
                        let local = (off - span_start) as usize;
                        let piece = slice
                            .get(local..local + len as usize)
                            .ok_or_else(|| anyhow!("tile slice OOB"))?;
                        content_by_offset
                            .entry(off)
                            .or_insert_with(|| Bytes::copy_from_slice(piece));
                    }
                }
                cursor = end + cd.discard as usize;
                pos = span_end + cd.discard;
            }
        }
    }

    tile_blobs.sort_by_key(|(id, _, _)| *id);
    let mut out = Vec::with_capacity(tile_blobs.len());
    let mut seen_ids = HashSet::new();
    for (id, off, _len) in tile_blobs {
        if !seen_ids.insert(id) {
            continue;
        }
        let Some(bytes) = content_by_offset.get(&off).cloned() else {
            continue;
        };
        let tid = TileId::new(id).map_err(|e| anyhow!("tile id: {e}"))?;
        out.push((TileCoord::from(tid), bytes));
    }
    Ok(CoalescedTiles {
        tile_type,
        tile_compression,
        header_max_zoom: header.max_zoom,
        tiles: out,
    })
}

fn parse_header(bytes: &[u8]) -> anyhow::Result<HeaderOffsets> {
    if bytes.len() < HEADER_SIZE {
        bail!("header too short");
    }
    if &bytes[0..7] != b"PMTiles" {
        bail!("invalid PMTiles magic");
    }
    let version = bytes[7];
    if version != 3 {
        bail!("unsupported PMTiles version {version}");
    }
    let u64_at = |o: usize| u64::from_le_bytes(bytes[o..o + 8].try_into().unwrap());
    Ok(HeaderOffsets {
        root_offset: u64_at(8),
        root_length: u64_at(16),
        leaf_offset: u64_at(40),
        data_offset: u64_at(56),
        clustered: bytes[96] == 1,
        // Compression::Gzip == 2 in the PMTiles header.
        internal_gzip: bytes[97] == 2,
        tile_compression: bytes[98],
        tile_type: bytes[99],
        max_zoom: bytes[101],
    })
}

fn decode_directory(raw: &[u8], gzip: bool) -> anyhow::Result<Vec<Entry>> {
    let decompressed = if gzip {
        let mut d = GzDecoder::new(raw);
        let mut out = Vec::new();
        d.read_to_end(&mut out).context("gzip directory")?;
        out
    } else {
        raw.to_vec()
    };
    parse_directory(&decompressed)
}

fn parse_directory(buffer: &[u8]) -> anyhow::Result<Vec<Entry>> {
    let mut i = 0usize;
    let n = read_varint(buffer, &mut i)? as usize;
    let mut entries = vec![
        Entry {
            tile_id: 0,
            offset: 0,
            length: 0,
            run_length: 0,
        };
        n
    ];

    let mut next_tile_id = 0u64;
    for e in &mut entries {
        next_tile_id += read_varint(buffer, &mut i)?;
        e.tile_id = next_tile_id;
    }
    for e in &mut entries {
        e.run_length = read_varint(buffer, &mut i)? as u32;
    }
    for e in &mut entries {
        e.length = read_varint(buffer, &mut i)? as u32;
    }

    let mut last_offset = 0u64;
    let mut last_length = 0u32;
    for (idx, e) in entries.iter_mut().enumerate() {
        let offset = read_varint(buffer, &mut i)?;
        e.offset = if offset == 0 {
            if idx == 0 {
                bail!("invalid directory entry");
            }
            last_offset + u64::from(last_length)
        } else {
            offset - 1
        };
        last_offset = e.offset;
        last_length = e.length;
    }
    Ok(entries)
}

fn read_varint(buf: &[u8], i: &mut usize) -> anyhow::Result<u64> {
    let mut result = 0u64;
    let mut shift = 0u32;
    loop {
        if *i >= buf.len() {
            bail!("varint EOF");
        }
        let b = buf[*i];
        *i += 1;
        result |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return Ok(result);
        }
        shift += 7;
        if shift > 63 {
            bail!("varint overflow");
        }
    }
}

fn relevant_entries(
    wanted: &HashSet<u64>,
    max_tile_id: u64,
    dir: &[Entry],
) -> (Vec<Entry>, Vec<Entry>) {
    let last_tile = max_tile_id + 1;
    let mut leaves = Vec::new();
    let mut tiles = Vec::new();
    for (idx, entry) in dir.iter().enumerate() {
        if entry.is_leaf() {
            let end = if idx + 1 < dir.len() {
                dir[idx + 1].tile_id
            } else {
                last_tile
            };
            // O(|wanted|), never walk a multi-million Hilbert span.
            if wanted.iter().any(|&id| id >= entry.tile_id && id < end) {
                leaves.push(entry.clone());
            }
        } else if entry.run_length == 1 {
            if wanted.contains(&entry.tile_id) {
                tiles.push(entry.clone());
            }
        } else {
            let mut current_id = entry.tile_id;
            let mut current_run = 0u32;
            for y in entry.tile_id..entry.tile_id + u64::from(entry.run_length) {
                if wanted.contains(&y) {
                    if current_run == 0 {
                        current_run = 1;
                        current_id = y;
                    } else {
                        current_run += 1;
                    }
                } else if current_run > 0 {
                    tiles.push(Entry {
                        tile_id: current_id,
                        offset: entry.offset,
                        length: entry.length,
                        run_length: current_run,
                    });
                    current_run = 0;
                }
            }
            if current_run > 0 {
                tiles.push(Entry {
                    tile_id: current_id,
                    offset: entry.offset,
                    length: entry.length,
                    run_length: current_run,
                });
            }
        }
    }
    (tiles, leaves)
}

/// Merge nearby ranges until the overfetch budget is exhausted (go-pmtiles algorithm).
fn merge_ranges(ranges: &[SrcRange], overfetch: f32) -> Vec<OverfetchRange> {
    if ranges.is_empty() {
        return Vec::new();
    }

    #[derive(Clone)]
    struct Item {
        rng: SrcRange,
        copy_discards: Vec<CopyDiscard>,
        bytes_to_next: u64,
    }

    let mut items: Vec<Item> = ranges
        .iter()
        .enumerate()
        .map(|(i, rng)| {
            let bytes_to_next = if i + 1 >= ranges.len() {
                u64::MAX
            } else {
                ranges[i + 1]
                    .src_offset
                    .saturating_sub(rng.src_offset + rng.length)
            };
            Item {
                rng: rng.clone(),
                copy_discards: vec![CopyDiscard {
                    wanted: rng.length,
                    discard: 0,
                }],
                bytes_to_next,
            }
        })
        .collect();

    let total_size: u64 = ranges.iter().map(|r| r.length).sum();
    let mut budget = (total_size as f64 * f64::from(overfetch)) as i64;

    while items.len() > 1 {
        let mut best = 0usize;
        for i in 0..items.len() - 1 {
            if items[i].bytes_to_next < items[best].bytes_to_next {
                best = i;
            }
        }
        let gap = items[best].bytes_to_next;
        if gap == u64::MAX || budget < gap as i64 {
            break;
        }

        let next = items[best + 1].clone();
        let cur = &mut items[best];
        cur.rng.length = cur.rng.length + gap + next.rng.length;
        if let Some(last) = cur.copy_discards.last_mut() {
            last.discard = gap;
        }
        cur.copy_discards.extend(next.copy_discards);
        items.remove(best + 1);

        for i in best.saturating_sub(1)..=best.min(items.len().saturating_sub(1)) {
            if i + 1 >= items.len() {
                items[i].bytes_to_next = u64::MAX;
            } else {
                let a = items[i].rng.src_offset + items[i].rng.length;
                let b = items[i + 1].rng.src_offset;
                items[i].bytes_to_next = b.saturating_sub(a);
            }
        }
        budget -= gap as i64;
    }

    items.sort_by(|a, b| b.rng.length.cmp(&a.rng.length));
    items
        .into_iter()
        .map(|it| OverfetchRange {
            src_offset: it.rng.src_offset,
            length: it.rng.length,
            copy_discards: it.copy_discards,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_adjacent_with_small_gap() {
        let ranges = vec![
            SrcRange {
                src_offset: 0,
                length: 100,
            },
            SrcRange {
                src_offset: 105,
                length: 100,
            },
        ];
        let merged = merge_ranges(&ranges, 0.05);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].length, 205);
        assert_eq!(merged[0].copy_discards.len(), 2);
        assert_eq!(merged[0].copy_discards[0].discard, 5);
    }

    #[test]
    fn no_merge_when_overfetch_zero() {
        let ranges = vec![
            SrcRange {
                src_offset: 0,
                length: 100,
            },
            SrcRange {
                src_offset: 200,
                length: 100,
            },
        ];
        let merged = merge_ranges(&ranges, 0.0);
        assert_eq!(merged.len(), 2);
    }
}
