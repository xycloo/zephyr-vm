use crate::{env_push_stack, read_raw, symbol, update_raw, write_raw, SdkError, TypeWrap};
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
pub struct TableRows {
    pub rows: Vec<TableRow>,
}

pub enum Condition {
    ColumnEqualTo(String, Vec<u8>)
}

#[derive(Clone, Deserialize, Serialize)]
pub struct TableRow {
    pub row: Vec<TypeWrap>,
}

#[derive(Clone, Default)]
pub struct Database {}

impl Database {
    pub fn read_table(table_name: &str, columns: &[&str]) -> Result<TableRows, SdkError> {
        let table_name = symbol::Symbol::try_from_bytes(table_name.as_bytes()).unwrap();
        let cols = columns
            .into_iter()
            .map(|col| symbol::Symbol::try_from_bytes(col.as_bytes()).unwrap().0 as i64)
            .collect::<Vec<i64>>();

        
        // Load instructions to env pseudo-store.
        unsafe {
            env_push_stack(table_name.0 as i64);
            env_push_stack(cols.len() as i64);

            for col in cols {
                env_push_stack(col)
            }
        };

        // Receive offset and size from env. 
        let (status, offset, size) = unsafe { read_raw() };
        SdkError::express_from_status(status)?;
        
        let table = {
            let memory: *const u8 = offset as *const u8;

            let slice = unsafe {
                core::slice::from_raw_parts(memory, size as usize)
            };

            if let Ok(table) = bincode::deserialize::<TableRows>(slice) {
                table
            } else {
                return Err(SdkError::Conversion)
            }
        };

        Ok(table)

    }

    pub fn write_table(table_name: &str, columns: &[&str], segments: &[&[u8]]) -> Result<(), SdkError> {
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

        let status = unsafe { write_raw() };
        SdkError::express_from_status(status)
    }

    pub fn update_table(table_name: &str, columns: &[&str], segments: &[&[u8]], conditions: &[Condition]) -> Result<(), SdkError> {
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

            env_push_stack(conditions.len() as i64);

            let mut args = Vec::new();
            for cond in conditions {
                let (colname, operator, value) = match cond {
                    Condition::ColumnEqualTo(colname, value) => (colname, 0, value)
                };

                env_push_stack(symbol::Symbol::try_from_bytes(colname.as_bytes()).unwrap().0 as i64);
                env_push_stack(operator as i64);

                args.push((value.as_ptr() as i64, value.len() as i64))
            }

            env_push_stack(args.len() as i64);

            for segment in args {
                env_push_stack(segment.0);
                env_push_stack(segment.1);
            }
        }

        let status = unsafe { update_raw() };
        SdkError::express_from_status(status)
    }
}
