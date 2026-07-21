//! Reference plugin: logs a single line via HostApi and returns.
#![no_std]
#![no_main]

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[no_mangle]
pub extern "C" fn plugin_main() {
    navi_plugin_sdk::host_log("log_hello: sandboxed call ok");
}
