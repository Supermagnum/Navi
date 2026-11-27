#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 256 * 1024 {
        return;
    }
    let _ = navi_cat::parse_rigctld_reply(data);
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = navi_cat::parse_rigctld_simple_value(s);
    }
});
