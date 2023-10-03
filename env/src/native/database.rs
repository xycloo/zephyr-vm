use crate::{db::{error::DatabaseError, database::ZephyrDatabase}, ZephyrStandard, ZephyrMock};
use anyhow::Result;


#[derive(Clone)]
pub struct DbAuth {
    host: String,
    dbname: String,
    user: String,
    password: String
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
            password 
        })
    }
}

impl ZephyrMock for DbAuth {
    fn mocked() -> Result<Self> {
        Ok(Self { host: Default::default(), dbname: Default::default(), user: Default::default(), password: Default::default() })
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
        Ok(MercuryDatabase { connect: DbAuth::mocked()? })
    }
}

impl ZephyrDatabase for MercuryDatabase {
    fn read_raw(&self, user_id: i64, read_point_hash: [u8; 32], read_data: &[i64]) -> Result<&[u8], DatabaseError> {
        Ok(&[])
    }

    fn write_raw(&self, user_id: i64, written_point_hash: [u8; 32], written: &[i64]) -> Result<(), DatabaseError> {
        Ok(())
    }
}

impl ZephyrStandard for MercuryDatabase {
    fn zephyr_standard() -> Result<Self> {
        Ok(MercuryDatabase { 
            connect: DbAuth::from_env()?
        })
    }
}
