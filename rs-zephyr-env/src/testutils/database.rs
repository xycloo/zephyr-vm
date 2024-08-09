use super::symbol;
use crate::{
    db::{
        database::{WhereCond, ZephyrDatabase},
        ledger::LedgerStateRead,
    },
    ZephyrMock,
};
use anyhow::Result;
use postgres::{
    self,
    types::{ToSql, Type},
    Client, NoTls,
};
use rs_zephyr_common::{ContractDataEntry, DatabaseError};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct LedgerReader {}

impl LedgerStateRead for LedgerReader {
    fn read_contract_data_entry_by_contract_id_and_key(
        &self,
        _contract: soroban_env_host::xdr::ScAddress,
        _key: soroban_env_host::xdr::ScVal,
    ) -> Option<ContractDataEntry> {
        None
    }

    fn read_contract_data_entries_by_contract_id(
        &self,
        _contract: soroban_env_host::xdr::ScAddress,
    ) -> Vec<ContractDataEntry> {
        vec![]
    }

    fn read_account(&self, account: String) -> Option<rs_zephyr_common::Account> {
        None
    }
}

impl ZephyrMock for LedgerReader {
    fn mocked() -> Result<Self>
    where
        Self: Sized,
    {
        Ok(Self {})
    }
}

#[derive(Clone)]
pub struct MercuryDatabase {
    pub postgres_arg: String,
}

impl ZephyrMock for MercuryDatabase {
    fn mocked() -> Result<Self> {
        Ok(MercuryDatabase {
            postgres_arg: "postgres://postgres:postgres@localhost:5432".to_string(),
        })
    }
}

impl ZephyrDatabase for MercuryDatabase {
    fn read_raw(
        &self,
        _: i64,
        read_point_hash: [u8; 16],
        read_data: &[i64],
        condition: Option<&[WhereCond]>,
        condition_args: Option<Vec<Vec<u8>>>,
    ) -> Result<Vec<u8>, DatabaseError> {
        let table_name = format!("zephyr_{}", hex::encode(read_point_hash).as_str());
        let mut columns: Vec<String> = Vec::new();

        for val in read_data {
            if let Ok(res) = symbol::Symbol(*val as u64).to_string() {
                columns.push(res);
            } else {
                return Err(DatabaseError::ZephyrQueryError);
            }
        }

        let mut client = if let Ok(client) = Client::connect(&self.postgres_arg, NoTls) {
            client
        } else {
            return Err(DatabaseError::ZephyrQueryError);
        };

        let mut columns_string = String::new();
        for (idx, column) in columns.iter().enumerate() {
            if idx == columns.len() - 1 {
                columns_string.push_str(&format!("{}", column))
            } else {
                columns_string.push_str(&format!("{}, ", column))
            }
        }

        let mut query = format!("SELECT {} FROM {}", columns_string, table_name);

        let mut params: Vec<&(dyn ToSql + Sync)> = Vec::new();
        let mut types = Vec::new();
        if let Some(condition) = condition {
            query.push_str(" WHERE ");

            for idx in 0..condition.len() {
                match condition[idx] {
                    WhereCond::ColEq(column) => {
                        let colname = if let Ok(string) = symbol::Symbol(column as u64).to_string()
                        {
                            string
                        } else {
                            return Err(DatabaseError::WriteError);
                        };

                        if idx != condition.len() - 1 {
                            query.push_str(&format!("{} = ${} AND ", colname, idx + 1));
                        } else {
                            query.push_str(&format!("{} = ${}", colname, idx + 1));
                        }
                    }
                }
                params.push(&condition_args.as_ref().unwrap()[idx])
            }

            for _ in 0..params.len() {
                types.push(Type::BYTEA)
            }
        }

        let stmt = if let Ok(stmt) = client.prepare_typed(&query, &types) {
            stmt
        } else {
            return Err(DatabaseError::ZephyrQueryMalformed);
        };

        let result = if let Ok(res) = client.query(&stmt, &params) {
            let mut rows = Vec::new();

            for row in res {
                let mut row_wrapped = Vec::new();

                let row_length = row.len();
                for in_row_idx in 0..row_length {
                    // currently we return an error.
                    // an alternative would be wrapping in an option.
                    let bytes = row
                        .try_get(in_row_idx)
                        .map_err(|_| DatabaseError::ZephyrQueryError)?;
                    row_wrapped.push(TypeWrap(bytes))
                }

                rows.push(TableRow { row: row_wrapped })
            }

            TableRows { rows }
        } else {
            return Err(DatabaseError::ZephyrQueryError);
        };

        Ok(bincode::serialize(&result).unwrap())
    }

