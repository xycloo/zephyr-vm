use rs_zephyr_sdk::{log, utils, AgnosticRequest, EnvClient};
use serde::{Deserialize, Serialize};
use soroban_sdk::{Env, Symbol, TryFromVal, Val};
//use stellar_xdr::next::{LedgerEntry, LedgerEntryData, ScSymbol, ScVal};
//use soroban_env_host::{Compare, Host};
//use soroban_env_common::{Symbol, TryFromVal, TryIntoVal};


fn into_val<T: TryFromVal<Env, Val>>(env: &Env, val: &Val) -> T {
    /*let s = Symbol::new(&env, "tdeptest");

    unsafe {
        if s == Symbol::new(&env, "tdeptest") {
            log(s.as_val().get_payload() as i64)
        }
    };*/

    T::try_from_val(env, val).unwrap()
}


#[derive(Deserialize, Serialize)]
pub struct Result {
    entries: Vec<LedgerEntry>,
    count: usize,
}

#[no_mangle]
pub extern "C" fn top_holders() {
    let soroban_env = Env::default();
    let env = EnvClient::empty();
    let contract_id = stellar_strkey::Contract::from_string(
        "CARDOVHUIQVBDUKEYKCS4YDFFM7VSAHIMKCZ57NZKS6CT7RBEZNRKKL5",
    )
    .unwrap()
    .0;
/* 
    let entries = env.read_contract_entries(contract_id).unwrap();

    let top_holders: Vec<LedgerEntry> = entries
        .iter()
        .filter_map(|entry| {
            if let (ScVal::Vec(Some(scvec)), LedgerEntryData::ContractData(data)) =
                (&entry.key, &entry.entry.data)
            {
                if let Some(val) = scvec.get(0) {
                    if into_val::<Symbol>(&soroban_env, val) == Symbol::new(&soroban_env, "Balance") {
                        if let ScVal::I128(parts) = &data.val {
                            if utils::parts_to_i128(parts) >= 50_000_000_000 {
                                return Some(entry.entry.clone());
                            }
                        }
                    }
                }
            }
            None
        })
        .collect();*/

    let map = env.read_contract_entries_to_env(&soroban_env, contract_id).unwrap();
    let top_holders: Vec<i128> = map.iter().filter_map(|entry| {
        if let soroban_sdk::vec![bal, addr] = into_val::<soroban_sdk::Vec<Val>>(&soroban_env, &entry.0) {
            unsafe {
                log(9 as i64)
            }
        }
    }).collect();
    
    let request = AgnosticRequest {
        url: "https://tdep.requestcatcher.com/test".into(),
        body: Some("From Zephyr".into()),
        method: rs_zephyr_sdk::Method::Post,
        headers: vec![("Test".into(), "Zephyr".into())]
    };
    
    //env.send_web_request(request);

    //use_host();

    env.conclude(Result {
        count: top_holders.len(),
        entries: top_holders,
    });
}
