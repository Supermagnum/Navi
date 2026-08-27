//! Minimal PBF decode wall-time + peak-RSS spike (not a full convert).
use std::env;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

fn peak_rss_mb() -> f64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(s) = std::fs::read_to_string("/proc/self/status") {
            for line in s.lines() {
                if let Some(rest) = line.strip_prefix("VmHWM:") {
                    let kb: f64 = rest
                        .split_whitespace()
                        .next()
                        .and_then(|x| x.parse().ok())
                        .unwrap_or(0.0);
                    return kb / 1024.0;
                }
            }
        }
    }
    0.0
}

fn main() {
    let path = env::args().nth(1).expect("usage: pbf-decode-bench <file.osm.pbf>");
    let path = Path::new(&path);
    let backend = env::var("PBF_BENCH_BACKEND").unwrap_or_else(|_| {
        if cfg!(feature = "fast-osmpbf") {
            "fast-osmpbf".into()
        } else if cfg!(feature = "osmpbf-zlib-ng") {
            "osmpbf-zlib-ng".into()
        } else {
            "osmpbf-rust-zlib".into()
        }
    });
    eprintln!("backend={backend} path={}", path.display());
    let t0 = Instant::now();
    let (nodes, ways, rels) = run(path);
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    let rss = peak_rss_mb();
    println!(
        "PASS backend={backend} wall_ms={ms:.1} peak_rss_mb={rss:.1} nodes={nodes} ways={ways} relations={rels}"
    );
}

#[cfg(feature = "osmpbf-rust-zlib")]
fn run(path: &Path) -> (u64, u64, u64) {
    use osmpbf_default::{BlobDecode, BlobReader};
    use rayon::iter::{ParallelBridge, ParallelIterator};
    let nodes = AtomicU64::new(0);
    let ways = AtomicU64::new(0);
    let rels = AtomicU64::new(0);
    BlobReader::from_path(path)
        .expect("open")
        .par_bridge()
        .for_each(|blob| {
            match blob.expect("blob").decode().expect("decode") {
                BlobDecode::OsmData(block) => {
                    let mut n = 0u64;
                    let mut w = 0u64;
                    let mut r = 0u64;
                    block.for_each_element(|el| match el {
                        osmpbf_default::Element::Node(_) | osmpbf_default::Element::DenseNode(_) => {
                            n += 1
                        }
                        osmpbf_default::Element::Way(_) => w += 1,
                        osmpbf_default::Element::Relation(_) => r += 1,
                    });
                    nodes.fetch_add(n, Ordering::Relaxed);
                    ways.fetch_add(w, Ordering::Relaxed);
                    rels.fetch_add(r, Ordering::Relaxed);
                }
                _ => {}
            }
        });
    (
        nodes.load(Ordering::Relaxed),
        ways.load(Ordering::Relaxed),
        rels.load(Ordering::Relaxed),
    )
}

#[cfg(feature = "osmpbf-zlib-ng")]
fn run(path: &Path) -> (u64, u64, u64) {
    use osmpbf_zlib_ng::{BlobDecode, BlobReader};
    use rayon::iter::{ParallelBridge, ParallelIterator};
    let nodes = AtomicU64::new(0);
    let ways = AtomicU64::new(0);
    let rels = AtomicU64::new(0);
    BlobReader::from_path(path)
        .expect("open")
        .par_bridge()
        .for_each(|blob| {
            match blob.expect("blob").decode().expect("decode") {
                BlobDecode::OsmData(block) => {
                    let mut n = 0u64;
                    let mut w = 0u64;
                    let mut r = 0u64;
                    block.for_each_element(|el| match el {
                        osmpbf_zlib_ng::Element::Node(_) | osmpbf_zlib_ng::Element::DenseNode(_) => {
                            n += 1
                        }
                        osmpbf_zlib_ng::Element::Way(_) => w += 1,
                        osmpbf_zlib_ng::Element::Relation(_) => r += 1,
                    });
                    nodes.fetch_add(n, Ordering::Relaxed);
                    ways.fetch_add(w, Ordering::Relaxed);
                    rels.fetch_add(r, Ordering::Relaxed);
                }
                _ => {}
            }
        });
    (
        nodes.load(Ordering::Relaxed),
        ways.load(Ordering::Relaxed),
        rels.load(Ordering::Relaxed),
    )
}

#[cfg(feature = "fast-osmpbf")]
fn run(path: &Path) -> (u64, u64, u64) {
    use fast_osmpbf::prelude::*;
    use fast_osmpbf::{ElementBlock, OsmReader};
    let reader = OsmReader::from_path(path).expect("open");
    let (n, w, r) = reader
        .par_blocks()
        .map(|block| match block {
            ElementBlock::DenseNodeBlock(b) => (b.iter().count() as u64, 0u64, 0u64),
            ElementBlock::NodeBlock(b) => (b.iter().count() as u64, 0, 0),
            ElementBlock::WayBlock(b) => (0, b.iter().count() as u64, 0),
            ElementBlock::RelationBlock(b) => (0, 0, b.iter().count() as u64),
        })
        .reduce(|| (0, 0, 0), |a, b| (a.0 + b.0, a.1 + b.1, a.2 + b.2));
    (n, w, r)
}
