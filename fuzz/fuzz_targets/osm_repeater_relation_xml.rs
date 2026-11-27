#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 2 * 1024 * 1024 {
        return;
    }
    let Ok(xml) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(db) = navi_cat::RepeaterDb::open_memory() else {
        return;
    };
    let _ = navi_cat::importers::import_osm_relation_xml(&db, xml);
});
