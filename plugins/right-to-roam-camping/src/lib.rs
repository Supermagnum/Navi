//! Thin wasm32 guest shim over `navi-right-to-roam-camping` (rules only; no RouteGraph).
//!
//! Host selects road∩track probes (native graph) and writes a JSON job to
//! `plugin_kv` key `rtr_suggest_job`. This guest evaluates with the same rule
//! engine and writes `rtr_suggest_result`. Live `clock_read` / `safety_config_read`
//! / destination / residency from HostApi prefer over job fields when present.

use std::collections::BTreeMap;

use navi_right_to_roam_camping::{
    suggest_overnight_fixed_probes, CampingHost, LocalDate, OvernightSafety, TravelMode,
    VehicleClass, VehicleProfile, DISCLAIMER,
};

struct SdkHost {
    buildings: Vec<(f64, f64)>,
    glaciers: Vec<Vec<[f64; 2]>>,
    safety: Option<OvernightSafety>,
    clock: Option<LocalDate>,
    kv_ok: bool,
    countries: BTreeMap<(i64, i64), String>,
    subdivisions: BTreeMap<(i64, i64), String>,
    travel_mode: TravelMode,
    vehicle: VehicleProfile,
    destination: Option<(f64, f64)>,
    residency: Option<String>,
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
        self.travel_mode
    }
    fn overnight_buildings(&self) -> &[(f64, f64)] {
        &self.buildings
    }
    fn overnight_glacier_rings(&self) -> &[Vec<[f64; 2]>] {
        &self.glaciers
    }
    fn vehicle_overnight_profile(&self) -> VehicleProfile {
        self.vehicle
    }
    fn route_destination(&self) -> Option<(f64, f64)> {
        self.destination
    }
    fn residency_country_iso(&self) -> Option<String> {
        self.residency.clone()
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
    #[serde(default)]
    travel_mode: Option<String>,
    #[serde(default)]
    vehicle_class: Option<String>,
    #[serde(default)]
    is_professional_driver_under_rest_rules: bool,
}

fn parse_travel_mode(s: Option<&str>) -> TravelMode {
    match s.map(|x| x.to_ascii_lowercase()).as_deref() {
        Some("motorised") | Some("motorized") => TravelMode::Motorised,
        Some("non_motorised") | Some("non_motorized") => TravelMode::NonMotorised,
        _ => TravelMode::Unknown,
    }
}

fn parse_vehicle_class(s: Option<&str>) -> VehicleClass {
    match s.map(|x| x.to_ascii_lowercase()).as_deref() {
        Some("car") => VehicleClass::Car,
        Some("campervan_motorhome") | Some("campervan") | Some("mobilehome") => {
            VehicleClass::CampervanMotorhome
        }
        Some("hgv") | Some("truck") => VehicleClass::Hgv,
        _ => VehicleClass::Unknown,
    }
}

