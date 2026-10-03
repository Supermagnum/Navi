//! ICE AFR / fuel-rate decode golden suite (no live adapter).

use std::path::PathBuf;

use driver_break_core::config::Profile;
use driver_break_core::ecu::decode::{
    apply_j1939_lfe_illustrative, apply_megasquirt, decode_elm327_mode01,
    decode_pgn_65266_fuel_rate, elm327_is_no_data, IceDecode,
};
use driver_break_core::ecu::fuel::{
    derive_maf_fuel_rate, derive_maf_fuel_rate_ex, megasquirt_fuel_rate_l_h, FuelRateSource,
    IceFuelKind, PETROL_DENSITY_KG_L, PETROL_STOICH_AFR,
};

#[path = "helpers/ecu_engine_model.rs"]
mod ecu_engine_model;

use ecu_engine_model::{all_scenarios, generated_section, run_scenario, EngineKind, FuelBlend};

const BEGIN_GEN: &str = "<!-- BEGIN GENERATED ECU SCENARIOS -->";
const END_GEN: &str = "<!-- END GENERATED ECU SCENARIOS -->";
use driver_break_core::ecu::self_test::{
    afr_fuel_rate_self_test_passed, afr_fuel_rate_self_test_report,
};
use driver_break_core::ecu::{
    refine_energy_cost, LiveEnergyProvider, LiveEnergySnapshot, NoLiveEnergy,
};

#[test]
fn self_test_report_passes() {
    let report = afr_fuel_rate_self_test_report();
    assert!(
        afr_fuel_rate_self_test_passed(),
        "self-test failed:\n{report}"
    );
    assert!(report.contains("TEST_KIND=ECU_AFR_FUEL_RATE"));
    assert!(report.contains("DATA_SOURCE=none"));
}

#[test]
fn elm327_pid_table_ice_only() {
    let mut d = IceDecode::default();
    decode_elm327_mode01("410D32", &mut d); // 50 km/h
    decode_elm327_mode01("41100C80", &mut d); // 32.00 g/s
    decode_elm327_mode01("4104FF", &mut d);
    decode_elm327_mode01("414300FF", &mut d);
    assert_eq!(d.speed_kmh, Some(50.0));
    assert!(d.calc_load_pct.is_some());
    assert!(d.abs_load_pct.is_some());
}

#[test]
fn pid_0c_rpm_formula() {
    let mut d = IceDecode::default();
    // A=32 B=0 → (32*256)/4 = 2048 rpm (Wikipedia example style).
    decode_elm327_mode01("410C2000", &mut d);
    assert!((d.rpm.unwrap() - 2048.0).abs() < 1e-9);
}

#[test]
fn diesel_without_rate_or_lambda_stays_none() {
    let mut d = IceDecode::default();
    decode_elm327_mode01("41100C80", &mut d);
    d.finish_fuel_rate(IceFuelKind::Diesel);
    assert!(d.fuel_rate_l_h.is_none());
    assert!(derive_maf_fuel_rate(IceFuelKind::Diesel, d.maf_g_s, None, None).is_none());
}

#[test]
fn petrol_flex_changes_maf_rate() {
    let e0 = derive_maf_fuel_rate(IceFuelKind::Petrol, Some(10.0), None, None).unwrap();
    let e85 = derive_maf_fuel_rate(IceFuelKind::Petrol, Some(10.0), None, Some(85.0)).unwrap();
    assert!(e85 > e0, "higher ethanol volume for same air mass");
}

#[test]
fn j1939_lfe_illustrative_matches_spn183_scale() {
    assert_eq!(decode_pgn_65266_fuel_rate(&[0x64, 0x00]), Some(5.0));
    let mut d = IceDecode::default();
    apply_j1939_lfe_illustrative(&mut d, &[0x64, 0x00, 0xC8, 0x00]);
    assert_eq!(d.fuel_rate_l_h, Some(5.0));
}

#[test]
fn megasquirt_skips_out_of_range() {
    assert!(megasquirt_fuel_rate_l_h(4.0, 3000.0, 4, 250.0).is_some());
    let mut d = IceDecode::default();
    apply_megasquirt(&mut d, 4.0, 0.0, 4, 250.0, None);
    assert!(d.fuel_rate_l_h.is_none());
}

#[test]
fn no_data_patterns() {
    assert!(elm327_is_no_data("NO DATA"));
    assert!(elm327_is_no_data("UNABLE TO CONNECT"));
    assert!(elm327_is_no_data("BUS INIT: ERROR"));
    assert!(elm327_is_no_data("?"));
}

