mod database;
mod ledger_meta;
mod symbol;

pub use database::{TableRow, TableRows};
pub use ledger_meta::MetaReader;

use database::Database;
use rs_zephyr_common::ZephyrStatus;
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use stellar_xdr::next::{Limits, ReadXdr};
use thiserror::Error;

pub use ledger_meta::EntryChanges;
pub use stellar_xdr;
pub use database::Condition;
pub use rs_zephyr_common::ZephyrVal;
pub use bincode;
pub use macros::DatabaseInteract as DatabaseDerive;


fn to_fixed<T, const N: usize>(v: Vec<T>) -> [T; N] {
    v.try_into()
        .unwrap_or_else(|v: Vec<T>| panic!("Expected a Vec of length {} but it was {}", N, v.len()))
}

extern crate wee_alloc;

extern "C" {
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

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

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
    xdr: stellar_xdr::next::LedgerCloseMeta,
}

// Note: some methods take self as param though it's not needed yet.
impl EnvClient {
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

        MetaReader::new(meta)
    }

    pub fn new() -> Self {
        let (offset, size) = unsafe { read_ledger_meta() };

        let ledger_meta = {
            let memory = 0 as *const u8;
            let slice = unsafe {
                let start = memory.offset(offset as isize);
                core::slice::from_raw_parts(start, size as usize)
            };

            stellar_xdr::next::LedgerCloseMeta::from_xdr(slice, Limits::none()).unwrap()
        };

        Self { xdr: ledger_meta }
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
