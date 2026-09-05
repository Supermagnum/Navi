//! Weather guest: select nearby cached samples and log diagnostics.
//!
//! Provider HTTPS fetch + SQLite cache stay in the host. This guest reads
//! semantic samples via `weather_read` and the current fix via `position_read`.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::format;

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[no_mangle]
pub extern "C" fn plugin_main() {
    match navi_plugin_sdk::host_position() {
        None => {
            navi_plugin_sdk::host_log("weather: no position fix");
        }
        Some(pos) => {
            let mut buf = navi_plugin_sdk::scratch(2048);
            let n = navi_plugin_sdk::host_weather_read(pos.lat, pos.lon, 25_000.0, &mut buf);
            if n == 0 {
                navi_plugin_sdk::host_log("weather: no cached samples near fix");
            } else {
                let preview = core::str::from_utf8(&buf[..n]).unwrap_or("");
                navi_plugin_sdk::host_log(&format!(
                    "weather: near {:.4},{:.4} -> {}",
                    pos.lat, pos.lon, preview
                ));
            }
        }
    }
}
