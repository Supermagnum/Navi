//! POI look-ahead guest scaffold (`poi_lookahead` / `poi_cone`).
//!
//! Product APK uses host-native UniFFI + MapHudPrefs (default OFF). Cone membership,
//! category filter, and opening-hours evaluation stay on the host so closed venues
//! never reach the guest/HUD. Guests must not open sockets or play alerts.
//!
//! `poi_query` remains radius-only; bearing filter is host-side (Position has no
//! heading). This tick only logs when a radius sample is available.

#![no_std]
#![no_main]

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[no_mangle]
pub extern "C" fn plugin_main() {
    navi_plugin_sdk::host_log(
        "poi_lookahead: guest tick (host owns 850m/±30° cone + open_now; default OFF)",
    );
    if let Some(pos) = navi_plugin_sdk::host_position() {
        let mut buf = [0u8; 2048];
        // Radius matches host cone; bearing / open_now filtering is host-side.
        let n = navi_plugin_sdk::host_poi_query(pos.lat, pos.lon, 850.0, &mut buf);
        if n > 0 {
            navi_plugin_sdk::host_log("poi_lookahead: poi_query sample ok");
        }
    }
}
