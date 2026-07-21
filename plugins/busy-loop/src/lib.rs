//! Reference plugin: busy-loops to exercise fuel / timeout isolation.
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
    // Intentional infinite work so the host must kill this via fuel or epoch.
    loop {
        core::hint::spin_loop();
    }
}
