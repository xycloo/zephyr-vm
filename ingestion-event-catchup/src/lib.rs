use std::{collections::{BTreeMap, HashMap}, env, rc::Rc, str::FromStr, sync::Arc};
use ledger::sample_ledger;
use query::{get_query, EventNode};
use reqwest::header::{HeaderMap, HeaderName};
use rs_zephyr_common::{http::{AgnosticRequest, Method}, ContractDataEntry, RelayedMessageRequest};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use stellar_xdr::next::{ContractEvent, ContractEventV0, Hash, LedgerCloseMeta, LedgerCloseMetaExt, LedgerCloseMetaV1, LedgerEntry, LedgerEntryChanges, LedgerHeader, LedgerHeaderHistoryEntry, Limits, OperationMeta, ReadXdr, ScAddress, ScVal, SorobanTransactionMeta, TransactionMetaV3, TransactionResult, TransactionResultMeta, TransactionResultPair, TransactionResultResult, WriteXdr};
use tokio::sync::mpsc::UnboundedSender;
use zephyr::{db::ledger::LedgerStateRead, host::Host, vm::Vm, ZephyrStandard};

use crate::database::MercuryDatabase;

mod database;
mod query;
mod ledger;

#[derive(Clone)]
pub struct LedgerReader {
    path: String
}

impl ZephyrStandard for LedgerReader {
    fn zephyr_standard() -> anyhow::Result<Self>
        where
            Self: Sized {

        Ok(Self { path: "/tmp/rs_ingestion_temp/stellar.db".into() })
    }
}

impl LedgerStateRead for LedgerReader {
    fn read_contract_data_entry_by_contract_id_and_key(&self, contract: ScAddress, key: ScVal) -> Option<ContractDataEntry> {        
        let conn = Connection::open(&self.path).unwrap();
        let query_string = format!("SELECT contractid, key, ledgerentry, \"type\", lastmodified FROM contractdata where contractid = ?1 AND key = ?2");
        
        let mut stmt = conn.prepare(&query_string).unwrap();
        let entries = stmt.query_map(params![contract.to_xdr_base64(Limits::none()).unwrap(), key.to_xdr_base64(Limits::none()).unwrap()], |row| {
            
            Ok(ContractDataEntry {
                contract_id: contract.clone(),
                key: ScVal::from_xdr_base64(row.get::<usize, String>(1).unwrap(), Limits::none()).unwrap(),
                entry: LedgerEntry::from_xdr_base64(row.get::<usize, String>(2).unwrap(), Limits::none()).unwrap(),
                durability: row.get(3).unwrap(),
                last_modified: row.get(4).unwrap(),
            })
        });

        let entries = entries.unwrap().map(|r| r.unwrap()).collect::<Vec<ContractDataEntry>>();

        entries.get(0).cloned()
    }
    
    fn read_contract_data_entries_by_contract_id(&self, contract: ScAddress) -> Vec<ContractDataEntry> {
        println!("address {}", contract.to_xdr_base64(Limits::none()).unwrap());
        let conn = Connection::open(&self.path).unwrap();

        let query_string = format!("SELECT contractid, key, ledgerentry, \"type\", lastmodified FROM contractdata where contractid = ?1");
        
        let mut stmt = conn.prepare(&query_string).unwrap();
        let entries = stmt.query_map(params![contract.to_xdr_base64(Limits::none()).unwrap()], |row| {
            let entry = ContractDataEntry {
                contract_id: contract.clone(),
                key: ScVal::from_xdr_base64(row.get::<usize, String>(1).unwrap(), Limits::none()).unwrap(),
                entry: LedgerEntry::from_xdr_base64(row.get::<usize, String>(2).unwrap(), Limits::none()).unwrap(),
                durability: row.get(3).unwrap(),
                last_modified: row.get(4).unwrap(),
            };
            
            Ok(entry)
        });

        entries.unwrap().map(|r| r.unwrap()).collect::<Vec<ContractDataEntry>>()
    }
}


#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InvokeZephyrFunction {
    fname: String,
    arguments: String
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ExecutionMode {
    EventCatchup(Vec<String>),
    Function(InvokeZephyrFunction)
}

/// NB: This is meant for internal API use.
/// This is unsafe to extern.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FunctionRequest {
    binary_id: u32,
    user_id: u32,
    jwt: String,
    pub mode: ExecutionMode
}

#[derive(Clone, Debug)]
pub struct ExecutionWrapper {
    request: FunctionRequest
}

