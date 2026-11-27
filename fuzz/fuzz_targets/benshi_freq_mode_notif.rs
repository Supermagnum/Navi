#![no_main]
use libfuzzer_sys::fuzz_target;
use navi_cat::benshi::codec::FreqModeStatusNotif;

fuzz_target!(|data: &[u8]| {
    if data.len() > 64 * 1024 {
        return;
    }
    let _ = FreqModeStatusNotif::from_event_body(data);
});