#[test]
fn refine_energy_cost_and_provider() {
    assert!(NoLiveEnergy.latest(Profile::Car).is_none());
    let predicted = 9.0e5;
    assert_eq!(refine_energy_cost(predicted, 100.0, None), predicted);
    let live = LiveEnergySnapshot {
        fuel_rate_l_h: Some(5.0),
        state_of_charge_pct: None,
        power_kw: None,
    };
    let cost = refine_energy_cost(predicted, 100.0, Some(&live));
    assert!(cost > 0.0 && cost != predicted);
}

#[test]
fn maf_density_disagrees_with_ecu_md_074() {
    let formulas = 2.0 * 3600.0 / (PETROL_STOICH_AFR * PETROL_DENSITY_KG_L * 1000.0);
    let ecu_md = 2.0 / 14.7 * 3600.0 / 740.0;
    assert!((formulas - ecu_md).abs() > 1e-4);
}

fn scenario_results() -> Vec<ecu_engine_model::ScenarioResult> {
    all_scenarios().iter().map(run_scenario).collect()
}

fn results_doc_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../docs/ecu-test-results.md")
}

fn splice_generated(doc: &str, generated: &str) -> String {
    let start = doc
        .find(BEGIN_GEN)
        .expect("docs/ecu-test-results.md missing BEGIN marker");
    let end = doc
        .find(END_GEN)
        .expect("docs/ecu-test-results.md missing END marker");
    let mut out = String::new();
    out.push_str(&doc[..start]);
    out.push_str(BEGIN_GEN);
    out.push('\n');
    out.push_str(generated);
    if !generated.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(END_GEN);
    out.push_str(&doc[end + END_GEN.len()..]);
    out
}

#[test]
fn write_scenario_results() {
    let rows = scenario_results();
    let generated = generated_section(&rows);
    let path = results_doc_path();
    let current = std::fs::read_to_string(&path).expect("read ecu-test-results.md");
    let next = splice_generated(&current, &generated);
    if std::env::var("NAVI_ECU_WRITE_RESULTS").ok().as_deref() == Some("1") {
        std::fs::write(&path, &next).expect("write ecu-test-results.md");
        eprintln!("wrote generated ECU scenarios to {}", path.display());
        return;
    }
    let start = current.find(BEGIN_GEN).expect("BEGIN marker");
    let end = current.find(END_GEN).expect("END marker");
    let committed = &current[start + BEGIN_GEN.len()..end];
    let expected = format!("\n{generated}");
    assert_eq!(
        committed, expected,
        "generated ECU scenario section is stale; run NAVI_ECU_WRITE_RESULTS=1 cargo test -p driver-break-core --test ecu_afr_fuel_rate write_scenario_results -- --nocapture"
    );
}

#[test]
fn diesel_fuel_rises_with_load() {
    let rows = scenario_results();
    for eng in [EngineKind::NaDiesel, EngineKind::TurboDiesel] {
        let mut rates: Vec<(f64, f64)> = rows
            .iter()
            .filter(|r| r.engine == eng && r.table_id.contains("/ load"))
            .filter_map(|r| {
                let load = r
                    .vary_label
                    .split_whitespace()
                    .nth(1)?
                    .trim_end_matches('%')
                    .parse::<f64>()
                    .ok()?;
                Some((load, r.fuel_l_h?))
            })
            .collect();
        rates.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        for w in rates.windows(2) {
            assert!(
                w[1].1 + 1e-6 >= w[0].1,
                "{:?} load {} -> {} L/h not monotonic",
                eng,
                w[0].0,
                w[1].1
            );
        }
    }
}

