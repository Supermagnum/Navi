//! ICE AFR / fuel-rate decode golden suite (no live adapter).

use driver_break_core::config::Profile;
use driver_break_core::ecu::decode::{
    apply_j1939_lfe_illustrative, apply_megasquirt, decode_elm327_mode01,
    decode_pgn_65266_fuel_rate, elm327_is_no_data, IceDecode,
};
use driver_break_core::ecu::fuel::{
    derive_maf_fuel_rate, megasquirt_fuel_rate_l_h, IceFuelKind, PETROL_DENSITY_KG_L,
    PETROL_STOICH_AFR,
};
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
