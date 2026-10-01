//! CATS WASM guest: repeater selection, auto-tune decisions, network follow.
#![cfg_attr(target_arch = "wasm32", no_std)]
#![cfg_attr(target_arch = "wasm32", no_main)]

extern crate alloc;

#[cfg(target_arch = "wasm32")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[cfg(target_arch = "wasm32")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

mod follow;
mod select;

/// Auto-tune radius (km) — must match HostApi cap and docs/CAT.md.
pub const AUTO_TUNE_RADIUS_KM: f64 = 150.0;

#[cfg(target_arch = "wasm32")]
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
            let json = core::str::from_utf8(&buf[..n]).unwrap_or("[]");
            if let Some(best) = select::pick_best_nfm(json, pos.lat, pos.lon) {
                let msg = alloc::format!("cat: candidate {}", best.callsign);
                navi_plugin_sdk::host_log(&msg);
            }
        }
        let _ = follow::tick_follow(pos.lat, pos.lon);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn plugin_main_native_for_tests() {
    // Desktop unit tests call select/follow directly.
}
