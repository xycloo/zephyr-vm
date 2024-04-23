mod database;
mod ledger_meta;
mod symbol;
mod ledger;

pub use database::{TableRow, TableRows};
pub use ledger_meta::MetaReader;
pub use rs_zephyr_common::ContractDataEntry;

use database::Database;
use rs_zephyr_common::{log::{LogLevel, ZephyrLog}, wrapping::WrappedMaxBytes, RelayedMessageRequest, ZephyrStatus};
use serde::{Deserialize, Serialize};
use core::slice;
use std::{alloc::{alloc, Layout}, convert::TryInto};
use stellar_xdr::next::{LedgerEntry, Limits, ReadXdr, ScVal, WriteXdr};
use thiserror::Error;

//pub use soroban_env_host;
pub use ledger_meta::EntryChanges;
pub use soroban_sdk;
pub use stellar_xdr;
pub use database::Condition;
pub use rs_zephyr_common::{ZephyrVal, http::{AgnosticRequest, Method}};
pub use bincode;
pub use macros::DatabaseInteract as DatabaseDerive;

pub type ServerlessResult = (i64, i64);

fn to_fixed<T, const N: usize>(v: Vec<T>) -> [T; N] {
    v.try_into()
        .unwrap_or_else(|v: Vec<T>| panic!("Expected a Vec of length {} but it was {}", N, v.len()))
}

extern crate wee_alloc;

extern "C" {
    #[allow(improper_ctypes)]
    #[link_name = "read_contract_data_entry_by_contract_id_and_key"]
    pub fn read_contract_data_entry_by_contract_id_and_key(contract_part_1: i64, contract_part_2: i64, contract_part_3: i64, contract_part_4: i64, offset: i64, size: i64) -> (i64, i64, i64);

    #[allow(improper_ctypes)]
    #[link_name = "read_contract_instance"]
    pub fn read_contract_instance(contract_part_1: i64, contract_part_2: i64, contract_part_3: i64, contract_part_4: i64) -> (i64, i64, i64);

    #[allow(improper_ctypes)]
    #[link_name = "read_contract_entries_by_contract"]
    pub fn read_contract_entries_by_contract(contract_part_1: i64, contract_part_2: i64, contract_part_3: i64, contract_part_4: i64) -> (i64, i64, i64);

    #[allow(improper_ctypes)]
    #[link_name = "read_contract_entries_by_contract_to_env"]
    pub fn read_contract_entries_by_contract_to_env(contract_part_1: i64, contract_part_2: i64, contract_part_3: i64, contract_part_4: i64) -> (i64, i64);

    #[allow(improper_ctypes)]
    #[link_name = "conclude"]
    pub fn conclude_host(offset: i64, size: i64);

    #[allow(improper_ctypes)]
    #[link_name = "tx_send_message"]
    pub fn tx_send_message(offset: i64, size: i64) -> i64;

    #[allow(improper_ctypes)] // we alllow as we enabled multi-value
    #[link_name = "read_raw"]
    pub fn read_raw() -> (i64, i64, i64);

    #[allow(improper_ctypes)] // we alllow as we enabled multi-value
    #[link_name = "write_raw"]
    fn write_raw() -> i64;

    #[allow(improper_ctypes)] // we alllow as we enabled multi-value
    #[link_name = "update_raw"]
    fn update_raw() -> i64;

    #[allow(improper_ctypes)] // we alllow as we enabled multi-value
    #[link_name = "read_ledger_meta"]
    pub fn read_ledger_meta() -> (i64, i64);

    #[link_name = "zephyr_stack_push"]
    pub fn env_push_stack(param: i64);

    #[link_name = "zephyr_logger"]
    pub fn log(param: i64);
}

//#[global_allocator]
//static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[derive(Clone, Debug, Copy, Error)]
pub enum SdkError {
    #[error("Conversion error.")]
    Conversion,

    #[error("Error in reading database.")]
    DbRead,

    #[error("Error in writing database.")]
    DbWrite,

    #[error("No value found on host pseudo store.")]
    NoValOnStack,

    #[error("Incorrect host configurations.")]
    HostConfiguration,

    #[error("Unknown error.")]
    Unknown
}

