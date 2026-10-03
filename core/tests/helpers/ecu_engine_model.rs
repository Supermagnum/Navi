//! Simplified synthetic ICE model for ECU scenario tables.
//!
//! This is **test scaffolding**, not measured vehicle data. Sensor values are
//! encoded as ELM327 Mode 01 (or one J1939 LFE frame) and decoded by Navi.

#![allow(dead_code)]

use driver_break_core::ecu::ambient::baro_kpa_from_altitude_m;
use driver_break_core::ecu::decode::{
    apply_j1939_lfe_illustrative, apply_j1939_spn174, apply_megasquirt_telemetry,
    decode_elm327_mode01, encode_mode01, IceDecode,
};
use driver_break_core::ecu::fuel::{
    fuel_density_kg_l_at, maf_to_fuel_rate_l_h, megasquirt_fuel_rate_l_h, stoich_afr,
    torque_pct_for_fuel_l_h, FuelRateQuality, FuelRateSource, IceFuelKind, LambdaState,
    DEFAULT_LAMBDA_EQ_MAX, DIESEL_STOICH_AFR, PETROL_DENSITY_KG_L, PETROL_STOICH_AFR,
};
use driver_break_core::ecu::megasquirt::{
    decode_ms_realtime_wideband, encode_ms_realtime_test, MsFirmwareKind, MsWidebandSettings,
    MS_WB_AFR_MAX,
};

const R_AIR: f64 = 287.058;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EngineKind {
    NaPetrol,
    TurboPetrol,
    NaDiesel,
    TurboDiesel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FuelBlend {
    E0,
    E10,
    E70,
    E85,
    Diesel,
}

pub struct EngineSpec {
    pub kind: EngineKind,
    pub disp_l: f64,
    pub boost_gauge_kpa: f64,
    pub map_abs_cap_kpa: f64,
}

impl EngineKind {
    pub fn spec(self) -> EngineSpec {
        match self {
            EngineKind::NaPetrol => EngineSpec {
                kind: self,
                disp_l: 1.8,
                boost_gauge_kpa: 0.0,
                map_abs_cap_kpa: 105.0,
            },
            EngineKind::TurboPetrol => EngineSpec {
                kind: self,
                disp_l: 1.8,
                boost_gauge_kpa: 60.0,
                map_abs_cap_kpa: 161.0,
            },
            EngineKind::NaDiesel => EngineSpec {
                kind: self,
                disp_l: 1.9,
                boost_gauge_kpa: 0.0,
                map_abs_cap_kpa: 105.0,
            },
            EngineKind::TurboDiesel => EngineSpec {
                kind: self,
                disp_l: 1.9,
                boost_gauge_kpa: 95.0,
                map_abs_cap_kpa: 196.0,
            },
        }
    }

    pub fn is_diesel(self) -> bool {
        matches!(self, EngineKind::NaDiesel | EngineKind::TurboDiesel)
    }

    pub fn label(self) -> &'static str {
        match self {
            EngineKind::NaPetrol => "NA petrol 1.8 L",
            EngineKind::TurboPetrol => "turbo petrol 1.8 L",
            EngineKind::NaDiesel => "NA diesel 1.9 L",
            EngineKind::TurboDiesel => "turbo diesel 1.9 L",
        }
    }

    pub fn ice_kind(self) -> IceFuelKind {
        if self.is_diesel() {
            IceFuelKind::Diesel
        } else {
            IceFuelKind::Petrol
        }
    }
}

impl FuelBlend {
    pub fn ethanol_pct(self) -> Option<f64> {
        match self {
            FuelBlend::E0 => Some(0.0),
            FuelBlend::E10 => Some(10.0),
            FuelBlend::E70 => Some(70.0),
            FuelBlend::E85 => Some(85.0),
            FuelBlend::Diesel => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            FuelBlend::E0 => "E0",
            FuelBlend::E10 => "E10",
            FuelBlend::E70 => "E70",
            FuelBlend::E85 => "E85",
            FuelBlend::Diesel => "diesel",
        }
    }

