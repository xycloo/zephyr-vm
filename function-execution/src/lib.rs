use std::{rc::Rc, str::FromStr};
use reqwest::header::{HeaderMap, HeaderName};
use rs_zephyr_common::{http::{AgnosticRequest, Method}, ContractDataEntry};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use stellar_xdr::next::{LedgerEntry, Limits, ReadXdr, ScAddress, ScVal, WriteXdr};
use tokio::sync::mpsc::UnboundedSender;
use zephyr::{db::ledger::LedgerStateRead, host::Host, testutils::database::MercuryDatabase, vm::Vm, ZephyrMock};

#[derive(Clone)]
pub struct LedgerReader {
    path: String
}

impl ZephyrMock for LedgerReader {
    fn mocked() -> anyhow::Result<Self>
        where
            Self: Sized {

        Ok(Self { path: "../../stellar.db".into() })
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

        Some(entries[0].clone())
    }
    
    fn read_contract_data_entries_by_contract_id(&self, contract: ScAddress) -> Vec<ContractDataEntry> {
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

#[derive(Serialize, Deserialize, Clone)]
pub struct FunctionRequest {
    pub fname: String
}

#[derive(Clone)]
pub struct ExecutionWrapper {
    binary: Vec<u8>
}

impl ExecutionWrapper {
    pub fn new(function_bin: &[u8]) -> Self {
        Self {
            binary: function_bin.to_vec()
        }
    }

    pub async fn reproduce_async_runtime(&self, fname: &str) -> String {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        
        let resp = self.execute_function(fname, tx);
        
        let _ = tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                let request: AgnosticRequest = bincode::deserialize(&message).unwrap();
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
        }).await;

        resp
    }

    pub fn execute_function(&self, fname: &str, tx: UnboundedSender<Vec<u8>>) -> String {
        let mut host = Host::<MercuryDatabase, LedgerReader>::mocked().unwrap();
        host.add_transmitter(tx);

        let start = std::time::Instant::now();
        let vm = Vm::new(&host, &self.binary).unwrap();
        
        host.load_context(Rc::downgrade(&vm)).unwrap();
        let res = vm.metered_function_call(&host, fname).unwrap();

        println!("elapsed {:?}", start.elapsed());

        res
    }

}

#[cfg(test)]
mod test {
    use std::fs::read;

    use crate::ExecutionWrapper;

    #[tokio::test]
    async fn run_instance_getter() {
        let code = { read("../target/wasm32-unknown-unknown/release/simple.wasm").unwrap() };
        let execution = ExecutionWrapper::new(&code);

        execution.reproduce_async_runtime("mytest").await;
    }

    #[tokio::test]
    async fn run_entries_filter() {
        let code = { read("../target/wasm32-unknown-unknown/release/entries_filter.wasm").unwrap() };
        let execution = ExecutionWrapper::new(&code);

        execution.reproduce_async_runtime("top_holders").await;
    }

    #[tokio::test]
    async fn run_alert() {
        let code = { read("../../zephyr-examples/zephyr-alert/target/wasm32-unknown-unknown/release/zephyr_alert.wasm").unwrap() };
        let execution = ExecutionWrapper::new(&code);

        execution.reproduce_async_runtime("on_close").await;
    }

    /// Simple reference impl for joined tokio handles.
    /// Can be useful when working with zephyr.
    mod simple_join_job {
        use std::time::Duration;

        async fn test_spawn_internal(v: Vec<String>) {
            let mut handles = Vec::new();
            
            for val in v {
                let t = tokio::spawn(async move {
                    println!("{val}");
                    let _ = tokio::time::sleep(Duration::from_secs(10)).await;
                });

                handles.push(t)
            }

            for job in handles {
                job.await;
            }
        }

        #[tokio::test]
        async fn test_spawn() {
            let v = vec![String::from("test"), String::from("tes3"), String::from("tesk")];

            test_spawn_internal(v).await;
        }
    }
}