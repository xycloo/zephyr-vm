use rs_zephyr_sdk::EnvClient;

#[no_mangle]
pub extern "C" fn on_close() {
    let env = EnvClient::default();
    
    env.db_write("test", &["price", "asset"], &[&3_i32.to_be_bytes(), &[0; 32]]).unwrap();

    let read = env.db_read("test", &["price", "asset"]).unwrap();

    let (previous_price, previous_asset) = {
        (&read.rows[read.rows.len() - 1].row[0], &read.rows[read.rows.len() - 1].row[1])
    };

    if previous_price.0 == 3_i32.to_be_bytes() && previous_asset.0 == [0; 32] {
        env.db_write("yay", &["res"], &[&1_u8.to_be_bytes()]).unwrap();
    }
}
