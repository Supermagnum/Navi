//! Plan CAT corridor polylines (Espa→Dombås E6; Espa→Litjfossen via Skansen Rv 3).
//!
//! Writes `testdata/cat/routes/*.polyline` (Navi `lon,lat;…` format) plus short meta JSON.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use navi::{
    ensure_corridor_skeleton, plan_car_route, FfiLatLon, FfiTollPolicy, FfiVehicleLimits,
    TravelProfile,
};

fn main() {
    let out = PathBuf::from(env::var("CAT_ROUTE_OUT").unwrap_or_else(|_| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../testdata/cat/routes")
            .display()
            .to_string()
    }));
    let pack = PathBuf::from(env::var("CAT_PACK_DIR").unwrap_or_else(|_| {
        "/mnt/2e9a1e9f-2097-408c-ab9a-a01b32f11d28/github-projects/Navi/target/espa-dombas-e2e"
            .into()
    }));
    let elev = PathBuf::from(env::var("CAT_ELEV_DIR").unwrap_or_else(|_| {
        "/mnt/2e9a1e9f-2097-408c-ab9a-a01b32f11d28/github-projects/Navi/core/target/integration-fixtures/elevation"
            .into()
    }));
    let pbf = pack.join("ostlandet-latest.osm.pbf");
    let cache = pack.join("cache-cat-corridors");
    fs::create_dir_all(&out).expect("out dir");
    fs::create_dir_all(&cache).ok();

    // Prefer indexed packs when the stem is registered; otherwise force PBF
    // cold-build via the debug pack_dir sentinel (still Navi's car router).
    let use_force_pbf = env::var("CAT_FORCE_PBF").map(|v| v == "1").unwrap_or(true);
    let pack_arg = if use_force_pbf {
        eprintln!("using __navi_force_pbf__ (Navi car router on real PBF)");
        "__navi_force_pbf__".to_string()
    } else {
        eprintln!("ensuring corridor skeleton for ostlandet-latest…");
        let sk = ensure_corridor_skeleton(
            pack.display().to_string(),
            "ostlandet-latest".into(),
            TravelProfile::Car,
        );
        eprint!("{sk}");
        if sk.contains("FAIL:") {
            std::process::exit(1);
        }
        pack.display().to_string()
    };
    let data_arg = pack.display().to_string();

    let empty_vehicle = || FfiVehicleLimits {
        axle_weight_kg: None,
        bogie_weight_kg: None,
        height_m: None,
        width_m: None,
        length_m: None,
        total_weight_kg: None,
    };

    // Espa → Dombås (E6 corridor)
    let r1 = plan_car_route(
        pbf.display().to_string(),
        elev.display().to_string(),
        cache.display().to_string(),
        60.563,
        11.257,
        62.076,
        9.128,
        false,
        TravelProfile::Car,
        false,
        FfiTollPolicy::Allow,
        false,
        false,
        empty_vehicle(),
        false,
        data_arg.clone(),
        pack_arg.clone(),
        false,
        None,
        Vec::new(),
    );
    write_route(
        &out,
        "espa_dombas_e6",
        "Espa to Dombas via E6 (Navi car router)",
        &r1,
    );

    // Espa → Litjfossen via Skansen rundkjøring (Rv 3 / Østerdalen)
    let vias = vec![FfiLatLon {
        lat: 60.88,
        lon: 11.56,
    }];
    let r2 = plan_car_route(
        pbf.display().to_string(),
        elev.display().to_string(),
        cache.display().to_string(),
        60.563,
        11.257,
        62.35,
        10.85,
        false,
        TravelProfile::Car,
        false,
        FfiTollPolicy::Allow,
        false,
        false,
        empty_vehicle(),
        false,
        data_arg,
        pack_arg,
        false,
        None,
        vias,
    );
    write_route(
        &out,
        "espa_litjfossen_rv3_skansen",
        "Espa to Litjfossen via Skansen rundkjøring Rv 3 (Navi car router)",
        &r2,
    );
}

fn write_route(out: &Path, stem: &str, title: &str, r: &navi::CorridorRouteResult) {
    let poly_path = out.join(format!("{stem}.polyline"));
    let meta_path = out.join(format!("{stem}.meta.json"));
    fs::write(&poly_path, &r.route_polyline).expect("write polyline");
    let n_pts = r
        .route_polyline
        .split(';')
        .filter(|s| !s.is_empty())
        .count();
    let meta = serde_json::json!({
        "title": title,
        "distance_km": r.distance_km,
        "eta_minutes": r.eta_minutes,
        "point_count": n_pts,
        "pack_hit": r.report.contains("pack_hit=true"),
        "report_head": r.report.lines().take(20).collect::<Vec<_>>().join("\n"),
        "ok": r.distance_km > 1.0 && n_pts > 10 && !r.report.contains("FAIL:"),
    });
    fs::write(&meta_path, serde_json::to_string_pretty(&meta).unwrap()).expect("meta");
    eprintln!(
        "{stem}: km={:.1} pts={n_pts} pack_hit={} ok={}",
        r.distance_km, meta["pack_hit"], meta["ok"]
    );
    if !meta["ok"].as_bool().unwrap_or(false) {
        eprintln!("report:\n{}", r.report);
        std::process::exit(1);
    }
}
