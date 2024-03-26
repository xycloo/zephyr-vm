use std::rc::Rc;

use rs_zephyr_common::ContractDataEntry;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use stellar_xdr::next::{LedgerEntry, Limits, ReadXdr, ScAddress, ScVal, WriteXdr};
use zephyr::{db::ledger::LedgerStateRead, host::Host, testutils::database::MercuryDatabase, vm::Vm, ZephyrMock};

#[derive(Clone)]
pub struct LedgerReader {
    path: String
}

impl ZephyrMock for LedgerReader {
    fn mocked() -> anyhow::Result<Self>
        where
            Self: Sized {

        Ok(Self { path: "/home/tommasodeponti/Desktop/stellar.db".into() })
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

    pub fn execute_function(&self, fname: &str) -> String {
        let host = Host::<MercuryDatabase, LedgerReader>::mocked().unwrap();
        
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

    #[test]
    fn run_instance_getter() {
        let code = { read("/mnt/storagehdd/projects/master/zephyr/target/wasm32-unknown-unknown/release/simple.wasm").unwrap() };
        let execution = ExecutionWrapper::new(&code);

        execution.execute_function("mytest");
    }

    #[test]
    fn run_entries_filter() {
        let code = { read("/mnt/storagehdd/projects/master/zephyr/target/wasm32-unknown-unknown/release/entries_filter.wasm").unwrap() };
        let execution = ExecutionWrapper::new(&code);

        execution.execute_function("top_holders");
    }
}