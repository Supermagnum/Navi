#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 4096 {
        return;
    }
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let _ = navi_cat::importers::normalize_mhz_str(s);
    if let Some(v) = navi_cat::importers::parse_mhz(s) {
        let _ = v.is_finite();
    }
});
