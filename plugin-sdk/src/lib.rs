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

/// Query host POIs; returns the raw JSON bytes written by the host (may be truncated).
pub fn host_poi_query(lat: f64, lon: f64, radius_m: f64, out: &mut [u8]) -> usize {
    let n = unsafe {
        poi_query(
            lat.to_bits(),
            lon.to_bits(),
            radius_m.to_bits(),
            out.as_mut_ptr() as u32,
            out.len() as u32,
        )
    };
    if n < 0 {
        0
    } else {
        n as usize
    }
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

/// Read cached weather samples near a point; returns raw JSON bytes (may truncate).
pub fn host_weather_read(lat: f64, lon: f64, radius_m: f64, out: &mut [u8]) -> usize {
    let n = unsafe {
        weather_read(
            lat.to_bits(),
            lon.to_bits(),
            radius_m.to_bits(),
            out.as_mut_ptr() as u32,
            out.len() as u32,
        )
    };
    if n < 0 {
        0
    } else {
        n as usize
    }
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
