//! Manual connectivity / region discovery against a navi-server pack host.
//!
//! Usage:
//!   cargo run -p driver-break-core --bin pack-server-check
//!   cargo run -p driver-break-core --bin pack-server-check -- http://192.168.1.195
//!   NAVI_PACK_SERVER_BASE_URL=https://navigate-me.duckdns.org \
//!     cargo run -p driver-break-core --bin pack-server-check
//!
//! Exit 0 always for unreachable/not-ready (soft fail). Exit 2 only on bad args.

use driver_break_core::pack_server::{
    check_connectivity_blocking, Connectivity, DEFAULT_PACK_SERVER_BASE_URL,
};

fn main() {
    let mut args = std::env::args().skip(1);
    let base = args
        .next()
        .or_else(|| std::env::var("NAVI_PACK_SERVER_BASE_URL").ok())
        .unwrap_or_else(|| DEFAULT_PACK_SERVER_BASE_URL.to_string());

    if args.next().is_some() {
        eprintln!(
            "usage: pack-server-check [base_url]\n\
             env:   NAVI_PACK_SERVER_BASE_URL (default: {DEFAULT_PACK_SERVER_BASE_URL})"
        );
        std::process::exit(2);
    }

    println!("pack server base_url={base}");
    match check_connectivity_blocking(&base) {
        Connectivity::Ready(catalog) => {
            println!("status=reachable");
            // Catalog marker only — not for freshness / cache comparison.
            println!(
                "catalog_generation={}  (catalog last-touched; not for freshness compare)",
                catalog.catalog_generation
            );
            println!("regions={}", catalog.regions.len());
            for region in &catalog.regions {
                let gen = region
                    .generation
                    .as_deref()
                    .unwrap_or("(missing region generation)");
                match region.bytes {
                    Some(bytes) => {
                        println!("  {}  generation={gen}  bytes={bytes}", region.region_id)
                    }
                    None => println!("  {}  generation={gen}", region.region_id),
                }
            }
        }
        Connectivity::Unreachable { reason } => {
            println!("status=unreachable / not ready");
            println!("reason={reason}");
            println!("fallback=Geofabrik (or equivalent)");
        }
    }
}
