#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Match the app import cap (40 MiB); keep the fuzzer away from OOM.
    if data.len() > 4 * 1024 * 1024 {
        return;
    }
    let _ = driver_break_core::import::gpx_import::parse_gpx_bytes(data);
});
