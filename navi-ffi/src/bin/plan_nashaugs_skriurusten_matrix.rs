//! Host matrix: Nashaugsætra → Skriurusten (hiking), four toggle combos.
//!
//! Mirrors [`plan_skolla_rondvassbu`] but sweeps:
//!   prefer_official_networks × use_networked_cabins
//! and prints a compact comparison from each plan report.
//!
//! Env (optional): `NAVI_PBF`, `NAVI_ELEV`, `NAVI_CACHE`, `NAVI_DATA_DIR`.

use std::env;
use std::fs;
use std::path::PathBuf;

const WPS: &str = r#"[
  {"name":"Nashaugsætra","lat":61.1511448,"lon":9.8433622},
  {"name":"Skriurusten","lat":61.3338272,"lon":9.3429367}
]"#;

fn main() {
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../core/target/integration-fixtures");
    let pbf = env::var("NAVI_PBF")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("ostlandet-latest.osm.pbf"));
    let elev = env::var("NAVI_ELEV")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("elevation"));
    let cache = env::var("NAVI_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("graph-cache-foot-ostlandet"));
    // Prefs (use_networked_cabins) are read from cache.parent()/navi.db.
    let data_dir = env::var("NAVI_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| cache.parent().unwrap_or(&root).to_path_buf());
    let _ = fs::create_dir_all(&cache);
    let _ = fs::create_dir_all(&data_dir);

    let out_dir = root.join("nashaugs_skriurusten_matrix");
    let _ = fs::create_dir_all(&out_dir);

    eprintln!("pbf={}", pbf.display());
    eprintln!("elev={}", elev.display());
    eprintln!("cache={}", cache.display());
    eprintln!("data_dir={}", data_dir.display());
    eprintln!("waypoints=Nashaugsætra → Skriurusten");

    navi::set_route_plan_timing_enabled(true);

    let combos: [(u8, bool, bool); 4] = [
        (1, false, false),
        (2, true, false),
        (3, false, true),
        (4, true, true),
    ];

    println!("combo\tofficial\tcabins\tdistance_km\teta_min\tauto_vias\tauto_via_names\tofftrail_note\taccessish");
    let mut any_fail = false;

    for (num, official, cabins) in combos {
        let ok_save = navi::save_use_networked_cabins(data_dir.display().to_string(), cabins);
        if !ok_save {
            eprintln!("FAIL: save_use_networked_cabins({cabins})");
            any_fail = true;
            continue;
        }
        let loaded = navi::load_use_networked_cabins(data_dir.display().to_string());
        if loaded != cabins {
            eprintln!("FAIL: pref round-trip want={cabins} got={loaded}");
            any_fail = true;
            continue;
        }

        eprintln!("\n=== combo {num}: official={official} cabins={cabins} ===");
        let t0 = std::time::Instant::now();
        let r = navi::plan_hiking_route(
            pbf.display().to_string(),
            elev.display().to_string(),
            cache.display().to_string(),
            WPS.to_string(),
            official,
            false, // pilgrim off
            String::new(),
        );
        let elapsed = t0.elapsed();
        eprintln!("plan_wall_s={:.1}", elapsed.as_secs_f64());

        let report_path = out_dir.join(format!("c{num}_report.txt"));
        let _ = fs::write(&report_path, &r.report);
        let _ = fs::write(
            out_dir.join(format!("c{num}_breaks.json")),
            &r.break_pois_json,
        );
        let _ = fs::write(
            out_dir.join(format!("c{num}_polyline.txt")),
            &r.route_polyline,
        );
        let _ = fs::write(
            out_dir.join(format!("c{num}_segments.json")),
            &r.route_segments_json,
        );

        let pass = r.report.contains("PASS");
        if !pass {
            eprintln!("FAIL: plan did not PASS\n{}", r.report);
            any_fail = true;
        }

        let auto_line = r
            .report
            .lines()
            .find(|l| l.starts_with("auto_vias="))
            .unwrap_or("auto_vias=?");
        let auto_names = auto_line
            .split("names=")
            .nth(1)
            .unwrap_or("")
            .trim()
            .to_string();
        let auto_n = auto_line
            .strip_prefix("auto_vias=")
            .and_then(|s| s.split(';').next())
            .unwrap_or("?");

        let pref_line = r
            .report
            .lines()
            .find(|l| l.starts_with("use_networked_cabins="))
            .unwrap_or("use_networked_cabins=?");
        eprintln!("{pref_line}");
        eprintln!("{auto_line}");

        let blob = format!(
            "{}\n{}\n{}\n{}",
            r.report, r.break_pois_json, r.off_trail_advisory, r.days_json
        )
        .to_lowercase();
        let accessish: Vec<&str> = [
            "member",
            "membership",
            "non-member",
            "emergency",
            "overnight",
            "stay",
            "enter the hut",
            "sleep",
            "dnt key",
        ]
        .into_iter()
        .filter(|k| blob.contains(k))
        .collect();

        let offtrail_owned = if r.off_trail_advisory.is_empty() {
            "-".to_string()
        } else {
            r.off_trail_advisory.replace('\n', " | ")
        };

        let access_col = if accessish.is_empty() {
            "-".to_string()
        } else {
            accessish.join(",")
        };

        let names_col = if auto_names.is_empty() {
            "-".to_string()
        } else {
            auto_names
        };

        println!(
            "{num}\t{official}\t{cabins}\t{:.2}\t{:.0}\t{auto_n}\t{names_col}\t{offtrail_owned}\t{access_col}",
            r.distance_km, r.eta_minutes,
        );

        let on = r.route_segments_json.matches("on_trail").count();
        let off = r.route_segments_json.matches("off_trail").count();
        eprintln!(
            "segments_json_bytes={} on_trail≈{on} off_trail≈{off} polyline_parts={}",
            r.route_segments_json.len(),
            r.route_polyline.matches(';').count().saturating_add(1)
        );
    }

    eprintln!("\nwrote reports under {}", out_dir.display());
    if any_fail {
        std::process::exit(1);
    }
}
