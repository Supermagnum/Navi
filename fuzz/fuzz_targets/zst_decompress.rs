#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 1024 * 1024 {
        return;
    }
    // Cap at 1 MiB in the harness so a bomb cannot exhaust the CI runner.
    let _ = navi_cat::mirror::decompress_zst_capped(data, 1024 * 1024);
});
