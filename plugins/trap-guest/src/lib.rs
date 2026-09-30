//! Reference plugin: traps immediately (unreachable) for isolation tests.
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
    // Deliberate trap — host must classify as PluginError::Trap and never crash.
    core::arch::wasm32::unreachable();
}