    pub fn applies_to(self, engine: EngineKind) -> bool {
        match self {
            FuelBlend::Diesel => engine.is_diesel(),
            _ => !engine.is_diesel(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct OperatingPoint {
    pub engine: EngineKind,
    pub blend: FuelBlend,
    pub rpm: f64,
    pub speed_kmh: f64,
    pub load_pct: f64,
    pub throttle_pct: f64,
    pub coolant_c: f64,
    pub iat_c: f64,
    pub fuel_temp_c: f64,
    pub altitude_m: f64,
    pub overrun: bool,
    pub use_j1939: bool,
    pub use_megasquirt: bool,
    pub ms_seconds: u16,
    pub ms_warmup: bool,
    /// When set, PID 52 uses this ethanol % (flex E70 sample).
    pub ethanol_pct_override: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct Scenario {
    pub table_id: String,
    pub vary_label: String,
    pub example: bool,
    pub op: OperatingPoint,
    pub notes: String,
}

#[derive(Clone, Debug)]
pub struct ScenarioResult {
    pub table_id: String,
    pub engine: EngineKind,
    pub blend: FuelBlend,
    pub vary_label: String,
    pub rpm: f64,
    pub speed_kmh: f64,
    pub maf_g_s: Option<f64>,
    pub lambda: Option<f64>,
    pub afr: Option<f64>,
    pub true_afr: Option<f64>,
    pub true_fuel_l_h: Option<f64>,
    pub lambda_saturated: bool,
    pub lambda_state: LambdaState,
    pub navi_err_pct: Option<f64>,
    pub density: Option<f64>,
    pub fuel_l_h: Option<f64>,
    pub l100: Option<f64>,
    pub source: FuelRateSource,
    pub quality: FuelRateQuality,
    pub naive_l_h: Option<f64>,
    pub naive_err_pct: Option<f64>,
    pub ms_pw_l_h: Option<f64>,
    pub ms_wb_l_h: Option<f64>,
    pub ms_diff_pct: Option<f64>,
    pub fuel_cut: bool,
    pub example: bool,
    pub elm_lines: Vec<String>,
    pub notes: String,
}

fn clamp_u8(v: f64) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

fn clamp_u16(v: f64) -> u16 {
    v.round().clamp(0.0, 65535.0) as u16
}

fn map_kpa(op: &OperatingPoint, baro: f64) -> f64 {
    let spec = op.engine.spec();
    if op.engine.is_diesel() {
        let target =
            (baro + spec.boost_gauge_kpa * (op.load_pct / 100.0)).min(spec.map_abs_cap_kpa);
        target.max(baro)
    } else {
        let wot = (baro + spec.boost_gauge_kpa).min(spec.map_abs_cap_kpa);
        let idle = 32.0_f64.min(baro);
        let t = (op.throttle_pct / 100.0).clamp(0.0, 1.0);
        idle + (wot - idle) * t
    }
}

fn ve(op: &OperatingPoint) -> f64 {
    if op.engine.is_diesel() {
        0.82 + 0.10 * (op.load_pct / 100.0)
    } else {
        0.55 + 0.32 * (op.throttle_pct / 100.0)
    }
}

fn maf_g_s_model(op: &OperatingPoint, baro: f64) -> f64 {
    let map = map_kpa(op, baro);
    let t_k = (op.iat_c + 273.15).max(200.0);
    let vd = op.engine.spec().disp_l * 0.001;
    let cycles = op.rpm / 60.0 / 2.0;
    let kg_s = (map * 1000.0) * vd * cycles * ve(op) / (R_AIR * t_k);
    (kg_s * 1000.0).max(0.05)
}

/// Band midpoints (AFR). `None` means fuel cut.
fn model_afr(op: &OperatingPoint) -> Option<f64> {
    if op.overrun {
        let petrol_cut_ok = !op.engine.is_diesel() && op.coolant_c >= 50.0;
        let diesel_cut = op.engine.is_diesel();
        if petrol_cut_ok || diesel_cut {
            return None;
        }
        // Cold petrol: still injecting; richer than stoich.
        return Some(if op.coolant_c < 0.0 { 12.0 } else { 13.0 });
    }
    if op.speed_kmh < 3.0 && op.rpm < 1200.0 {
        return Some(idle_afr(op));
    }
    if op.load_pct >= 92.0 || op.throttle_pct >= 92.0 {
        return Some(full_load_afr(op));
    }
    Some(cruise_afr(op))
}

fn idle_afr(op: &OperatingPoint) -> f64 {
    if op.engine.is_diesel() {
        match op.engine {
            EngineKind::NaDiesel => interp_coolant(
                op.coolant_c,
                &[
                    (-30.0, 42.5),
                    (0.0, 60.0),
                    (20.0, 77.5),
                    (40.0, 80.0),
                    (90.0, 72.5),
                ],
            ),
            _ => interp_coolant(
                op.coolant_c,
                &[
                    (-30.0, 42.5),
                    (0.0, 60.0),
                    (20.0, 77.5),
                    (40.0, 80.0),
                    (90.0, 80.0),
                ],
            ),
        }
    } else {
        interp_coolant(
            op.coolant_c,
            &[
                (-30.0, 10.0),
                (0.0, 12.0),
                (20.0, 13.0),
                (40.0, 14.1),
                (90.0, 14.7),
            ],
        )
    }
}

fn cruise_afr(op: &OperatingPoint) -> f64 {
    match op.engine {
        EngineKind::NaPetrol | EngineKind::TurboPetrol => 14.7,
        EngineKind::NaDiesel => 36.5,
        EngineKind::TurboDiesel => 40.0,
    }
}

fn full_load_afr(op: &OperatingPoint) -> f64 {
    let peak: f64 = match op.engine {
        EngineKind::NaPetrol => 12.75,
        EngineKind::TurboPetrol => 12.15,
        EngineKind::NaDiesel => 17.0,
        EngineKind::TurboDiesel => 21.0,
    };
    if op.engine == EngineKind::NaDiesel {
        // Smoke limit lambda 1.20 when air is thin/hot.
        let stoich = 14.5;
        peak.max(1.20 * stoich) // 1.20 lambda smoke limit vs band midpoint
    } else {
        peak
    }
}

fn interp_coolant(c: f64, knots: &[(f64, f64)]) -> f64 {
    if c <= knots[0].0 {
        return knots[0].1;
    }
    for w in knots.windows(2) {
        let (x0, y0) = w[0];
        let (x1, y1) = w[1];
        if c <= x1 {
            let t = (c - x0) / (x1 - x0);
            return y0 + t * (y1 - y0);
        }
    }
    knots[knots.len() - 1].1
}

pub fn baseline(engine: EngineKind, blend: FuelBlend) -> OperatingPoint {
    OperatingPoint {
        engine,
        blend,
        rpm: 2500.0,
        speed_kmh: 90.0,
        load_pct: 50.0,
        throttle_pct: if engine.is_diesel() { 50.0 } else { 35.0 },
        coolant_c: 90.0,
        iat_c: 20.0,
        fuel_temp_c: 15.0,
        altitude_m: 0.0,
        overrun: false,
        use_j1939: false,
        use_megasquirt: false,
        ms_seconds: 120,
        ms_warmup: false,
        ethanol_pct_override: None,
    }
}

fn encode_and_decode(op: &OperatingPoint) -> (IceDecode, Vec<String>, bool) {
    let baro = baro_kpa_from_altitude_m(op.altitude_m).unwrap_or(101.325);
    let maf = maf_g_s_model(op, baro);
    let afr_opt = model_afr(op);
    let fuel_cut = afr_opt.is_none();
    let ethanol = op.ethanol_pct_override.or(op.blend.ethanol_pct());
    // Ethanol blends keep the same lambda as E0, not the same AFR.
    let lambda = afr_opt.map(|a| {
        if op.engine.is_diesel() {
            a / DIESEL_STOICH_AFR
        } else {
            a / PETROL_STOICH_AFR
        }
    });
    let mut lines = Vec::new();
    let mut d = IceDecode::default();

    if op.use_j1939 && fuel_cut {
        apply_j1939_lfe_illustrative(&mut d, &[0x00, 0x00]);
        lines.push("J1939 LFE 00 00 (fuel cut)".into());
    } else if op.use_j1939 {
        if let Some(afr) = afr_opt {
            if let Some(rate) = driver_break_core::ecu::fuel::maf_to_fuel_rate_l_h(
                maf,
                afr,
                driver_break_core::ecu::fuel::fuel_density_kg_l_at(
                    op.engine.ice_kind(),
                    op.blend.ethanol_pct(),
                    Some(op.fuel_temp_c),
                ),
            ) {
                let raw = (rate / 0.05).round().clamp(0.0, 65534.0) as u16;
                apply_j1939_lfe_illustrative(&mut d, &[(raw & 0xff) as u8, (raw >> 8) as u8]);
                lines.push(format!("J1939 LFE raw={raw}"));
            }
        }
    }

    let push = |lines: &mut Vec<String>, d: &mut IceDecode, pid: u8, payload: &[u8]| {
        let s = encode_mode01(pid, payload);
        decode_elm327_mode01(&s, d);
        lines.push(s);
    };

    let rpm_raw = clamp_u16(op.rpm * 4.0);
    push(
        &mut lines,
        &mut d,
        0x0C,
        &[(rpm_raw >> 8) as u8, rpm_raw as u8],
    );
    push(&mut lines, &mut d, 0x0D, &[clamp_u8(op.speed_kmh)]);
    let maf_raw = clamp_u16(maf * 100.0);
    push(
        &mut lines,
        &mut d,
        0x10,
        &[(maf_raw >> 8) as u8, maf_raw as u8],
    );
    push(&mut lines, &mut d, 0x05, &[clamp_u8(op.coolant_c + 40.0)]);
    push(&mut lines, &mut d, 0x0F, &[clamp_u8(op.iat_c + 40.0)]);
    if op.engine.is_diesel() {
        push(
            &mut lines,
            &mut d,
            0x5A,
            &[clamp_u8(op.throttle_pct * 255.0 / 100.0)],
        );
    } else {
        push(
            &mut lines,
            &mut d,
            0x11,
            &[clamp_u8(op.throttle_pct * 255.0 / 100.0)],
        );
    }
    push(
        &mut lines,
        &mut d,
        0x04,
        &[clamp_u8(op.load_pct * 255.0 / 100.0)],
    );
    let abs = clamp_u16(op.load_pct * 255.0 / 100.0);
    push(&mut lines, &mut d, 0x43, &[(abs >> 8) as u8, abs as u8]);
    push(&mut lines, &mut d, 0x33, &[clamp_u8(baro)]);
    if let Some(e) = ethanol {
        push(&mut lines, &mut d, 0x52, &[clamp_u8(e * 255.0 / 100.0)]);
    }
    if fuel_cut && !op.use_megasquirt {
        push(&mut lines, &mut d, 0x5E, &[0x00, 0x00]);
        let lam_raw = if op.engine.is_diesel() || op.coolant_c >= 50.0 {
            0xFFFFu16
        } else {
            clamp_u16(12.0 / PETROL_STOICH_AFR / DEFAULT_LAMBDA_EQ_MAX * 65536.0)
        };
        push(
            &mut lines,
            &mut d,
            0x24,
            &[(lam_raw >> 8) as u8, lam_raw as u8],
        );
    } else if !op.use_megasquirt {
        if let Some(lam) = lambda {
            let lam_raw = clamp_u16(lam / DEFAULT_LAMBDA_EQ_MAX * 65536.0);
            push(
                &mut lines,
                &mut d,
                0x24,
                &[(lam_raw >> 8) as u8, lam_raw as u8],
            );
        }
    }
    push(&mut lines, &mut d, 0x4F, &[0x00, 0x00, 0x00, 0x00]);
    let rho = fuel_density_kg_l_at(op.engine.ice_kind(), ethanol, Some(op.fuel_temp_c));
    let maf_d = d.maf_g_s.unwrap_or(maf);
    let true_afr_mass = afr_opt.map(|a| {
        if op.engine.is_diesel() {
            a
        } else {
            (a / PETROL_STOICH_AFR) * stoich_afr(IceFuelKind::Petrol, ethanol)
        }
    });
    let true_fuel = true_afr_mass.and_then(|a| maf_to_fuel_rate_l_h(maf_d, a, rho));
    let tq = if fuel_cut {
        0.0
    } else {
        true_fuel
            .and_then(|tf| {
                torque_pct_for_fuel_l_h(
                    op.engine.ice_kind(),
                    tf,
                    if op.engine.is_diesel() { 210.0 } else { 150.0 },
                    op.rpm,
                    op.speed_kmh,
                    ethanol,
                    Some(op.fuel_temp_c),
                )
            })
            .unwrap_or(op.load_pct.min(99.0))
    };
    push(&mut lines, &mut d, 0x61, &[clamp_u8(tq + 125.0)]);
    push(&mut lines, &mut d, 0x62, &[clamp_u8(tq + 125.0)]);
    let tref = if op.engine.is_diesel() {
        210u16
    } else {
        150u16
    };
    push(&mut lines, &mut d, 0x63, &[(tref >> 8) as u8, tref as u8]);
    let ft_raw = clamp_u8(op.fuel_temp_c + 40.0);
    apply_j1939_spn174(&mut d, ft_raw);
    lines.push(format!("J1939 SPN174 raw={ft_raw}"));
    if op.use_megasquirt {
        let afr_rep = if fuel_cut {
            MS_WB_AFR_MAX
        } else {
            afr_opt.unwrap_or(PETROL_STOICH_AFR)
        };
        let secs = if op.ms_warmup { 10 } else { op.ms_seconds };
        let block = encode_ms_realtime_test(
            MsFirmwareKind::Ms3,
            secs,
            Some(afr_rep),
            Some(afr_rep),
            Some(afr_rep),
        );
        let wb = decode_ms_realtime_wideband(
            "MS3 Format 0262.14",
            &block,
            MsWidebandSettings::default(),
            ethanol,
            Some(op.rpm),
            fuel_cut,
            false,
        );
        let pw = if fuel_cut {
            Some(0.0)
        } else {
            true_fuel.map(|r| r * 2_000_000.0 / (op.rpm * 4.0 * 250.0))
        };
        apply_megasquirt_telemetry(&mut d, pw, op.rpm, 4, Some(250.0), ethanol, wb, fuel_cut);
        d.engine_disp_l = Some(op.engine.spec().disp_l);
        d.ve = Some(ve(op));
        lines.push(format!(
            "MS3 realtime AFR={afr_rep:.2} pw={pw:?} secs={secs}"
        ));
    }
    if !op.use_j1939 || d.fuel_rate_l_h.is_none() {
        d.finish_fuel_rate(op.engine.ice_kind());
    }
    (d, lines, fuel_cut)
}

pub fn run_scenario(sc: &Scenario) -> ScenarioResult {
    let (d, elm_lines, fuel_cut) = encode_and_decode(&sc.op);
    let naive = d
        .maf_g_s
        .and_then(|m| maf_to_fuel_rate_l_h(m, PETROL_STOICH_AFR, PETROL_DENSITY_KG_L));
    let ethanol = sc.op.ethanol_pct_override.or(sc.op.blend.ethanol_pct());
    let baro = baro_kpa_from_altitude_m(sc.op.altitude_m).unwrap_or(101.325);
    let maf_true = maf_g_s_model(&sc.op, baro);
    let true_afr = model_afr(&sc.op).map(|a| {
        if sc.op.engine.is_diesel() {
            a
        } else {
            (a / PETROL_STOICH_AFR) * stoich_afr(IceFuelKind::Petrol, ethanol)
        }
    });
    let rho_true = fuel_density_kg_l_at(sc.op.engine.ice_kind(), ethanol, Some(sc.op.fuel_temp_c));
    let true_fuel = if fuel_cut {
        Some(0.0)
    } else {
        true_afr.and_then(|a| maf_to_fuel_rate_l_h(maf_true, a, rho_true))
    };
    let afr_used = if fuel_cut || !d.lambda_state.usable() {
        None
    } else {
        d.lambda
            .map(|l| l * stoich_afr(sc.op.engine.ice_kind(), ethanol))
    };
    let naive_err = match (true_fuel, naive) {
        (Some(t), Some(n)) if t.abs() > 1e-9 && t.is_finite() && n.is_finite() => {
            Some((n - t) / t * 100.0)
        }
        _ => None,
    };
    let navi_err = match (true_fuel, d.fuel_rate_l_h) {
        (Some(t), Some(n)) if t.abs() > 1e-9 && t.is_finite() && n.is_finite() => {
            Some((n - t) / t * 100.0)
        }
        (Some(0.0), Some(0.0)) => Some(0.0),
        _ => None,
    };
    let density = match d.fuel_rate_source {
        FuelRateSource::MafDerived | FuelRateSource::TorqueBsfc | FuelRateSource::MegaSquirt => {
            Some(fuel_density_kg_l_at(
                sc.op.engine.ice_kind(),
                ethanol,
                d.fuel_temp_c,
            ))
        }
        _ => None,
    };
    let ms_pw_l_h = if sc.op.use_megasquirt {
        d.ms_pw_ms
            .and_then(|pw| megasquirt_fuel_rate_l_h(pw, sc.op.rpm, 4, 250.0))
            .or(d
                .fuel_rate_l_h
                .filter(|_| d.fuel_rate_source == FuelRateSource::MegaSquirt))
    } else {
        None
    };
    let ms_wb_l_h = d.ms_wb_crosscheck_l_h;
    let ms_diff_pct = match (ms_pw_l_h, ms_wb_l_h) {
        (Some(a), Some(b)) if a.abs() > 1e-9 => Some((b - a) / a * 100.0),
        _ => None,
    };
    ScenarioResult {
        table_id: sc.table_id.clone(),
        engine: sc.op.engine,
        blend: sc.op.blend,
        vary_label: sc.vary_label.clone(),
        rpm: d.rpm.unwrap_or(sc.op.rpm),
        speed_kmh: d.speed_kmh.unwrap_or(sc.op.speed_kmh),
        maf_g_s: d.maf_g_s,
        lambda: d.lambda,
        afr: afr_used,
        true_afr,
        true_fuel_l_h: true_fuel,
        lambda_saturated: d.lambda_saturated,
        lambda_state: d.lambda_state,
        navi_err_pct: navi_err,
        density,
        fuel_l_h: d.fuel_rate_l_h,
        l100: d.instant_l_per_100km(),
        source: d.fuel_rate_source,
        quality: d.fuel_rate_quality,
        naive_l_h: naive,
        naive_err_pct: naive_err,
        ms_pw_l_h,
        ms_wb_l_h,
        ms_diff_pct,
        fuel_cut,
        example: sc.example,
        elm_lines,
        notes: sc.notes.clone(),
    }
}

fn sc(
    table: &str,
    vary: String,
    mut op: OperatingPoint,
    example: bool,
    overrun: bool,
    notes: &str,
) -> Scenario {
    op.overrun = overrun;
    if overrun {
        op.throttle_pct = 0.0;
        op.load_pct = 8.0;
    }
    Scenario {
        table_id: table.to_string(),
        vary_label: vary,
        example,
        op,
        notes: notes.to_string(),
    }
}

pub fn all_scenarios() -> Vec<Scenario> {
    let mut out = Vec::new();
    let engines = [
        EngineKind::NaPetrol,
        EngineKind::TurboPetrol,
        EngineKind::NaDiesel,
        EngineKind::TurboDiesel,
    ];
    for eng in engines {
        let blends: &[FuelBlend] = if eng.is_diesel() {
            &[FuelBlend::Diesel]
        } else {
            &[FuelBlend::E0]
        };
        for &blend in blends {
            let b = baseline(eng, blend);
            let prefix = format!("{} / {}", eng.label(), blend.label());

            out.push(sc(
                &format!("{prefix} / load"),
                "load 10%".into(),
                OperatingPoint {
                    load_pct: 10.0,
                    throttle_pct: if eng.is_diesel() { 10.0 } else { 12.0 },
                    ..b.clone()
                },
                false,
                false,
                "",
            ));
            out.push(sc(
                &format!("{prefix} / load"),
                "load 25%".into(),
                OperatingPoint {
                    load_pct: 25.0,
                    throttle_pct: if eng.is_diesel() { 25.0 } else { 20.0 },
                    ..b.clone()
                },
                false,
                false,
                "",
            ));
            out.push(sc(
                &format!("{prefix} / load"),
                "load 50%".into(),
                OperatingPoint {
                    load_pct: 50.0,
                    ..b.clone()
                },
                true,
                false,
                "baseline part load",
            ));
            out.push(sc(
                &format!("{prefix} / load"),
                "load 75%".into(),
                OperatingPoint {
                    load_pct: 75.0,
                    throttle_pct: if eng.is_diesel() { 75.0 } else { 70.0 },
                    ..b.clone()
                },
                false,
                false,
                "",
            ));
            out.push(sc(
                &format!("{prefix} / load"),
                "load 100%".into(),
                OperatingPoint {
                    load_pct: 100.0,
                    throttle_pct: 100.0,
                    ..b.clone()
                },
                false,
                false,
                "",
            ));

            for (lab, thr, ov) in [
                ("throttle 0% (overrun)", 0.0, true),
                ("throttle 10%", 10.0, false),
                ("throttle 25%", 25.0, false),
                ("throttle 50%", 50.0, false),
                ("throttle 75%", 75.0, false),
                ("throttle 100%", 100.0, false),
            ] {
                let mut op = b.clone();
                op.throttle_pct = thr;
                op.load_pct = if ov { 8.0 } else { thr.max(10.0) };
                out.push(sc(
                    &format!("{prefix} / throttle"),
                    lab.into(),
                    op,
                    lab.contains("50%"),
                    ov,
                    "",
                ));
            }

            for (lab, c, ex) in [
                ("coolant -30 C", -30.0, false),
                ("coolant 0 C", 0.0, false),
                ("coolant 20 C", 20.0, false),
                ("coolant 40 C", 40.0, false),
                ("coolant 90 C", 90.0, true),
            ] {
                out.push(sc(
                    &format!("{prefix} / coolant"),
                    lab.into(),
                    OperatingPoint {
                        coolant_c: c,
                        rpm: 900.0,
                        speed_kmh: 0.0,
                        load_pct: 20.0,
                        throttle_pct: 8.0,
                        ..b.clone()
                    },
                    ex,
                    false,
                    "idle; closed-loop cruise would hide coolant enrichment",
                ));
            }

            for (lab, t, ex) in [
                ("IAT -30 C", -30.0, false),
                ("IAT 0 C", 0.0, false),
                ("IAT 20 C", 20.0, true),
                ("IAT 40 C", 40.0, false),
            ] {
                out.push(sc(
                    &format!("{prefix} / intake air"),
                    lab.into(),
                    OperatingPoint {
                        iat_c: t,
                        throttle_pct: 100.0,
                        load_pct: 100.0,
                        ..b.clone()
                    },
                    ex,
                    false,
                    "",
                ));
            }

            for (lab, t, ex) in [
                ("fuel -30 C", -30.0, false),
                ("fuel 0 C", 0.0, false),
                ("fuel 15 C", 15.0, true),
                ("fuel 40 C", 40.0, false),
                ("fuel 60 C", 60.0, false),
            ] {
                out.push(sc(
                    &format!("{prefix} / fuel temp"),
                    lab.into(),
                    OperatingPoint {
                        fuel_temp_c: t,
                        ..b.clone()
                    },
                    ex,
                    false,
                    "",
                ));
            }

            for (lab, h, ex) in [
                ("0 m", 0.0, true),
                ("500 m", 500.0, false),
                ("1000 m", 1000.0, false),
                ("1500 m", 1500.0, false),
                ("2000 m", 2000.0, false),
                ("2500 m", 2500.0, false),
            ] {
                out.push(sc(
                    &format!("{prefix} / altitude"),
                    lab.into(),
                    OperatingPoint {
                        altitude_m: h,
                        throttle_pct: 100.0,
                        load_pct: 100.0,
                        ..b.clone()
                    },
                    ex,
                    false,
                    "",
                ));
            }
        }

        if !eng.is_diesel() {
            for blend in [FuelBlend::E0, FuelBlend::E10, FuelBlend::E85] {
                let b = baseline(eng, blend);
                out.push(sc(
                    &format!("{} / fuel type", eng.label()),
                    blend.label().into(),
                    b,
                    blend == FuelBlend::E0,
                    false,
                    "",
                ));
            }
        }
    }

    // Combined scenarios
    for eng in engines {
        let blend = if eng.is_diesel() {
            FuelBlend::Diesel
        } else {
            FuelBlend::E0
        };
        let b = baseline(eng, blend);
        let p = format!("{} / combined", eng.label());
        out.push(sc(
            &p,
            "cold start idle -30 C".into(),
            OperatingPoint {
                rpm: 900.0,
                speed_kmh: 0.0,
                load_pct: 20.0,
                throttle_pct: 8.0,
                coolant_c: -30.0,
                iat_c: -30.0,
                fuel_temp_c: -30.0,
                ..b.clone()
            },
            true,
            false,
            "",
        ));
        out.push(sc(
            &p,
            "motorway 110 km/h +40 C day".into(),
            OperatingPoint {
                rpm: 2800.0,
                speed_kmh: 110.0,
                load_pct: 45.0,
                iat_c: 40.0,
                coolant_c: 95.0,
                ..b.clone()
            },
            false,
            false,
            "",
        ));
        out.push(sc(
            &p,
            "full load 1500 m at 0 C IAT".into(),
            OperatingPoint {
                load_pct: 100.0,
                throttle_pct: 100.0,
                altitude_m: 1500.0,
                iat_c: 0.0,
                rpm: 3500.0,
                speed_kmh: 80.0,
                ..b.clone()
            },
            false,
            false,
            "",
        ));
        out.push(sc(
            &p,
            "downhill fuel cut, warm".into(),
            OperatingPoint {
                rpm: 2200.0,
                speed_kmh: 80.0,
                altitude_m: 1500.0,
                coolant_c: 90.0,
                ..b.clone()
            },
            false,
            true,
            "",
        ));
        out.push(sc(
            &p,
            "downhill in gear, cold engine".into(),
            OperatingPoint {
                rpm: 2200.0,
                speed_kmh: 80.0,
                altitude_m: 1500.0,
                coolant_c: 0.0,
                ..b.clone()
            },
            false,
            true,
            "",
        ));
        if !eng.is_diesel() {
            out.push(sc(
                &p,
                "E85 at -20 C coolant and fuel".into(),
                OperatingPoint {
                    blend: FuelBlend::E85,
                    coolant_c: -20.0,
                    fuel_temp_c: -20.0,
                    iat_c: -20.0,
                    rpm: 900.0,
                    speed_kmh: 0.0,
                    load_pct: 25.0,
                    ..b.clone()
                },
                false,
                false,
                "",
            ));
        }
    }

    // Flex refuel: two samples E10 then E70
    let a = baseline(EngineKind::NaPetrol, FuelBlend::E10);
    out.push(sc(
        "NA petrol 1.8 L / flex refuel",
        "before refuel E10".into(),
        a.clone(),
        true,
        false,
        "",
    ));
    let mut e70 = baseline(EngineKind::NaPetrol, FuelBlend::E70);
    e70.ethanol_pct_override = Some(70.0);
    out.push(sc(
        "NA petrol 1.8 L / flex refuel",
        "after refuel E70".into(),
        e70,
        false,
        false,
        "PID 52 ethanol 70% after refill",
    ));

    let mut j = baseline(EngineKind::TurboDiesel, FuelBlend::Diesel);
    j.use_j1939 = true;
    out.push(sc(
        "turbo diesel 1.9 L / combined",
        "J1939 LFE truck-style part load".into(),
        j,
        false,
        false,
        "",
    ));

    for eng in [EngineKind::NaPetrol, EngineKind::TurboPetrol] {
        for blend in [FuelBlend::E0, FuelBlend::E10, FuelBlend::E85] {
            let mut b = baseline(eng, blend);
            b.use_megasquirt = true;
            let prefix = format!("{} / MegaSquirt {} ", eng.label(), blend.label());
            for (lab, load, thr) in [
                ("load 10%", 10.0, 12.0),
                ("load 25%", 25.0, 20.0),
                ("load 50%", 50.0, 35.0),
                ("load 75%", 75.0, 70.0),
                ("load 100%", 100.0, 100.0),
            ] {
                let mut op = b.clone();
                op.load_pct = load;
                op.throttle_pct = thr;
                out.push(sc(
                    &format!("{prefix}/ load"),
                    lab.into(),
                    op,
                    lab.contains("50%"),
                    false,
                    "",
                ));
            }
            for (lab, thr, ov) in [
                ("throttle 0% (overrun)", 0.0, true),
                ("throttle 25%", 25.0, false),
                ("throttle 50%", 50.0, false),
                ("throttle 100%", 100.0, false),
            ] {
                let mut op = b.clone();
                op.throttle_pct = thr;
                op.load_pct = if ov { 8.0 } else { thr.max(10.0) };
                out.push(sc(
                    &format!("{prefix}/ throttle"),
                    lab.into(),
                    op,
                    false,
                    ov,
                    "",
                ));
            }
            for (lab, c) in [
                ("coolant -30 C", -30.0),
                ("coolant 0 C", 0.0),
                ("coolant 20 C", 20.0),
                ("coolant 90 C", 90.0),
            ] {
                let mut op = b.clone();
                op.coolant_c = c;
                op.rpm = 900.0;
                op.speed_kmh = 0.0;
                op.load_pct = 20.0;
                op.throttle_pct = 8.0;
                out.push(sc(
                    &format!("{prefix}/ coolant"),
                    lab.into(),
                    op,
                    c == 90.0,
                    false,
                    "idle; cold AFR 9-11 stays inside the 7.4-22.4 wideband",
                ));
            }
            let mut wot = b.clone();
            wot.load_pct = 100.0;
            wot.throttle_pct = 100.0;
            wot.rpm = 3500.0;
            out.push(sc(
                &format!("{prefix}/ WOT"),
                "full throttle enrichment".into(),
                wot,
                true,
                false,
                "",
            ));
            let mut wu = b.clone();
            wu.ms_warmup = true;
            wu.ms_seconds = 10;
            out.push(sc(
                &format!("{prefix}/ warmup"),
                "sensor warm-up 10 s".into(),
                wu,
                false,
                false,
                "wideband not ready; pulse width still the rate",
            ));
        }
    }

    out
}

fn fmt_opt(v: Option<f64>, digits: usize) -> String {
    match v {
        Some(x) if x.is_nan() || x.is_infinite() => "NaN".into(),
        Some(x) => format!("{x:.digits$}"),
        None => "n/a".into(),
    }
}

fn fmt_rate(r: &ScenarioResult) -> String {
    if r.fuel_cut || r.fuel_l_h == Some(0.0) {
        "0.0 (fuel cut)".into()
    } else {
        fmt_opt(r.fuel_l_h, 3)
    }
}

fn fmt_l100(r: &ScenarioResult) -> String {
    if r.speed_kmh < 3.0 {
        "None (standstill)".into()
    } else if r.fuel_cut || r.fuel_l_h == Some(0.0) {
        "0.0 (fuel cut)".into()
    } else {
        fmt_opt(r.l100, 2)
    }
}

fn fmt_err(e: Option<f64>) -> String {
    match e {
        Some(x) if x.is_infinite() => "n/a".into(),
        Some(x) => format!("{x:.1}"),
        None => "n/a".into(),
    }
}

fn source_cell(r: &ScenarioResult) -> String {
    format!("{:?} / {:?}", r.source, r.quality)
}

pub fn render_markdown(rows: &[ScenarioResult]) -> String {
    let mut md = String::new();
    md.push_str("The sensor inputs come from a simplified synthetic engine model built on estimated AFR bands; the decode and fuel-rate numbers are computed by Navi's real code; none of this is measured data from a vehicle.\n\n");
    md.push_str("Petrol/ethanol stoichiometric AFR is mixed by **mass fraction** (E10 ~14.10, E85 ~9.82). PID 24/34/44 report SAE J1979 **lambda** (AFR/AFRstoich, lean greater than 1), despite the standard's 'equivalence ratio' name. Default maximum is 2 (PID 4F byte A = 0). A reading within 1 % of either rail is saturated and is not used as AFR. Diesel idle/cruise then use torque x BSFC. MegaSquirt wideband rails are AFR 7.4-22.4 on the ECU's petrol scale; pulse width is never scaled by lambda or ethanol.\n\n");

    md.push_str("### Summary (min / max across generated scenarios)\n\n");
    md.push_str("| Engine | Fuel | min L/h | max L/h | min L/100 km | max L/100 km | worst naive-14.7 error % |\n");
    md.push_str("| --- | --- | ---: | ---: | ---: | ---: | ---: |\n");
    let mut keys: Vec<(EngineKind, FuelBlend)> = rows.iter().map(|r| (r.engine, r.blend)).collect();
    keys.sort();
    keys.dedup();
    for (eng, blend) in keys {
        let subset: Vec<_> = rows
            .iter()
            .filter(|r| r.engine == eng && r.blend == blend)
            .collect();
        let rates: Vec<f64> = subset
            .iter()
            .filter_map(|r| r.fuel_l_h)
            .filter(|x| x.is_finite())
            .collect();
        let l100s: Vec<f64> = subset
            .iter()
            .filter(|r| r.speed_kmh >= 3.0)
            .filter_map(|r| r.l100)
            .filter(|x| x.is_finite())
            .collect();
        let errs: Vec<f64> = subset
            .iter()
            .filter_map(|r| r.naive_err_pct)
            .filter(|x| x.is_finite())
            .map(|x| x.abs())
            .collect();
        let min_r = rates.iter().cloned().fold(f64::INFINITY, f64::min);
        let max_r = rates.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let min_l = l100s.iter().cloned().fold(f64::INFINITY, f64::min);
        let max_l = l100s.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let worst = errs.iter().cloned().fold(0.0_f64, f64::max);
        md.push_str(&format!(
            "| {} | {} | {:.3} | {:.3} | {:.2} | {:.2} | {:.1} |\n",
            eng.label(),
            blend.label(),
            min_r,
            max_r,
            min_l,
            max_l,
            worst
        ));
    }
    md.push('\n');

    let mut tables: Vec<String> = rows.iter().map(|r| r.table_id.clone()).collect();
    tables.sort();
    tables.dedup();
    for table in tables {
        let subset: Vec<_> = rows.iter().filter(|r| r.table_id == table).collect();
        md.push_str(&format!("### {table}\n\n"));
        let ms_table = table.contains("MegaSquirt");
        if ms_table {
            md.push_str("| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | true AFR (model) | AFR used | lambda state | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | Navi vs true % | naive fixed-14.7 L/h | naive vs true % | PW L/h | WB cross-check L/h | PW vs WB % |\n");
            md.push_str(
                "| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |\n",
            );
        } else {
            md.push_str("| varied input | rpm | speed km/h | MAF g/s (decoded) | lambda (decoded) | true AFR (model) | AFR used | lambda state | fuel density kg/L | fuel rate L/h | L/100 km | rate source and quality | Navi vs true % | naive fixed-14.7 L/h | naive vs true % |\n");
            md.push_str(
                "| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: |\n",
            );
        }
        for r in &subset {
            let mut line = format!(
                "| {} | {:.0} | {:.0} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
                r.vary_label,
                r.rpm,
                r.speed_kmh,
                fmt_opt(r.maf_g_s, 2),
                fmt_opt(r.lambda, 3),
                fmt_opt(r.true_afr, 2),
                fmt_opt(r.afr, 2),
                r.lambda_state.label(),
                fmt_opt(r.density, 3),
                fmt_rate(r),
                fmt_l100(r),
                source_cell(r),
                fmt_err(r.navi_err_pct),
                fmt_opt(r.naive_l_h, 3),
                fmt_err(r.naive_err_pct),
            );
            if ms_table {
                line.push_str(&format!(
                    " {} | {} | {} |",
                    fmt_opt(r.ms_pw_l_h, 3),
                    fmt_opt(r.ms_wb_l_h, 3),
                    fmt_err(r.ms_diff_pct),
                ));
            }
            line.push('\n');
            md.push_str(&line);
        }
        md.push('\n');
        if let Some(ex) = subset.iter().find(|r| r.example) {
            md.push_str("Example chain (this row goes through `decode.rs` then `fuel.rs`):\n\n");
            md.push_str("```\n");
            for line in &ex.elm_lines {
                md.push_str(line);
                md.push('\n');
            }
            md.push_str(&format!(
                "decoded MAF={:?} lambda={:?} coolant/IAT not printed\n",
                fmt_opt(ex.maf_g_s, 2),
                fmt_opt(ex.lambda, 3)
            ));
            if let (Some(maf), Some(afr), Some(rho), Some(rate)) =
                (ex.maf_g_s, ex.afr, ex.density, ex.fuel_l_h)
            {
                md.push_str(&format!(
                    "fuel_l_h = {maf:.2} * 3600 / ({afr:.2} * {rho:.3} * 1000) = {rate:.3} L/h\n"
                ));
            } else if ex.fuel_cut {
                md.push_str(
                    "fuel cut: PID 5E = 0 and/or torque <= 0 while moving -> Some(0.0) L/h\n",
                );
            } else if ex.source == FuelRateSource::TorqueBsfc {
                md.push_str(
                    "torque path: power_kW = torque_pct/100 * Tref * rpm * 2*pi/60 / 1000; fuel_g_h = power * BSFC + idle; L/h = fuel_g_h / (rho*1000)\n",
                );
            }
            md.push_str("```\n\n");
        }
        md.push_str(&trend_sentence(&table, &subset));
        md.push('\n');
    }

    md.push_str("### Inputs with no effect on L/h (when MAF + lambda are both present)\n\n");
    md.push_str("- **Intake air temperature:** the MAF reading is already a mass flow. The same decoded MAF and lambda give the same L/h at -30 C and +40 C IAT; IAT only changes L/h because it changes the modelled (then encoded) MAF.\n");
    md.push_str("- **Altitude / barometric pressure:** same rule. MAF is not pressure-corrected in `fuel.rs`. Altitude changes L/h only by changing the encoded MAF (ideal-gas air mass) and, for NA diesel, the smoke-limited AFR.\n");
    md.push_str("- **Throttle vs pedal:** diesels have no throttle plate; PID 5A/49 is recorded and does not enter the MAF formula.\n");
    md.push_str("- **Saturated / not-ready lambda:** not used as AFR. Diesel idle/cruise go through torque x BSFC so coolant can still move the rate via the encoded torque. MegaSquirt pulse width is the rate even when the wideband is at a rail or warming up.\n");
    md
}

fn pct_drop(a: f64, b: f64) -> f64 {
    if a.abs() < 1e-9 {
        0.0
    } else {
        (a - b) / a * 100.0
    }
}

fn trend_sentence(table: &str, rows: &[&ScenarioResult]) -> String {
    let rates: Vec<f64> = rows.iter().filter_map(|r| r.fuel_l_h).collect();
    if table.contains("altitude") && rates.len() >= 2 {
        let r0 = rows.iter().find(|r| r.vary_label.starts_with("0 m"));
        let r2 = rows.iter().find(|r| r.vary_label.starts_with("1000 m"));
        if let (Some(a), Some(b), Some(ra), Some(rb)) = (
            r0,
            r2,
            r0.and_then(|x| x.fuel_l_h),
            r2.and_then(|x| x.fuel_l_h),
        ) {
            let drop = pct_drop(ra, rb);
            let _ = a;
            let _ = b;
            return format!(
                "From 0 m to 1000 m, decoded fuel rate changes by **{drop:.1}%** (model air mass and, for NA diesel, smoke-limit AFR).\n"
            );
        }
    }
    if table.contains("load") && rates.len() >= 2 {
        if let (Some(lo), Some(hi)) = (rates.first(), rates.last()) {
            return format!(
                "Fuel rate from 10% to 100% load goes **{lo:.3}** to **{hi:.3} L/h** at this rpm.\n"
            );
        }
    }
    if table.contains("MegaSquirt") && table.contains("warmup") {
        return "Wideband is not ready for the first 30 s after start. Pulse width remains the reported rate; AFR is not used.\n".into();
    }
    if table.contains("MegaSquirt") && table.contains("coolant") {
        return "Petrol cold idle AFR 9-11 is inside the MegaSquirt 7.4-22.4 window, so the wideband stays valid; pulse width is still the rate.\n".into();
    }
    if table.contains("MegaSquirt") {
        return "Pulse width is the fuel rate (already flexed). Wideband is HUD plus a cross-check; disagreement above 15 % only downgrades quality.\n".into();
    }
    if table.contains("fuel type") {
        return "Same air mass and lambda: E85 uses more litres than E0 because stoich AFR and density both move toward ethanol.\n".into();
    }
    if table.contains("fuel temp") {
        return "MAF path: hotter fuel is less dense, so the same fuel mass is more litres. Volume PIDs would not show this.\n".into();
    }
    if table.contains("intake air") {
        return "Decoded MAF falls as intake air warms (ideal gas in the model). With MAF+lambda, L/h tracks that MAF; `fuel.rs` does not apply an extra IAT correction.\n".into();
    }
    if table.contains("coolant") {
        return "Petrol cold idle encodes a richer lambda (MAF path). Diesel idle lambda is saturated; L/h follows encoded torque from the model AFR, so coolant is no longer a flat column.\n".into();
    }
    if table.contains("throttle") {
        return "Throttle 0% is overrun: diesels and warm petrol encode PID 5E = 0 (Some(0.0)); cold petrol keeps injecting.\n".into();
    }
    if table.contains("combined") {
        return "Combined points reuse the same decode path: idle L/100 km is None (standstill); downhill cut is 0.0 L/h when evidence is present.\n".into();
    }
    "Rows share the baseline rpm/speed unless the varied input is that axis.\n".into()
}

pub fn generated_section(rows: &[ScenarioResult]) -> String {
    render_markdown(rows)
}
