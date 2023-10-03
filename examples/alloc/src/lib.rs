#![no_std]

use core::{alloc::{GlobalAlloc, Layout}, panic::PanicInfo, slice};
extern crate wee_alloc;

extern "C" {
    #[link_name = "read_raw"]
    fn read_raw() -> (i64, i64);

    #[link_name = "zephyr_stack_push"]
    fn env_push_stack(param: i64);

    #[link_name = "zephyr_logger"]
    fn log(param: i32);
}

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[cfg(target_family = "wasm")]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}

#[no_mangle]
pub extern "C" fn on_close() {
    unsafe {
        env_push_stack(12348);
        env_push_stack(1);
        env_push_stack(123181);
    };

    let (offset, size) = unsafe {
        read_raw()
    };

    let memory: *const u8 =  0 as *const u8;

    let slice = unsafe {
        let start = memory.offset(offset as isize);
        slice::from_raw_parts(start, size as usize)
    };

    unsafe { 
        for byte in slice {
            log(*byte as i32)
        } 
    }
}