#[test]
fn maf_vs_altitude_and_iat_at_wot() {
    let rows = scenario_results();
    for eng in [
        EngineKind::NaPetrol,
        EngineKind::TurboPetrol,
        EngineKind::NaDiesel,
        EngineKind::TurboDiesel,
    ] {
        let alt: Vec<_> = rows
            .iter()
            .filter(|r| r.engine == eng && r.table_id.contains("/ altitude"))
            .collect();
        let maf0 = alt
            .iter()
            .find(|r| r.vary_label.starts_with("0 m"))
            .and_then(|r| r.maf_g_s)
            .unwrap();
        let maf2500 = alt
            .iter()
            .find(|r| r.vary_label.starts_with("2500 m"))
            .and_then(|r| r.maf_g_s)
            .unwrap();
        assert!(maf2500 < maf0, "{:?} MAF should fall by 2500 m", eng);
        let iat: Vec<_> = rows
            .iter()
            .filter(|r| r.engine == eng && r.table_id.contains("/ intake air"))
            .collect();
        let cold = iat
            .iter()
            .find(|r| r.vary_label.contains("-30"))
            .and_then(|r| r.maf_g_s)
            .unwrap();
        let hot = iat
            .iter()
            .find(|r| r.vary_label.contains("40 C"))
            .and_then(|r| r.maf_g_s)
            .unwrap();
        assert!(hot < cold, "{:?} MAF should fall as IAT rises", eng);
    }
    let na0 = rows
        .iter()
        .find(|r| r.engine == EngineKind::NaPetrol && r.vary_label.starts_with("0 m"))
        .unwrap()
        .maf_g_s
        .unwrap();
    let na5 = rows
        .iter()
        .find(|r| r.engine == EngineKind::NaPetrol && r.vary_label.starts_with("500 m"))
        .unwrap()
        .maf_g_s
        .unwrap();
    let tu0 = rows
        .iter()
        .find(|r| r.engine == EngineKind::TurboPetrol && r.vary_label.starts_with("0 m"))
        .unwrap()
        .maf_g_s
        .unwrap();
    let tu5 = rows
        .iter()
        .find(|r| r.engine == EngineKind::TurboPetrol && r.vary_label.starts_with("500 m"))
        .unwrap()
        .maf_g_s
        .unwrap();
    let na_drop = (na0 - na5) / na0;
    let tu_drop = (tu0 - tu5) / tu0;
    assert!(
        tu_drop < na_drop,
        "turbo should hold MAF better than NA to 500 m (tu {tu_drop} na {na_drop})"
    );
}

#[test]
fn same_maf_lambda_same_litres() {
    let a = derive_maf_fuel_rate_ex(
        IceFuelKind::Petrol,
        Some(25.0),
        Some(1.0),
        None,
        Some(15.0),
        Some(90.0),
        Some(40.0),
    )
    .unwrap();
    let b = derive_maf_fuel_rate_ex(
        IceFuelKind::Petrol,
        Some(25.0),
        Some(1.0),
        None,
        Some(15.0),
        Some(90.0),
        Some(40.0),
    )
    .unwrap();
    assert!((a.fuel_l_h - b.fuel_l_h).abs() < 1e-12);
}

#[test]
fn hotter_fuel_more_litres_on_maf_path() {
    let rows = scenario_results();
    let cold = rows
        .iter()
        .find(|r| r.engine == EngineKind::NaPetrol && r.vary_label == "fuel -30 C")
        .unwrap();
    let hot = rows
        .iter()
        .find(|r| r.engine == EngineKind::NaPetrol && r.vary_label == "fuel 60 C")
        .unwrap();
    let c = cold.fuel_l_h.unwrap();
    let h = hot.fuel_l_h.unwrap();
    let pct = (h - c) / c * 100.0;
    assert!(
        pct > 6.0 && pct < 10.0,
        "petrol -30 C to +60 C litre change {pct}%"
    );
}

#[test]
fn e85_uses_more_litres_than_e0() {
    let rows = scenario_results();
    let e0 = rows
        .iter()
        .find(|r| {
            r.engine == EngineKind::NaPetrol
                && r.table_id.contains("fuel type")
                && r.blend == FuelBlend::E0
        })
        .unwrap()
        .fuel_l_h
        .unwrap();
    let e85 = rows
        .iter()
        .find(|r| {
            r.engine == EngineKind::NaPetrol
                && r.table_id.contains("fuel type")
                && r.blend == FuelBlend::E85
        })
        .unwrap()
        .fuel_l_h
        .unwrap();
    let pct = (e85 - e0) / e0 * 100.0;
    assert!(pct > 30.0 && pct < 55.0, "E85 vs E0 litre change {pct}%");
}

#[test]
fn colder_coolant_higher_petrol_idle() {
    let rows = scenario_results();
    let cold = rows
        .iter()
        .find(|r| r.engine == EngineKind::NaPetrol && r.vary_label == "coolant -30 C")
        .unwrap()
        .fuel_l_h
        .unwrap();
    let warm = rows
        .iter()
        .find(|r| r.engine == EngineKind::NaPetrol && r.vary_label == "coolant 90 C")
        .unwrap()
        .fuel_l_h
        .unwrap();
    assert!(cold > warm);
}

