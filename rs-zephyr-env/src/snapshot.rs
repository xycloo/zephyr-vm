//! Snapshot utilites required to correctly perform tx simulation
//! calculations.

use std::rc::Rc;

use rusqlite::{params, Connection};
use snapshot_utils::get_ttl;
use soroban_env_host::storage::{EntryWithLiveUntil, SnapshotSource};
use soroban_env_host::xdr::{
    AccountEntry, LedgerEntry, LedgerEntryExt, LedgerKey, Limits, PublicKey, ReadXdr,
    SequenceNumber, Thresholds, WriteXdr,
};
use soroban_env_host::HostError;
use soroban_simulation::SnapshotSourceWithArchive;

pub struct DynamicSnapshot {}

pub mod snapshot_utils {
    use rusqlite::{params, Connection};
    use sha2::{Digest, Sha256};
    use soroban_env_host::xdr::{
        Hash, LedgerEntry, LedgerEntryData, LedgerKey, Limits, ReadXdr, WriteXdr,
    };

    pub fn get_current_ledger_sequence() -> (i32, i64) {
        let conn = Connection::open("/tmp/rs_ingestion_temp/stellar.db").unwrap();
        let query_string = format!(
            "SELECT ledgerseq, closetime FROM ledgerheaders ORDER BY ledgerseq DESC LIMIT 1"
        );

        let mut stmt = conn.prepare(&query_string).unwrap();
        let mut entries = stmt.query(params![]).unwrap();

        let row = entries.next().unwrap();

        if row.is_none() {
            // Unrecoverable: no ledger is running
            return (0, 0);
        }

        (
            row.unwrap().get(0).unwrap_or(0),
            row.unwrap().get(1).unwrap_or(0),
        )
    }

    pub fn get_ttl(key: LedgerKey) -> u32 {
        let mut hasher = Sha256::new();
        hasher.update(key.to_xdr(Limits::none()).unwrap());
        let result = {
            let hashed = hasher.finalize().as_slice().try_into().unwrap();
            Hash(hashed).to_xdr_base64(Limits::none()).unwrap()
        };

        let conn = Connection::open("/tmp/rs_ingestion_temp/stellar.db").unwrap();
        let query_string = format!("SELECT ledgerentry FROM ttl WHERE keyhash = ?1");

        let mut stmt = conn.prepare(&query_string).unwrap();
        let mut entries = stmt.query(params![result]).unwrap();

        let row = entries.next().unwrap();

        if row.is_none() {
            // TODO: error log
            return 0;
        }

        let entry = {
            let string: String = row.unwrap().get(0).unwrap();
            LedgerEntry::from_xdr_base64(&string, Limits::none()).unwrap()
        };

        let LedgerEntryData::Ttl(ttl) = entry.data else {
            return 0;
        };
        ttl.live_until_ledger_seq
    }
}

impl SnapshotSourceWithArchive for DynamicSnapshot {
    fn get_including_archived(
        &self,
        key: &Rc<LedgerKey>,
    ) -> std::result::Result<Option<EntryWithLiveUntil>, soroban_env_host::HostError> {
        let LedgerKey::ConfigSetting(setting) = key.as_ref() else {
            return Err(HostError::from(
                soroban_env_host::Error::from_contract_error(0),
            ));
        };

        let conn = Connection::open("/tmp/rs_ingestion_temp/stellar.db").unwrap();
        let query_string =
            format!("SELECT ledgerentry FROM configsettings WHERE configsettingid = ?1");

        let mut stmt = conn.prepare(&query_string).unwrap();
        let mut entries = stmt
            .query(params![setting.config_setting_id as i32])
            .unwrap();

        let row = entries.next().unwrap();

        if row.is_none() {
            // TODO: error log
            return Err(HostError::from(
                soroban_env_host::Error::from_contract_error(0),
            ));
        }

        let entry = {
            let string: String = row.unwrap().get(0).unwrap();
            LedgerEntry::from_xdr_base64(&string, Limits::none()).unwrap()
        };

        Ok(Some((Rc::new(entry), Some(u32::MAX))))
    }
}

