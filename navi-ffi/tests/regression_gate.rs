//! Host regression gate for long-trip car planning against real packs.
//!
//! Calls `navi::plan_car_route` with the same arguments `MainActivity` passes
//! (eco off, toll allow, long trip on, no country limit, vias as `FfiLatLon`),
//! so the planner, its skeleton freshness check and its options are the app's.
//! Before planning, stale corridor skeletons are rebuilt with
//! `navi::ensure_corridor_skeleton`, the call the app's idle builder makes, so
//! the plans read skeletons of the current build, and the country polygon index
//! is warmed with `navi::warm_country_polys` as the app does at process start.
//! Both run outside the timed plans.
//!
//! Environment:
//! - `NAVI_GATE_PACKS` (required): long-trip pack dir (manifests, graph tiles,
//!   corridor skeletons, `elevation/`), same layout as the device
//!   `long-trip-packs` dir.
//! - `NAVI_GATE_REFS` (required): dir with the external reference lines
//!   (`bad-luster.json`, `roa-florø.json`, `elsa-sjuvass.geojson`). References
//!   named `gate:<file>` are stored with the gate in `tests/regression_gate_refs/`.
//! - `NAVI_GATE_WORK`: scratch dir for data/cache dirs and the result JSON
//!   (default: `target/navi-gate-work`).
//! - `NAVI_GATE_BASELINE`: baseline JSON (default: `tests/regression_gate_baseline.json`).
//! - `NAVI_GATE_WRITE_BASELINE=1`: write this run's wall time and peak memory as
//!   the baseline instead of comparing.
//! - `NAVI_GATE_CASES`: comma list of case ids to run (default: all).
//! - `NAVI_GATE_EMU`: output dir of `scripts/emulator-long-trip-plan.py` for
//!   this build. Its wall time and planning peak memory are stored with the run;
//!   the peak is checked against [`EMU_PEAK_LIMIT_MB`]. Case e also stores an
//!   emulator wall/peak baseline (25 %). Host/emu time rose from 7.3 s / 36.5 s
//!   when corridor decimation was removed. The place index on the pack volume
//!   must exist, be non-empty, pass `quick_check`, and keep every region and
//!   row count in `tests/regression_gate_place_index.json`.
//!
//! Case e (Elsa to Sjuvass) must pass: the plan completes, 0 ferries,
//! intermediate hop ends within 50 m of the coarse-path joint (or an on-path
//! fallback). Accepted path-over-chord windows are the E 45 hook at Sveg and
//! the start stretch the reference shares. Distance vs the 1944.2 km reference,
//! and the unexplained out-and-back at 60.57,9.11, are known failures, not a
//! gate fail.
//!
//! Run: `cargo test --release -p navi-ffi --test regression_gate -- --ignored --nocapture`

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use driver_break_core::routing::path_repair::unexplained_out_and_backs;
use navi::{
    corridor_skeleton_is_ready, ensure_corridor_skeleton, plan_car_route, warm_country_polys,
    FfiLatLon, FfiTollPolicy, FfiVehicleLimits, TravelProfile,
};

const VIA_MAX_M: f64 = 50.0;
const POLY_DISTANCE_TOL: f64 = 0.005;
const SPIKE_RATIO: f64 = 2.5;
const SPIKE_MIN_EXTRA_KM: f64 = 1.5;
const SPIKE_WINDOWS_KM: [f64; 4] = [3.0, 6.0, 12.0, 25.0];
const SPIKE_REF_MATCH_M: f64 = 1000.0;
const RESAMPLE_M: f64 = 100.0;
const BASELINE_REGRESSION: f64 = 1.25;
/// Stored reference lines under `tests/regression_gate_refs/`.
const GATE_REF_PREFIX: &str = "gate:";

/// Planning peak memory bound on the emulator (app running, sidecars ready).
const EMU_PEAK_LIMIT_MB: f64 = 1400.0;

/// Tests outside the gate that are known to fail; listed in every gate report.
const KNOWN_TEST_FAILURES: &[(&str, &str)] = &[];

const BEVENSEN: (f64, f64) = (53.079686, 10.587198);
const VAGAAVEGEN_80: (f64, f64) = (61.8691419, 9.1055130);
const DALSOREN: (f64, f64) = (61.4433766, 7.4614016);
const BRENNERIROA_AGA: (f64, f64) = (60.82718, 11.30278);
const AGA: (f64, f64) = (60.29870, 6.60322);
const BRENNERIROA_FLORO: (f64, f64) = (60.82712, 11.30249);
const GROTLI: (f64, f64) = (62.013569, 7.630359);
const FLORO: (f64, f64) = (61.60145, 5.02658);
const ELSA: (f64, f64) = (69.9742, 29.63342);
const SJUVASS: (f64, f64) = (59.80326, 9.39866);

const OTTA: (f64, f64) = (61.7727, 9.5404);
const LOM: (f64, f64) = (61.8381, 8.5676);
const TOWN_PASS_M: f64 = 1500.0;
const RV15_MIN_SHARE: f64 = 0.90;
/// OSM name of Rv 15 between Otta and Lom; sim-sample street labels prefer the
/// name over the ref.
const RV15_OTTA_LOM_NAME: &str = "Ottadalsvegen";