impl ExecutionWrapper {
    pub fn new(request: FunctionRequest) -> Self {
        Self {
            request
        }
    }
    
    pub async fn retrieve_events(&self, contracts_ids: &[String]) -> query::Response {
        let jwt = &self.request.jwt;
        let network = std::env::var("NETWORK").unwrap_or_else(|e| panic!("{}: {}", "NETWORK", e));
        
        let client = reqwest::Client::new();
        
        let graphql_endpoint = if network == "mainnet" {
            "https://mainnet.mercurydata.app:2083/graphql"
        } else {
            "https://api.mercurydata.app:2083/graphql"
        };
        
        let res = client.post(graphql_endpoint)
            .bearer_auth(jwt)
            .json(&get_query(contracts_ids))
            .send()
            .await.unwrap();

        let resp: crate::query::Response = res.json().await.unwrap();

        resp
    }

    pub fn build_transitions_from_events(events_response: query::Response) -> Vec<LedgerCloseMeta> {
        let mut all_events_by_ledger: BTreeMap<i64, Vec<EventNode>> = BTreeMap::new();
        
        for event in events_response.data.eventByContractId.nodes {
            let seq = event.txInfoByTx.ledgerByLedger.sequence;
            if all_events_by_ledger.contains_key(&seq) {
                let mut other_events: Vec<EventNode> = all_events_by_ledger.get(&seq).unwrap().to_vec();
                other_events.push(event);
                all_events_by_ledger.insert(seq, other_events);
            } else {
                all_events_by_ledger.insert(seq, vec![event]);
            }
        }

        let mut metas = Vec::new();
        for (ledger, event_set) in all_events_by_ledger.iter() {
            let meta = LedgerCloseMeta::from_xdr_base64(sample_ledger(), Limits::none()).unwrap();            
            let mut v1 = if let LedgerCloseMeta::V1(mut v1) = meta {
                v1.ledger_header.header.ledger_seq = *ledger as u32;
                v1
            } else {panic!()};

            let mut mut_tx_processing = v1.tx_processing.to_vec();

            for event in event_set {
                let result = TransactionResultMeta {
                    result: TransactionResultPair {
                        transaction_hash: Hash([0;32]),
                        result: TransactionResult {
                            fee_charged: 0,
                            result: TransactionResultResult::TxSuccess(vec![].try_into().unwrap()),
                            ext: stellar_xdr::next::TransactionResultExt::V0
                        }
                    },
                    fee_processing: LedgerEntryChanges(vec![].try_into().unwrap()),
                    tx_apply_processing: stellar_xdr::next::TransactionMeta::V3(TransactionMetaV3 {
                        ext: stellar_xdr::next::ExtensionPoint::V0,
                        tx_changes_before: LedgerEntryChanges(vec![].try_into().unwrap()),
                        tx_changes_after: LedgerEntryChanges(vec![].try_into().unwrap()),
                        operations: vec![OperationMeta {
                            changes: LedgerEntryChanges(vec![].try_into().unwrap())
                        }].try_into().unwrap(),
                        soroban_meta: Some(SorobanTransactionMeta {
                            ext: stellar_xdr::next::SorobanTransactionMetaExt::V0,
                            return_value: ScVal::Void,
                            diagnostic_events: vec![].try_into().unwrap(),
                            events: vec![ContractEvent {
                                ext: stellar_xdr::next::ExtensionPoint::V0,
                                contract_id: Some(Hash(stellar_strkey::Contract::from_string(&event.contractId).unwrap().0)),
                                type_: stellar_xdr::next::ContractEventType::Contract,
                                body: stellar_xdr::next::ContractEventBody::V0(ContractEventV0 {
                                    topics: vec![ScVal::from_xdr_base64(event.topic1.clone().unwrap_or("".into()), Limits::none()).unwrap_or(ScVal::Void),
                                    ScVal::from_xdr_base64(event.topic2.clone().unwrap_or("".into()), Limits::none()).unwrap_or(ScVal::Void),
                                    ScVal::from_xdr_base64(event.topic3.clone().unwrap_or("".into()), Limits::none()).unwrap_or(ScVal::Void),
                                    ScVal::from_xdr_base64(event.topic4.clone().unwrap_or("".into()), Limits::none()).unwrap_or(ScVal::Void)].try_into().unwrap(),
                                    data: ScVal::from_xdr_base64(event.data.clone(), Limits::none()).unwrap_or(ScVal::Void)
                                })
                                
                            }].try_into().unwrap()
                        })

                    })
                };

                mut_tx_processing.push(result)
            }

            v1.tx_processing = mut_tx_processing.try_into().unwrap();
            metas.push(LedgerCloseMeta::V1(v1))
        }

        metas
    }