pub fn snapshot_get_universal(
    //key: &std::rc::Rc<soroban_env_host::xdr::LedgerKey>,
    key: Vec<u8>,
) -> Result<Option<(Vec<u8>, Option<u32>)>, soroban_env_host::HostError> {
    let key = LedgerKey::from_xdr(key, Limits::none())
        .map_err(|_| soroban_env_host::xdr::Error::Invalid)?;

    let entry: Option<EntryWithLiveUntil> = match key {
        LedgerKey::Account(key) => {
            let PublicKey::PublicKeyTypeEd25519(ed25519) = key.account_id.0.clone();
            let id = stellar_strkey::ed25519::PublicKey(ed25519.0).to_string();

            let conn = Connection::open("/tmp/rs_ingestion_temp/stellar.db").unwrap();
            let query_string = format!("SELECT balance FROM accounts where accountid = ?1");

            let mut stmt = conn.prepare(&query_string).unwrap();
            let mut entries = stmt.query(params![id]).unwrap();

            let row = entries.next().unwrap();

            if row.is_none() {
                return Ok(None);
            }
            let row = row.unwrap();

            let entry = LedgerEntry {
                last_modified_ledger_seq: 0,
                ext: LedgerEntryExt::V0,
                data: soroban_env_host::xdr::LedgerEntryData::Account(AccountEntry {
                    account_id: key.account_id.clone(),
                    balance: row.get(0).unwrap(),
                    seq_num: SequenceNumber(0),
                    num_sub_entries: 0,
                    inflation_dest: None,
                    flags: 0,
                    home_domain: Default::default(),
                    thresholds: Thresholds([0; 4]),
                    signers: vec![].try_into().unwrap(),
                    ext: soroban_env_host::xdr::AccountEntryExt::V0,
                }),
            };

            Some((Rc::new(entry), None))
        }

        LedgerKey::ContractCode(key) => {
            let hash = key.hash.clone();
            let conn = Connection::open("/tmp/rs_ingestion_temp/stellar.db").unwrap();
            let query_string = format!("SELECT ledgerentry FROM contractcode where hash = ?1");

            let mut stmt = conn.prepare(&query_string).unwrap();
            let mut entries = stmt
                .query(params![hash.to_xdr_base64(Limits::none()).unwrap()])
                .unwrap();

            let row = entries.next().unwrap();

            if row.is_none() {
                return Ok(None);
            }
            let row = row.unwrap();

            let xdr_entry: String = row.get(0).unwrap();
            let xdr_entry = LedgerEntry::from_xdr_base64(xdr_entry, Limits::none()).unwrap();

            Some((
                Rc::new(xdr_entry),
                Some(get_ttl(LedgerKey::ContractCode(key.clone()))),
            ))
        }

        LedgerKey::ContractData(key) => {
            let contract = key.contract.clone();
            let scval = key.key.clone();

            let conn = Connection::open("/tmp/rs_ingestion_temp/stellar.db").unwrap();
            let query_string =
                format!("SELECT ledgerentry FROM contractdata where contractid = ?1 AND key = ?2");

            let mut stmt = conn.prepare(&query_string).unwrap();
            let mut entries = stmt
                .query(params![
                    contract.to_xdr_base64(Limits::none()).unwrap(),
                    scval.to_xdr_base64(Limits::none()).unwrap()
                ])
                .unwrap();
            let row = entries.next().unwrap();

            if row.is_none() {
                return Ok(None);
            }
            let row = row.unwrap();

            let xdr_entry: String = row.get(0).unwrap();
            let xdr_entry = LedgerEntry::from_xdr_base64(xdr_entry, Limits::none()).unwrap();

            Some((
                Rc::new(xdr_entry),
                Some(get_ttl(LedgerKey::ContractData(key.clone()))),
            ))
        }

        _ => None,
    };

    if let Some(key) = entry {
        Ok(Some((key.0.to_xdr(Limits::none())?, key.1)))
    } else {
        Ok(None)
    }
}

impl SnapshotSource for DynamicSnapshot {
    fn get(
        &self,
        key: &std::rc::Rc<soroban_env_host::xdr::LedgerKey>,
    ) -> Result<Option<soroban_env_host::storage::EntryWithLiveUntil>, soroban_env_host::HostError>
    {
        let xdred = snapshot_get_universal(key.as_ref().to_xdr(Limits::none()).unwrap())?;
        if let Some(xdr_key) = xdred {
            Ok(Some((
                Rc::new(LedgerEntry::from_xdr(xdr_key.0, Limits::none())?),
                xdr_key.1,
            )))
        } else {
            Ok(None)
        }
    }
}
