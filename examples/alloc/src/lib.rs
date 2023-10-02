#![no_std]

use core::{alloc::{GlobalAlloc, Layout}, panic::PanicInfo, slice};
extern crate wee_alloc;

extern "C" {
    #[link_name = "read_raw"]
    fn read_raw() -> ();

    #[link_name = "zephyr_logger"]
    fn log(param: i32);
}

#[cfg(target_family = "wasm")]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[no_mangle]
pub extern "C" fn on_close() {
    let read = unsafe {
        read_raw()
    };

    let x = 40;

    unsafe {
        // Define a reference to the linear memory.
        let memory: *const u8 = 0 as *const u8;

        // Calculate the memory address at the specified offset.
        let start = memory.offset(x as isize);

        // Check if the requested slice is within the bounds of the linear memory.
        
        let slice = slice::from_raw_parts(start, 4);

        log(slice[2] as i32);
    }
}

