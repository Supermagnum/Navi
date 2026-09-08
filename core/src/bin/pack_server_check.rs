//! Manual connectivity / region discovery against a navi-server pack host.
//!
//! Usage:
//!   cargo run -p driver-break-core --bin pack-server-check
//!   cargo run -p driver-break-core --bin pack-server-check -- http://192.168.1.195
//!   NAVI_PACK_SERVER_BASE_URL=https://navigate-me.duckdns.org \
//!     cargo run -p driver-break-core --bin pack-server-check
//!
//! With no args, probes the LAN → duckdns chain. Exit 0 always for
//! unreachable/not-ready (soft fail). Exit 2 only on bad args.

use driver_break_core::pack_server::{
    check_connectivity_blocking, check_connectivity_chain_blocking, pack_server_discovery_bases,
    Connectivity, PackDataSource,
};

fn main() {
    let mut args = std::env::args().skip(1);
    let override_base = args.next();

    if args.next().is_some() {
        eprintln!(
            "usage: pack-server-check [base_url]\n\
             env:   NAVI_PACK_SERVER_BASE_URL\n\
             default chain: LAN http://192.168.1.195 then https://navigate-me.duckdns.org"
        );
        std::process::exit(2);
    }

    let (conn, hop) = if let Some(base) = override_base {
        println!("pack server base_url={base} (single-host override)");
        let c = check_connectivity_blocking(&base);
        let hop = if c.is_ready() {
            Some(
                if base.trim_end_matches('/') == "https://navigate-me.duckdns.org" {
                    PackDataSource::ServerDuckdns
                } else {
                    PackDataSource::ServerLan
                },
            )
        } else {
            None
        };
        (c, hop)
    } else {
        let bases = pack_server_discovery_bases();
        println!(
            "pack server discovery chain: {}",
            bases
                .iter()
                .map(|(s, u)| format!("{}={u}", s.as_str()))
                .collect::<Vec<_>>()
                .join(" -> ")
        );
        check_connectivity_chain_blocking(&bases)
    };

    match conn {
        Connectivity::Ready(catalog) => {
            let src = hop.map(|h| h.as_str()).unwrap_or("unknown");
            println!("status=reachable source={src}");
            println!(
                "catalog_generation={}  (catalog last-touched; not for freshness compare)",
                catalog.catalog_generation
            );
            println!("served_from={}", catalog.served_from);
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
            println!("status=unreachable / not ready source=local-bake");
            println!("reason={reason}");
            println!("fallback=Geofabrik (or equivalent)");
        }
    }
}
