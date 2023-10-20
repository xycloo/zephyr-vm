use rs_zephyr_sdk::{Database, EntryChanges, EnvClient, MetaReader};
use stellar_xdr::{LedgerEntry, LedgerEntryData, ScSymbol, ScVal, ScVec, VecM};

const XYCLOANS_CONTRACT: [u8; 32] = [
    228, 86, 123, 16, 235, 194, 45, 195, 51, 232, 164, 150, 178, 46, 102, 251, 216, 147, 78, 42,
    44, 22, 67, 39, 37, 194, 147, 10, 24, 77, 188, 146,
];

fn write_step(state_entry: LedgerEntry, idx: usize, step: &mut [Option<i128>; 2]) {
    let tot_s_val = ScVal::Symbol(ScSymbol("TotSupply".to_string().try_into().unwrap()));
    let key_val = ScVal::Vec(Some(ScVec(VecM::try_from(vec![tot_s_val]).unwrap())));

    if let LedgerEntryData::ContractData(data) = &state_entry.data {
        let contract = match &data.contract {
            stellar_xdr::ScAddress::Contract(id) => id.0,
            stellar_xdr::ScAddress::Account(_) => {
                unreachable!()
            }
        };

        if contract == XYCLOANS_CONTRACT && data.key == ScVal::LedgerKeyContractInstance {
            let val = &data.val;

            if let ScVal::ContractInstance(instance) = val {
                if let Some(map) = &instance.storage {
                    for entry in map.iter() {
                        if entry.key == key_val {
                            if let ScVal::I128(parts) = &entry.val {
                                step[idx] = Some(((parts.hi as i128) << 64) | (parts.lo as i128))
                            }
                        }
                    }
                }
            }
        };
    }
}

#[no_mangle]
pub extern "C" fn on_close() {
    let meta = EnvClient::get_last_ledger_meta();
    let mut step: [Option<i128>; 2] = [None; 2];

    let reader = MetaReader::new(&meta);

    let sequence = reader.ledger_sequence();
    let EntryChanges { state, updated, .. } = reader.v2_ledger_entries();

    for state_entry in state {
        write_step(state_entry, 0, &mut step)
    }

    for updated_entry in updated {
        write_step(updated_entry, 1, &mut step)
    }

    if let [Some(previous), Some(new)] = step {
        let delta: i128 = new - previous;
        Database::write_table(
            "liquidity",
            &["ledger", "delta"],
            &[&sequence.to_be_bytes(), delta.to_be_bytes().as_slice()],
        )
    }
}
