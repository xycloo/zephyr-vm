use zephyr_sdk::{prelude::*, soroban_sdk::xdr::ScVal, Condition, DatabaseDerive, EnvClient};

#[derive(DatabaseDerive, Clone)]
#[with_name("curr_seq")]
struct Sequence {
    pub current: ScVal,
}

#[no_mangle]
pub extern "C" fn on_close() {
    let env = EnvClient::new();
    let reader = env.reader();
    env.log().debug(
        format!("test received sequence, {}", reader.ledger_sequence()),
        None,
    );

    let sequence = Sequence {
        current: ScVal::U32(reader.ledger_sequence()),
    };

    sequence.put(&env);
}
