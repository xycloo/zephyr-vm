use rs_zephyr_sdk::{utils, AgnosticRequest, EnvClient};
use serde::{Deserialize, Serialize};
use stellar_xdr::next::{LedgerEntry, LedgerEntryData, ScSymbol, ScVal};

#[derive(Deserialize, Serialize)]
pub struct Result {
    entries: Vec<LedgerEntry>,
    count: usize,
}

#[no_mangle]
pub extern "C" fn top_holders() {
    let env = EnvClient::empty();
    let contract_id = stellar_strkey::Contract::from_string(
        "CARDOVHUIQVBDUKEYKCS4YDFFM7VSAHIMKCZ57NZKS6CT7RBEZNRKKL5",
    )
    .unwrap()
    .0;

    let entries = env.read_contract_entries(contract_id).unwrap();

    let top_holders: Vec<LedgerEntry> = entries
        .iter()
        .filter_map(|entry| {
            if let (ScVal::Vec(Some(scvec)), LedgerEntryData::ContractData(data)) =
                (&entry.key, &entry.entry.data)
            {
                if let Some(val) = scvec.get(0) {
                    if val == &ScVal::Symbol(ScSymbol("Balance".try_into().unwrap())) {
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
        .collect();

    
    let request = AgnosticRequest {
        url: "https://tdep.requestcatcher.com/test".into(),
        body: Some("From Zephyr".into()),
        method: rs_zephyr_sdk::Method::Post,
        headers: vec![("Test".into(), "Zephyr".into())]
    };
    
    env.send_web_request(request);

    env.conclude(Result {
        count: top_holders.len(),
        entries: top_holders,
    });
}
