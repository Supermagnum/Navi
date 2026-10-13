//! Host diagnostic: exercise the navi-server repeater Update path, then count
//! onboard DB hits along sample corridors (same samples as the CAT regression).
//!
//! Behind the `diag-bins` feature so default / release / CI builds skip it.
//!
//! ```text
//! cargo run -p navi-cat --features diag-bins --bin repeater_corridor_check -- \
//!   [--base-url https://navigate-me.duckdns.org] [--db /tmp/cat_repeaters.sqlite]
//! ```
//!
//! Requires the repo-pinned toolchain (`rust-toolchain.toml` → 1.98.0). Do not
//! build this outside the workspace with an older host rustc: `navi-cat` pulls
//! `navi-hamlib-sys` (workspace `rust-version = "1.98"`).

use std::path::PathBuf;

use navi_cat::mirror::{self, DEFAULT_REPEATERS_BASE};
use navi_cat::repeater::RepeaterDb;

fn main() {
    let mut base = DEFAULT_REPEATERS_BASE.to_string();
    let mut db_path = PathBuf::from("/tmp/navi-cat-repeater_corridor_check.sqlite");
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--base-url" => {
                base = args.next().unwrap_or_else(|| usage());
            }
            "--db" => {
                db_path = PathBuf::from(args.next().unwrap_or_else(|| usage()));
            }
            "-h" | "--help" => usage(),
            other => {
                eprintln!("unknown arg: {other}");
                usage();
            }
        }
    }

    eprintln!(
        "repeater_corridor_check base={base} db={}",
        db_path.display()
    );
    let report = mirror::fetch_and_update(&base, &db_path);
    println!(
        "UPDATE ok={} generation={} imported={} kept_last_good={} sources_ok={:?} missing={:?} failed={:?} error={:?}",
        report.ok,
        report.generation,
        report.imported,
        report.kept_last_good,
        report.sources_ok,
        report.sources_missing,
        report.sources_failed,
        report.error
    );
    if !report.ok {
        std::process::exit(2);
    }

    let db = match RepeaterDb::open_path(&db_path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("open db: {e:#}");
            std::process::exit(3);
        }
    };

    // Sample corridors from the 0.3.23 / #169 regression notes (@50 km).
    let corridors: &[(&str, &[(f64, f64)])] = &[
        (
            "lat_band_59.109_to_62.288",
            &[
                (59.109, 10.0),
                (59.5, 10.2),
                (60.0, 10.5),
                (60.5, 10.7),
                (61.0, 10.8),
                (61.5, 10.9),
                (62.0, 11.0),
                (62.288, 11.1),
            ],
        ),
        (
            "hiking_gjende_leir_skjolden",
            &[(61.49, 8.69), (61.55, 8.40), (61.49, 7.60)],
        ),
        (
            "german_route_norway_segment",
            &[(59.2, 10.4), (60.4, 10.5), (61.1, 10.5)],
        ),
    ];

    let radius_km = 50.0;
    let mut all_ok = true;
    for (name, pts) in corridors {
        let mut hits = 0usize;
        let mut sum = 0usize;
        for &(lat, lon) in *pts {
            let n = db.query_near(lat, lon, radius_km, None).len();
            if n > 0 {
                hits += 1;
            }
            sum += n;
            println!("SAMPLE corridor={name} lat={lat:.3} lon={lon:.3} near={n}");
        }
        let need = pts.len();
        let ok = hits == need;
        all_ok &= ok;
        println!("CORRIDOR name={name} samples_with_hits={hits}/{need} hit_sum={sum} ok={ok}");
    }

    // Second Update with unchanged server data must not re-download / re-import.
    let again = mirror::fetch_and_update(&base, &db_path);
    println!(
        "UPDATE_AGAIN ok={} imported={} kept_last_good={} generation={}",
        again.ok, again.imported, again.kept_last_good, again.generation
    );
    if !again.ok || !again.kept_last_good || again.imported != 0 {
        eprintln!(
            "FAIL: second Update should keep last good with imported=0 (unchanged generation)"
        );
        std::process::exit(5);
    }

    if !all_ok {
        std::process::exit(4);
    }
    println!("PASS repeater_corridor_check");
}

fn usage() -> ! {
    eprintln!(
        "usage: repeater_corridor_check [--base-url URL] [--db PATH]\n\
         build: cargo run -p navi-cat --features diag-bins --bin repeater_corridor_check"
    );
    std::process::exit(1);
}
