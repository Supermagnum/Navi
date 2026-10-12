//! Host gate: ECO pack-hit plan completes with eco_reweight_ms > 0 and a
//! bounded distance (same corridor as eco-off).
//!
//! Historical “ECO hang” on Raufoss→Bergen was pack/corridor connectivity
//! (empty packDir, tile budget, missing Vestlandet), not joule reweight
//! (`eco_reweight_ms` is typically 50–260 ms). This gate asserts the eco
//! overlay actually runs and the plan finishes without spinning.
//!
//! Environment:
//! - `NAVI_GATE_PACKS` (required): long-trip pack dir with ostlandet +
//!   vestlandet v9 manifests/tiles and `elevation/`.
//!
//! Run:
//! ```text
//! NAVI_GATE_PACKS=/path/to/packs cargo test --release -p navi-ffi \
//!   --features regression-gate --test eco_corridor_plan_gate \
//!   -- --ignored --nocapture
//! ```

use std::path::PathBuf;
use std::time::Instant;

use navi::{
    plan_car_route, set_route_plan_timing_enabled, FfiTollPolicy, FfiVehicleLimits, TravelProfile,
};

/// Raufoss → Dombås (same-stem Ostlandet control).
const RAUFOSS: (f64, f64) = (60.7277483, 10.6109403);
const DOMBAS: (f64, f64) = (62.0755, 9.1278);
/// Documented pack-hit distance band (km) for this corridor.
const EXPECT_KM: f64 = 206.81;
const KM_TOL: f64 = 5.0;
/// Same-stem pack-hit should finish well under this (cold load included).
const WALL_BUDGET_SECS: f64 = 30.0;

#[test]
#[ignore = "needs NAVI_GATE_PACKS (ostlandet+vestlandet v9 + elevation)"]
fn eco_raufoss_dombas_pack_hit_reweight_and_distance() {
    let packs = match std::env::var("NAVI_GATE_PACKS") {
        Ok(p) if PathBuf::from(&p).is_dir() => PathBuf::from(p),
        _ => {
            eprintln!("skip: NAVI_GATE_PACKS unset or not a directory");
            return;
        }
    };
    assert!(
        packs.join("ostlandet-latest.navi-manifest.json").is_file(),
        "missing ostlandet manifest in {}",
        packs.display()
    );

    let elev = packs.join("elevation");
    let cache = packs.join("graph-cache-eco-gate");
    let _ = std::fs::create_dir_all(&cache);
    let pbf = packs.join("ostlandet-latest.osm.pbf");
    let pbf_s = pbf.to_string_lossy().into_owned();
    let elev_s = elev.to_string_lossy().into_owned();
    let cache_s = cache.to_string_lossy().into_owned();
    let packs_s = packs.to_string_lossy().into_owned();

    set_route_plan_timing_enabled(true);

    let limits = FfiVehicleLimits {
        axle_weight_kg: None,
        bogie_weight_kg: None,
        height_m: None,
        width_m: None,
        length_m: None,
        total_weight_kg: None,
    };

    let plan = |eco: bool| {
        let t0 = Instant::now();
        let r = plan_car_route(
            pbf_s.clone(),
            elev_s.clone(),
            cache_s.clone(),
            RAUFOSS.0,
            RAUFOSS.1,
            DOMBAS.0,
            DOMBAS.1,
            eco,
            TravelProfile::Car,
            false,
            FfiTollPolicy::Allow,
            false,
            false,
            limits.clone(),
            false,
            packs_s.clone(),
            packs_s.clone(),
            false,
            None,
            Vec::new(),
        );
        (t0.elapsed().as_secs_f64(), r)
    };

    let (wall_off, off) = plan(false);
    assert!(
        wall_off < WALL_BUDGET_SECS,
        "eco-off wall {wall_off:.1}s exceeds {WALL_BUDGET_SECS}s budget; report={}",
        off.report
    );
    assert!(
        off.report.contains("pack_hit=true"),
        "eco-off must pack-hit; report={}",
        off.report
    );
    assert!(
        (off.distance_km - EXPECT_KM).abs() <= KM_TOL,
        "eco-off distance {:.2} not in {EXPECT_KM}±{KM_TOL}; report={}",
        off.distance_km,
        off.report
    );

    let (wall_on, on) = plan(true);
    assert!(
        wall_on < WALL_BUDGET_SECS,
        "eco-on wall {wall_on:.1}s exceeds {WALL_BUDGET_SECS}s budget; report={}",
        on.report
    );
    assert!(
        !on.report.contains("FAIL"),
        "eco-on must not FAIL; report={}",
        on.report
    );
    assert!(
        on.report.contains("pack_hit=true"),
        "eco-on must pack-hit; report={}",
        on.report
    );
    let eco_ms = extract_u64(&on.report, "eco_reweight_ms");
    assert!(
        eco_ms > 0,
        "eco_reweight_ms must be > 0 on eco pack-hit, got {eco_ms}; report={}",
        on.report
    );
    assert!(
        (on.distance_km - EXPECT_KM).abs() <= KM_TOL,
        "eco-on distance {:.2} not in {EXPECT_KM}±{KM_TOL}; report={}",
        on.distance_km,
        on.report
    );
    // Eco wall should stay in the same ballpark as eco-off (no multi-minute hang).
    assert!(
        wall_on < wall_off * 3.0 + 5.0,
        "eco-on wall {wall_on:.1}s >> eco-off {wall_off:.1}s; report={}",
        on.report
    );

    eprintln!(
        "eco_gate ok: off wall={wall_off:.1}s km={:.2}; on wall={wall_on:.1}s km={:.2} eco_reweight_ms={eco_ms}",
        off.distance_km, on.distance_km
    );
}

fn extract_u64(report: &str, key: &str) -> u64 {
    let prefix = format!("{key}=");
    for part in report.split(['\n', ' ', ';', '|']) {
        if let Some(rest) = part.strip_prefix(&prefix) {
            if let Ok(v) = rest.parse::<u64>() {
                return v;
            }
        }
    }
    0
}
