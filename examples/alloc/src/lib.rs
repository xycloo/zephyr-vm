use core::slice;
use stellar_xdr::{LedgerCloseMeta, ReadXdr, ScVal, TransactionMeta, WriteXdr};

extern crate wee_alloc;

extern "C" {
    #[allow(improper_ctypes)] // we alllow as we enabled multi-value
    #[link_name = "read_raw"]
    fn read_raw() -> (i64, i64);

    #[allow(improper_ctypes)] // we alllow as we enabled multi-value
    #[link_name = "write_raw"]
    fn write_raw();

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

/*fn db_read_test() {
    unsafe {
        env_push_stack(12348);
        env_push_stack(1);
        env_push_stack(123181);
    };

    let (offset, size) = unsafe { read_raw() };

    let memory: *const u8 = 0 as *const u8;

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

        _ => panic!(),
    }

    let s = [2, 0, 3, 4, 5, 6, 7];
    let r = [3, 4, 5, 6];

    unsafe {
        log(s.as_ptr() as i64);
        log(r.as_ptr() as i64);
    }
}

fn db_write_test() {
    let c1_value: [u8; 8] = [2, 0, 5, 6, 7, 2, 3, 4];

    unsafe {
        env_push_stack(12348);
        env_push_stack(1);
        env_push_stack(123181);
        env_push_stack(1);
        env_push_stack(c1_value.as_ptr() as i64);
        env_push_stack(c1_value.len() as i64);
    };

    unsafe { write_raw() }
}
*/
fn get_ledger_meta_test() {
    let (offset, size) = unsafe { read_ledger_meta() };

    let ledger_meta = {
        let memory = 0 as *const u8;
        let slice = unsafe {
            let start = memory.offset(offset as isize);
            slice::from_raw_parts(start, size as usize)
        };

        ScVal::from_xdr(slice)
    };

    match ledger_meta {
        Ok(ScVal::Bool(true)) => unsafe { log(0) },
        Ok(ScVal::Bool(false)) => unsafe { log(1) },
        _ => (),
    }
}

#[no_mangle]
pub extern "C" fn on_close() {
    get_ledger_meta_test();
    //unsafe { log(2) }
}

#[test]
fn t() {
    let meta = stellar_xdr::TransactionMeta::from_xdr_base64("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").unwrap();
    println!("{:?}", meta.to_xdr().unwrap());
}

#[test]
fn test() {
    let addr = stellar_strkey::Contract::from_string(
        "CBYTTONE7AK2IEPRQUIPAJF6G35KE6HQCA3RFZWKH4HZQGIVQANUMVAN",
    )
    .unwrap();

    println!("{:?}", addr);
}

#[test]
fn test_change() {
    let d = [
        0, 0, 0, 19, 0, 0, 0, 0, 82, 186, 155, 50, 82, 157, 242, 23, 255, 202, 33, 171, 173, 37,
        223, 82, 121, 119, 62, 116, 17, 232, 253, 190, 80, 237, 208, 20, 213, 29, 96, 130, 0, 0, 0,
        1, 0, 0, 0, 2, 0, 0, 0, 16, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 15, 0, 0, 0, 7, 84, 111, 107,
        101, 110, 73, 100, 0, 0, 0, 0, 18, 0, 0, 0, 1, 215, 146, 139, 114, 194, 112, 60, 207, 234,
        247, 235, 159, 244, 239, 77, 80, 74, 85, 168, 185, 121, 252, 155, 69, 14, 162, 200, 66,
        180, 209, 206, 97, 0, 0, 0, 16, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 15, 0, 0, 0, 9, 84, 111,
        116, 83, 117, 112, 112, 108, 121, 0, 0, 0, 0, 0, 0, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        65, 144, 171, 0,
    ];

    let val = ScVal::from_xdr_base64("AAAAEAAAAAEAAAACAAAADgAAAAdCYWxhbmNlAAAAAA4AAAA4R0NHT1JCRDVEQjRKRElLVklBNTM2Q0pFM0VXTVdaNktCVUJXWldSUU03WTNOSEZSQ0xPS1lWQUw=").unwrap();
    println!("{:?} {}", val, val.to_xdr().unwrap().len());
}

#[test]
fn to_contract() {
    let c = stellar_strkey::Contract([
        215, 146, 139, 114, 194, 112, 60, 207, 234, 247, 235, 159, 244, 239, 77, 80, 74, 85, 168,
        185, 121, 252, 155, 69, 14, 162, 200, 66, 180, 209, 206, 97,
    ]);

    println!("{:?}", c.to_string());
}

#[test]
fn gen_scval() {
    let val = ScVal::Bool(true);
    println!("{:?}", val.to_xdr());
}
