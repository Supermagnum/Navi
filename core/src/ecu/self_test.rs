//! Golden-vector self-test for ICE decode / fuel-rate (no adapter).

use super::decode::{
    apply_j1939_spn183, apply_j1939_spn96, apply_megasquirt, decode_elm327_mode01, IceDecode,
};
use super::fuel::{
    derive_maf_fuel_rate, fuel_current_l, instant_l_per_100km, maf_to_fuel_rate_l_h, range_km,
    FuelRateSource, IceFuelKind, PETROL_DENSITY_KG_L, PETROL_STOICH_AFR,
};
use super::refine_energy_cost;
use super::LiveEnergySnapshot;

fn check(ok: &mut bool, lines: &mut Vec<String>, name: &str, pass: bool, detail: String) {
    if pass {
        lines.push(format!("PASS {name} {detail}"));
    } else {
        lines.push(format!("FAIL {name} {detail}"));
        *ok = false;
    }
}

fn approx(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() <= eps
}

/// Run every ICE AFR / fuel-rate golden case. Report ends with PASS or FAIL.
pub fn afr_fuel_rate_self_test_report() -> String {
    let mut ok = true;
    let mut lines = vec![
        "TEST_KIND=ECU_AFR_FUEL_RATE".to_string(),
        "DATA_SOURCE=none".to_string(),
        "note=pure decode; no live adapter/BT/serial/CAN".to_string(),
    ];

    // Part 1 — ELM327 PID 5E (ECU.md example).
    let mut d = IceDecode::default();
    let parsed = decode_elm327_mode01("415E0064", &mut d);
    check(
        &mut ok,
        &mut lines,
        "elm_5e",
        parsed && d.fuel_rate_l_h == Some(5.0),
        format!("rate={:?}", d.fuel_rate_l_h),
    );
    let mut spaced = IceDecode::default();
    decode_elm327_mode01("41 5E 00 64", &mut spaced);
    check(
        &mut ok,
        &mut lines,
        "elm_5e_spaces",
        spaced.fuel_rate_l_h == Some(5.0),
        format!("rate={:?}", spaced.fuel_rate_l_h),
    );
    let mut nodata = IceDecode::default();
    let got = decode_elm327_mode01("NO DATA", &mut nodata);
    check(
        &mut ok,
        &mut lines,
        "elm_no_data",
        !got && nodata.fuel_rate_l_h.is_none(),
        "absent=None".into(),
    );

    // Part 2 — J1939 SPN 183 (formulas.md / ECU.md 5.0 L/h example).
    let mut j = IceDecode::default();
    apply_j1939_spn183(&mut j, 100);
    check(
        &mut ok,
        &mut lines,
        "j1939_spn183",
        j.fuel_rate_l_h == Some(5.0) && j.fuel_rate_source == FuelRateSource::J1939Spn183,
        format!("rate={:?}", j.fuel_rate_l_h),
    );
    let mut jna = IceDecode::default();
    apply_j1939_spn183(&mut jna, 0xFFFF);
    check(
        &mut ok,
        &mut lines,
        "j1939_spn183_na",
        jna.fuel_rate_l_h.is_none(),
        "NA=None".into(),
    );

    // Part 3 — MegaSquirt formulas.md (not ECU.md 1200-duty sketch).
    let mut ms = IceDecode::default();
    apply_megasquirt(&mut ms, 4.0, 3000.0, 4, 250.0, Some(70.0));
    let ms_expected = 4.0 * 3000.0 * 4.0 * 250.0 / 2_000_000.0;
    let ecu_md_duty = (4.0 * 3000.0) / 1200.0;
    let ecu_md_sketch = 250.0 * ecu_md_duty * 4.0 * 0.06 / 1000.0;
    check(
        &mut ok,
        &mut lines,
        "megasquirt_formulas_md",
        ms.fuel_rate_l_h.is_some() && approx(ms.fuel_rate_l_h.unwrap(), ms_expected, 1e-9),
        format!("rate={:?} expected={ms_expected}", ms.fuel_rate_l_h),
    );
    check(
        &mut ok,
        &mut lines,
        "megasquirt_not_ecu_md_sketch",
        (ms_expected - ecu_md_sketch).abs() > 0.1,
        format!("formulas={ms_expected} ecu_md_sketch={ecu_md_sketch}"),
    );
    check(
        &mut ok,
        &mut lines,
        "megasquirt_flex_no_double_scale",
        ms.ethanol_pct == Some(70.0) && approx(ms.fuel_rate_l_h.unwrap(), ms_expected, 1e-9),
        "PW already flexed".into(),
    );

    // Part 4 — AFR-aware MAF (formulas.md density 0.745).
    let maf_rate = maf_to_fuel_rate_l_h(2.0, PETROL_STOICH_AFR, PETROL_DENSITY_KG_L).unwrap();
    let formulas_expected = 2.0 * 3600.0 / (14.7 * 0.745 * 1000.0);
    check(
        &mut ok,
        &mut lines,
        "maf_petrol_formulas",
        approx(maf_rate, formulas_expected, 1e-12),
        format!("rate={maf_rate}"),
    );
    let mut maf = IceDecode::default();
    decode_elm327_mode01("411000C8", &mut maf);
    maf.finish_fuel_rate(IceFuelKind::Petrol);
    check(
        &mut ok,
        &mut lines,
        "maf_pid10_derive",
        maf.maf_g_s == Some(2.0)
            && maf.fuel_rate_source == FuelRateSource::MafDerived
            && maf.fuel_rate_l_h.is_some()
            && approx(maf.fuel_rate_l_h.unwrap(), formulas_expected, 1e-9),
        format!("maf={:?} rate={:?}", maf.maf_g_s, maf.fuel_rate_l_h),
    );

    // Part 5 — diesel never 14.7 without lambda / 5E.
    let diesel_none = derive_maf_fuel_rate(IceFuelKind::Diesel, Some(12.0), None, None);
    check(
        &mut ok,
        &mut lines,
        "diesel_maf_no_lambda",
        diesel_none.is_none(),
        "None".into(),
    );
    let mut d5e = IceDecode::default();
    decode_elm327_mode01("415E0064", &mut d5e);
    d5e.finish_fuel_rate(IceFuelKind::Diesel);
    check(
        &mut ok,
        &mut lines,
        "diesel_pid5e_ok",
        d5e.fuel_rate_l_h == Some(5.0),
        "5E wins".into(),
    );

    // Part 6 — Some(0.0) vs None.
    let mut zero = IceDecode::default();
    decode_elm327_mode01("415E0000", &mut zero);
    check(
        &mut ok,
        &mut lines,
        "valid_zero_rate",
        zero.fuel_rate_l_h == Some(0.0),
        "Some(0.0)".into(),
    );
    check(
        &mut ok,
        &mut lines,
        "missing_not_zero",
        nodata.fuel_rate_l_h.is_none() && fuel_current_l(None, Some(60.0)).is_none(),
        "None".into(),
    );

    // Part 7 — refine_energy_cost public behaviour.
    let predicted = 1.2e6;
    let none_live = refine_energy_cost(predicted, 500.0, None);
    check(
        &mut ok,
        &mut lines,
        "refine_no_live",
        none_live == predicted,
        format!("{none_live}"),
    );
    let snap = LiveEnergySnapshot {
        fuel_rate_l_h: Some(6.5),
        state_of_charge_pct: None,
        power_kw: None,
    };
    let refined = refine_energy_cost(predicted, 500.0, Some(&snap));
    let hours = 500.0 / crate::config::DEFAULT_CRUISE_SPEED_M_S / 3600.0;
    let expected_joules = 6.5 * hours * 36_000_000.0;
    check(
        &mut ok,
        &mut lines,
        "refine_live_rate",
        approx(refined, expected_joules, 1e-6),
        format!("refined={refined} expected={expected_joules}"),
    );
    let snap_missing_rate = LiveEnergySnapshot {
        fuel_rate_l_h: None,
        state_of_charge_pct: None,
        power_kw: None,
    };
    check(
        &mut ok,
        &mut lines,
        "refine_snapshot_without_rate",
        refine_energy_cost(predicted, 500.0, Some(&snap_missing_rate)) == predicted,
        "falls back".into(),
    );
    let snap_zero = LiveEnergySnapshot {
        fuel_rate_l_h: Some(0.0),
        ..snap
    };
    check(
        &mut ok,
        &mut lines,
        "refine_some_zero",
        refine_energy_cost(predicted, 500.0, Some(&snap_zero)) == 0.0,
        "Some(0.0) costs 0".into(),
    );

    // Part 8 — load, flex, L/100km, fuel level, baro.
    let mut load = IceDecode::default();
    decode_elm327_mode01("410480", &mut load);
    decode_elm327_mode01("41430080", &mut load);
    decode_elm327_mode01("4152B3", &mut load);
    decode_elm327_mode01("410D64", &mut load);
    decode_elm327_mode01("412F80", &mut load);
    decode_elm327_mode01("413365", &mut load);
    load.fuel_rate_l_h = Some(8.0);
    load.finish_fuel_rate(IceFuelKind::Petrol);
    let l100 = instant_l_per_100km(load.fuel_rate_l_h, load.speed_kmh);
    let litres = load.fuel_current_l(Some(60.0));
    check(
        &mut ok,
        &mut lines,
        "load_pids",
        load.calc_load_pct.is_some() && load.abs_load_pct.is_some(),
        format!("calc={:?} abs={:?}", load.calc_load_pct, load.abs_load_pct),
    );
    check(
        &mut ok,
        &mut lines,
        "flex_pid52",
        load.ethanol_pct.is_some() && load.ethanol_pct.unwrap() > 60.0,
        format!("e={:?}", load.ethanol_pct),
    );
    check(
        &mut ok,
        &mut lines,
        "l100km",
        l100.is_some() && approx(l100.unwrap(), 8.0, 1e-9),
        format!("{l100:?} at 100 km/h"),
    );
    check(
        &mut ok,
        &mut lines,
        "fuel_level_volume",
        litres.is_some() && (litres.unwrap() - 60.0 * 128.0 / 255.0).abs() < 1e-9,
        format!("{litres:?}"),
    );
    apply_j1939_spn96(&mut load, 125);
    check(
        &mut ok,
        &mut lines,
        "j1939_spn96",
        load.fuel_level_pct.is_some() && (load.fuel_level_pct.unwrap() - 50.0).abs() < 1e-9,
        format!("{:?}", load.fuel_level_pct),
    );
    check(
        &mut ok,
        &mut lines,
        "baro_altitude",
        load.altitude_m.is_some() && load.baro_kpa == Some(101.0),
        format!("h={:?} p={:?}", load.altitude_m, load.baro_kpa),
    );
    check(
        &mut ok,
        &mut lines,
        "range_needs_consumption",
        range_km(Some(30.0), None).is_none() && range_km(Some(30.0), Some(10.0)) == Some(300.0),
        "None vs 300 km".into(),
    );

    if ok {
        lines.push("PASS".into());
    } else {
        lines.push("FAIL".into());
    }
    lines.join("\n") + "\n"
}

pub fn afr_fuel_rate_self_test_passed() -> bool {
    let report = afr_fuel_rate_self_test_report();
    report.trim_end().ends_with("PASS") && !report.contains("FAIL ")
}