#[no_mangle]
pub extern "C" fn plugin_main() {
    let mut safety_buf = [0u8; 512];
    let safety_n = navi_plugin_sdk::host_safety_config_read(&mut safety_buf);
    let live_safety = if safety_n > 0 {
        std::str::from_utf8(&safety_buf[..safety_n])
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
            .and_then(|v| {
                if v.is_null() {
                    return None;
                }
                Some(OvernightSafety {
                    min_building_distance_m: v.get("min_building_distance_m")?.as_f64()?,
                    min_glacier_distance_m: v
                        .get("min_glacier_distance_m")
                        .and_then(|x| x.as_f64())
                        .unwrap_or(1_000.0),
                })
            })
    } else {
        None
    };

    let mut clock_buf = [0u8; 256];
    let clock_n = navi_plugin_sdk::host_clock_read(&mut clock_buf);
    let live_clock = if clock_n > 0 {
        std::str::from_utf8(&clock_buf[..clock_n])
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
            .and_then(|v| {
                if v.is_null() {
                    return None;
                }
                Some(LocalDate {
                    year: v.get("year")?.as_i64()? as i32,
                    month: v.get("month")?.as_u64()? as u32,
                    day: v.get("day")?.as_u64()? as u32,
                })
            })
    } else {
        None
    };
    navi_plugin_sdk::host_log(&format!(
        "rtr_camping: live safety={} clock={}",
        live_safety.is_some(),
        live_clock.is_some()
    ));

    let mut dest_buf = [0u8; 128];
    let dest_n = navi_plugin_sdk::host_route_destination_read(&mut dest_buf);
    let live_dest = if dest_n > 0 {
        std::str::from_utf8(&dest_buf[..dest_n])
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
            .and_then(|v| {
                let lat = v.get("lat")?.as_f64()?;
                let lon = v.get("lon")?.as_f64()?;
                Some((lat, lon))
            })
    } else {
        None
    };

    let mut trav_buf = [0u8; 128];
    let trav_n = navi_plugin_sdk::host_traveller_profile_read(&mut trav_buf);
    let live_residency = if trav_n > 0 {
        std::str::from_utf8(&trav_buf[..trav_n])
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
            .and_then(|v| {
                v.get("residency_country")
                    .and_then(|c| c.as_str())
                    .map(|s| s.to_string())
            })
    } else {
        None
    };

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

    // Prefer live host travel/vehicle reads when present; fall back to job fields.
    let mut mode_buf = [0u8; 64];
    let mode_n = navi_plugin_sdk::host_travel_mode_read(&mut mode_buf);
    let host_mode = if mode_n > 0 {
        std::str::from_utf8(&mode_buf[..mode_n])
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
            .and_then(|v| {
                v.get("mode")
                    .or_else(|| v.as_str().map(|_| &v))
                    .and_then(|m| m.as_str())
                    .map(|m| parse_travel_mode(Some(m)))
            })
    } else {
        None
    };

    let mut veh_buf = [0u8; 512];
    let veh_n = navi_plugin_sdk::host_vehicle_profile_read(&mut veh_buf);
    let host_vehicle = if veh_n > 0 {
        std::str::from_utf8(&veh_buf[..veh_n])
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
            .map(|v| {
                let class = v
                    .get("class")
                    .and_then(|c| c.as_str())
                    .map(|c| parse_vehicle_class(Some(c)))
                    .unwrap_or(VehicleClass::Unknown);
                let pro = v
                    .get("is_professional_driver_under_rest_rules")
                    .and_then(|b| b.as_bool())
                    .unwrap_or(false);
                VehicleProfile {
                    class,
                    is_professional_driver_under_rest_rules: pro,
                }
            })
    } else {
        None
    };

    let travel_mode = match host_mode {
        Some(TravelMode::Motorised) | Some(TravelMode::NonMotorised) => host_mode.unwrap(),
        _ => {
            let from_job = parse_travel_mode(job.travel_mode.as_deref());
            if matches!(from_job, TravelMode::Unknown) {
                // Legacy jobs omitted travel_mode; tent path remains non-motorised.
                TravelMode::NonMotorised
            } else {
                from_job
            }
        }
    };
    let vehicle = match host_vehicle {
        Some(v)
            if !matches!(v.class, VehicleClass::Unknown)
                || v.is_professional_driver_under_rest_rules =>
        {
            v
        }
        Some(v) => {
            // Host returned explicit unknown with default pro flag — still honour job overrides.
            if job.vehicle_class.is_some() {
                VehicleProfile {
                    class: parse_vehicle_class(job.vehicle_class.as_deref()),
                    is_professional_driver_under_rest_rules: job
                        .is_professional_driver_under_rest_rules,
                }
            } else {
                v
            }
        }
        None => VehicleProfile {
            class: parse_vehicle_class(job.vehicle_class.as_deref()),
            is_professional_driver_under_rest_rules: job.is_professional_driver_under_rest_rules,
        },
    };

    let mut host = SdkHost {
        buildings,
        glaciers: job.glaciers,
        safety: live_safety.or(job.safety),
        clock: live_clock.or(job.clock),
        kv_ok: job.kv_ok,
        countries,
        subdivisions,
        travel_mode,
        vehicle,
        destination: live_dest,
        residency: live_residency,
    };

    let out = suggest_overnight_fixed_probes(&mut host, &probes, job.max_suggestions);
    let mut accepted = 0usize;
    let mut rejected = 0usize;
    let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
    for e in &out.probe_log {
        if e.road_highway == "vehicle" {
            continue;
        }
        *reasons.entry(e.reason.clone()).or_default() += 1;
        if e.accepted {
            accepted += 1;
        } else {
            rejected += 1;
        }
    }
    let list = serde_json::to_value(&out.list).unwrap_or(serde_json::Value::Null);
    let vehicle = serde_json::to_value(&out.vehicle).unwrap_or(serde_json::Value::Null);
    let on_foot_from_here =
        serde_json::to_value(&out.on_foot_from_here).unwrap_or(serde_json::Value::Null);
    let result = serde_json::json!({
        "accepted": accepted,
        "rejected": rejected,
        "reasons": reasons,
        "vehicle_accepted": out.vehicle.probes_accepted,
        "on_foot_accepted": out.on_foot_from_here.probes_accepted,
        "on_foot_rejected": out.on_foot_from_here.cards.iter().filter(|c| !c.accepted).count(),
        "list": list,
        "vehicle": vehicle,
        "on_foot_from_here": on_foot_from_here,
        "disclaimer": DISCLAIMER,
    });
    let text = result.to_string();
    let _ = navi_plugin_sdk::host_plugin_kv_set("rtr_suggest_result", &text);
    navi_plugin_sdk::host_log(&format!(
        "rtr_camping: done accepted={accepted} rejected={rejected} vehicle={} on_foot={}",
        out.vehicle.probes_accepted, out.on_foot_from_here.probes_accepted
    ));
}
