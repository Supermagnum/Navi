#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 256 * 1024 {
        return;
    }
    let text = String::from_utf8_lossy(data);
    // Fail-closed gating: any input must produce a decision, never panic.
    let _ = navi_cat::gate_from_dump_caps(&text, false, false);
    let _ = navi_cat::gate_from_dump_caps(&text, true, false);
    let _ = navi_cat::gate_from_dump_caps(&text, true, true);
});
