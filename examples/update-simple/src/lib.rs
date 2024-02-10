use macros::DatabaseInteract;
use rs_zephyr_sdk::{Condition, DatabaseInteract, EnvClient};
use serde::{Deserialize, Serialize};


#[derive(DatabaseInteract, Serialize, Deserialize)]
#[with_name("myledger")] 
pub struct Ledger {
    current: u32,
    previous: u32
}


#[no_mangle]
pub extern "C" fn on_close() {
    let env = EnvClient::new();
    let current = env.reader().ledger_sequence();
    
    let ledger = env.read::<Ledger>();
    if let Some(last) = ledger.last() {
        env.update(&Ledger {
            current,
            previous: last.current
        }, &[Condition::ColumnEqualTo("current".into(), bincode::serialize(&ZephyrVal::U32(last.current)).unwrap())])
    } else {
        env.put(&Ledger {
            current,
            previous: 0
        })
    }
}
