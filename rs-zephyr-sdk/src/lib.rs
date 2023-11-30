mod database;
mod ledger_meta;
mod symbol;

use database::{Database, TableRows};
use ledger_meta::MetaReader;
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use stellar_xdr::ReadXdr;
use thiserror::Error;

pub use ledger_meta::EntryChanges;
pub use stellar_xdr;

fn to_fixed<T, const N: usize>(v: Vec<T>) -> [T; N] {
    v.try_into()
        .unwrap_or_else(|v: Vec<T>| panic!("Expected a Vec of length {} but it was {}", N, v.len()))
}

extern crate wee_alloc;

extern "C" {
    #[allow(improper_ctypes)] // we alllow as we enabled multi-value
    #[link_name = "read_raw"]
    pub fn read_raw() -> (i64, i64);

    #[allow(improper_ctypes)] // we alllow as we enabled multi-value
    #[link_name = "write_raw"]
    fn write_raw();

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

#[derive(Clone, Debug, Error)]
pub enum SdkError {
    #[error("Conversion error.")]
    Conversion,
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

#[derive(Clone, Default)]
pub struct EnvClient {
    xdr: Option<stellar_xdr::LedgerCloseMeta>,
}

// Note: some methods take self as param though it's not needed yet.
impl EnvClient {
    pub fn db_write(&self, table_name: &str, columns: &[&str], segments: &[&[u8]]) {
        Database::write_table(table_name, columns, segments)
    }

    pub fn db_read(&self, table_name: &str, columns: &[&str]) -> TableRows {
        Database::read_table(table_name, columns)
    }

    pub fn reader(&mut self) -> MetaReader {
        let meta = Self::last_ledger_meta_xdr(self);

        MetaReader::new(meta)
    }

    pub fn last_ledger_meta_xdr(&mut self) -> &stellar_xdr::LedgerCloseMeta {
        if self.xdr.is_none() {
            let (offset, size) = unsafe { read_ledger_meta() };

            let ledger_meta = {
                let memory = 0 as *const u8;
                let slice = unsafe {
                    let start = memory.offset(offset as isize);
                    core::slice::from_raw_parts(start, size as usize)
                };

                stellar_xdr::LedgerCloseMeta::from_xdr(slice).unwrap()
            };

            self.xdr = Some(ledger_meta);
        }

        self.xdr.as_ref().unwrap()
    }
}

pub mod scval_utils {
    use stellar_xdr::{ScMapEntry, ScSymbol, ScVal, ScVec, VecM};

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
}
