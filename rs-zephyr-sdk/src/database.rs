use crate::{env_push_stack, read_raw, symbol, write_raw, SdkError, TypeWrap};
use serde::{Deserialize, Serialize};

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

#[derive(Clone, Default)]
pub struct Database {}

impl Database {
    pub fn read_table(table_name: &str, columns: &[&str]) -> TableRows {
        let table_name = symbol::Symbol::try_from_bytes(table_name.as_bytes()).unwrap();
        let cols = columns
            .into_iter()
            .map(|col| symbol::Symbol::try_from_bytes(col.as_bytes()).unwrap().0 as i64)
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
            .map(|col| symbol::Symbol::try_from_bytes(col.as_bytes()).unwrap().0 as i64)
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

        unsafe { write_raw() }
    }
}
