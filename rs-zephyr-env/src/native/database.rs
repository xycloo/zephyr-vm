use crate::{
    db::{database::ZephyrDatabase, error::DatabaseError},
    ZephyrMock, ZephyrStandard,
};
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct DbAuth {
    host: String,
    dbname: String,
    user: String,
    password: String,
}

impl DbAuth {
    fn from_env() -> Result<Self> {
        let host = std::env::var("ZEPHYRDB_HOST")?;
        let dbname = std::env::var("ZEPHYRDB_NAME")?;
        let user = std::env::var("ZEPHYRDB_USER")?;
        let password = std::env::var("ZEPHYRDB_PWD")?;

        Ok(Self {
            host,
            dbname,
            user,
            password,
        })
    }
}

impl ZephyrMock for DbAuth {
    fn mocked() -> Result<Self> {
        Ok(Self {
            host: Default::default(),
            dbname: Default::default(),
            user: Default::default(),
            password: Default::default(),
        })
    }
}

#[derive(Clone)]
pub struct MercuryDatabase {
    connect: DbAuth,
}

impl MercuryDatabase {
    fn run_sql_insert(&self) -> Result<(), DatabaseError> {
        Ok(())
    }
}

impl ZephyrMock for MercuryDatabase {
    fn mocked() -> Result<Self> {
        Ok(MercuryDatabase {
            connect: DbAuth::mocked()?,
        })
    }
}

impl ZephyrDatabase for MercuryDatabase {
    fn read_raw(
        &self,
        user_id: i64,
        read_point_hash: [u8; 16],
        read_data: &[i64],
    ) -> Result<Vec<u8>, DatabaseError> {
        let rows = TableRows {
            rows: vec![TableRow {
                row: vec![TypeWrap(vec![2, 5, 4, 2, 4])],
            }],
        };

        Ok(bincode::serialize(&rows).unwrap())
    }

    fn write_raw(
        &self,
        user_id: i64,
        written_point_hash: [u8; 16],
        write_data: &[i64],
        written: Vec<Vec<u8>>,
    ) -> Result<(), DatabaseError> {
        println!("{:?}", write_data);
        println!("{:?}", written);
        Ok(())
    }
}

impl ZephyrStandard for MercuryDatabase {
    fn zephyr_standard() -> Result<Self> {
        Ok(MercuryDatabase {
            connect: DbAuth::from_env()?,
        })
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct TableRows {
    pub rows: Vec<TableRow>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct TableRow {
    pub row: Vec<TypeWrap>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct TypeWrap(pub Vec<u8>);
