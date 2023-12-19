use rs_zephyr_sdk::{EntryChanges, EnvClient, TableRow, TypeWrap, TableRows, log};
use stellar_xdr::{LedgerEntry, LedgerEntryData, ScSymbol, ScVal, ScVec, VecM};

#[no_mangle]
pub extern "C" fn on_close() {
    let env = EnvClient::default();
    
    //env.db_write("test", &["price", "asset"], &[&3_i32.to_be_bytes(), &[0; 32]]);

    let read = env.db_read("test", &["price", "asset"]);

    unsafe { log(read.rows[0].row[0].0[0] as i64) }

    /*let (previous_price, previous_asset) = {
        (&read.rows[read.rows.len() - 1].row[0], &read.rows[read.rows.len() - 1].row[1])
    };

    if previous_price.0 == 3_i32.to_be_bytes() && previous_asset.0 == [0; 32] {
        env.db_write("test", &["price", "asset"], &[&3_i32.to_be_bytes(), &[0; 32]]);
    }*/
}

#[test]
fn test() {
    let rows = [TableRow { row: vec![TypeWrap(vec![0, 0, 0, 3]), TypeWrap(vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])] }, TableRow { row: vec![TypeWrap(vec![0, 0, 0, 3]), TypeWrap(vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])] }, TableRow { row: vec![TypeWrap(vec![0, 0, 0, 3]), TypeWrap(vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])] }, TableRow { row: vec![TypeWrap(vec![0, 0, 0, 3]), TypeWrap(vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])] }, TableRow { row: vec![TypeWrap(vec![0, 0, 0, 3]), TypeWrap(vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])] }, TableRow { row: vec![TypeWrap(vec![0, 0, 0, 3]), TypeWrap(vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])] }, TableRow { row: vec![TypeWrap(vec![0, 0, 0, 3]), TypeWrap(vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])] }, TableRow { row: vec![TypeWrap(vec![0, 0, 0, 3]), TypeWrap(vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])] }, TableRow { row: vec![TypeWrap(vec![0, 0, 0, 3]), TypeWrap(vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])] }, TableRow { row: vec![TypeWrap(vec![0, 0, 0, 3]), TypeWrap(vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])] }, TableRow { row: vec![TypeWrap(vec![0, 0, 0, 3]), TypeWrap(vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])] }];
    let read = TableRows {
        rows: rows.to_vec()
    };

    let (previous_price, previous_asset) = {
        (&read.rows[read.rows.len() - 1].row[0], &read.rows[read.rows.len() - 1].row[1])
    };

    if previous_price.0 == 3_i32.to_be_bytes() && previous_asset.0 == [0; 32] {
        println!("1")
    }
}
