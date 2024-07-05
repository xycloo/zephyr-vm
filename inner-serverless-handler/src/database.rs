use std::env;

use anyhow::Result;
use postgres::{
    self,
    types::{ToSql, Type},
    Client, NoTls,
};
use rs_zephyr_common::DatabaseError;
use serde::{Deserialize, Serialize};
use zephyr::{
    db::database::{WhereCond, ZephyrDatabase},
    ZephyrMock, ZephyrStandard,
};

pub mod execution {
    use std::env;

    use postgres::types::Type;

    pub async fn read_binary(id: i64) -> Result<Vec<u8>, ()> {
        let (client, connection) =
            tokio_postgres::connect(&env::var("INGESTOR_DB").unwrap(), tokio_postgres::NoTls)
                .await
                .unwrap();

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("connection error: {}", e);
            }
        });

        let code = client
            .prepare_typed(
                "select code from public.zephyr_programs WHERE id = $1",
                &[Type::INT8],
            )
            .await
            .unwrap();

        let rows = client.query(&code, &[&id]).await.unwrap();
        let code: Vec<u8> = rows.get(0).ok_or(())?.get(0);

        Ok(code)
    }
}

mod symbol {
    const TAG: u8 = 14;

    #[derive(Debug)]
    pub enum SymbolError {
        InvalidSymbol,
    }

    pub struct Symbol(pub u64);

    impl Symbol {
        pub fn to_string(&self) -> Result<String, SymbolError> {
            let mut body = self.0;

            if (body & (TAG as u64)) != (TAG as u64) {
                return Err(SymbolError::InvalidSymbol);
            }

            body >>= 8; // Remove the tag
            let mut result = String::new();

            while body > 0 {
                let index = (body & 0x3F) as u8;
                body >>= 6;
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
    }
}

#[derive(Clone)]
pub struct MercuryDatabase {
    pub postgres_arg: String,
}

impl ZephyrMock for MercuryDatabase {
    fn mocked() -> Result<Self> {
        Ok(MercuryDatabase {
            postgres_arg: env::var("INGESTOR_DB").unwrap(),
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

        println!("reading from {table_name}");
        // auth is actually already performed in that it's the host
        // that hashes the table name for the user.

        let mut columns: Vec<String> = Vec::new();

        for val in read_data {
            if let Ok(res) = symbol::Symbol(*val as u64).to_string() {
                columns.push(res);
            } else {
                println!("error in columns");
                return Err(DatabaseError::ZephyrQueryError);
            }
        }

        println!("columns {:?}", columns);

        let connection = Client::connect(&self.postgres_arg, NoTls);
        let mut client = if let Ok(client) = connection {
            client
        } else {
            println!("failed to connect to db: {:?}", connection.err());
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

        /*         let query = format!("SELECT {} FROM {}", columns_string, table_name);
        println!("query is {query}");
        println!("{:?}", client.prepare_typed(&query, &[]).err());
        let stmt = if let Ok(stmt) = client.prepare_typed(&query, &[]) {
            stmt
        } else {
            return Err(DatabaseError::ZephyrQueryMalformed);
        };*/

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

        println!("query is {}", query);
        let stmt = if let Ok(stmt) = client.prepare_typed(&query, &types) {
            stmt
        } else {
            return Err(DatabaseError::ZephyrQueryMalformed);
        };

        let query_res = client.query(&stmt, &params);
        let result = if let Ok(res) = query_res {
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
            println!("error at {:?}", query_res);
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

    fn update_raw(
        &self,
        _: i64,
        written_point_hash: [u8; 16],
        write_data: &[i64],
        written: Vec<Vec<u8>>,
        condition: &[zephyr::db::database::WhereCond],
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

impl ZephyrStandard for MercuryDatabase {
    fn zephyr_standard() -> Result<Self> {
        Ok(MercuryDatabase {
            postgres_arg: env::var("INGESTOR_DB").unwrap(),
        })
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
