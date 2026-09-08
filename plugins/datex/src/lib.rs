//! DATEX guest scaffold.
//!
//! Fetch, parse, and corridor filtering stay in the host (`driver_break_core::datex`).
//! Guests must never hold NPRA credentials or open sockets. Product APK does not
//! load plugin-host yet (wasmtime gate).

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
    navi_plugin_sdk::host_log("datex: guest tick (host owns navi-server GET + parse; default OFF)");
}
