#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 64 * 1024 {
        return;
    }
    if let Ok((frames, _consumed)) = navi_cat::benshi::codec::gaia_decode(data) {
        for body in frames {
            let _ = navi_cat::benshi::codec::parse_body(&body);
        }
    }
});
