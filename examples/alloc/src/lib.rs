use core::slice;
use stellar_xdr::{ReadXdr, ScVal, WriteXdr, LedgerCloseMeta};

extern crate wee_alloc;

extern "C" {
    #[allow(improper_ctypes)] // we alllow as we enabled multi-value
    #[link_name = "read_raw"]
    fn read_raw() -> (i64, i64);

    #[allow(improper_ctypes)] // we alllow as we enabled multi-value
    #[link_name = "read_ledger_meta"]
    fn read_ledger_meta() -> (i64, i64);

    #[link_name = "zephyr_stack_push"]
    fn env_push_stack(param: i64);

    #[link_name = "zephyr_logger"]
    fn log(param: i64);
}

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;


fn db_read_test() {
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

    let topic = ScVal::from_xdr(slice).unwrap();
    
    match topic {
        ScVal::Symbol(inner) => {
            let string = inner.to_string().unwrap();
            if string.as_str() == "deposit" {
                unsafe { log(1) }
            } else {
                unsafe { log(0) }
            }
        }

        _ => panic!()
    }

    let s = [2, 0, 3, 4, 5, 6, 7];
    let r = [3, 4, 5, 6];
    
    unsafe {
        log(s.as_ptr() as i64);
        log(r.as_ptr() as i64);
    }
    
}

#[no_mangle]
pub extern "C" fn on_close() {
    let (offset, size) = unsafe {
        read_ledger_meta()
    };

    let ledger_meta = {
        let memory = 0 as *const u8;
        let slice = unsafe {
            let start = memory.offset(offset as isize);
            slice::from_raw_parts(start, size as usize)      
        };

        LedgerCloseMeta::from_xdr(slice).unwrap()
    };

    let ledger_seq = match ledger_meta {
        LedgerCloseMeta::V1(v1) => v1.ledger_header.header.ledger_seq,
        LedgerCloseMeta::V0(v0) => v0.ledger_header.header.ledger_seq,
        LedgerCloseMeta::V2(v2) => v2.ledger_header.header.ledger_seq,
    };

    unsafe {
        log(ledger_seq as i64)
    }

}


#[test]
fn xdr_to_bytes() {
    let xdr = ScVal::from_xdr_base64("AAAADwAAAAdkZXBvc2l0AA==").unwrap();
    println!("{:?}", xdr.to_xdr().unwrap());
}
