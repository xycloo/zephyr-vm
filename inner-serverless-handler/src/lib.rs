use ledger::sample_ledger;
use postgres::NoTls;
use query::{get_query, get_query_after_ledger, EventNode};
use reqwest::header::{HeaderMap, HeaderName};
use rs_zephyr_common::{http::Method, ContractDataEntry, RelayedMessageRequest};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use soroban_env_host::xdr::{
    ContractEvent, ContractEventV0, Hash, LedgerCloseMeta, LedgerEntry, LedgerEntryChanges, Limits,
    OperationMeta, ReadXdr, ScAddress, ScVal, SorobanTransactionMeta, TimePoint, TransactionMetaV3,
    TransactionResult, TransactionResultMeta, TransactionResultPair, TransactionResultResult,
    WriteXdr,
};
use std::{collections::BTreeMap, env, rc::Rc, str::FromStr};
use tokio::{runtime::Handle, sync::mpsc::UnboundedSender, task::JoinHandle};
use zephyr::{db::ledger::LedgerStateRead, host::Host, vm::Vm, ZephyrStandard};

use crate::database::MercuryDatabase;

pub mod caching;
mod database;
pub mod jobs_manager;
mod ledger;
mod query;

#[derive(Clone)]
pub struct LedgerReader {
    path: String,
}

impl ZephyrStandard for LedgerReader {
    fn zephyr_standard() -> anyhow::Result<Self>
    where
        Self: Sized,
    {
        Ok(Self {
            path: "/tmp/rs_ingestion_temp/stellar.db".into(),
        })
    }
}

impl LedgerStateRead for LedgerReader {
    fn read_contract_data_entry_by_contract_id_and_key(
        &self,
        contract: ScAddress,
        key: ScVal,
    ) -> Option<ContractDataEntry> {
        let conn = Connection::open(&self.path).unwrap();
        let query_string = format!("SELECT contractid, key, ledgerentry, \"type\", lastmodified FROM contractdata where contractid = ?1 AND key = ?2");

        let mut stmt = conn.prepare(&query_string).unwrap();
        let entries = stmt.query_map(
            params![
                contract.to_xdr_base64(Limits::none()).unwrap(),
                key.to_xdr_base64(Limits::none()).unwrap()
            ],
            |row| {
                Ok(ContractDataEntry {
                    contract_id: contract.clone(),
                    key: ScVal::from_xdr_base64(
                        row.get::<usize, String>(1).unwrap(),
                        Limits::none(),
                    )
                    .unwrap(),
                    entry: LedgerEntry::from_xdr_base64(
                        row.get::<usize, String>(2).unwrap(),
                        Limits::none(),
                    )
                    .unwrap(),
                    durability: row.get(3).unwrap(),
                    last_modified: row.get(4).unwrap(),
                })
            },
        );

        let entries = entries
            .unwrap()
            .map(|r| r.unwrap())
            .collect::<Vec<ContractDataEntry>>();

        entries.get(0).cloned()
    }

    fn read_contract_data_entries_by_contract_id(
        &self,
        contract: ScAddress,
    ) -> Vec<ContractDataEntry> {
        println!(
            "address {}",
            contract.to_xdr_base64(Limits::none()).unwrap()
        );
        let conn = Connection::open(&self.path).unwrap();

        let query_string = format!("SELECT contractid, key, ledgerentry, \"type\", lastmodified FROM contractdata where contractid = ?1");

        let mut stmt = conn.prepare(&query_string).unwrap();
        let entries = stmt.query_map(
            params![contract.to_xdr_base64(Limits::none()).unwrap()],
            |row| {
                let entry = ContractDataEntry {
                    contract_id: contract.clone(),
                    key: ScVal::from_xdr_base64(
                        row.get::<usize, String>(1).unwrap(),
                        Limits::none(),
                    )
                    .unwrap(),
                    entry: LedgerEntry::from_xdr_base64(
                        row.get::<usize, String>(2).unwrap(),
                        Limits::none(),
                    )
                    .unwrap(),
                    durability: row.get(3).unwrap(),
                    last_modified: row.get(4).unwrap(),
                };

                Ok(entry)
            },
        );

        entries
            .unwrap()
            .map(|r| r.unwrap())
            .collect::<Vec<ContractDataEntry>>()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InvokeZephyrFunction {
    pub fname: String,
    arguments: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ExecutionMode {
    EventCatchup(Vec<String>),
    Function(InvokeZephyrFunction),
}

/// NB: This is meant for internal API use.
/// This is unsafe to extern.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FunctionRequest {
    pub binary_id: u32,
    user_id: u32,
    jwt: String,
    pub mode: ExecutionMode,
}

impl FunctionRequest {
    pub fn needs_job(&self) -> bool {
        if let ExecutionMode::EventCatchup(_) = self.mode {
            true
        } else {
            false
        }
    }

    pub fn dashboard(binary_id: u32, user_id: u32) -> Self {
        Self {
            binary_id,
            user_id,
            jwt: "".into(),
            mode: ExecutionMode::Function(InvokeZephyrFunction {
                fname: "dashboard".into(),
                arguments: "{}".into(),
            }),
        }
    }
}

pub async fn zephyr_update_status(user: i32, running: bool) {
    let postgres_args: String = env::var("INGESTOR_DB").unwrap();

    let (client, connection) = tokio_postgres::connect(&postgres_args, NoTls)
        .await
        .unwrap();

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {}", e);
        }
    });

    let stmt = client
        .prepare_typed(
            "UPDATE public.zephyr_programs SET running = $1 WHERE user_id = $2",
            &[
                tokio_postgres::types::Type::BOOL,
                tokio_postgres::types::Type::INT8,
            ],
        )
        .await
        .unwrap();

    client
        .execute(&stmt, &[&running, &(user as i64)])
        .await
        .unwrap();
}

