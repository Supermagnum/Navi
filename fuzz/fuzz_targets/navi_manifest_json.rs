#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 1024 * 1024 {
        return;
    }
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let _: Result<driver_break_core::routing::indexed::NaviManifest, _> = serde_json::from_str(s);
});
