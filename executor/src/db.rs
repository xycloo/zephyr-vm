use std::{collections::HashMap, env};

use anyhow::{Context, Result};
use postgres::{Client, NoTls, types::{ToSql, Type}};
use rs_zephyr_common::{DatabaseError, ZephyrVal};
use serde::{Deserialize, Serialize};
use zephyr_vm::{
    db::database::{WhereCond, ZephyrDatabase},
    ZephyrMock, ZephyrStandard,
};

pub mod execution {
    use super::*;
    use tokio_postgres;

    #[derive(Clone, Deserialize, Serialize, Debug)]
    struct Column {
        name: String,
        col_type: String,
        primary: Option<bool>,
        index: Option<bool>,
    }

    #[derive(Deserialize, Serialize, Debug, Clone)]
    pub struct NewZephyrTable {
        user_id: u32,
        table: Option<String>,
        columns: Option<Vec<Column>>,
    }

    /// Establishes an asynchronous connection using the environment variable `INGESTOR_DB`.
    async fn get_async_connection() -> Result<(tokio_postgres::Client)> {
        let conn_str = env::var("INGESTOR_DB")
            .context("INGESTOR_DB env var is not set")?;
        let (client, connection) =
            tokio_postgres::connect(&conn_str, tokio_postgres::NoTls).await?;
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("connection error: {}", e);
            }
        });
        Ok(client)
    }

    #[derive(Debug, Clone)]
    pub struct ConstructedZephyrBinary {
        pub user_id: i32,
        pub code: Vec<u8>,
        pub running: bool,
        pub is_contract: bool,
        pub contracts: Option<Vec<String>>,
    }

    fn i64_to_bytes(value: i64) -> [u8; 8] {
        let byte0 = ((value >> 0) & 0xFF) as u8;
        let byte1 = ((value >> 8) & 0xFF) as u8;
        let byte2 = ((value >> 16) & 0xFF) as u8;
        let byte3 = ((value >> 24) & 0xFF) as u8;
        let byte4 = ((value >> 32) & 0xFF) as u8;
        let byte5 = ((value >> 40) & 0xFF) as u8;
        let byte6 = ((value >> 48) & 0xFF) as u8;
        let byte7 = ((value >> 56) & 0xFF) as u8;

        [byte0, byte1, byte2, byte3, byte4, byte5, byte6, byte7]
    }

    pub async fn new_zephyr_table(request: NewZephyrTable) -> Result<String> {
        let client = get_async_connection().await?;
    
        let hash: [u8; 16] = {
            let sym = symbol::Symbol::try_from_bytes(request.table.unwrap().as_bytes()).unwrap();
            let bytes = i64_to_bytes(sym.0 as i64);
            md5::compute([bytes, i64_to_bytes(request.user_id.into())].concat()).into()
        };
    
        let drop_table = format!("DROP TABLE IF EXISTS zephyr_{}", hex::encode(hash).as_str());
        client.execute(&drop_table, &[]).await.unwrap();
    
        let mut new_table_stmt = String::from(&format!(
            "CREATE TABLE zephyr_{} (",
            hex::encode(hash).as_str()
        ));
    
        let table_name = format!("zephyr_{}", hex::encode(hash).as_str());
    
        if let Some(columns) = &request.columns {
            let mut primary_key_set = false;
            for (index, column) in columns.iter().enumerate() {
                new_table_stmt.push_str(&format!(
                    "{} {}",
                    column.name,
                    column.col_type
                ));
    
                if column.primary == Some(true) && !primary_key_set {
                    new_table_stmt.push_str(" PRIMARY KEY");
                    primary_key_set = true;
                }
    
                if index < columns.len() - 1 {
                    new_table_stmt.push_str(", ");
                }
            }
        }
    
        new_table_stmt.push(')');
        client
            .batch_execute(&new_table_stmt)
            .await?;

        if let Some(columns) = &request.columns {
            for column in columns.iter() {
                if column.primary != Some(true) {
                    if column.index == Some(true) {
                        let index_stmt = format!(
                            "CREATE INDEX idx_{}_{}_{} ON {} ({})",
                            hex::encode(hash).as_str(),
                            column.name,
                            chrono::Utc::now().timestamp(),
                            table_name,
                            column.name
                        );
                        client
                            .execute(&index_stmt, &[])
                            .await?;
                    }
                }
            }
        }

        let stmt2 = client
            .prepare_typed(
                &format!("grant select on {} to public", table_name),
                &[],
            )
            .await
            .unwrap();
    
        client.execute(&stmt2, &[]).await.unwrap();
    
        let new_table_stmt = String::from(&format!(
            "CREATE TABLE IF NOT EXISTS zephyr_user_tables (
                user_id INT,
                name TEXT unique
            )",
        ));
    
        client.execute(&new_table_stmt, &[]).await.unwrap();
    
        let add_table_to_list_stmt = client
            .prepare_typed(
                &format!("INSERT INTO zephyr_user_tables (user_id, name) VALUES ($1, $2)"),
                &[Type::INT8, Type::TEXT],
            )
            .await
            .unwrap();
    
        let _ = client
            .execute(
                &add_table_to_list_stmt,
                &[&(request.user_id as i64), &table_name],
            )
            .await;
    
        Ok(table_name)
    }
}

