use rs_zephyr_sdk::{log, utils, AgnosticRequest, EnvClient};
use serde::{Deserialize, Serialize};
use soroban_sdk::{Env, Symbol, TryFromVal, Val};
//use stellar_xdr::next::{LedgerEntry, LedgerEntryData, ScSymbol, ScVal};
//use soroban_env_host::{Compare, Host};
//use soroban_env_common::{Symbol, TryFromVal, TryIntoVal};


fn into_val<T: TryFromVal<Env, Val>>(env: &Env, val: &Val) -> Option<T> {
    if let Ok(v) = T::try_from_val(env, val) {
        Some(v)
    } else {
        None
    }
}


#[derive(Deserialize, Serialize)]
pub struct Result {
    entries: Vec<i128>,
    count: usize,
}

#[no_mangle]
pub extern "C" fn top_holders() {
    let env = Env::default();
    let z_env = EnvClient::empty();
    let contract_id = stellar_strkey::Contract::from_string(
        "CARDOVHUIQVBDUKEYKCS4YDFFM7VSAHIMKCZ57NZKS6CT7RBEZNRKKL5",
    )
    .unwrap()
    .0;

    let map = z_env.read_contract_entries_to_env(&env, contract_id).unwrap();
    
    let top_holders: Vec<i128> = map.iter().filter_map(|entry| {
        let vec: soroban_sdk::Vec<Val> = into_val::<soroban_sdk::Vec<_>>(&env, &entry.0).unwrap();
        
        if into_val::<Symbol>(&env, &vec.get(0).unwrap()).unwrap() == Symbol::new(&env, "Balance") {
            into_val::<i128>(&env, &entry.1).filter(|&n| n >= 50_000_000_000)
        } else {
            None
        }
    }).collect();
    
    unsafe {
        for holder in top_holders {
            log(holder as i64)
        }
    }
}