#[test]
fn overrun_fuel_cut_rules() {
    let rows = scenario_results();
    for r in rows
        .iter()
        .filter(|r| r.vary_label.contains("overrun") || r.vary_label.contains("downhill"))
    {
        if r.engine.is_diesel() {
            assert_eq!(r.fuel_l_h, Some(0.0), "{}", r.vary_label);
        } else if r.vary_label.contains("throttle 0") {
            assert_eq!(r.fuel_l_h, Some(0.0));
        }
    }
    let cold_down = rows
        .iter()
        .find(|r| r.engine == EngineKind::NaPetrol && r.vary_label.contains("cold engine"))
        .unwrap();
    assert!(
        cold_down.fuel_l_h.unwrap() > 0.0,
        "cold petrol downhill should still inject"
    );
}

#[test]
fn diesel_never_uses_naive_as_result() {
    let rows = scenario_results();
    for r in rows.iter().filter(|r| r.engine.is_diesel()) {
        assert_ne!(r.source, FuelRateSource::None);
        if let (Some(real), Some(naive)) = (r.fuel_l_h, r.naive_l_h) {
            if real > 0.05 {
                assert!(
                    (real - naive).abs() > 1e-4,
                    "diesel result must not equal naive 14.7"
                );
            }
        }
        assert!(
            !matches!(r.source, FuelRateSource::MafDerived)
                || r.afr.unwrap_or(0.0) > 15.0
                || r.fuel_l_h == Some(0.0)
                || r.lambda.unwrap_or(0.0) > 1.05
        );
    }
}

/// Torque-path error vs model true rate. PID 62 is 1 %/bit; idle intercept and
/// low-load BSFC widening are the same function used to encode torque, so the
/// leftover is quantization. Bound 20 % covers that plus MAF PID rounding on
/// the true-rate side.
const DIESEL_TORQUE_ERR_BOUND_PCT: f64 = 20.0;

#[test]
fn diesel_never_uses_saturated_lambda_as_afr() {
    let rows = scenario_results();
    for r in rows.iter().filter(|r| r.engine.is_diesel()) {
        if r.lambda_saturated {
            assert!(
                r.afr.is_none(),
                "saturated AFR used {} {}",
                r.table_id,
                r.vary_label
            );
            assert_ne!(r.source, FuelRateSource::MafDerived);
        }
    }
}

#[test]
fn diesel_idle_and_cruise_torque_error_bound() {
    let rows = scenario_results();
    for r in rows.iter().filter(|r| r.engine.is_diesel()) {
        let idle_or_cruise = r.table_id.contains("/ coolant")
            || (r.table_id.contains("/ load") && r.vary_label.contains("50%"))
            || r.vary_label.contains("motorway");
        if !idle_or_cruise || r.fuel_cut {
            continue;
        }
        if r.source != FuelRateSource::TorqueBsfc && r.lambda_saturated {
            panic!(
                "expected torque path when lambda sat: {} {}",
                r.table_id, r.vary_label
            );
        }
        if r.source == FuelRateSource::TorqueBsfc {
            let e = r.navi_err_pct.expect("navi vs true");
            assert!(
                e.abs() <= DIESEL_TORQUE_ERR_BOUND_PCT,
                "{} {} error {e}% exceeds {DIESEL_TORQUE_ERR_BOUND_PCT}%",
                r.table_id,
                r.vary_label
            );
        }
    }
}

#[test]
fn diesel_coolant_sweep_not_flat() {
    let rows = scenario_results();
    let rates: Vec<f64> = rows
        .iter()
        .filter(|r| r.engine == EngineKind::NaDiesel && r.table_id.contains("/ coolant"))
        .filter_map(|r| r.fuel_l_h)
        .collect();
    let min = rates.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = rates.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    assert!(
        max > min * 1.05,
        "diesel coolant sweep still flat {min}..{max}"
    );
}

#[test]
fn no_nan_or_inf_cells() {
    let rows = scenario_results();
    for r in &rows {
        for x in [
            r.maf_g_s,
            r.lambda,
            r.afr,
            r.density,
            r.fuel_l_h,
            r.l100,
            r.naive_l_h,
            r.naive_err_pct,
            r.navi_err_pct,
            r.true_afr,
            r.true_fuel_l_h,
        ]
        .into_iter()
        .flatten()
        {
            assert!(
                x.is_finite(),
                "non-finite in {} {}",
                r.table_id,
                r.vary_label
            );
        }
    }
}
