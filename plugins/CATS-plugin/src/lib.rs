//! CATS WASM guest: repeater selection, auto-tune decisions, network follow.

mod follow;
mod select;

/// Auto-tune radius (km) — must match HostApi cap and docs/CAT.md.
pub const AUTO_TUNE_RADIUS_KM: f64 = 150.0;

#[no_mangle]
pub extern "C" fn plugin_main() {
    navi_plugin_sdk::host_log("cat: guest loaded");
    if let Some(pos) = navi_plugin_sdk::host_position() {
        let mut buf = [0u8; 65536];
        let n = navi_plugin_sdk::host_repeater_query(
            pos.lat,
            pos.lon,
            AUTO_TUNE_RADIUS_KM,
            None,
            &mut buf,
        );
        if n > 0 {
            let json = std::str::from_utf8(&buf[..n]).unwrap_or("[]");
            if let Some(best) = select::pick_best_nfm(json, pos.lat, pos.lon) {
                navi_plugin_sdk::host_log(&format!("cat: candidate {}", best.callsign));
            }
        }
        let _ = follow::tick_follow(pos.lat, pos.lon);
    }
}
