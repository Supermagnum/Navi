#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 64 * 1024 {
        return;
    }
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let _ = navi_cat::repeaterbook::parse_kml_point(s);
});