#[derive(Clone, Debug)]
pub struct ExecutionWrapper {
    request: FunctionRequest,
    network: String,
}

impl ExecutionWrapper {
    pub fn new(request: FunctionRequest, network: String) -> Self {
        Self { request, network }
    }

    pub async fn retrieve_events(&self, contracts_ids: &[String]) -> query::Response {
        let jwt = &self.request.jwt;

        let client = reqwest::Client::new();

        let graphql_endpoint = if env::var("LOCAL").unwrap() == "true" {
            "http://localhost:8084/graphql"
        } else if &self.network == "Public Global Stellar Network ; September 2015" {
            "https://mainnet.mercurydata.app:2083/graphql"
        } else {
            "https://api.mercurydata.app:2083/graphql"
        };

        let res = client
            .post(graphql_endpoint)
            .bearer_auth(jwt)
            .json(&get_query(contracts_ids))
            .send()
            .await
            .unwrap();

        let resp: crate::query::Response = res.json().await.unwrap();

        resp
    }

    pub async fn retrieve_events_after_ledger(
        &self,
        contracts_ids: &[String],
        ledger: i64,
    ) -> query::Response {
        let jwt = &self.request.jwt;

        let client = reqwest::Client::new();

        let graphql_endpoint = if env::var("LOCAL").unwrap() == "true" {
            "http://localhost:8084/graphql"
        } else if &self.network == "Public Global Stellar Network ; September 2015" {
            "https://mainnet.mercurydata.app:2083/graphql"
        } else {
            "https://api.mercurydata.app:2083/graphql"
        };

        let res = client
            .post(graphql_endpoint)
            .bearer_auth(jwt)
            .json(&get_query_after_ledger(contracts_ids, ledger))
            .send()
            .await
            .unwrap();

        let resp: crate::query::ResponseAfterLedger = res.json().await.unwrap();
        let resp = crate::query::Response {
            data: crate::query::Data {
                eventByContractIds: resp.data.eventByContractIds,
            },
        };

        resp
    }

    async fn get_current_ledger_sequence() -> i64 {
        let handle = Handle::current();
        let res = handle
            .spawn_blocking(move || {
                let conn = Connection::open("/tmp/rs_ingestion_temp/stellar.db").unwrap();
                let query_string =
                    format!("SELECT ledgerseq FROM ledgerheaders ORDER BY ledgerseq DESC LIMIT 1");

                let mut stmt = conn.prepare(&query_string).unwrap();
                let mut entries = stmt.query(params![]).unwrap();

                let row = entries.next().unwrap();

                if row.is_none() {
                    // TODO: error log
                    println!("unrecoverable: no ledger running");
                    return 0;
                }

                row.unwrap().get(0).unwrap_or(0_i32)
            })
            .await
            .unwrap();

        res.into()
    }

    async fn recursion_catchups(runtime: Self, events_response: query::Response) {
        println!(
            "turning off live ingestion for {}",
            runtime.request.binary_id as i32
        );
        let handle = Handle::current();
        handle
            .spawn_blocking(move || async move {
                zephyr_update_status(runtime.request.user_id as i32, false).await;
            })
            .await
            .unwrap()
            .await;
        println!("turned off live ingestion");

        let mut latest = Self::do_catchups_on_events(runtime.clone(), events_response).await;
        let mut diff = Self::get_current_ledger_sequence().await - latest;

        println!("Precision is at {diff}. Latest ledger is {latest}");

        let ExecutionMode::EventCatchup(contract_ids) = &runtime.request.mode else {
            panic!()
        };
        while diff > 0 {
            println!("caught diff > 0");
            let new_events = runtime
                .retrieve_events_after_ledger(contract_ids.as_slice(), latest)
                .await;
            if new_events.data.eventByContractIds.nodes.len() > 0 {
                latest = Self::do_catchups_on_events(runtime.clone(), new_events).await;
                diff = Self::get_current_ledger_sequence().await - latest;
            } else {
                diff = 0
            }
        }

        println!("turning program on live ingestion");
        zephyr_update_status(runtime.request.user_id as i32, true).await;
        println!("turned on live ingestion");

        println!("Catchup completely completed yay ted");
    }

