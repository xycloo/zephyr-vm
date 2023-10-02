#![no_std]

use core::{alloc::{GlobalAlloc, Layout}, panic::PanicInfo, slice};

extern crate wee_alloc;

extern "C" {
    #[link_name = "read_raw"]
    fn read_raw() -> i32;

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


    let slice = unsafe {
        let read = read as *const u8;
        let slice = slice::from_raw_parts(read, 4);

        let a = slice[0];

        if a == 1 {
            log(1 as i32);
        }
    };
}

