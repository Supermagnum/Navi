//! Instrument-cluster export guest (`instrument-cluster`).
//!
//! Reads the host guidance snapshot and publishes one `navi.cluster.v1` JSON
//! document per tick through `vehicle_signal_publish`. The trusted host owns
//! USB / Kuksa / log sinks; this guest never opens sockets.
//!
//! Fuel economy leaves stay null until core publishes real values.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[no_mangle]
pub extern "C" fn plugin_main() {
    let mut buf = navi_plugin_sdk::scratch(8192);
    let n = navi_plugin_sdk::host_nav_guidance_read(&mut buf);
    if n == 0 {
        navi_plugin_sdk::host_log("instrument-cluster: empty nav_guidance_read");
        return;
    }
    let guidance = match core::str::from_utf8(&buf[..n]) {
        Ok(s) => s,
        Err(_) => {
            navi_plugin_sdk::host_log("instrument-cluster: guidance not utf-8");
            return;
        }
    };

    let cluster = guidance_to_cluster_v1(guidance);
    match navi_plugin_sdk::host_vehicle_signal_publish(&cluster) {
        Ok(()) => navi_plugin_sdk::host_log("instrument-cluster: published navi.cluster.v1"),
        Err(rc) => navi_plugin_sdk::host_log(&format!(
            "instrument-cluster: vehicle_signal_publish failed rc={rc}"
        )),
    }
}

/// Map a host `navi.guidance.v1` (or already-cluster) JSON blob into
/// `navi.cluster.v1`. Keeps parsing allocation-light: rewrite the schema key
/// when needed and ensure reserved `fuel` is present as nulls.
fn guidance_to_cluster_v1(guidance: &str) -> String {
    let trimmed = guidance.trim();
    if trimmed.is_empty() {
        return idle_cluster_json();
    }
    // Already a cluster document — pass through (host may pre-shape).
    if trimmed.contains("\"schema\":\"navi.cluster.v1\"")
        || trimmed.contains("\"schema\": \"navi.cluster.v1\"")
    {
        return ensure_fuel_nulls(trimmed);
    }
    // Guidance snapshot: change schema tag and ensure fuel block.
    let mut out = if trimmed.contains("\"schema\":\"navi.guidance.v1\"") {
        trimmed.replacen(
            "\"schema\":\"navi.guidance.v1\"",
            "\"schema\":\"navi.cluster.v1\"",
            1,
        )
    } else if trimmed.contains("\"schema\": \"navi.guidance.v1\"") {
        trimmed.replacen(
            "\"schema\": \"navi.guidance.v1\"",
            "\"schema\": \"navi.cluster.v1\"",
            1,
        )
    } else if trimmed.starts_with('{') {
        // No schema — wrap by injecting one after the opening brace.
        let rest = trimmed[1..].trim_start();
        format!("{{\"schema\":\"navi.cluster.v1\",{rest}")
    } else {
        return idle_cluster_json();
    };
    out = ensure_fuel_nulls(&out);
    // Prefer a fresh ISO-ish ts when host omitted it.
    if !out.contains("\"ts\"") {
        if let Some(pos) = out.find('{') {
            out.insert_str(pos + 1, "\"ts\":null,");
        }
    }
    out
}

fn ensure_fuel_nulls(json: &str) -> String {
    if json.contains("\"fuel\"") {
        return String::from(json);
    }
    // Insert before the final closing brace.
    let bytes: Vec<u8> = json.as_bytes().to_vec();
    if let Some(i) = bytes.iter().rposition(|&b| b == b'}') {
        let mut out = String::from(&json[..i]);
        let needs_comma = out
            .trim_end()
            .chars()
            .last()
            .map(|c| c != '{' && c != ',')
            .unwrap_or(false);
        if needs_comma {
            out.push(',');
        }
        out.push_str("\"fuel\":{\"l_per_scandinavian_mil\":null,\"l_per_us_mile\":null}}");
        out
    } else {
        String::from(json)
    }
}

fn idle_cluster_json() -> String {
    String::from(
        "{\"schema\":\"navi.cluster.v1\",\"ts\":null,\"route_active\":false,\"position\":null,\"maneuver\":null,\"warning\":null,\"trip\":null,\"rest\":null,\"eco\":{\"enabled\":false,\"profile\":null},\"attitude\":null,\"fuel\":{\"l_per_scandinavian_mil\":null,\"l_per_us_mile\":null},\"route\":{\"polyline\":\"\"}}",
    )
}
