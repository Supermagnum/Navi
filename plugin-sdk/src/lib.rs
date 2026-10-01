//! Thin guest wrappers around the `navi` host imports.
//!
//! Plugins compile to `wasm32-unknown-unknown` (no WASI filesystem). Use these
//! helpers instead of raw pointer serialization.

#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

#[link(wasm_import_module = "navi")]
extern "C" {
    fn log(ptr: u32, len: u32);
    fn get_position(out_ptr: u32) -> i32;
    fn poi_query(
        lat_bits: u64,
        lon_bits: u64,
        radius_m_bits: u64,
        out_ptr: u32,
        out_cap: u32,
    ) -> i32;
    fn poi_write(ptr: u32, len: u32) -> i32;
    fn weather_read(
        lat_bits: u64,
        lon_bits: u64,
        radius_m_bits: u64,
        out_ptr: u32,
        out_cap: u32,
    ) -> i32;
    fn route_read(out_ptr: u32, out_cap: u32) -> i32;
    fn route_destination_read(out_ptr: u32, out_cap: u32) -> i32;
    fn safety_config_read(out_ptr: u32, out_cap: u32) -> i32;
    fn admin_region_read(lat_bits: u64, lon_bits: u64, out_ptr: u32, out_cap: u32) -> i32;
    fn clock_read(out_ptr: u32, out_cap: u32) -> i32;
    fn plugin_kv_get(key_ptr: u32, key_len: u32, out_ptr: u32, out_cap: u32) -> i32;
    fn plugin_kv_set(key_ptr: u32, key_len: u32, val_ptr: u32, val_len: u32) -> i32;
    fn protected_area_query(lat_bits: u64, lon_bits: u64, out_ptr: u32, out_cap: u32) -> i32;
    fn land_tenure_query(lat_bits: u64, lon_bits: u64, out_ptr: u32, out_cap: u32) -> i32;
    fn landcover_query(lat_bits: u64, lon_bits: u64, out_ptr: u32, out_cap: u32) -> i32;
    fn travel_mode_read(out_ptr: u32, out_cap: u32) -> i32;
    fn vehicle_profile_read(out_ptr: u32, out_cap: u32) -> i32;
    fn traveller_profile_read(out_ptr: u32, out_cap: u32) -> i32;
    fn host_nop();
}

/// Write a UTF-8 log line to the host.
pub fn host_log(msg: &str) {
    unsafe { log(msg.as_ptr() as u32, msg.len() as u32) }
}

#[derive(Debug, Clone, Copy)]
pub struct Position {
    pub lat: f64,
    pub lon: f64,
}

/// Read host position. Returns `None` when the host has no fix.
pub fn host_position() -> Option<Position> {
    let mut buf = [0u8; 16];
    let ok = unsafe { get_position(buf.as_mut_ptr() as u32) };
    if ok == 0 {
        return None;
    }
    let lat = f64::from_le_bytes(buf[0..8].try_into().ok()?);
    let lon = f64::from_le_bytes(buf[8..16].try_into().ok()?);
    Some(Position { lat, lon })
}

macro_rules! host_json_buf {
    ($call:expr, $out:expr) => {{
        let n = unsafe { $call };
        if n < 0 {
            0
        } else {
            n as usize
        }
    }};
}

/// Query host POIs; returns the raw JSON bytes written by the host (may be truncated).
pub fn host_poi_query(lat: f64, lon: f64, radius_m: f64, out: &mut [u8]) -> usize {
    host_json_buf!(
        poi_query(
            lat.to_bits(),
            lon.to_bits(),
            radius_m.to_bits(),
            out.as_mut_ptr() as u32,
            out.len() as u32,
        ),
        out
    )
}

/// Upsert a POI on the host. `json` must be UTF-8 JSON with name/lat/lon/kind.
pub fn host_poi_write_json(json: &str) -> Result<(), i32> {
    let rc = unsafe { poi_write(json.as_ptr() as u32, json.len() as u32) };
    if rc == 0 {
        Ok(())
    } else {
        Err(rc)
    }
}

