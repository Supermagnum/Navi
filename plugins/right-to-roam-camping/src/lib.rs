//! Thin wasm32 guest shim over `navi-right-to-roam-camping` (rules only; no RouteGraph).
//!
//! Host selects road∩track probes (native graph) and writes a JSON job to
//! `plugin_kv` key `rtr_suggest_job`. This guest evaluates with the same rule
//! engine and writes `rtr_suggest_result`. Android ship gate stays closed.

use std::collections::BTreeMap;

use navi_right_to_roam_camping::{
    suggest_overnight_fixed_probes, CampingHost, LocalDate, OvernightSafety, TravelMode,
};

struct SdkHost {
    buildings: Vec<(f64, f64)>,
    glaciers: Vec<Vec<[f64; 2]>>,
    safety: Option<OvernightSafety>,
    clock: Option<LocalDate>,
    kv_ok: bool,
    countries: BTreeMap<(i64, i64), String>,
    subdivisions: BTreeMap<(i64, i64), String>,
}

fn cell(lat: f64, lon: f64) -> (i64, i64) {
    ((lat * 1e5).round() as i64, (lon * 1e5).round() as i64)
}

impl CampingHost for SdkHost {
    fn safety_config(&self) -> Option<OvernightSafety> {
        self.safety
    }
    fn clock_local(&self) -> Option<LocalDate> {
        self.clock
    }
    fn plugin_kv_available(&self) -> bool {
        self.kv_ok
    }
    fn kv_get(&self, key: &str) -> Option<String> {
        let mut buf = [0u8; 4096];
        let n = navi_plugin_sdk::host_plugin_kv_get(key, &mut buf)?;
        std::str::from_utf8(&buf[..n]).ok().map(|s| s.to_string())
    }
    fn kv_set(&mut self, key: &str, value: &str) -> Result<(), String> {
        navi_plugin_sdk::host_plugin_kv_set(key, value).map_err(|e| format!("kv_set {e}"))
    }
    fn admin_country_iso(&self, lat: f64, lon: f64) -> Option<String> {
        self.countries.get(&cell(lat, lon)).cloned()
    }
    fn admin_subdivision_iso(&self, lat: f64, lon: f64) -> Option<String> {
        self.subdivisions.get(&cell(lat, lon)).cloned()
    }
    fn travel_mode(&self) -> TravelMode {
        TravelMode::NonMotorised
    }
    fn overnight_buildings(&self) -> &[(f64, f64)] {
        &self.buildings
    }
    fn overnight_glacier_rings(&self) -> &[Vec<[f64; 2]>] {
        &self.glaciers
    }
}

#[derive(serde::Deserialize)]
struct SuggestJob {
    probes: Vec<[f64; 2]>,
    max_suggestions: Option<usize>,
    buildings: Vec<[f64; 2]>,
    glaciers: Vec<Vec<[f64; 2]>>,
    safety: Option<OvernightSafety>,
    clock: Option<LocalDate>,
    kv_ok: bool,
    countries: Vec<(f64, f64, String)>,
    subdivisions: Vec<(f64, f64, String)>,
}

#[no_mangle]
pub extern "C" fn plugin_main() {
    let mut safety_buf = [0u8; 512];
    let safety_n = navi_plugin_sdk::host_safety_config_read(&mut safety_buf);
    let mut clock_buf = [0u8; 128];
    let clock_n = navi_plugin_sdk::host_clock_read(&mut clock_buf);
    navi_plugin_sdk::host_log(&format!(
        "rtr_camping: failsafe safety_bytes={safety_n} clock_bytes={clock_n}"
    ));

    let mut job_buf = vec![0u8; 1024 * 1024];
    let Some(n) = navi_plugin_sdk::host_plugin_kv_get("rtr_suggest_job", &mut job_buf) else {
        navi_plugin_sdk::host_log("rtr_camping: no rtr_suggest_job — failsafe tick only");
        return;
    };
    let Ok(raw) = std::str::from_utf8(&job_buf[..n]) else {
        navi_plugin_sdk::host_log("rtr_camping: job not utf8");
        return;
    };
    let job: SuggestJob = match serde_json::from_str(raw) {
        Ok(j) => j,
        Err(e) => {
            navi_plugin_sdk::host_log(&format!("rtr_camping: job json parse failed: {e}"));
            return;
        }
    };

    let mut countries = BTreeMap::new();
    for (lat, lon, iso) in job.countries {
        countries.insert(cell(lat, lon), iso);
    }
    let mut subdivisions = BTreeMap::new();
    for (lat, lon, iso) in job.subdivisions {
        subdivisions.insert(cell(lat, lon), iso);
    }
    let buildings: Vec<(f64, f64)> = job.buildings.iter().map(|p| (p[0], p[1])).collect();
    let probes: Vec<(f64, f64)> = job.probes.iter().map(|p| (p[0], p[1])).collect();

    let mut host = SdkHost {
        buildings,
        glaciers: job.glaciers,
        safety: job.safety,
        clock: job.clock,
        kv_ok: job.kv_ok,
        countries,
        subdivisions,
    };

    let out = suggest_overnight_fixed_probes(&mut host, &probes, job.max_suggestions);
    let mut accepted = 0usize;
    let mut rejected = 0usize;
    let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
    for e in &out.probe_log {
        *reasons.entry(e.reason.clone()).or_default() += 1;
        if e.accepted {
            accepted += 1;
        } else {
            rejected += 1;
        }
    }
    let result = serde_json::json!({
        "accepted": accepted,
        "rejected": rejected,
        "reasons": reasons,
    });
    let text = result.to_string();
    let _ = navi_plugin_sdk::host_plugin_kv_set("rtr_suggest_result", &text);
    navi_plugin_sdk::host_log(&format!(
        "rtr_camping: done accepted={accepted} rejected={rejected}"
    ));
}
