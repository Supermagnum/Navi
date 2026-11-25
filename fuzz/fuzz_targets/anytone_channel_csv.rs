#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 2 * 1024 * 1024 {
        return;
    }
    let csv = navi_cat::importers::decode_cps_bytes(data);
    let Ok(db) = navi_cat::RepeaterDb::open_memory() else {
        return;
    };
    let _ = navi_cat::importers::import_anytone_channel_csv(&db, &csv);
});
