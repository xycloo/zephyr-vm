#![no_std]

use core::{alloc::{GlobalAlloc, Layout}, panic::PanicInfo, slice};

extern crate wee_alloc;

extern "C" {
    #[link_section = "db"]
    #[link_name = "read_raw"]
    fn read_raw() -> i32;
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[no_mangle]
pub extern "C" fn on_close() {
    let read = unsafe {
        read_raw()
    };

    if read == 0 {
        panic!()
    }

    let slice = unsafe {
        let read = read as *const u8;
        slice::from_raw_parts(read, 3)
    };
}

#[no_mangle]
pub extern "C" fn alloc() -> *mut u8 {
    unsafe {
        ALLOC.alloc_zeroed(Layout::from_size_align(1024, 1).unwrap())
    }
}
