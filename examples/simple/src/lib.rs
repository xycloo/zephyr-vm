use rs_zephyr_sdk::{Database, EnvClient, MetaReader, EntryChanges};
use stellar_xdr::{
    LedgerCloseMeta, LedgerEntryChange, LedgerEntryData, ScContractInstance, ScSymbol, ScVal,
    String32, StringM, TransactionMeta, WriteXdr, LedgerEntry,
};

const XYCLOANS_CONTRACT: [u8; 32] = [0; 32];

fn write_step(state_entry: LedgerEntry, idx: usize, step: &mut [Option<i128>; 2]) {
    let key_val = ScVal::Symbol(ScSymbol("TotSupply".to_string().try_into().unwrap()));

    match &state_entry.data {
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

#[no_mangle]
pub extern "C" fn on_close() {
    let meta = EnvClient::get_last_ledger_meta();
    let mut step = [None; 2];

    let reader = MetaReader::new(&meta);
    let EntryChanges { state, updated, .. } = reader.v2_ledger_entries();

    for state_entry in state {
        write_step(state_entry, 0, &mut step)
    }

    for updated_entry in updated {
        write_step(updated_entry, 1, &mut step)
    }

    if let [Some(new), Some(previous)] = step {
        let delta = new - previous;
        Database::write_table("liquidity", &["delta"], &[delta.to_be_bytes().as_slice()])
    }
}