pub fn host_weather_read(lat: f64, lon: f64, radius_m: f64, out: &mut [u8]) -> usize {
    host_json_buf!(
        weather_read(
            lat.to_bits(),
            lon.to_bits(),
            radius_m.to_bits(),
            out.as_mut_ptr() as u32,
            out.len() as u32,
        ),
        out
    )
}

pub fn host_route_read(out: &mut [u8]) -> usize {
    host_json_buf!(
        route_read(out.as_mut_ptr() as u32, out.len() as u32),
        out
    )
}

pub fn host_route_destination_read(out: &mut [u8]) -> usize {
    host_json_buf!(
        route_destination_read(out.as_mut_ptr() as u32, out.len() as u32),
        out
    )
}

pub fn host_safety_config_read(out: &mut [u8]) -> usize {
    host_json_buf!(
        safety_config_read(out.as_mut_ptr() as u32, out.len() as u32),
        out
    )
}

pub fn host_admin_region_read(lat: f64, lon: f64, out: &mut [u8]) -> usize {
    host_json_buf!(
        admin_region_read(
            lat.to_bits(),
            lon.to_bits(),
            out.as_mut_ptr() as u32,
            out.len() as u32,
        ),
        out
    )
}

pub fn host_clock_read(out: &mut [u8]) -> usize {
    host_json_buf!(clock_read(out.as_mut_ptr() as u32, out.len() as u32), out)
}

pub fn host_plugin_kv_get(key: &str, out: &mut [u8]) -> Option<usize> {
    let n = unsafe {
        plugin_kv_get(
            key.as_ptr() as u32,
            key.len() as u32,
            out.as_mut_ptr() as u32,
            out.len() as u32,
        )
    };
    if n < 0 {
        None
    } else {
        Some(n as usize)
    }
}

pub fn host_plugin_kv_set(key: &str, value: &str) -> Result<(), i32> {
    let rc = unsafe {
        plugin_kv_set(
            key.as_ptr() as u32,
            key.len() as u32,
            value.as_ptr() as u32,
            value.len() as u32,
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(rc)
    }
}

pub fn host_protected_area_query(lat: f64, lon: f64, out: &mut [u8]) -> usize {
    host_json_buf!(
        protected_area_query(
            lat.to_bits(),
            lon.to_bits(),
            out.as_mut_ptr() as u32,
            out.len() as u32,
        ),
        out
    )
}

pub fn host_land_tenure_query(lat: f64, lon: f64, out: &mut [u8]) -> usize {
    host_json_buf!(
        land_tenure_query(
            lat.to_bits(),
            lon.to_bits(),
            out.as_mut_ptr() as u32,
            out.len() as u32,
        ),
        out
    )
}

pub fn host_landcover_query(lat: f64, lon: f64, out: &mut [u8]) -> usize {
    host_json_buf!(
        landcover_query(
            lat.to_bits(),
            lon.to_bits(),
            out.as_mut_ptr() as u32,
            out.len() as u32,
        ),
        out
    )
}

pub fn host_travel_mode_read(out: &mut [u8]) -> usize {
    host_json_buf!(
        travel_mode_read(out.as_mut_ptr() as u32, out.len() as u32),
        out
    )
}

pub fn host_vehicle_profile_read(out: &mut [u8]) -> usize {
    host_json_buf!(
        vehicle_profile_read(out.as_mut_ptr() as u32, out.len() as u32),
        out
    )
}

pub fn host_traveller_profile_read(out: &mut [u8]) -> usize {
    host_json_buf!(
        traveller_profile_read(out.as_mut_ptr() as u32, out.len() as u32),
        out
    )
}

/// Touch the host (useful for proving the import table is wired).
pub fn host_ping() {
    unsafe { host_nop() }
}

/// Build a minimal JSON object for [`host_poi_write_json`] without pulling serde.
pub fn poi_write_json(name: &str, lat: f64, lon: f64, kind: &str) -> String {
    use alloc::format;
    format!(
        "{{\"name\":\"{}\",\"lat\":{},\"lon\":{},\"kind\":\"{}\"}}",
        escape(name),
        lat,
        lon,
        escape(kind)
    )
}

fn escape(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
    }
    out
}

/// Helper for plugins that want a growable scratch buffer.
pub fn scratch(cap: usize) -> Vec<u8> {
    alloc::vec![0u8; cap]
}
