mod symbol;
mod database;

use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use stellar_xdr::{
    LedgerCloseMeta, LedgerEntry, LedgerEntryChange, LedgerKey, ReadXdr, TransactionMeta,
};
use thiserror::Error;
use database::Database;

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

#[derive(Clone, Default)]
pub struct EnvClient {
    db: Database,
}

impl EnvClient {
    pub fn db(&self) -> &Database {
        &self.db
    }

    pub fn db_write(&self, table_name: &str, columns: &[&str], segments: &[&[u8]]) {
        
    }

    pub fn get_last_ledger_meta() -> stellar_xdr::LedgerCloseMeta {
        let (offset, size) = unsafe { read_ledger_meta() };

        let ledger_meta = {
            let memory = 0 as *const u8;
            let slice = unsafe {
                let start = memory.offset(offset as isize);
                core::slice::from_raw_parts(start, size as usize)
            };

            stellar_xdr::LedgerCloseMeta::from_xdr(slice).unwrap()
        };

        ledger_meta
    }
}

#[derive(Clone)]
pub struct EntryChanges {
    pub state: Vec<LedgerEntry>,
    pub removed: Vec<LedgerKey>,
    pub updated: Vec<LedgerEntry>,
    pub created: Vec<LedgerEntry>,
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

pub struct MetaReader<'a>(&'a stellar_xdr::LedgerCloseMeta);

impl<'a> MetaReader<'a> {
    pub fn new(meta: &'a LedgerCloseMeta) -> Self {
        Self(meta)
    }

    pub fn ledger_sequence(&self) -> u32 {
        match &self.0 {
            LedgerCloseMeta::V1(v1) => v1.ledger_header.header.ledger_seq,
            LedgerCloseMeta::V0(v0) => v0.ledger_header.header.ledger_seq,
            LedgerCloseMeta::V2(v2) => v2.ledger_header.header.ledger_seq,
        }
    }

    pub fn v2_ledger_entries(&self) -> EntryChanges {
        let mut state_entries = Vec::new();
        let mut removed_entries = Vec::new();
        let mut updated_entries = Vec::new();
        let mut created_entries = Vec::new();

        match &self.0 {
            LedgerCloseMeta::V0(_) => (),
            LedgerCloseMeta::V1(_) => (),
            LedgerCloseMeta::V2(v2) => {
                for tx_processing in v2.tx_processing.iter() {
                    match &tx_processing.tx_apply_processing {
                        TransactionMeta::V3(meta) => {
                            let ops = &meta.operations;

                            for operation in ops.clone().into_vec() {
                                for change in operation.changes.0.iter() {
                                    match &change {
                                        LedgerEntryChange::State(state) => {
                                            state_entries.push(state.clone())
                                        }
                                        LedgerEntryChange::Created(created) => {
                                            created_entries.push(created.clone())
                                        }
                                        LedgerEntryChange::Updated(updated) => {
                                            updated_entries.push(updated.clone())
                                        }
                                        LedgerEntryChange::Removed(removed) => {
                                            removed_entries.push(removed.clone())
                                        }
                                    };
                                }
                            }
                        }
                        _ => (),
                    }
                }
            }
        };

        EntryChanges {
            state: state_entries,
            removed: removed_entries,
            updated: updated_entries,
            created: created_entries,
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