mod symbol {
    /// A symbol represents an encoded value where the lowest byte is a tag.
    const TAG: u8 = 14;

    #[derive(Debug)]
    pub enum SymbolError {
        /// Returned when attempting to form a [SymbolSmall] from a string with more
        /// than 9 characters.
        TooLong(usize),
        /// Returned when attempting to form a [SymbolObject] or [SymbolSmall] from
        /// a string with characters outside the range `[a-zA-Z0-9_]`.
        BadChar(char),

        InvalidSymbol,
    }

    pub struct Symbol(pub u64);

    impl Symbol {
        pub fn to_string(&self) -> Result<String, SymbolError> {
            let mut value = self.0;
            if (value & (TAG as u64)) != (TAG as u64) {
                return Err(SymbolError::InvalidSymbol);
            }
            value >>= 8;
            let mut result = String::new();

            while value > 0 {
                let index = (value & 0x3F) as u8;
                value >>= 6;
                let ch = match index {
                    1 => '_',
                    2..=11 => (b'0' + index - 2) as char,
                    12..=37 => (b'A' + index - 12) as char,
                    38..=63 => (b'a' + index - 38) as char,
                    _ => return Err(SymbolError::InvalidSymbol),
                };
                result.push(ch);
            }
            Ok(result.chars().rev().collect())
        }

        pub fn from_body(body: u64) -> Self {
            Symbol((body << 8) | (TAG as u64))
        }

        fn encode_char(ch: char) -> Result<u64, SymbolError> {
            let v = match ch {
                '_' => 1,
                '0'..='9' => 2 + ((ch as u64) - ('0' as u64)),
                'A'..='Z' => 12 + ((ch as u64) - ('A' as u64)),
                'a'..='z' => 38 + ((ch as u64) - ('a' as u64)),
                _ => return Err(SymbolError::BadChar(ch)),
            };
            Ok(v)
        }

        pub fn try_from_bytes(b: &[u8]) -> Result<Self, SymbolError> {
            let mut n = 0;
            let mut accum: u64 = 0;
            while n < b.len() {
                let ch = b[n] as char;
                println!("{}", ch);
                if n >= 9 {
                    return Err(SymbolError::TooLong(b.len()));
                }
                n += 1;
                accum <<= 6;
                let v = match Self::encode_char(ch) {
                    Ok(v) => v,
                    Err(e) => return Err(e),
                };
                accum |= v;
            }
            Ok(Self::from_body(accum))
        }
    }
}

pub mod mercury_db {
    use super::{symbol::Symbol, *};

    #[derive(Clone)]
    pub struct MercuryDatabase {
        pub postgres_arg: String,
    }

    impl ZephyrMock for MercuryDatabase {
        fn mocked() -> Result<Self> {
            Ok(MercuryDatabase {
                postgres_arg: env::var("INGESTOR_DB")
                    .context("INGESTOR_DB env var is not set")?,
            })
        }
    }

    /// A helper enum to wrap query parameters.
    pub enum WriteParam {
        Bytes(Vec<u8>),
        Integer(i64),
    }

