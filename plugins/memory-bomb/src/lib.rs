//! Reference plugin: grows linear memory until the host memory ceiling traps.
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
    // Grow by 16 pages (1 MiB) each step until the host StoreLimits trap.
    loop {
        let prev = core::arch::wasm32::memory_grow(0, 16);
        if prev == usize::MAX {
            // Without trap_on_grow_failure the grow fails softly; spin so the
            // fuel/epoch path still kills us. With trap_on_grow_failure the
            // grow itself traps as MemoryExceeded before we get here.
            core::hint::spin_loop();
        }
    }
}
