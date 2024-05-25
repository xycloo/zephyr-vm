use rs_zephyr_sdk::{log, utils, AgnosticRequest, EnvClient};
use serde::{Deserialize, Serialize};
use soroban_sdk::{
    contracttype,
    xdr::{FromXdr, ReadXdr, ScVal, ToXdr},
    Address, Bytes, Env, Symbol, TryFromVal, TryIntoVal, Val,
};
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
pub struct ExecResult {
    total_count: usize,
    top_holders: usize,
    special_address: bool,
}

#[derive(Clone, PartialEq)]
#[contracttype]
pub enum DataKey {
    TokenId,
    TotSupply,
    FeePerShareUniversal,
    Dust,
    Balance(Address),
    FeePerShareParticular(Address),
    MaturedFeesParticular(Address),
}

#[no_mangle]
pub extern "C" fn top_holders() {
    let mut result = ExecResult {
        top_holders: 0,
        total_count: 0,
        special_address: false,
    };

    let env = Env::default();
    let z_env = EnvClient::empty();
    let contract_id = stellar_strkey::Contract::from_string(
        "CARDOVHUIQVBDUKEYKCS4YDFFM7VSAHIMKCZ57NZKS6CT7RBEZNRKKL5",
    )
    .unwrap()
    .0;
    let addr = soroban_sdk::Address::from_string(&soroban_sdk::String::from_str(
        &env,
        "GBLEJ7XTXCVHOCLPLM33JLBKT3OVOQXL3KRRWY6UUY44UO4XNWC44ZNM",
    ));

    let map = z_env
        .read_contract_entries_to_env(&env, contract_id)
        .unwrap();
    let top_holders: Vec<i128> = map
        .iter()
        .filter_map(|entry| {
            if DataKey::Balance(addr.clone()) == entry.0.try_into_val(&env).unwrap() {
                result.special_address = true;
            }

            let vec: soroban_sdk::Vec<Val> =
                into_val::<soroban_sdk::Vec<_>>(&env, &entry.0).unwrap();

            if into_val::<Symbol>(&env, &vec.get(0).unwrap()).unwrap()
                == Symbol::new(&env, "Balance")
            {
                result.total_count += 1;
                into_val::<i128>(&env, &entry.1).filter(|&n| n >= 50_000_000_000)
            } else {
                None
            }
        })
        .collect();

    result.top_holders = top_holders.len();

    z_env.conclude(result)
}
