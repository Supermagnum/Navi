#![no_main]
use libfuzzer_sys::fuzz_target;
use chrono::NaiveDate;

fuzz_target!(|data: &[u8]| {
    if data.len() > 8192 {
        return;
    }
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let dt = NaiveDate::from_ymd_opt(2026, 6, 15)
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap();
    let _ = driver_break_core::routing::conditional::conditional_maxspeed_kmh_at(s, dt);
});