    impl WriteParam {
        pub fn as_tosql(&self) -> &(dyn ToSql + Sync) {
            match self {
                WriteParam::Bytes(bytes) => bytes,
                WriteParam::Integer(int) => int,
            }
        }
    }

    /// Reads raw data from a table built from a hashed name.
    impl MercuryDatabase {
        fn read_raw_simple(
            &self,
            _id: i64,
            read_point_hash: [u8; 16],
            read_data: &[i64],
            condition: Option<&[WhereCond]>,
            condition_args: Option<Vec<Vec<u8>>>,
        ) -> Result<Vec<u8>, DatabaseError> {
            let table_name = format!("zephyr_{}", hex::encode(read_point_hash));
            let columns: Vec<String> = read_data
                .iter()
                .map(|&val| Symbol(val as u64)
                    .to_string()
                    .map_err(|_| DatabaseError::ZephyrQueryError))
                .collect::<Result<Vec<_>, _>>()?;
            let columns_string = columns.join(", ");

            let mut client = Client::connect(&self.postgres_arg, NoTls)
                .map_err(|_| DatabaseError::ZephyrQueryError)?;

            let types_map = get_table_types(&mut client, &table_name);
            let mut query = format!("SELECT {} FROM {}", columns_string, table_name);
            let mut owned_params = Vec::new();
            let mut types = Vec::new();

            if let Some(conds) = condition {
                query.push_str(" WHERE ");
                for (idx, cond) in conds.iter().enumerate() {
                    let (operator, col_val) = match cond {
                        WhereCond::ColEq(col) => ("=", col),
                        WhereCond::ColGt(col) => (">", col),
                        WhereCond::ColLt(col) => ("<", col),
                    };
                    let colname = Symbol(*col_val as u64)
                        .to_string()
                        .map_err(|_| DatabaseError::WriteError)?;
                    query.push_str(&format!("{} {} ${}{}", colname, operator, idx + 1,
                        if idx < conds.len() - 1 { " AND " } else { "" }));

                    let col_type = types_map.get(&colname).ok_or(DatabaseError::WriteError)?;
                    let param_raw = &condition_args.as_ref().ok_or(DatabaseError::WriteError)?[idx];
                    if col_type == "bigint" {
                        let param_deser = bincode::deserialize::<ZephyrVal>(param_raw)
                            .map_err(|_| DatabaseError::WriteError)?;
                        let native = match param_deser {
                            ZephyrVal::I128(num) => num as i64,
                            ZephyrVal::I32(num) => num as i64,
                            ZephyrVal::I64(num) => num as i64,
                            ZephyrVal::U32(num) => num as i64,
                            ZephyrVal::U64(num) => num as i64,
                            _ => return Err(DatabaseError::WriteError),
                        };
                        owned_params.push(WriteParam::Integer(native));
                        types.push(Type::INT8);
                    } else {
                        owned_params.push(WriteParam::Bytes(param_raw.clone()));
                        types.push(Type::BYTEA);
                    }
                }
            }

            let stmt = client
                .prepare_typed(&query, &types)
                .map_err(|_| DatabaseError::ZephyrQueryMalformed)?;
            let params: Vec<&(dyn ToSql + Sync)> =
                owned_params.iter().map(|p| p.as_tosql()).collect();

            let result = client.query(&stmt, &params)
                .map_err(|_| DatabaseError::ZephyrQueryError)?;

            // Process the returned rows into our TableRows structure.
            let rows_serialized = result
                .into_iter()
                .map(|row| {
                    let row_wrapped = (0..row.len())
                        .map(|i| {
                            let bytes: Vec<u8> = row.try_get(i)
                                .unwrap_or_else(|_| {
                                    let integer: i64 = row.try_get(i).unwrap();
                                    bincode::serialize(&ZephyrVal::I64(integer)).unwrap()
                                });
                            TypeWrap(bytes)
                        })
                        .collect();
                    TableRow { row: row_wrapped }
                })
                .collect();
            let table_rows = TableRows { rows: rows_serialized };

            Ok(bincode::serialize(&table_rows)
                .map_err(|_| DatabaseError::ZephyrQueryError)?)
        }
    }