    pub async fn catchup_spawn_jobs(&self) -> String {
        println!("executing {:?}", self.request);
        match &self.request.mode {
            ExecutionMode::EventCatchup(contract_ids) => {
                let events = self.retrieve_events(contract_ids.as_slice()).await;
                let metas = Self::build_transitions_from_events(events);

                for meta in metas {
                    self.reproduce_async_runtime(Some(meta), None).await;
                };

                "Catchup complete".into()
            }

            ExecutionMode::Function(function) => {
                self.reproduce_async_runtime(None, Some(function)).await        
            }
        }
    }

    pub async fn reproduce_async_runtime(&self, meta: Option<LedgerCloseMeta>, function: Option<&InvokeZephyrFunction>) -> String {
        let handle = tokio::runtime::Handle::current();
        
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        
        let cloned = self.clone();
        
        let binary = database::execution::read_binary(self.request.binary_id as i64).await;
        
        let join_handle = match meta {
            Some(meta) => {
                let join_handle = handle.spawn_blocking(move || {
                    cloned.execute_with_transition( tx, meta, binary)
                });

                join_handle
            }
            None => {
                let function = function.cloned().unwrap();
                let join_handle = handle.spawn_blocking(move || {
                    cloned.execute_function(tx, binary, function)
                });

                join_handle
            }
        };

        let resp = join_handle.await.unwrap();
        
        let _ = tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                let request: RelayedMessageRequest = bincode::deserialize(&message).unwrap();
                
                match request {
                    RelayedMessageRequest::Http(request) => {
                        let client = reqwest::Client::new();                
                        let mut headers = HeaderMap::new();
                        for (k, v) in &request.headers {
                            
                            headers.insert(HeaderName::from_str(&k).unwrap(), v.parse().unwrap());
                        }

                        let builder = match request.method {
                            Method::Get => {
                                let builder = client.get(&request.url).headers(headers);

                                if let Some(body) = &request.body {
                                    builder.body(body.clone())
                                } else {
                                    builder
                                }
                            },

                            Method::Post => {
                                let builder = client.post(&request.url).headers(headers);

                                if let Some(body) = &request.body {
                                    builder.body(body.clone())
                                } else {
                                    builder
                                }
                            }
                        };

                        // We ignore the result of the request.
                        let _ = builder.send().await;
                    }

                    RelayedMessageRequest::Log(log) => {
                        println!("{:?}", log);
                    }
                }
            }
        }).await;
        
        resp
    }
}

impl ExecutionWrapper {
    fn execute_with_transition(&self,sender:UnboundedSender<Vec<u8>>, transition:LedgerCloseMeta, binary: Vec<u8>) -> String {
        let mut host = Host::<MercuryDatabase, LedgerReader>::from_id(self.request.user_id as i64).unwrap();
        host.add_transmitter(sender);

        let start = std::time::Instant::now();
        let vm = Vm::new(&host, &binary).unwrap();
        
        host.load_context(Rc::downgrade(&vm)).unwrap();
        host.add_ledger_close_meta(transition.to_xdr(Limits::none()).unwrap()).unwrap();
        let res = vm.metered_function_call(&host, "on_close").unwrap_or("no response".into());

        println!("{res}: elapsed {:?}", start.elapsed());

        "execution successful".into()
    }

    fn execute_function(&self,sender:UnboundedSender<Vec<u8>>, binary: Vec<u8>, function: InvokeZephyrFunction) -> String {
        let mut host = Host::<MercuryDatabase, LedgerReader>::from_id(self.request.user_id as i64).unwrap();
        host.add_transmitter(sender);

        let start = std::time::Instant::now();
        let vm = Vm::new(&host, &binary).unwrap();
        
        host.load_context(Rc::downgrade(&vm)).unwrap();
        println!("{:?}", serde_json::from_str::<serde_json::Value>(&function.arguments));
        host.add_ledger_close_meta(bincode::serialize(&function.arguments).unwrap()).unwrap();
        
        let res = vm.metered_function_call(&host, &function.fname).unwrap_or("no response".into());

        println!("{res}: elapsed {:?}", start.elapsed());

        res
    }
}