    fn write_raw(
        &self,
        _: i64,
        written_point_hash: [u8; 16],
        write_data: &[i64],
        written: Vec<Vec<u8>>,
    ) -> Result<(), DatabaseError> {
        let connection = Client::connect(&self.postgres_arg, NoTls);
        let mut client = if let Ok(client) = connection {
            client
        } else {
            println!("{:?}", connection.err().unwrap());
            return Err(DatabaseError::ZephyrQueryError);
        };

        let mut params: Vec<&(dyn ToSql + Sync)> = Vec::new();
        let mut types = Vec::new();

        let mut query = String::from("INSERT INTO ");
        query.push_str(&format!(
            "zephyr_{}",
            hex::encode(written_point_hash).as_str()
        ));
        query.push_str(" (");

        for idx in 0..write_data.len() {
            let col = if let Ok(string) = symbol::Symbol(write_data[idx] as u64).to_string() {
                string
            } else {
                return Err(DatabaseError::WriteError);
            };
            let bytes = &written[idx];

            query.push_str(&col);

            if idx != write_data.len() - 1 {
                query.push_str(", ");
            }

            params.push(bytes);
        }

        query.push(')');

        query.push_str(" VALUES (");
        for n in 1..=params.len() {
            if n == params.len() {
                query.push_str(&format!("${}", n))
            } else {
                query.push_str(&format!("${}, ", n))
            }
        }
        query.push(')');

        for _ in 0..params.len() {
            types.push(Type::BYTEA)
        }

        let prepared = client.prepare_typed(&query, &types);
        let statement = if let Ok(stmt) = prepared {
            stmt
        } else {
            return Err(DatabaseError::WriteError);
        };

        let insert = client.execute(&statement, &params);
        if let Ok(_) = insert {
            Ok(())
        } else {
            Err(DatabaseError::WriteError)
        }
    }

    fn update_raw(
        &self,
        _: i64,
        written_point_hash: [u8; 16],
        write_data: &[i64],
        written: Vec<Vec<u8>>,
        condition: &[WhereCond],
        condition_args: Vec<Vec<u8>>,
    ) -> Result<(), DatabaseError> {
        let connection = Client::connect(&self.postgres_arg, NoTls);
        let mut client = if let Ok(client) = connection {
            client
        } else {
            println!("{:?}", connection.err().unwrap());
            return Err(DatabaseError::ZephyrQueryError);
        };
        let mut params: Vec<&(dyn ToSql + Sync)> = Vec::new();
        let mut types = Vec::new();

        let mut query = String::from("UPDATE ");
        query.push_str(&format!(
            "zephyr_{}",
            hex::encode(written_point_hash).as_str()
        ));
        query.push_str(" SET ");

        for idx in 0..write_data.len() {
            let col = if let Ok(string) = symbol::Symbol(write_data[idx] as u64).to_string() {
                string
            } else {
                return Err(DatabaseError::WriteError);
            };
            let bytes = &written[idx];

            query.push_str(&col);

            if idx != write_data.len() - 1 {
                query.push_str(&format!(" = ${}, ", idx + 1));
            } else {
                query.push_str(&format!(" = ${}", idx + 1));
            }

            params.push(bytes);
        }

        query.push_str(" WHERE ");

        for idx in 0..condition.len() {
            match condition[idx] {
                WhereCond::ColEq(column) => {
                    let colname = if let Ok(string) = symbol::Symbol(column as u64).to_string() {
                        string
                    } else {
                        return Err(DatabaseError::WriteError);
                    };

                    if idx != condition.len() - 1 {
                        query.push_str(&format!(
                            "{} = ${} AND ",
                            colname,
                            write_data.len() + idx + 1
                        ));
                    } else {
                        query.push_str(&format!("{} = ${}", colname, write_data.len() + idx + 1));
                    }
                }
            }

            params.push(&condition_args[idx])
        }

        for _ in 0..params.len() {
            types.push(Type::BYTEA)
        }

        let statement = if let Ok(stmt) = client.prepare_typed(&query, &types) {
            stmt
        } else {
            return Err(DatabaseError::WriteError);
        };

        if let Ok(_) = client.execute(&statement, &params) {
            Ok(())
        } else {
            Err(DatabaseError::WriteError)
        }
    }
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