    impl ZephyrDatabase for MercuryDatabase {
        fn read_raw(
            &self,
            id: i64,
            read_point_hash: [u8; 16],
            read_data: &[i64],
            condition: Option<&[WhereCond]>,
            condition_args: Option<Vec<Vec<u8>>>,
        ) -> Result<Vec<u8>, DatabaseError> {
            self.read_raw_simple(id, read_point_hash, read_data, condition, condition_args)
        }

        fn write_raw(
            &self,
            _id: i64,
            written_point_hash: [u8; 16],
            write_data: &[i64],
            written: Vec<Vec<u8>>,
        ) -> Result<(), DatabaseError> {
            let mut client = Client::connect(&self.postgres_arg, NoTls)
                .map_err(|_| DatabaseError::ZephyrQueryError)?;
            let table_name = format!("zephyr_{}", hex::encode(written_point_hash));
            let types_map = get_table_types(&mut client, &table_name);

            let mut owned_params = Vec::new();
            let mut types = Vec::new();
            let mut columns = Vec::new();

            for (&col_val, bytes) in write_data.iter().zip(written.iter()) {
                let col = Symbol(col_val as u64)
                    .to_string()
                    .map_err(|_| DatabaseError::WriteError)?;
                columns.push(col.clone());

                let col_type = types_map.get(&col).ok_or(DatabaseError::WriteError)?;
                if col_type == "bigint" {
                    let param_deser: ZephyrVal = bincode::deserialize(bytes)
                        .map_err(|_| DatabaseError::WriteError)?;
                    let native = match param_deser {
                        ZephyrVal::I128(num) => num as i64,
                        ZephyrVal::I32(num) => num as i64,
                        ZephyrVal::I64(num) => num as i64,
                        ZephyrVal::U32(num) => num as i64,
                        ZephyrVal::U64(num) => num as i64,
                        _ => return Err(DatabaseError::WriteError),
                    };
                    owned_params.push(WriteParam::Integer(native));
                    types.push(Type::INT8);
                } else {
                    owned_params.push(WriteParam::Bytes(bytes.clone()));
                    types.push(Type::BYTEA);
                }
            }

            let query = format!(
                "INSERT INTO {} ({}) VALUES ({})",
                table_name,
                columns.join(", "),
                (1..=owned_params.len())
                    .map(|i| format!("${}", i))
                    .collect::<Vec<_>>()
                    .join(", ")
            );

            let stmt = client.prepare_typed(&query, &types)
                .map_err(|_| DatabaseError::WriteError)?;
            let params: Vec<&(dyn ToSql + Sync)> =
                owned_params.iter().map(|p| p.as_tosql()).collect();
            client.execute(&stmt, &params)
                .map_err(|_| DatabaseError::WriteError)?;
            Ok(())
        }

