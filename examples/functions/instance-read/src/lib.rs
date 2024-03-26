use rs_zephyr_sdk::{log, utils, EnvClient, ServerlessResult};
use stellar_xdr::next::{ContractDataDurability, Hash, LedgerEntry, LedgerEntryData, LedgerKey, LedgerKeyContractData, LedgerKeyTtl, Limits, ReadXdr, ScAddress, ScSymbol, ScVal, ScVec, VecM, WriteXdr};


#[no_mangle]
pub extern "C" fn mytest() -> ServerlessResult {
    let env = EnvClient::empty();
    let contract_id = stellar_strkey::Contract::from_string("CARDOVHUIQVBDUKEYKCS4YDFFM7VSAHIMKCZ57NZKS6CT7RBEZNRKKL5").unwrap().0;
    let instance = env.read_contract_instance(contract_id).unwrap();

    //let mut to_record_balances = Vec::new();
    
    if let Some(instance) = instance {        
        if let LedgerEntryData::ContractData(data) = instance.entry.data {
            if let ScVal::ContractInstance(instance) = data.val {
                if let Some(map) = instance.storage {
                    let supply = map.iter().find(|entry| entry.key == ScVal::Symbol(ScSymbol("supply".try_into().unwrap())));
                    if let Some(supply) = supply {
                        if let ScVal::I128(n) = &supply.val {
                            if utils::parts_to_i128(&n) > 5_000_000_000 {

                            }
                        }
                    }
                }
            }
        }
    };

    (0, 0)
}
