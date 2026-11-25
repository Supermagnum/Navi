#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 256 * 1024 {
        return;
    }
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let _ = navi_cat::mirror::checksum_for_file(text, "repeaters.json.zst");
    let _ = navi_cat::mirror::checksum_for_file(text, "manifest.json");
});
