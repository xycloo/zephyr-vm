use rs_zephyr_common::{wrapping::WrappedMaxBytes, ContractDataEntry};
use soroban_sdk::{FromVal, Map, TryFromVal, Val};
use stellar_xdr::next::{Limits, ScVal, WriteXdr};

use crate::{
    log, read_contract_data_entry_by_contract_id_and_key, read_contract_entries_by_contract,
    read_contract_entries_by_contract_to_env, read_contract_instance, EnvClient, SdkError,
};

impl EnvClient {
    fn express_and_deser_entry(
        status: i64,
        offset: i64,
        size: i64,
    ) -> Result<Option<ContractDataEntry>, SdkError> {
        SdkError::express_from_status(status)?;

        let memory: *const u8 = offset as *const u8;
        let slice = unsafe { core::slice::from_raw_parts(memory, size as usize) };

        bincode::deserialize::<Option<ContractDataEntry>>(slice).map_err(|_| SdkError::Conversion)
    }

    pub fn read_contract_instance(
        &self,
        contract: [u8; 32],
    ) -> Result<Option<ContractDataEntry>, SdkError> {
        let contract_parts = WrappedMaxBytes::array_to_max_parts::<4>(&contract);
        let (status, offset, size) = unsafe {
            read_contract_instance(
                contract_parts[0],
                contract_parts[1],
                contract_parts[2],
                contract_parts[3],
            )
        };

        Self::express_and_deser_entry(status, offset, size)
    }

    pub fn read_contract_entry_by_key(
        &self,
        contract: [u8; 32],
        key: ScVal,
    ) -> Result<Option<ContractDataEntry>, SdkError> {
        let key_bytes = key.to_xdr(Limits::none()).unwrap();
        let (offset, size) = (key_bytes.as_ptr() as i64, key_bytes.len() as i64);

        let contract_parts = WrappedMaxBytes::array_to_max_parts::<4>(&contract);
        let (status, inbound_offset, inbound_size) = unsafe {
            read_contract_data_entry_by_contract_id_and_key(
                contract_parts[0],
                contract_parts[1],
                contract_parts[2],
                contract_parts[3],
                offset,
                size,
            )
        };

        Self::express_and_deser_entry(status, inbound_offset, inbound_size)
    }

    pub fn read_contract_entries(
        &self,
        contract: [u8; 32],
    ) -> Result<Vec<ContractDataEntry>, SdkError> {
        let contract_parts = WrappedMaxBytes::array_to_max_parts::<4>(&contract);

        let (status, offset, size) = unsafe {
            read_contract_entries_by_contract(
                contract_parts[0],
                contract_parts[1],
                contract_parts[2],
                contract_parts[3],
            )
        };

        SdkError::express_from_status(status)?;

        let memory: *const u8 = offset as *const u8;
        let slice = unsafe { core::slice::from_raw_parts(memory, size as usize) };

        bincode::deserialize::<Vec<ContractDataEntry>>(slice).map_err(|_| SdkError::Conversion)
    }

    pub fn read_contract_entries_to_env(
        &self,
        env: &soroban_sdk::Env,
        contract: [u8; 32],
    ) -> Result<Map<Val, Val>, SdkError> {
        let contract_parts = WrappedMaxBytes::array_to_max_parts::<4>(&contract);

        let (status, mapobject) = unsafe {
            read_contract_entries_by_contract_to_env(
                contract_parts[0],
                contract_parts[1],
                contract_parts[2],
                contract_parts[3],
            )
        };

        SdkError::express_from_status(status)?;

        Ok(Map::try_from_val(env, &Val::from_payload(mapobject as u64)).unwrap())

        //Ok(Map::new(env))
    }
}
