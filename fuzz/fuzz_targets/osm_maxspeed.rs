#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 4096 {
        return;
    }
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let _ = driver_break_core::routing::eta::parse_maxspeed_kmh(s);
    let _ = driver_break_core::routing::graph::parse_metric(s);
    let _ = driver_break_core::routing::graph::is_tunnel_tag(s);
    let _ = driver_break_core::routing::graph::classify_surface_tags(None, Some(s), None);
});