struct Case {
    id: &'static str,
    start: (f64, f64),
    vias: &'static [(f64, f64)],
    end: (f64, f64),
    pbf_stem: &'static str,
    avoid_ferries: bool,
    /// Exact ferry count and the terminal names each ferry must contain.
    ferries: &'static [&'static [&'static str]],
    /// (reference km, relative tolerance).
    distance: Option<(f64, f64)>,
    rv15_otta_vaga_lom: bool,
    reference: Option<&'static str>,
    expected_fail: Option<&'static str>,
    /// Path-over-chord windows at these points are required and accepted.
    accepted_spikes: &'static [(f64, f64)],
    /// If set, a miss vs [`Case::distance`] is tracked, not a gate failure.
    known_distance: Option<&'static str>,
    /// If set, path-over-chord windows that are not on the reference and not
    /// in [`Case::accepted_spikes`], and accepted windows the route misses,
    /// are tracked, not a gate failure.
    known_spikes: Option<&'static str>,
    /// Cold runs timed; the wall-time check uses their median.
    timing_runs: usize,
}

const CASES: &[Case] = &[
    Case {
        id: "a_bevensen_vaga_dalsoren",
        start: BEVENSEN,
        vias: &[VAGAAVEGEN_80],
        end: DALSOREN,
        pbf_stem: "vestlandet-latest",
        avoid_ferries: false,
        ferries: &[&["puttgarden", "rødby"]],
        distance: Some((1440.985, 0.02)),
        rv15_otta_vaga_lom: true,
        reference: Some("bad-luster.json"),
        expected_fail: None,
        accepted_spikes: &[],
        known_distance: None,
        known_spikes: None,
        timing_runs: 3,
    },
    Case {
        id: "b_brenneriroa_aga",
        start: BRENNERIROA_AGA,
        vias: &[],
        end: AGA,
        pbf_stem: "vestlandet-latest",
        avoid_ferries: false,
        ferries: &[&["kinsarvik", "utne"]],
        distance: None,
        rv15_otta_vaga_lom: false,
        // Approved Navi route (Kinsarvik - Utne, 375.3 km).
        reference: Some("gate:brenneriroa-aga-fu20.geojson"),
        expected_fail: None,
        accepted_spikes: &[],
        known_distance: None,
        known_spikes: None,
        timing_runs: 1,
    },
    Case {
        id: "c_brenneriroa_grotli_floro",
        start: BRENNERIROA_FLORO,
        vias: &[GROTLI],
        end: FLORO,
        pbf_stem: "vestlandet-latest",
        avoid_ferries: false,
        ferries: &[],
        distance: Some((524.2, 0.05)),
        rv15_otta_vaga_lom: false,
        reference: Some("roa-florø.json"),
        expected_fail: None,
        accepted_spikes: &[],
        known_distance: None,
        known_spikes: None,
        timing_runs: 1,
    },
    Case {
        id: "d_bevensen_vaga_dalsoren_avoid_ferries",
        start: BEVENSEN,
        vias: &[VAGAAVEGEN_80],
        end: DALSOREN,
        pbf_stem: "vestlandet-latest",
        avoid_ferries: true,
        ferries: &[],
        distance: None,
        rv15_otta_vaga_lom: false,
        // Same destination as case a; shared roads keep their own spikes.
        reference: Some("bad-luster.json"),
        expected_fail: None,
        accepted_spikes: &[],
        known_distance: None,
        known_spikes: None,
        timing_runs: 1,
    },
    Case {
        id: "e_elsa_sjuvass",
        start: ELSA,
        vias: &[],
        end: SJUVASS,
        pbf_stem: "ostlandet-latest",
        avoid_ferries: false,
        ferries: &[],
        distance: Some((1944.2, 0.02)),
        rv15_otta_vaga_lom: false,
        reference: Some("elsa-sjuvass.geojson"),
        expected_fail: None,
        accepted_spikes: &[(69.69, 29.37), (62.04, 14.39)],
        known_distance: Some("distance against the 1944.2 km reference"),
        known_spikes: Some("unexplained out-and-back at 60.57,9.11"),
        timing_runs: 1,
    },
];

#[derive(Default)]
struct Outcome {
    pass: bool,
    failures: Vec<String>,
    known: Vec<String>,
    distance_km: f64,
    poly_km: f64,
    eta_min: f64,
    ferries: Vec<String>,
    via_m: Vec<f64>,
    spikes: Vec<String>,
    wall_s: f64,
    /// Wall time of every timed run; [`Outcome::wall_s`] is their median.
    wall_runs_s: Vec<f64>,
    peak_mb: f64,
    terminate: String,
}

fn empty_vehicle() -> FfiVehicleLimits {
    FfiVehicleLimits {
        axle_weight_kg: None,
        bogie_weight_kg: None,
        height_m: None,
        width_m: None,
        length_m: None,
        total_weight_kg: None,
    }
}

fn haversine_m(a: (f64, f64), b: (f64, f64)) -> f64 {
    let r = 6_371_008.8_f64;
    let (la1, lo1) = (a.0.to_radians(), a.1.to_radians());
    let (la2, lo2) = (b.0.to_radians(), b.1.to_radians());
    let h = ((la2 - la1) / 2.0).sin().powi(2)
        + la1.cos() * la2.cos() * ((lo2 - lo1) / 2.0).sin().powi(2);
    2.0 * r * h.sqrt().asin()
}

/// Distance from `p` to segment `a`-`b`, local equirectangular projection.
fn point_segment_m(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let k = p.0.to_radians().cos();
    let m_per_deg = 111_195.0;
    let ax = (a.1 - p.1) * k * m_per_deg;
    let ay = (a.0 - p.0) * m_per_deg;
    let bx = (b.1 - p.1) * k * m_per_deg;
    let by = (b.0 - p.0) * m_per_deg;
    let (dx, dy) = (bx - ax, by - ay);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 {
        (-(ax * dx + ay * dy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (cx, cy) = (ax + t * dx, ay + t * dy);
    (cx * cx + cy * cy).sqrt()
}

/// `"lon,lat;lon,lat;..."` to `(lat, lon)` points.
fn parse_polyline(s: &str) -> Vec<(f64, f64)> {
    s.split(';')
        .filter_map(|p| {
            let mut it = p.split(',');
            let lon: f64 = it.next()?.trim().parse().ok()?;
            let lat: f64 = it.next()?.trim().parse().ok()?;
            Some((lat, lon))
        })
        .collect()
}

fn line_length_m(pts: &[(f64, f64)]) -> f64 {
    pts.windows(2).map(|w| haversine_m(w[0], w[1])).sum()
}

/// Nearest distance from `p` to the line, with the index of the segment start.
fn nearest_on_line(p: (f64, f64), pts: &[(f64, f64)]) -> (f64, usize) {
    let mut best = (f64::INFINITY, 0usize);
    for (i, w) in pts.windows(2).enumerate() {
        let d = point_segment_m(p, w[0], w[1]);
        if d < best.0 {
            best = (d, i);
        }
    }
    best
}

/// First LineString `coordinates` array found in a GeoJSON or ORS export.
fn reference_line(path: &Path) -> Result<Vec<(f64, f64)>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let v: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    fn find(v: &serde_json::Value) -> Option<Vec<(f64, f64)>> {
        match v {
            serde_json::Value::Object(m) => {
                if m.get("type").and_then(|t| t.as_str()) == Some("LineString") {
                    if let Some(serde_json::Value::Array(cs)) = m.get("coordinates") {
                        let pts: Vec<(f64, f64)> = cs
                            .iter()
                            .filter_map(|c| {
                                let a = c.as_array()?;
                                Some((a.get(1)?.as_f64()?, a.first()?.as_f64()?))
                            })
                            .collect();
                        if pts.len() > 1 {
                            return Some(pts);
                        }
                    }
                }
                m.values().find_map(find)
            }
            serde_json::Value::Array(a) => a.iter().find_map(find),
            _ => None,
        }
    }
    find(&v).ok_or_else(|| format!("{}: no LineString", path.display()))
}

/// Points every `step_m` along the line with their cumulative distance.
fn resample(pts: &[(f64, f64)], step_m: f64) -> Vec<((f64, f64), f64)> {
    let mut out = Vec::new();
    if pts.is_empty() {
        return out;
    }
    out.push((pts[0], 0.0));
    let mut cum = 0.0;
    let mut next = step_m;
    for w in pts.windows(2) {
        let seg = haversine_m(w[0], w[1]);
        while seg > 0.0 && cum + seg >= next {
            let t = (next - cum) / seg;
            let p = (
                w[0].0 + (w[1].0 - w[0].0) * t,
                w[0].1 + (w[1].1 - w[0].1) * t,
            );
            out.push((p, next));
            next += step_m;
        }
        cum += seg;
    }
    if let Some(last) = pts.last() {
        out.push((*last, cum));
    }
    out
}

struct Spike {
    from_km: f64,
    to_km: f64,
    path_km: f64,
    chord_km: f64,
    a: (f64, f64),
    /// Midpoint (by path) of the worst window.
    mid: (f64, f64),
    /// Resampled points over the merged span.
    span: Vec<(f64, f64)>,
}

/// Windows where path over chord is at or above `SPIKE_RATIO` with at least
/// `SPIKE_MIN_EXTRA_KM` extra; overlapping windows merge into one spike that
/// keeps its worst window.
fn find_spikes(pts: &[(f64, f64)]) -> Vec<Spike> {
    let rs = resample(pts, RESAMPLE_M);
    let mut raw: Vec<Spike> = Vec::new();
    for w_km in SPIKE_WINDOWS_KM {
        let w_m = w_km * 1000.0;
        let mut j = 0usize;
        for i in 0..rs.len() {
            if j < i {
                j = i;
            }
            while j + 1 < rs.len() && rs[j].1 - rs[i].1 < w_m {
                j += 1;
            }
            let path = rs[j].1 - rs[i].1;
            if path < w_m * 0.99 {
                break;
            }
            let chord = haversine_m(rs[i].0, rs[j].0);
            if path >= SPIKE_RATIO * chord && path - chord >= SPIKE_MIN_EXTRA_KM * 1000.0 {
                let half = rs[i].1 + path / 2.0;
                let mid = rs[i..=j]
                    .iter()
                    .find(|p| p.1 >= half)
                    .map(|p| p.0)
                    .unwrap_or(rs[j].0);
                raw.push(Spike {
                    from_km: rs[i].1 / 1000.0,
                    to_km: rs[j].1 / 1000.0,
                    path_km: path / 1000.0,
                    chord_km: chord / 1000.0,
                    a: rs[i].0,
                    mid,
                    span: Vec::new(),
                });
            }
        }
    }
    raw.sort_by(|x, y| x.from_km.total_cmp(&y.from_km));
    let mut merged: Vec<Spike> = Vec::new();
    for s in raw {
        if let Some(m) = merged.last_mut() {
            if s.from_km <= m.to_km {
                let worse = s.path_km - s.chord_km > m.path_km - m.chord_km;
                let from = m.from_km.min(s.from_km);
                let to = m.to_km.max(s.to_km);
                if worse {
                    *m = s;
                }
                m.from_km = from;
                m.to_km = to;
                continue;
            }
        }
        merged.push(s);
    }
    for m in &mut merged {
        m.span = rs
            .iter()
            .filter(|p| p.1 >= m.from_km * 1000.0 && p.1 <= m.to_km * 1000.0)
            .map(|p| p.0)
            .collect();
    }
    merged
}

/// True when the reference line has its own spike (same rule) within
/// `SPIKE_REF_MATCH_M` of this spike's worst window.
fn reference_has_spike(s: &Spike, reference_spikes: &[Spike]) -> bool {
    reference_spikes.iter().any(|r| {
        r.span
            .iter()
            .any(|&p| haversine_m(p, s.mid) <= SPIKE_REF_MATCH_M)
    })
}

/// Distinct ferries in the report: every `route_ferry_fp=` entry (hop and
/// trip lines), deduped by the unordered terminal pair, plus one unnamed entry
/// per report section that counts ferry legs without a fingerprint.
fn ferries_from_report(report: &str) -> Vec<String> {
    let mut named: BTreeMap<String, String> = BTreeMap::new();
    let mut unnamed = 0usize;
    let mut section_legs: Option<u64> = None;
    let mut section_fp: Option<String> = None;
    fn close(legs: &mut Option<u64>, fp: &mut Option<String>, unnamed: &mut usize) {
        if legs.unwrap_or(0) > 0 && fp.as_deref().map(str::is_empty).unwrap_or(true) {
            *unnamed += 1;
        }
        *legs = None;
        *fp = None;
    }
    for line in report.lines() {
        if line.starts_with("--- leg") {
            close(&mut section_legs, &mut section_fp, &mut unnamed);
            continue;
        }
        if let Some(v) = line.strip_prefix("route_ferry_legs=") {
            if section_legs.is_none() {
                section_legs = v.trim().parse().ok();
            }
        }
        if let Some(v) = line.strip_prefix("route_ferry_fp=") {
            if section_fp.is_none() {
                section_fp = Some(v.trim().to_string());
            }
            for part in v.split('|') {
                let label = part.split('@').next().unwrap_or("").trim();
                if label.is_empty() {
                    continue;
                }
                let mut ends: Vec<String> = label
                    .split(" - ")
                    .map(|e| e.trim().to_lowercase())
                    .collect();
                ends.sort();
                named
                    .entry(ends.join(" | "))
                    .or_insert_with(|| label.to_string());
            }
        }
    }
    close(&mut section_legs, &mut section_fp, &mut unnamed);
    let mut out: Vec<String> = named.into_values().collect();
    // The trip summary repeats hop fingerprints; an unnamed hop is only extra
    // when no named ferry explains it.
    for i in 0..unnamed.saturating_sub(out.len()) {
        out.push(format!("unnamed ferry {}", i + 1));
    }
    out
}

/// Street labels by sample distance between the samples nearest Otta and Lom.
fn rv15_share(sim_samples_json: &str) -> Result<(f64, Vec<(String, f64)>), String> {
    let v: serde_json::Value =
        serde_json::from_str(sim_samples_json).map_err(|e| format!("sim_samples_json: {e}"))?;
    let arr = v.as_array().ok_or("sim_samples_json is not an array")?;
    let samples: Vec<((f64, f64), f64, String)> = arr
        .iter()
        .filter_map(|s| {
            Some((
                (s.get("lat")?.as_f64()?, s.get("lon")?.as_f64()?),
                s.get("cum_m")?.as_f64()?,
                s.get("street")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
            ))
        })
        .collect();
    if samples.len() < 2 {
        return Err("no sim samples".into());
    }
    let nearest = |p: (f64, f64)| {
        samples
            .iter()
            .enumerate()
            .map(|(i, s)| (haversine_m(p, s.0), i))
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .unwrap()
    };
    let (d_otta, i_otta) = nearest(OTTA);
    let (d_lom, i_lom) = nearest(LOM);
    if d_otta > TOWN_PASS_M || d_lom > TOWN_PASS_M || i_otta >= i_lom {
        return Err(format!(
            "route does not pass Otta then Lom (Otta {d_otta:.0} m idx {i_otta}, Lom {d_lom:.0} m idx {i_lom})"
        ));
    }
    let mut by_label: BTreeMap<String, f64> = BTreeMap::new();
    let mut total = 0.0;
    let mut rv15 = 0.0;
    for w in samples[i_otta..=i_lom].windows(2) {
        let d = (w[1].1 - w[0].1).max(0.0);
        total += d;
        *by_label.entry(w[0].2.clone()).or_default() += d;
        let on_15 = w[0].2 == RV15_OTTA_LOM_NAME
            || w[0]
                .2
                .split(['/', ';', ',', ' '])
                .any(|t| t == "15" || t.eq_ignore_ascii_case("Rv15"));
        if on_15 {
            rv15 += d;
        }
    }
    let mut labels: Vec<(String, f64)> = by_label.into_iter().collect();
    labels.sort_by(|a, b| b.1.total_cmp(&a.1));
    Ok((if total > 0.0 { rv15 / total } else { 0.0 }, labels))
}

/// Differences between what the gate sent and the planner's `plan_inputs` line.
fn plan_inputs_mismatch(report: &str, case: &Case) -> Vec<String> {
    let Some(line) = report.lines().find(|l| l.starts_with("plan_inputs ")) else {
        return vec!["no plan_inputs line in the report".into()];
    };
    let field = |k: &str| {
        line.split_whitespace()
            .find_map(|t| t.strip_prefix(&format!("{k}=")))
            .unwrap_or("")
            .to_string()
    };
    let mut out = Vec::new();
    let want = [
        ("profile", "Car".to_string()),
        ("eco", "false".to_string()),
        ("avoid_ferries", case.avoid_ferries.to_string()),
        ("long_trip", "true".to_string()),
        ("vias", case.vias.len().to_string()),
    ];
    for (k, v) in want {
        if field(k) != v {
            out.push(format!("plan_inputs {k}={} (sent {v})", field(k)));
        }
    }
    let got: Vec<(f64, f64)> = field("via_coords")
        .split(';')
        .filter_map(|p| {
            let (a, b) = p.split_once(',')?;
            Some((a.parse().ok()?, b.parse().ok()?))
        })
        .collect();
    let same = got.len() == case.vias.len()
        && got
            .iter()
            .zip(case.vias)
            .all(|(g, w)| (g.0 - w.0).abs() < 1e-5 && (g.1 - w.1).abs() < 1e-5);
    if !same {
        out.push(format!(
            "plan_inputs via_coords={} (sent {:?})",
            field("via_coords"),
            case.vias
        ));
    }
    out
}

fn read_proc_kb(key: &str) -> Option<f64> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    s.lines()
        .find(|l| l.starts_with(key))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

fn reset_peak_rss() {
    let _ = std::fs::write("/proc/self/clear_refs", "5");
}

fn snap_end_dist_m(leg: &str) -> Option<f64> {
    let after = leg.split_once("snap_end=")?.1;
    let rest = after.split_once("dist_m=")?.1;
    let end = rest
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// Intermediate hop ends must stay within 50 m of the intended joint, or log
/// an on-path component fallback. The last hop is the user destination.
fn check_intermediate_hop_ends(report: &str, o: &mut Outcome) {
    let mut legs = Vec::new();
    let mut rest = report;
    while let Some(i) = rest.find("--- leg") {
        let from = i;
        let search = &rest[from + 7..];
        if let Some(j) = search.find("--- leg") {
            legs.push(&rest[from..from + 7 + j]);
            rest = &rest[from + 7 + j..];
        } else {
            legs.push(&rest[from..]);
            break;
        }
    }
    if legs.len() < 2 {
        return;
    }
    for (i, leg) in legs.iter().enumerate() {
        if i + 1 == legs.len() {
            continue;
        }
        let fallback = leg.contains("hop_end_fallback=");
        let Some(dist) = snap_end_dist_m(leg) else {
            o.failures.push(format!("hop {} missing snap_end", i + 1));
            continue;
        };
        if dist > 50.0 && !fallback {
            o.failures.push(format!(
                "hop {} end snapped {dist:.0} m (max 50 m, no on-path fallback)",
                i + 1
            ));
        }
    }
}

fn run_case(case: &Case, packs: &Path, refs: &Path, work: &Path) -> Outcome {
    let mut o = Outcome::default();
    let data_dir = work.join(format!("data-{}", case.id));
    let cache_dir = work.join(format!("cache-{}", case.id));
    let _ = std::fs::create_dir_all(&data_dir);
    let _ = std::fs::create_dir_all(&cache_dir);
    let elev = packs.join("elevation");
    let pbf = packs.join(format!("{}.osm.pbf", case.pbf_stem));
    let vias: Vec<FfiLatLon> = case
        .vias
        .iter()
        .map(|v| FfiLatLon { lat: v.0, lon: v.1 })
        .collect();

    reset_peak_rss();
    let t0 = Instant::now();
    let r = plan_car_route(
        pbf.display().to_string(),
        elev.display().to_string(),
        cache_dir.display().to_string(),
        case.start.0,
        case.start.1,
        case.end.0,
        case.end.1,
        false,
        TravelProfile::Car,
        false,
        FfiTollPolicy::Allow,
        case.avoid_ferries,
        false,
        empty_vehicle(),
        false,
        data_dir.display().to_string(),
        packs.display().to_string(),
        true,
        None,
        vias,
    );
    o.wall_s = t0.elapsed().as_secs_f64();
    o.peak_mb = read_proc_kb("VmHWM:").unwrap_or(0.0) / 1024.0;
    o.distance_km = r.distance_km;
    o.eta_min = r.eta_minutes;
    o.terminate = r.search_terminate_reason.clone();
    let _ = std::fs::write(work.join(format!("{}.report.txt", case.id)), &r.report);
    let _ = std::fs::write(
        work.join(format!("{}.polyline.txt", case.id)),
        &r.route_polyline,
    );

    o.failures.extend(plan_inputs_mismatch(&r.report, case));
    let line = parse_polyline(&r.route_polyline);
    if !r.report.contains("PASS") || line.len() < 2 {
        o.failures.push(format!(
            "plan did not pass (terminate={}, points={})",
            r.search_terminate_reason,
            line.len()
        ));
        return o;
    }

    for (k, v) in case.vias.iter().enumerate() {
        let (d, _) = nearest_on_line(*v, &line);
        o.via_m.push(d);
        if d > VIA_MAX_M {
            o.failures.push(format!(
                "via {} at {:.6},{:.6} is {:.0} m from the route (max {VIA_MAX_M:.0} m)",
                k + 1,
                v.0,
                v.1,
                d
            ));
        }
    }

    o.poly_km = line_length_m(&line) / 1000.0;
    if o.poly_km > 0.0 {
        let rel = (o.distance_km - o.poly_km).abs() / o.poly_km;
        if rel > POLY_DISTANCE_TOL {
            o.failures.push(format!(
                "reported {:.1} km vs polyline {:.1} km ({:.2} %, max {:.1} %)",
                o.distance_km,
                o.poly_km,
                rel * 100.0,
                POLY_DISTANCE_TOL * 100.0
            ));
        }
    }

    o.ferries = ferries_from_report(&r.report);
    if o.ferries.len() != case.ferries.len() {
        o.failures.push(format!(
            "{} ferries {:?}, expected {}",
            o.ferries.len(),
            o.ferries,
            case.ferries.len()
        ));
    } else {
        for want in case.ferries {
            let hit = o.ferries.iter().any(|f| {
                let f = f.to_lowercase();
                want.iter().all(|w| f.contains(w))
            });
            if !hit {
                o.failures
                    .push(format!("no ferry matching {want:?} in {:?}", o.ferries));
            }
        }
    }

    if let Some((ref_km, tol)) = case.distance {
        let rel = (o.distance_km - ref_km).abs() / ref_km;
        if rel > tol {
            let msg = format!(
                "distance {:.1} km is {:.2} % from {ref_km} km (max {:.0} %)",
                o.distance_km,
                rel * 100.0,
                tol * 100.0
            );
            if let Some(why) = case.known_distance {
                o.known.push(format!("{msg}; {why}"));
            } else {
                o.failures.push(msg);
            }
        }
    }

    check_intermediate_hop_ends(&r.report, &mut o);

    if case.rv15_otta_vaga_lom {
        let (dv, iv) = nearest_on_line(VAGAAVEGEN_80, &line);
        let (dotta, iotta) = nearest_on_line(OTTA, &line);
        let (dlom, ilom) = nearest_on_line(LOM, &line);
        if dotta > TOWN_PASS_M
            || dlom > TOWN_PASS_M
            || dv > TOWN_PASS_M
            || !(iotta <= iv && iv <= ilom)
        {
            o.failures.push(format!(
                "route does not run Otta ({dotta:.0} m) then Vaga ({dv:.0} m) then Lom ({dlom:.0} m) in order"
            ));
        }
        match rv15_share(&r.sim_samples_json) {
            Ok((share, labels)) => {
                let top: Vec<String> = labels
                    .iter()
                    .take(6)
                    .map(|(l, m)| format!("{l:?} {:.1} km", m / 1000.0))
                    .collect();
                eprintln!(
                    "[gate] {} Otta-Lom on ref 15: {:.1} %; labels {}",
                    case.id,
                    share * 100.0,
                    top.join(", ")
                );
                if share < RV15_MIN_SHARE {
                    o.failures.push(format!(
                        "Otta to Lom only {:.1} % on Rv 15 (min {:.0} %): {}",
                        share * 100.0,
                        RV15_MIN_SHARE * 100.0,
                        top.join(", ")
                    ));
                }
            }
            Err(e) => o.failures.push(format!("Rv 15 check: {e}")),
        }
    }

    let gate_refs = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/regression_gate_refs");
    let reference = case
        .reference
        .map(|f| match f.strip_prefix(GATE_REF_PREFIX) {
            Some(stored) => reference_line(&gate_refs.join(stored)),
            None => reference_line(&refs.join(f)),
        });
    let reference = match reference {
        Some(Ok(l)) => Some(l),
        Some(Err(e)) => {
            o.failures.push(format!("reference line: {e}"));
            None
        }
        None => None,
    };
    let reference_spikes = reference.as_deref().map(find_spikes).unwrap_or_default();
    let mut accepted_hit = vec![false; case.accepted_spikes.len()];
    for s in find_spikes(&line) {
        let shared = reference_has_spike(&s, &reference_spikes);
        let accepted_at = case
            .accepted_spikes
            .iter()
            .position(|&p| haversine_m(s.a, p) <= 8_000.0 || haversine_m(s.mid, p) <= 8_000.0);
        if let Some(i) = accepted_at {
            accepted_hit[i] = true;
        }
        let desc = format!(
            "km {:.1}-{:.1} path {:.2} km chord {:.2} km at {:.5},{:.5}{}",
            s.from_km,
            s.to_km,
            s.path_km,
            s.chord_km,
            s.a.0,
            s.a.1,
            if shared { " (reference too)" } else { "" }
        );
        if !shared && accepted_at.is_none() {
            let msg = format!("spike {desc}");
            if let Some(why) = case.known_spikes {
                o.known.push(format!("{msg}; {why}"));
            } else {
                o.failures.push(msg);
            }
        }
        o.spikes.push(desc);
    }
    if !case.accepted_spikes.is_empty() {
        for (i, &p) in case.accepted_spikes.iter().enumerate() {
            if !accepted_hit[i] {
                let msg = format!(
                    "accepted path-over-chord window at {:.2},{:.2} is missing",
                    p.0, p.1
                );
                if let Some(why) = case.known_spikes {
                    o.known.push(format!("{msg}; {why}"));
                } else {
                    o.failures.push(msg);
                }
            }
        }
    }

    let vias: Vec<(f64, f64)> = case.vias.to_vec();
    for b in unexplained_out_and_backs(&line, &vias) {
        let desc = format!(
            "km {:.1}-{:.1} path {:.2} km turn {:.5},{:.5}",
            b.from_km, b.to_km, b.path_km, b.turnaround.0, b.turnaround.1
        );
        let at_known_e =
            (b.turnaround.0 - 60.57).abs() < 0.05 && (b.turnaround.1 - 9.11).abs() < 0.05;
        if at_known_e {
            if let Some(why) = case.known_spikes {
                o.known.push(format!("out-and-back {desc}; {why}"));
            } else {
                o.failures.push(format!("out-and-back {desc}"));
            }
        } else {
            o.failures.push(format!("out-and-back {desc}"));
        }
        o.spikes.push(format!("out-and-back {desc}"));
    }

    o.pass = o.failures.is_empty();
    o
}

/// Rebuild every installed pack's stale car skeleton, as the app does when idle.
fn prepare_skeletons(packs: &Path) {
    let mut stems: Vec<String> = std::fs::read_dir(packs)
        .expect("read NAVI_GATE_PACKS")
        .flatten()
        .filter_map(|e| {
            e.file_name()
                .to_string_lossy()
                .strip_suffix(".navi-manifest.json")
                .map(str::to_string)
        })
        .collect();
    stems.sort();
    for stem in stems {
        let dir = packs.display().to_string();
        if corridor_skeleton_is_ready(dir.clone(), stem.clone(), TravelProfile::Car) {
            continue;
        }
        let t0 = Instant::now();
        let r = ensure_corridor_skeleton(dir, stem.clone(), TravelProfile::Car);
        eprintln!(
            "[gate] skeleton build {stem}: {:.1} s: {}",
            t0.elapsed().as_secs_f64(),
            r.trim()
        );
    }
}

/// Emulator figures for this build from the harness `result.json` in `dir`:
/// wall time and planning peak memory, with the peak checked against
/// [`EMU_PEAK_LIMIT_MB`]. Case e also has a stored emu wall/peak baseline
/// (25 %); host/emu time rose from 7.3 s / 36.5 s when corridor decimation
/// was removed.
fn emulator_check(
    dir: &Path,
    gate_failures: &mut Vec<&'static str>,
    baseline: &serde_json::Value,
) -> serde_json::Value {
    let path = dir.join("result.json");
    let r: serde_json::Value = match std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
    {
        Some(r) => r,
        None => {
            eprintln!("[gate] emulator FAIL: cannot read {}", path.display());
            gate_failures.push("emulator");
            return serde_json::json!({ "status": "FAIL", "failures": ["no result.json"] });
        }
    };
    let mut failures = Vec::new();
    if r["accepted"].as_bool() != Some(true) || r["status"].as_str() != Some("done") {
        failures.push(format!(
            "run not accepted (status {}, mismatch {})",
            r["status"], r["input_mismatch"]
        ));
    }
    let wall_s = r["wall_s"].as_f64().unwrap_or(0.0);
    let peak_mb = r["plan_peak_mb"].as_f64().unwrap_or(f64::INFINITY);
    if peak_mb > EMU_PEAK_LIMIT_MB {
        failures.push(format!(
            "planning peak {peak_mb:.0} MB vs limit {EMU_PEAK_LIMIT_MB:.0} MB"
        ));
    }
    if r["trip"].as_str() == Some("elsa") {
        let base = &baseline["cases"]["e_elsa_sjuvass"];
        if let (Some(bw), Some(bm)) = (base["emu_wall_s"].as_f64(), base["emu_peak_mb"].as_f64()) {
            if wall_s > bw * BASELINE_REGRESSION {
                failures.push(format!(
                    "emu wall {wall_s:.1} s vs baseline {bw:.1} s (+{:.0} %)",
                    (wall_s / bw - 1.0) * 100.0
                ));
            }
            if peak_mb > bm * BASELINE_REGRESSION {
                failures.push(format!(
                    "emu peak {peak_mb:.0} MB vs baseline {bm:.0} MB (+{:.0} %)",
                    (peak_mb / bm - 1.0) * 100.0
                ));
            }
        }
    }
    failures.extend(place_index_failures(&r["place_index"]));
    failures.extend(search_check_failures(&r["searches"]));
    failures.extend(offline_map_check_failures(&r["offline_map"]));
    failures.extend(display_check_failures(&r["display_checks"]));
    let status = if failures.is_empty() { "PASS" } else { "FAIL" };
    eprintln!(
        "[gate] emulator {status}: trip {}, {:.1} km, ferries {}, hops {}, wall {wall_s:.1} s, \
         planning peak {peak_mb:.0} MB",
        r["trip"],
        r["distance_km"].as_f64().unwrap_or(0.0),
        r["ferries"],
        r["hops"]
    );
    for m in &failures {
        eprintln!("[gate]   - {m}");
    }
    if !failures.is_empty() {
        gate_failures.push("emulator");
    }
    serde_json::json!({
        "status": status,
        "source": path,
        "trip": r["trip"],
        "datex": r["datex"],
        "distance_km": r["distance_km"],
        "ferries": r["ferries"],
        "via_m": r["via_m"],
        "hops": r["hops"],
        "wall_s": wall_s,
        "plan_peak_mb": peak_mb,
        "place_index": r["place_index"],
        "failures": failures,
    })
}

/// Place index facts from the harness against the stored per-region row
/// counts: a missing, empty or damaged file, a lost region or fewer rows fail.
fn place_index_failures(pi: &serde_json::Value) -> Vec<String> {
    let expected: BTreeMap<String, u64> = serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/regression_gate_place_index.json"),
        )
        .expect("read regression_gate_place_index.json"),
    )
    .expect("parse regression_gate_place_index.json");
    let mut failures = Vec::new();
    if let Some(e) = pi["error"].as_str() {
        failures.push(format!("place index: {e}"));
        return failures;
    }
    let path = pi["path"].as_str().unwrap_or("?");
    if pi["bytes"].as_u64().unwrap_or(0) == 0 {
        failures.push(format!("place index {path} missing or empty"));
        return failures;
    }
    if pi["quick_check"].as_str() != Some("ok") {
        failures.push(format!("place index quick_check: {}", pi["quick_check"]));
    }
    for (region, want) in &expected {
        match pi["rows"][region.as_str()].as_u64() {
            None => failures.push(format!(
                "place index region {region} missing (expected {want} rows)"
            )),
            Some(got) if got < *want => failures.push(format!(
                "place index region {region}: {got} rows, expected {want}"
            )),
            Some(_) => {}
        }
    }
    let rows = pi["rows"].as_object().map(|m| m.len()).unwrap_or(0);
    eprintln!(
        "[gate] place index {path}: {} bytes, quick_check {}, {rows} regions, {} stored",
        pi["bytes"],
        pi["quick_check"],
        expected.len()
    );
    failures
}

const EMU_SEARCH_EXPECT: &[(&str, &str)] = &[
    ("Oslo", "Oslo"),
    ("Hamar", "Hamar"),
    ("Lillehammer", "Lillehammer"),
    ("Luleå", "Luleå"),
    ("Kiruna", "Kiruna"),
    ("Piteå", "Piteå"),
    ("Falun", "Falun"),
    ("Mora", "Mora"),
];

fn fold_place_name(s: &str) -> String {
    s.to_lowercase()
        .replace('å', "a")
        .replace('ä', "a")
        .replace('ö', "o")
        .replace('æ', "ae")
        .replace('ø', "o")
}

/// Fixed search list through the app's real search path after an APK install.
fn search_check_failures(searches: &serde_json::Value) -> Vec<String> {
    let mut failures = Vec::new();
    let Some(arr) = searches.as_array() else {
        failures.push("emulator searches missing (run the harness search check)".into());
        return failures;
    };
    for (q, want) in EMU_SEARCH_EXPECT {
        let hit = arr.iter().find(|s| s["q"].as_str() == Some(*q));
        let Some(hit) = hit else {
            failures.push(format!("search {q}: no result"));
            continue;
        };
        let n = hit["n"].as_u64().unwrap_or(0);
        let top = hit["top"].as_str().unwrap_or("");
        if n == 0 || !fold_place_name(top).contains(&fold_place_name(want)) {
            failures.push(format!("search {q}: expected {want}, got n={n} top={top}"));
        }
    }
    failures
}

/// Network off and on: each gate-route start, via and destination must have tiles.
fn offline_map_check_failures(maps: &serde_json::Value) -> Vec<String> {
    let mut failures = Vec::new();
    let Some(arr) = maps.as_array() else {
        failures.push(
            "emulator offline_map missing (tiles at each gate start/via/dest, network off and on)"
                .into(),
        );
        return failures;
    };
    if arr.is_empty() {
        failures.push("emulator offline_map empty".into());
        return failures;
    }
    for rec in arr {
        let trip = rec["trip"].as_str().unwrap_or("?");
        let role = rec["role"].as_str().unwrap_or("from");
        let net = rec["network"].as_str().unwrap_or("off");
        let visible = rec["visible"].as_u64().unwrap_or(0);
        let ok = rec["ok"].as_bool().unwrap_or(false);
        if !ok || visible == 0 {
            failures.push(format!(
                "map blank at {trip} {role} network={net} (visible={visible} kind={})",
                rec["kind"]
            ));
        }
    }
    for trip in ["bevensen", "aga", "floro", "elsa"] {
        for net in ["off", "on"] {
            for role in ["from", "to"] {
                let hit = arr.iter().any(|r| {
                    r["trip"].as_str() == Some(trip)
                        && r["role"].as_str() == Some(role)
                        && r["network"].as_str() == Some(net)
                });
                if !hit {
                    failures.push(format!("map check missing {trip} {role} network={net}"));
                }
            }
        }
    }
    failures
}

/// Hop across three regions, pan a border, and plan without blanking the map.
fn display_check_failures(checks: &serde_json::Value) -> Vec<String> {
    let mut failures = Vec::new();
    if !checks.is_object() {
        failures.push("emulator display_checks missing (hop / border pan / plan-no-blank)".into());
        return failures;
    }
    if checks["hooks_cleared"].as_bool() == Some(false) {
        failures.push("forced-online or forced-source was not cleared".into());
    }
    for key in ["hop", "border_pan", "plan_no_blank"] {
        if checks[key]["ok"].as_bool() != Some(true) {
            failures.push(format!("display check {key} failed: {}", checks[key]));
        }
    }
    failures
}

fn median(xs: &[f64]) -> f64 {
    let mut v = xs.to_vec();
    v.sort_by(f64::total_cmp);
    v.get(v.len() / 2).copied().unwrap_or(0.0)
}

fn load_baseline(path: &Path) -> serde_json::Value {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({ "cases": {} }))
}

#[test]
#[ignore = "needs NAVI_GATE_PACKS (device long-trip packs) and NAVI_GATE_REFS (reference lines)"]
fn long_trip_regression_gate() {
    let packs = PathBuf::from(std::env::var("NAVI_GATE_PACKS").expect("set NAVI_GATE_PACKS"));
    let refs = PathBuf::from(std::env::var("NAVI_GATE_REFS").expect("set NAVI_GATE_REFS"));
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let work = std::env::var("NAVI_GATE_WORK")
        .map(PathBuf::from)
        .unwrap_or_else(|_| manifest.join("../target/navi-gate-work"));
    let baseline_path = std::env::var("NAVI_GATE_BASELINE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| manifest.join("tests/regression_gate_baseline.json"));
    let write_baseline = std::env::var("NAVI_GATE_WRITE_BASELINE").as_deref() == Ok("1");
    let only: Option<Vec<String>> = std::env::var("NAVI_GATE_CASES")
        .ok()
        .map(|s| s.split(',').map(|x| x.trim().to_string()).collect());
    std::fs::create_dir_all(&work).expect("work dir");

    prepare_skeletons(&packs);
    let t0 = Instant::now();
    let country_bytes = warm_country_polys();
    eprintln!(
        "[gate] country polygons warm: {:.1} s, {country_bytes} bytes",
        t0.elapsed().as_secs_f64()
    );
    let baseline = load_baseline(&baseline_path);
    let mut new_baseline = serde_json::Map::new();
    let mut results = Vec::new();
    let mut gate_failures = Vec::new();

    for case in CASES {
        if let Some(ref only) = only {
            if !only.iter().any(|c| c == case.id) {
                continue;
            }
        }
        eprintln!("[gate] {} ...", case.id);
        let mut o = run_case(case, &packs, &refs, &work);
        o.wall_runs_s.push(o.wall_s);
        for n in 1..case.timing_runs {
            let t = run_case(
                case,
                &packs,
                &refs,
                &work.join(format!("timing-run{}", n + 1)),
            );
            eprintln!(
                "[gate] {} timing run {}: wall {:.1} s",
                case.id,
                n + 1,
                t.wall_s
            );
            if (t.distance_km - o.distance_km).abs() > 1e-6 {
                o.failures.push(format!(
                    "timing run {} distance {:.3} km differs from run 1 {:.3} km",
                    n + 1,
                    t.distance_km,
                    o.distance_km
                ));
            }
            o.wall_runs_s.push(t.wall_s);
        }
        o.wall_s = median(&o.wall_runs_s);
        o.pass = o.failures.is_empty();

        let base = &baseline["cases"][case.id];
        if !write_baseline {
            if let (Some(bw), Some(bm)) = (base["wall_s"].as_f64(), base["peak_mb"].as_f64()) {
                if o.wall_s > bw * BASELINE_REGRESSION {
                    o.failures.push(format!(
                        "wall {:.1} s vs baseline {:.1} s (+{:.0} %)",
                        o.wall_s,
                        bw,
                        (o.wall_s / bw - 1.0) * 100.0
                    ));
                }
                if o.peak_mb > bm * BASELINE_REGRESSION {
                    o.failures.push(format!(
                        "peak {:.0} MB vs baseline {:.0} MB (+{:.0} %)",
                        o.peak_mb,
                        bm,
                        (o.peak_mb / bm - 1.0) * 100.0
                    ));
                }
                o.pass = o.failures.is_empty();
            } else {
                eprintln!("[gate] {}: no baseline entry", case.id);
            }
        }
        // Only a run that produced a route is a timing reference.
        if o.poly_km > 0.0 {
            new_baseline.insert(
                case.id.to_string(),
                serde_json::json!({ "wall_s": o.wall_s, "peak_mb": o.peak_mb }),
            );
        }

        let status = match (o.pass, case.expected_fail) {
            (true, None) => "PASS",
            (false, None) => "FAIL",
            (false, Some(_)) => "XFAIL",
            (true, Some(_)) => "XPASS",
        };
        eprintln!(
            "[gate] {} {status}: {:.1} km (polyline {:.1}), {:.0} min, ferries {:?}, vias {:?} m, \
             wall {:.1} s, peak {:.0} MB, terminate {}",
            case.id,
            o.distance_km,
            o.poly_km,
            o.eta_min,
            o.ferries,
            o.via_m.iter().map(|d| d.round()).collect::<Vec<_>>(),
            o.wall_s,
            o.peak_mb,
            o.terminate
        );
        for f in &o.failures {
            eprintln!("[gate]   - {f}");
        }
        for k in &o.known {
            eprintln!("[gate]   - {k}");
        }
        if let Some(why) = case.expected_fail {
            eprintln!("[gate]   expected fail: {why}");
        } else if !o.pass {
            gate_failures.push(case.id);
        }
        results.push(serde_json::json!({
            "id": case.id,
            "status": status,
            "distance_km": o.distance_km,
            "polyline_km": o.poly_km,
            "eta_min": o.eta_min,
            "ferries": o.ferries,
            "via_m": o.via_m,
            "spikes": o.spikes,
            "wall_s": o.wall_s,
            "wall_runs_s": o.wall_runs_s,
            "peak_mb": o.peak_mb,
            "terminate": o.terminate,
            "failures": o.failures,
            "known_failures": o.known,
            "expected_fail": case.expected_fail,
        }));
    }

    let emulator = std::env::var("NAVI_GATE_EMU")
        .ok()
        .map(|d| emulator_check(Path::new(&d), &mut gate_failures, &baseline));
    if emulator.is_none() {
        eprintln!("[gate] emulator: not measured for this run (NAVI_GATE_EMU not set)");
    }

    for (test, why) in KNOWN_TEST_FAILURES {
        eprintln!("[gate] known test failure outside the gate: {test}: {why}");
    }
    let known_tests: Vec<serde_json::Value> = KNOWN_TEST_FAILURES
        .iter()
        .map(|(test, why)| serde_json::json!({ "test": test, "reason": why }))
        .collect();
    let _ = std::fs::write(
        work.join("gate-results.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "results": results,
            "emulator": emulator,
            "known_test_failures": known_tests,
        }))
        .unwrap(),
    );
    if write_baseline {
        let mut cases = baseline["cases"].as_object().cloned().unwrap_or_default();
        cases.extend(new_baseline);
        std::fs::write(
            &baseline_path,
            serde_json::to_string_pretty(&serde_json::json!({ "cases": cases })).unwrap() + "\n",
        )
        .expect("write baseline");
        eprintln!("[gate] baseline written to {}", baseline_path.display());
    }
    assert!(
        gate_failures.is_empty(),
        "regression gate failed: {gate_failures:?}"
    );
}