impl SdkError {
    fn express_from_status(status: i64) -> Result<(), Self> {
        match ZephyrStatus::from(status as u32) {
            ZephyrStatus::Success => Ok(()),
            ZephyrStatus::DbReadError => Err(SdkError::DbRead),
            ZephyrStatus::DbWriteError => Err(SdkError::DbWrite),
            ZephyrStatus::NoValOnStack => Err(SdkError::NoValOnStack),
            ZephyrStatus::HostConfiguration => Err(SdkError::HostConfiguration),
            ZephyrStatus::Unknown => Err(SdkError::Unknown)
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct TypeWrap(pub Vec<u8>);

impl TypeWrap {
    pub fn to_i128(&self) -> i128 {
        let bytes = to_fixed::<u8, 16>(self.0.clone());
        i128::from_be_bytes(bytes)
    }

    pub fn to_u64(&self) -> u64 {
        let bytes = to_fixed::<u8, 8>(self.0.clone());
        u64::from_be_bytes(bytes)
    }
}

#[derive(Clone)]
pub struct EnvClient {
    pub xdr: Option<stellar_xdr::next::LedgerCloseMeta>,
    //pub inner_soroban_host: soroban_env_host::Host,
}

pub struct EnvLogger;

impl EnvLogger {
    pub fn error(&self, message: impl ToString, data: Option<Vec<u8>>) {
        let log = ZephyrLog {
            level: LogLevel::Error,
            message: message.to_string(),
            data
        };

        EnvClient::message_relay(RelayedMessageRequest::Log(log));
    }

    pub fn debug(&self, message: impl ToString, data: Option<Vec<u8>>) {
        let log = ZephyrLog {
            level: LogLevel::Debug,
            message: message.to_string(),
            data
        };

        EnvClient::message_relay(RelayedMessageRequest::Log(log));
    }

    pub fn warning(&self, message: impl ToString, data: Option<Vec<u8>>) {
        let log = ZephyrLog {
            level: LogLevel::Warning,
            message: message.to_string(),
            data
        };

        EnvClient::message_relay(RelayedMessageRequest::Log(log));
    }
}

// Note: some methods take self as param though it's not needed yet.
impl EnvClient {
    pub fn log(&self) -> EnvLogger {
        EnvLogger
    }

    pub fn message_relay(message: impl Serialize) {
        let serialized = bincode::serialize(&message).unwrap();
        
        let res = unsafe {
            tx_send_message(
                serialized.as_ptr() as i64, 
                serialized.len() as i64
            )
        };

        SdkError::express_from_status(res).unwrap()
    }

    pub fn send_web_request(&self, request: AgnosticRequest) {
        let message = RelayedMessageRequest::Http(request);

        Self::message_relay(message)
    }
    
    pub fn conclude<T: Serialize>(&self, result: T) {
        let v = bincode::serialize(&serde_json::to_string(&result).unwrap()).unwrap();
        
        unsafe {
            conclude_host(v.as_ptr() as i64, v.len() as i64)
        }
    }

    pub fn read<T: DatabaseInteract>(&self) -> Vec<T> {
        T::read_to_rows(&self)
    }

    pub fn put<T: DatabaseInteract>(&self, row: &T) {
        row.put(&self)
    }

    pub fn update<T: DatabaseInteract>(&self, row: &T, conditions: &[Condition]) {
        row.update(&self, conditions)
    }

    pub fn db_write(&self, table_name: &str, columns: &[&str], segments: &[&[u8]]) -> Result<(), SdkError> {
        Database::write_table(table_name, columns, segments)
    }

    pub fn db_update(&self, table_name: &str, columns: &[&str], segments: &[&[u8]], conditions: &[Condition]) -> Result<(), SdkError> {
        Database::update_table(table_name, columns, segments, conditions)
    }

    pub fn db_read(&self, table_name: &str, columns: &[&str]) -> Result<TableRows, SdkError> {
        Database::read_table(table_name, columns)
    }

    pub fn reader(&self) -> MetaReader {
        let meta = &self.xdr;

        if let Some(meta) = meta {
            MetaReader::new(meta)
        } else {
            panic!("Internal SDK error") // todo: handle
        }
    }

    pub fn new() -> Self {
        let (offset, size) = unsafe { read_ledger_meta() };

        let ledger_meta = {
            let memory = 0 as *const u8;
            let slice = unsafe {
                let start = memory.offset(offset as isize);
                core::slice::from_raw_parts(start, size as usize)
            };
            
            Some(stellar_xdr::next::LedgerCloseMeta::from_xdr(slice, Limits::none()).unwrap())
        };
        
        Self { xdr: ledger_meta }
    }

    pub fn empty() -> Self {
        Self { xdr: None }
    }
}

pub mod utils {
    use stellar_xdr::next::{Int128Parts, ScMapEntry, ScSymbol, ScVal, ScVec, VecM};

    use crate::SdkError;

    pub fn to_datakey_u32(int: u32) -> ScVal {
        ScVal::U32(int)
    }

    pub fn to_datakey_symbol(variant_str: &str) -> ScVal {
        let tot_s_val = ScVal::Symbol(ScSymbol(variant_str.to_string().try_into().unwrap()));

        ScVal::Vec(Some(ScVec(VecM::try_from(vec![tot_s_val]).unwrap())))
    }

    pub fn instance_entries(val: &ScVal) -> Option<Vec<ScMapEntry>> {
        if let ScVal::ContractInstance(instance) = val {
            if let Some(map) = &instance.storage {
                return Some(map.to_vec());
            }
        }

        None
    }

    pub fn to_scval_symbol(from: &str) -> Result<ScVal, SdkError> {
        Ok(ScVal::Symbol(ScSymbol(from.try_into().map_err(|_| SdkError::Conversion)?)))
    }

    pub fn parts_to_i128(parts: &Int128Parts) -> i128 {
        ((parts.hi as i128) << 64) | (parts.lo as i128)
    }

    pub fn to_array<T, const N: usize>(v: Vec<T>) -> [T; N] {
        v.try_into()
            .unwrap_or_else(|v: Vec<T>| panic!("Expected a Vec of length {} but it was {}", N, v.len()))
    }
}


pub trait DatabaseInteract {
    fn read_to_rows(env: &EnvClient) -> Vec<Self> where Self: Sized;

    fn put(&self, env: &EnvClient);

    fn update(&self, env: &EnvClient, conditions: &[Condition]);
}
