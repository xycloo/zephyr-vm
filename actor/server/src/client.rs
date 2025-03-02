use std::collections::BTreeMap;

use anyhow::Result;
use async_trait::async_trait;
use base64::{prelude::BASE64_STANDARD, Engine};
use ledger::sample_ledger;
use mercury::query::{DataAfterLedger, EventNode};
use stellar_xdr::next::*;

use crate::{execute_function, BinaryType, FInput};

pub mod mercury;
mod ledger;

#[async_trait]
pub trait EventFeed {
    async fn events(&self, contract_ids: Vec<String>, topics: [Vec<String>; 4], start_at_ledger: i64) -> Result<mercury::query::ResponseAfterLedger>;
}

fn to_txhash(v: Vec<u8>) -> [u8; 32] {
    v.try_into().expect("corrupt transaction hash received")
}

pub async fn do_catchups_on_events(
    events_response: DataAfterLedger,
    start_ledger: i64,
) -> i64 {
    let mut all_events_by_ledger: BTreeMap<i64, (i64, Vec<EventNode>)> = BTreeMap::new();
    

    for event in events_response.eventByContractIds.nodes {
        let seq = event.txInfoByTx.ledgerByLedger.sequence;
        let time = event.txInfoByTx.ledgerByLedger.closeTime;
        all_events_by_ledger
            .entry(seq)
            .and_modify(|(t, events)| events.push(event.clone()))
            .or_insert((time, vec![event]));
    }

    println!("got {} ledgers to process", all_events_by_ledger.len());
    let mut latest_ledger = start_ledger;
    for (ledger, (time, event_set)) in all_events_by_ledger.iter() {
        let meta = LedgerCloseMeta::from_xdr_base64(sample_ledger(), Limits::none()).unwrap();
        let mut v1 = if let LedgerCloseMeta::V1(mut v1) = meta {
            v1.ledger_header.header.ledger_seq = *ledger as u32;
            v1.ledger_header.header.scp_value.close_time = TimePoint(*time as u64);
            v1
        } else {
            panic!()
        };

        let mut mut_tx_processing = v1.tx_processing.to_vec();
        mut_tx_processing.clear();

        for event in event_set {
            let event_tx_hash = {
                let vec = BASE64_STANDARD
                    .decode(&event.txInfoByTx.txHash)
                    .unwrap_or([0; 32].to_vec());
                to_txhash(vec)
            };

            let result = TransactionResultMeta {
                result: TransactionResultPair {
                    transaction_hash: Hash(event_tx_hash),
                    result: TransactionResult {
                        fee_charged: 0,
                        result: TransactionResultResult::TxSuccess(vec![].try_into().unwrap()),
                        ext: TransactionResultExt::V0,
                    },
                },
                fee_processing: LedgerEntryChanges(vec![].try_into().unwrap()),
                tx_apply_processing: TransactionMeta::V3(
                    TransactionMetaV3 {
                        ext: ExtensionPoint::V0,
                        tx_changes_before: LedgerEntryChanges(vec![].try_into().unwrap()),
                        tx_changes_after: LedgerEntryChanges(vec![].try_into().unwrap()),
                        operations: vec![OperationMeta {
                            changes: LedgerEntryChanges(vec![].try_into().unwrap()),
                        }]
                        .try_into()
                        .unwrap(),
                        soroban_meta: Some(SorobanTransactionMeta {
                            ext: SorobanTransactionMetaExt::V0,
                            return_value: ScVal::Void,
                            diagnostic_events: vec![].try_into().unwrap(),
                            events: vec![ContractEvent {
                                ext: ExtensionPoint::V0,
                                contract_id: Some(Hash(
                                    stellar_strkey::Contract::from_string(&event.contractId)
                                        .unwrap()
                                        .0,
                                )),
                                type_: ContractEventType::Contract,
                                body: ContractEventBody::V0(
                                    ContractEventV0 {
                                        topics: vec![
                                            ScVal::from_xdr_base64(
                                                event.topic1.clone().unwrap_or("".into()),
                                                Limits::none(),
                                            )
                                            .unwrap_or(ScVal::Void),
                                            ScVal::from_xdr_base64(
                                                event.topic2.clone().unwrap_or("".into()),
                                                Limits::none(),
                                            )
                                            .unwrap_or(ScVal::Void),
                                            ScVal::from_xdr_base64(
                                                event.topic3.clone().unwrap_or("".into()),
                                                Limits::none(),
                                            )
                                            .unwrap_or(ScVal::Void),
                                            ScVal::from_xdr_base64(
                                                event.topic4.clone().unwrap_or("".into()),
                                                Limits::none(),
                                            )
                                            .unwrap_or(ScVal::Void),
                                        ]
                                        .try_into()
                                        .unwrap(),
                                        data: ScVal::from_xdr_base64(
                                            event.data.clone(),
                                            Limits::none(),
                                        )
                                        .unwrap_or(ScVal::Void),
                                    },
                                ),
                            }]
                            .try_into()
                            .unwrap(),
                        }),
                    },
                ),
            };

            mut_tx_processing.push(result)
        }

        v1.tx_processing = mut_tx_processing.try_into().unwrap();
        let ledger_close_meta = LedgerCloseMeta::V1(v1);
        let function = FInput {
            associated_data: ledger_close_meta
                .to_xdr(Limits::none())
                .expect("corrupt data received while building function"),
            binary: BinaryType::Path(
                std::env::var("WASM_PATH").expect("missing WASM_PATH from environment"),
            ),
            user_id: 0,
            network_id: [0; 32],
            fname: "on_close".into(),
        };

        if let Err(e) = execute_function(function).await {
            println!("Got error {:?} while executing function", e);
        }

        latest_ledger = *ledger
    }

    latest_ledger
}
