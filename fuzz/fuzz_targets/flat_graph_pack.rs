#![no_main]
use libfuzzer_sys::fuzz_target;
use std::io::Write;

fuzz_target!(|data: &[u8]| {
    if data.len() > 4 * 1024 * 1024 {
        return;
    }
    let Ok(dir) = tempfile::tempdir() else {
        return;
    };
    let path = dir.path().join("fuzz.rkyv");
    if std::fs::File::create(&path)
        .and_then(|mut f| f.write_all(data))
        .is_err()
    {
        return;
    }
    let _ = driver_break_core::routing::indexed::load_graph_pack(
        &path,
        driver_break_core::routing::graph::RoutingProfile::Car,
    );
});