    pub async fn do_catchups_on_events(runtime: Self, events_response: query::Response) -> i64 {
        let mut all_events_by_ledger: BTreeMap<i64, (i64, Vec<EventNode>)> = BTreeMap::new();

        for event in events_response.data.eventByContractIds.nodes {
            let seq = event.txInfoByTx.ledgerByLedger.sequence;
            let time = event.txInfoByTx.ledgerByLedger.closeTime;

            if all_events_by_ledger.contains_key(&seq) {
                let mut other_events: Vec<EventNode> =
                    all_events_by_ledger.get(&seq).unwrap().1.to_vec();
                other_events.push(event);
                all_events_by_ledger.insert(seq, (time, other_events));
            } else {
                all_events_by_ledger.insert(seq, (time, vec![event]));
            }
        }

        let mut latest_ledger = 0;

        //let mut metas = Vec::new();
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

            for event in event_set {
                let result = TransactionResultMeta {
                    result: TransactionResultPair {
                        transaction_hash: Hash([0; 32]),
                        result: TransactionResult {
                            fee_charged: 0,
                            result: TransactionResultResult::TxSuccess(vec![].try_into().unwrap()),
                            ext: soroban_env_host::xdr::TransactionResultExt::V0,
                        },
                    },
                    fee_processing: LedgerEntryChanges(vec![].try_into().unwrap()),
                    tx_apply_processing: soroban_env_host::xdr::TransactionMeta::V3(
                        TransactionMetaV3 {
                            ext: soroban_env_host::xdr::ExtensionPoint::V0,
                            tx_changes_before: LedgerEntryChanges(vec![].try_into().unwrap()),
                            tx_changes_after: LedgerEntryChanges(vec![].try_into().unwrap()),
                            operations: vec![OperationMeta {
                                changes: LedgerEntryChanges(vec![].try_into().unwrap()),
                            }]
                            .try_into()
                            .unwrap(),
                            soroban_meta: Some(SorobanTransactionMeta {
                                ext: soroban_env_host::xdr::SorobanTransactionMetaExt::V0,
                                return_value: ScVal::Void,
                                diagnostic_events: vec![].try_into().unwrap(),
                                events: vec![ContractEvent {
                                    ext: soroban_env_host::xdr::ExtensionPoint::V0,
                                    contract_id: Some(Hash(
                                        stellar_strkey::Contract::from_string(&event.contractId)
                                            .unwrap()
                                            .0,
                                    )),
                                    type_: soroban_env_host::xdr::ContractEventType::Contract,
                                    body: soroban_env_host::xdr::ContractEventBody::V0(
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
            runtime
                .reproduce_async_runtime(Some(ledger_close_meta), None)
                .await;

            latest_ledger = *ledger
        }

        latest_ledger
    }

    pub async fn catchup_spawn_jobs(&self) -> Result<JoinHandle<String>, ()> {
        println!("executing {:?}", self.request);
        match &self.request.mode {
            ExecutionMode::EventCatchup(contract_ids) => {
                let events = self.retrieve_events(contract_ids.as_slice()).await;
                let cloned = self.clone();

                let job = Handle::current().spawn(async move {
                    Self::recursion_catchups(cloned, events).await;
                    //Self::do_catchups_on_events(cloned, events).await;
                    "Catchup in progress".into()
                });
                /*let job = Handle::current().spawn(async move {
                    for meta in metas {
                        cloned.reproduce_async_runtime(Some(meta), None).await;
                    }

                    "Catchup in progress".into()
                });*/

                Ok(job)
            }

            ExecutionMode::Function(function) => {
                self.reproduce_async_runtime(None, Some(function)).await
            }
        }
    }

    pub async fn reproduce_async_runtime(
        &self,
        meta: Option<LedgerCloseMeta>,
        function: Option<&InvokeZephyrFunction>,
    ) -> Result<JoinHandle<String>, ()> {
        let handle = tokio::runtime::Handle::current();

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();

        let cloned = self.clone();

        let binary = database::execution::read_binary(self.request.binary_id as i64).await?;

        let join_handle = match meta {
            Some(meta) => {
                let join_handle =
                    handle.spawn_blocking(move || cloned.execute_with_transition(tx, meta, binary));

                join_handle
            }
            None => {
                let function = function.cloned().unwrap();
                let join_handle =
                    handle.spawn_blocking(move || cloned.execute_function(tx, binary, function));

                join_handle
            }
        };

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
                            }

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
        })
        .await;

        Ok(join_handle)
    }
}

mod newtork_utils {
    use sha2::{Digest, Sha256};
    use soroban_env_host::xdr::Hash;

    pub struct Network {
        passphrase: Vec<u8>,
        id: [u8; 32],
    }

    pub type BinarySha256Hash = [u8; 32];

    pub fn sha256<T: AsRef<[u8]>>(data: T) -> BinarySha256Hash {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hasher.finalize().as_slice().try_into().unwrap()
    }

    impl Network {
        /// Construct a new `Network` for the given `passphrase`
        pub fn new(passphrase: &[u8]) -> Network {
            let id = sha256(passphrase);
            let passphrase = passphrase.to_vec();
            Network { passphrase, id }
        }

        /// Return the SHA-256 hash of the passphrase
        ///
        /// This hash is used for signing transactions.
        pub fn get_id(&self) -> Hash {
            Hash(self.id)
        }
    }
}

impl ExecutionWrapper {
    fn get_network_id(&self) -> Hash {
        let network = newtork_utils::Network::new(self.network.as_bytes());
        network.get_id()
    }

    fn execute_with_transition(
        &self,
        sender: UnboundedSender<Vec<u8>>,
        transition: LedgerCloseMeta,
        binary: Vec<u8>,
    ) -> String {
        let mut host = Host::<MercuryDatabase, LedgerReader>::from_id(
            self.request.user_id as i64,
            self.get_network_id().0,
        )
        .unwrap();
        host.add_transmitter(sender);

        let start = std::time::Instant::now();
        let vm = Vm::new(&host, &binary).unwrap();

        host.load_context(Rc::downgrade(&vm)).unwrap();
        host.add_ledger_close_meta(transition.to_xdr(Limits::none()).unwrap())
            .unwrap();
        let res = vm
            .metered_function_call(&host, "on_close")
            .unwrap_or("no response".into());

        println!("{res}: elapsed {:?}", start.elapsed());

        "execution successful".into()
    }

    fn execute_function(
        &self,
        sender: UnboundedSender<Vec<u8>>,
        binary: Vec<u8>,
        function: InvokeZephyrFunction,
    ) -> String {
        let mut host = Host::<MercuryDatabase, LedgerReader>::from_id(
            self.request.user_id as i64,
            self.get_network_id().0,
        )
        .unwrap();
        host.add_transmitter(sender);

        let start = std::time::Instant::now();
        let vm = Vm::new(&host, &binary).unwrap();

        host.load_context(Rc::downgrade(&vm)).unwrap();
        println!(
            "{:?}",
            serde_json::from_str::<serde_json::Value>(&function.arguments)
        );
        host.add_ledger_close_meta(bincode::serialize(&function.arguments).unwrap())
            .unwrap();

        let res = vm
            .metered_function_call(&host, &function.fname)
            .unwrap_or("no response".into());

        println!("{res}: elapsed {:?}", start.elapsed());

        res
    }
}

#[tokio::test]
async fn test() {
    //println!("{}", serde_json::to_string_pretty(&get_query_after_ledger(&["CDVQVKOY2YSXS2IC7KN6MNASSHPAO7UN2UR2ON4OI2SKMFJNVAMDX6DP".into()], 51931046)).unwrap());

    let jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJyb2xlIjoidGRlcDEiLCJleHAiOjE3MTc4NzYyNTEsInVzZXJfaWQiOjcsInVzZXJuYW1lIjoidGRlcEB4eWNsb28uY29tIiwiaWF0IjoxNzE3MjcxNDU0LCJhdWQiOiJwb3N0Z3JhcGhpbGUiLCJpc3MiOiJwb3N0Z3JhcGhpbGUifQ.X056_xJvXV9ZTCnTmEiXq4vNSkZBQtxw-xO72iKJAG4";
    let client = reqwest::Client::new();

    let graphql_endpoint = "https://mainnet.mercurydata.app:2083/graphql";

    let res = client
        .post(graphql_endpoint)
        .bearer_auth(jwt)
        .json(&get_query_after_ledger(
            &["CDVQVKOY2YSXS2IC7KN6MNASSHPAO7UN2UR2ON4OI2SKMFJNVAMDX6DP".into()],
            51931046,
        ))
        .send()
        .await
        .unwrap();

    let resp: crate::query::ResponseAfterLedger = res.json().await.unwrap();
    let resp = crate::query::Response {
        data: crate::query::Data {
            eventByContractIds: resp.data.eventByContractIds,
        },
    };

    println!("{}", serde_json::to_string(&resp).unwrap())
}
