use zephyr_sdk::{prelude::*, soroban_sdk::{Bytes, BytesN, String as SorobanString, Symbol}, EnvClient};

#[no_mangle]
pub extern "C" fn on_close() {
    let env = EnvClient::empty();
    
    let symbol = Symbol::new(&env.soroban(), "testlargersymbol");
    let string = SorobanString::from_str(&env.soroban(), "testlargersymbol");
    let bytes = Bytes::from_array(&env.soroban(), &[32; 32]);
    let vec = bytes.to_alloc_vec();
    let bytesn = BytesN::from_array(&env.soroban(), &[32; 32]);
    let mut vec = [0;32];
    
}
