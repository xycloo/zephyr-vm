mod symbol;

use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use stellar_xdr::{ReadXdr, LedgerEntryChange, LedgerEntry, TransactionMeta, LedgerCloseMeta, LedgerKey};
use thiserror::Error;

fn to_fixed<T, const N: usize>(v: Vec<T>) -> [T; N] {
    v.try_into()
        .unwrap_or_else(|v: Vec<T>| panic!("Expected a Vec of length {} but it was {}", N, v.len()))
}

extern crate wee_alloc;

extern "C" {
    #[allow(improper_ctypes)] // we alllow as we enabled multi-value
    #[link_name = "read_raw"]
    fn read_raw() -> (i64, i64);

    #[allow(improper_ctypes)] // we alllow as we enabled multi-value
    #[link_name = "write_raw"]
    fn write_raw();

    #[allow(improper_ctypes)] // we alllow as we enabled multi-value
    #[link_name = "read_ledger_meta"]
    fn read_ledger_meta() -> (i64, i64);

    #[link_name = "zephyr_stack_push"]
    fn env_push_stack(param: i64);

    #[link_name = "zephyr_logger"]
    fn log(param: i64);
}

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[derive(Clone, Deserialize, Serialize)]
pub struct TableRows {
    pub rows: Vec<TableRow>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct TableRow {
    pub row: Vec<TypeWrap>,
}

impl TableRows {
    pub fn from_raw_parts(offset: i64, size: usize) -> Result<Self, SdkError> {
        let memory: *const u8 = 0 as *const u8;

        let slice = unsafe {
            let start = memory.offset(offset as isize);
            core::slice::from_raw_parts(start, size as usize)
        };

        if let Ok(table) = bincode::deserialize::<Self>(slice) {
            Ok(table)
        } else {
            Err(SdkError::Conversion)
        }
    }
}

#[derive(Clone, Debug, Error)]
pub enum SdkError {
    #[error("Conversion error.")]
    Conversion,
}

#[derive(Clone, Default)]
pub struct Database {}

impl Database {
    pub fn read_table(table_name: &str, columns: &[&str]) -> TableRows {
        let table_name = symbol::Symbol::try_from_bytes(table_name.as_bytes()).unwrap();
        let cols = columns
            .into_iter()
            .map(|col| (symbol::Symbol::try_from_bytes(col.as_bytes()).unwrap().0 as i64).into())
            .collect::<Vec<i64>>();

        unsafe {
            env_push_stack(table_name.0 as i64);
            env_push_stack(cols.len() as i64);

            for col in cols {
                env_push_stack(col)
            }
        };

        let (offset, size) = unsafe { read_raw() };

        TableRows::from_raw_parts(offset, size as usize).unwrap()
    }

    pub fn write_table(table_name: &str, columns: &[&str], segments: &[&[u8]]) {
        let table_name = symbol::Symbol::try_from_bytes(table_name.as_bytes()).unwrap();
        let cols = columns
            .into_iter()
            .map(|col| (symbol::Symbol::try_from_bytes(col.as_bytes()).unwrap().0 as i64).into())
            .collect::<Vec<i64>>();

        let segments = segments
            .into_iter()
            .map(|segment| (segment.as_ptr() as i64, segment.len() as i64))
            .collect::<Vec<(i64, i64)>>();

        unsafe {
            env_push_stack(table_name.0 as i64);
            env_push_stack(columns.len() as i64);

            for col in cols {
                env_push_stack(col);
            }

            env_push_stack(segments.len() as i64);

            for segment in segments {
                env_push_stack(segment.0);
                env_push_stack(segment.1);
            }
        }
    }
}

#[derive(Clone, Default)]
pub struct EnvClient {
    db: Database,
}

impl EnvClient {
    pub fn db(&self) -> &Database {
        &self.db
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

pub struct MetaReader<'a>(&'a stellar_xdr::LedgerCloseMeta);

impl<'a> MetaReader<'a> {
    pub fn new(meta: &'a LedgerCloseMeta) -> Self {
        Self(meta)
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
                                        LedgerEntryChange::State(state) => state_entries.push(state.clone()),
                                        LedgerEntryChange::Created(created) => created_entries.push(created.clone()),
                                        LedgerEntryChange::Updated(updated) => updated_entries.push(updated.clone()),
                                        LedgerEntryChange::Removed(removed) => removed_entries.push(removed.clone()),
                                    };
                                }
                                }
                            }
                            _ => ()
                        }
                    }
                }
            };
        
        EntryChanges { 
            state: state_entries, 
            removed: removed_entries, 
            updated: updated_entries, 
            created: created_entries 
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