        fn update_raw(
            &self,
            _id: i64,
            written_point_hash: [u8; 16],
            write_data: &[i64],
            written: Vec<Vec<u8>>,
            condition: &[WhereCond],
            condition_args: Vec<Vec<u8>>,
        ) -> Result<(), DatabaseError> {
            let mut client = Client::connect(&self.postgres_arg, NoTls)
                .map_err(|_| DatabaseError::ZephyrQueryError)?;
            let table_name = format!("zephyr_{}", hex::encode(written_point_hash));
            let types_map = get_table_types(&mut client, &table_name);
            let mut owned_params = Vec::new();
            let mut types = Vec::new();

            let set_clause: Vec<String> = write_data
                .iter()
                .enumerate()
                .map(|(idx, &col_val)| {
                    let col = Symbol(col_val as u64)
                        .to_string()
                        .map_err(|_| DatabaseError::WriteError)?;
                    let bytes = &written[idx];
                    let col_type = types_map.get(&col).ok_or(DatabaseError::WriteError)?;
                    if col_type == "bigint" {
                        let param_deser: ZephyrVal = bincode::deserialize(bytes)
                            .map_err(|_| DatabaseError::WriteError)?;
                        let native = match param_deser {
                            ZephyrVal::I128(num) => num as i64,
                            ZephyrVal::I32(num) => num as i64,
                            ZephyrVal::I64(num) => num as i64,
                            ZephyrVal::U32(num) => num as i64,
                            ZephyrVal::U64(num) => num as i64,
                            _ => return Err(DatabaseError::WriteError),
                        };
                        owned_params.push(WriteParam::Integer(native));
                        types.push(Type::INT8);
                    } else {
                        owned_params.push(WriteParam::Bytes(bytes.clone()));
                        types.push(Type::BYTEA);
                    }
                    Ok(format!("{} = ${}", col, idx + 1))
                })
                .collect::<Result<Vec<_>, _>>()?;

            let where_clause: Vec<String> = condition
                .iter()
                .enumerate()
                .map(|(idx, cond)| {
                    let (operator, col_val) = match cond {
                        WhereCond::ColEq(col) => ("=", col),
                        WhereCond::ColGt(col) => (">", col),
                        WhereCond::ColLt(col) => ("<", col),
                    };
                    let colname = Symbol(*col_val as u64)
                        .to_string()
                        .map_err(|_| DatabaseError::WriteError)?;
                    let clause = format!("{} {} ${}", colname, operator, write_data.len() + idx + 1);
                    let col_type = types_map.get(&colname).ok_or(DatabaseError::WriteError)?;
                    let param_raw = &condition_args[idx];
                    if col_type == "bigint" {
                        let param_deser = bincode::deserialize::<ZephyrVal>(param_raw)
                            .map_err(|_| DatabaseError::WriteError)?;
                        let native = match param_deser {
                            ZephyrVal::I128(num) => num as i64,
                            ZephyrVal::I32(num) => num as i64,
                            ZephyrVal::I64(num) => num as i64,
                            ZephyrVal::U32(num) => num as i64,
                            ZephyrVal::U64(num) => num as i64,
                            _ => return Err(DatabaseError::WriteError),
                        };
                        owned_params.push(WriteParam::Integer(native));
                        types.push(Type::INT8);
                    } else {
                        owned_params.push(WriteParam::Bytes(param_raw.clone()));
                        types.push(Type::BYTEA);
                    }
                    Ok(clause)
                })
                .collect::<Result<Vec<_>, _>>()?;

            let query = format!(
                "UPDATE {} SET {} WHERE {}",
                table_name,
                set_clause.join(", "),
                where_clause.join(" AND ")
            );
            let stmt = client.prepare_typed(&query, &types)
                .map_err(|_| DatabaseError::WriteError)?;
            let params: Vec<&(dyn ToSql + Sync)> =
                owned_params.iter().map(|p| p.as_tosql()).collect();
            client.execute(&stmt, &params)
                .map_err(|_| DatabaseError::WriteError)?;
            Ok(())
        }
    }

    /// Retrieves a mapping of column names to their Postgres data types for the given table.
    fn get_table_types(client: &mut Client, table_name: &str) -> HashMap<String, String> {
        let mut types_map = HashMap::new();
        let query = r#"
            SELECT a.attname as column_name,
                   pg_catalog.format_type(a.atttypid, a.atttypmod) as data_type
            FROM pg_catalog.pg_attribute a
            JOIN pg_catalog.pg_class c ON a.attrelid = c.oid
            JOIN pg_catalog.pg_namespace n ON c.relnamespace = n.oid
            WHERE c.relname = $1
              AND a.attnum > 0
              AND NOT a.attisdropped;
        "#;
        let stmt = client.prepare(query).unwrap();
        let rows = client.query(&stmt, &[&table_name]).unwrap();
        for row in rows {
            let column_name: &str = row.get("column_name");
            let data_type: &str = row.get("data_type");
            types_map.insert(column_name.to_string(), data_type.to_string());
        }
        types_map
    }

    #[derive(Clone, Deserialize, Serialize, Debug)]
    pub struct TableRows {
        pub rows: Vec<TableRow>,
    }

    #[derive(Clone, Deserialize, Serialize, Debug)]
    pub struct TableRow {
        pub row: Vec<TypeWrap>,
    }

    #[derive(Clone, Deserialize, Serialize, Debug)]
    pub struct TypeWrap(pub Vec<u8>);
}

impl ZephyrStandard for mercury_db::MercuryDatabase {
    fn zephyr_standard() -> Result<Self> {
        Ok(mercury_db::MercuryDatabase {
            postgres_arg: env::var("INGESTOR_DB")
                .context("INGESTOR_DB env var is not set")?,
        })
    }
}
