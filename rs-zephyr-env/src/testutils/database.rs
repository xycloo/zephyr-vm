use crate::{
    db::{database::ZephyrDatabase, ledger::LedgerStateRead},
    ZephyrMock, ZephyrStandard,
};
use anyhow::Result;
use rs_zephyr_common::{ContractDataEntry, DatabaseError};
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

impl ZephyrMock for MercuryDatabase {
    fn mocked() -> Result<Self> {
        Ok(MercuryDatabase {
            connect: DbAuth::mocked()?,
        })
    }
}

#[derive(Clone)]
pub struct LedgerReader {}

impl LedgerStateRead for LedgerReader {
    /*fn read_contract_data_entries_by_contract_ids(&self, contracts: impl IntoIterator<Item = soroban_env_host::xdr::ScAddress>) -> Vec<ContractDataEntry> {
        vec![]
    }

    fn read_contract_instance_by_contract_ids(&self, contracts: impl IntoIterator<Item = soroban_env_host::xdr::ScAddress>) -> Vec<ContractDataEntry> {
        vec![]
    }*/

    fn read_contract_data_entry_by_contract_id_and_key(
        &self,
        contract: soroban_env_host::xdr::ScAddress,
        key: soroban_env_host::xdr::ScVal,
    ) -> Option<ContractDataEntry> {
        None
    }

    fn read_contract_data_entries_by_contract_id(
        &self,
        contract: soroban_env_host::xdr::ScAddress,
    ) -> Vec<ContractDataEntry> {
        vec![]
    }

    //fn read_contract_instance_by_contract_id(&self, contract: soroban_env_host::xdr::ScAddress) -> Option<ContractDataEntry> {
    //None
    //}
}

impl ZephyrDatabase for MercuryDatabase {
    fn read_raw(
        &self,
        user_id: i64,
        read_point_hash: [u8; 16],
        read_data: &[i64],
        condition: Option<&[crate::db::database::WhereCond]>,
        condition_args: Option<Vec<Vec<u8>>>,
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
        Ok(())
    }

    fn update_raw(
        &self,
        user_id: i64,
        written_point_hash: [u8; 16],
        write_data: &[i64],
        written: Vec<Vec<u8>>,
        condition: &[crate::db::database::WhereCond],
        condition_args: Vec<Vec<u8>>,
    ) -> Result<(), DatabaseError> {
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
