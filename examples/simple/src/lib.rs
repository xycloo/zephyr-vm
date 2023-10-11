use rs_zephyr_sdk::{Database, EnvClient};
use stellar_xdr::{
    LedgerCloseMeta, LedgerEntryChange, LedgerEntryData, ScContractInstance, ScSymbol, ScVal,
    String32, StringM, TransactionMeta, WriteXdr,
};

const XYCLOANS_CONTRACT: [u8; 32] = [0; 32];

#[no_mangle]
pub extern "C" fn on_close() {
    let key_val = ScVal::Symbol(ScSymbol("TotSupply".to_string().try_into().unwrap()));

    let meta = EnvClient::get_last_ledger_meta();
    let mut step = [None; 2];

    match &meta {
        LedgerCloseMeta::V0(_) => (),
        LedgerCloseMeta::V1(_) => (),
        LedgerCloseMeta::V2(v2) => {
            for tx_processing in v2.tx_processing.iter() {
                match &tx_processing.tx_apply_processing {
                    TransactionMeta::V3(meta) => {
                        let ops = &meta.operations;

                        for operation in ops.clone().into_vec() {
                            for change in operation.changes.0.iter() {
                                let state = match &change {
                                    LedgerEntryChange::Removed(_) => None,

                                    LedgerEntryChange::State(state) => Some((state, 0)),

                                    LedgerEntryChange::Updated(state) => Some((state, 1)),

                                    _ => None,
                                };

                                if let Some((state, idx)) = state {
                                    match &state.data {
                                        LedgerEntryData::ContractData(data) => {
                                            let contract = match &data.contract {
                                                stellar_xdr::ScAddress::Contract(id) => id.0,
                                                stellar_xdr::ScAddress::Account(_) => {
                                                    unreachable!()
                                                }
                                            };

                                            if contract == XYCLOANS_CONTRACT {
                                                if data.key == ScVal::LedgerKeyContractInstance {
                                                    let val = &data.val;
                                                    match val {
                                                        ScVal::ContractInstance(instance) => {
                                                            if let Some(map) = &instance.storage {
                                                                for entry in map.iter() {
                                                                    if entry.key == key_val {
                                                                        match &entry.val {
                                                                            ScVal::I128(parts) => {
                                                                                step[idx] = Some(((parts.hi as i128) << 64) | (parts.lo as i128))
                                                                            }
                                                                            _ => ()
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                        _ => (),
                                                    }
                                                }
                                            };
                                        }
                                        _ => (),
                                    }
                                }
                            }
                        }
                    }

                    _ => todo!("unknown xdr structure"),
                }
            }
        }
    };

    if let [Some(new), Some(previous)] = step {
        let delta = new - previous;
        Database::write_table("liquidity", &["delta"], &[delta.to_be_bytes().as_slice()])
    }
}
