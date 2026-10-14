//! Host place-index trial helper (Follow-up 16 Option A).
//!
//! Example:
//!   cargo run -p navi-ffi --bin navi-place-index --release -- \
//!     --pbf /path/halland-latest.osm.pbf \
//!     --db /path/halland-trial-place_index.db \
//!     --region europe/sweden/halland

use std::env;
use std::path::PathBuf;
use std::process;
use std::time::Instant;

use driver_break_core::pack_server::build_place_index_from_pbf;

fn usage() -> ! {
    eprintln!(
        "usage: navi-place-index --pbf <file.osm.pbf> --db <place_index.db> --region <geofabrik_path>"
    );
    process::exit(2);
}

fn count_region_rows(db: &std::path::Path, region: &str) -> i64 {
    rusqlite::Connection::open_with_flags(
        db,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()
    .and_then(|conn| {
        conn.query_row(
            "SELECT COUNT(*) FROM name_entries WHERE region_id = ?1",
            [region],
            |r| r.get::<_, i64>(0),
        )
        .ok()
    })
    .unwrap_or(0)
}

fn main() {
    let mut args = env::args().skip(1);
    let mut pbf: Option<PathBuf> = None;
    let mut db: Option<PathBuf> = None;
    let mut region = String::new();
    while let Some(a) = args.next() {
        match a.as_str() {
            "--pbf" => pbf = args.next().map(PathBuf::from),
            "--db" => db = args.next().map(PathBuf::from),
            "--region" => region = args.next().unwrap_or_default(),
            _ => usage(),
        }
    }
    let pbf = pbf.filter(|p| p.is_file()).unwrap_or_else(|| usage());
    let db = db.unwrap_or_else(|| usage());
    let region = region.trim().trim_matches('/').to_string();
    if region.is_empty() {
        usage();
    }
    if let Some(parent) = db.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let pbf_bytes = std::fs::metadata(&pbf).map(|m| m.len()).unwrap_or(0);
    eprintln!(
        "place-index trial region={region} pbf={} pbf_bytes={pbf_bytes} db={}",
        pbf.display(),
        db.display()
    );
    let t0 = Instant::now();
    let (indexed, cache_hit, index_ms) = match build_place_index_from_pbf(&pbf, &db, &region, true)
    {
        Ok(v) => v,
        Err(e) => {
            eprintln!("FAIL: {e}");
            process::exit(1);
        }
    };
    let wall_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let rows = count_region_rows(&db, &region);
    let db_bytes = std::fs::metadata(&db).map(|m| m.len()).unwrap_or(0);
    println!("PASS");
    println!("region_id={region}");
    println!("pbf={}", pbf.display());
    println!("pbf_bytes={pbf_bytes}");
    println!("indexed={indexed}");
    println!("cache_hit={cache_hit}");
    println!("rows={rows}");
    println!("db={}", db.display());
    println!("db_bytes={db_bytes}");
    println!("index_ms={index_ms:.1}");
    println!("wall_ms={wall_ms:.1}");
}
