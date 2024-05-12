use std::{env, rc::Rc};

use rusqlite::{params, Connection};
use soroban_env_host::storage::{SnapshotSource, EntryWithLiveUntil};
use stellar_xdr::next::{AccountEntry, ContractCodeEntry, LedgerEntry, LedgerEntryExt, LedgerEntryExtensionV1, LedgerKey, Limits, PublicKey, ReadXdr, SequenceNumber, Thresholds, WriteXdr};

pub struct DynamicSnapshot {}

impl SnapshotSource for DynamicSnapshot {
    fn get(&self, key: &std::rc::Rc<stellar_xdr::next::LedgerKey>) -> Result<Option<soroban_env_host::storage::EntryWithLiveUntil>, soroban_env_host::HostError> {
        println!("requested {:?}", key);
        let entry: Option<EntryWithLiveUntil> = match key.as_ref() {
            LedgerKey::Account(key) => {
                let PublicKey::PublicKeyTypeEd25519(ed25519) = key.account_id.0.clone();
                let id = stellar_strkey::ed25519::PublicKey(ed25519.0).to_string();

                let conn = Connection::open("/tmp/rs_ingestion_temp/stellar.db").unwrap();
                let query_string = format!("SELECT balance FROM accounts where accountid = ?1");
                
                let mut stmt = conn.prepare(&query_string).unwrap();
                let mut entries = stmt.query(params![id]).unwrap();
                let row = entries.next().unwrap().unwrap();
                let entry = LedgerEntry {
                    last_modified_ledger_seq: 0,
                    ext: LedgerEntryExt::V0,
                    data: stellar_xdr::next::LedgerEntryData::Account(AccountEntry {
                        account_id: key.account_id.clone(),
                        balance: row.get(0).unwrap(),
                        seq_num: SequenceNumber(0),
                        num_sub_entries: 0,
                        inflation_dest: None,
                        flags: 0,
                        home_domain: Default::default(),
                        thresholds: Thresholds([0;4]),
                        signers: vec![].try_into().unwrap(),
                        ext: stellar_xdr::next::AccountEntryExt::V0
                    })
                };


                Some((Rc::new(entry), None))
            }

            LedgerKey::ContractCode(key) => {
                let hash = key.hash.clone();
                let conn = Connection::open("/tmp/rs_ingestion_temp/stellar.db").unwrap();
                let query_string = format!("SELECT ledgerentry FROM contractcode where hash = ?1");
                
                let mut stmt = conn.prepare(&query_string).unwrap();
                let mut entries = stmt.query(params![hash.to_xdr_base64(Limits::none()).unwrap()]).unwrap();
                let row = entries.next().unwrap().unwrap();
                let xdr_entry: String = row.get(0).unwrap();
                let xdr_entry = LedgerEntry::from_xdr_base64(xdr_entry, Limits::none()).unwrap();
                
                Some((Rc::new(xdr_entry), Some(u32::MAX)))
            }

            LedgerKey::ContractData(key) => {
                let contract = key.contract.clone();
                let scval = key.key.clone();
                
                let conn = Connection::open("/tmp/rs_ingestion_temp/stellar.db").unwrap();
                let query_string = format!("SELECT ledgerentry FROM contractdata where contractid = ?1 AND key = ?2");
                
                let mut stmt = conn.prepare(&query_string).unwrap();
                let mut entries = stmt.query(params![contract.to_xdr_base64(Limits::none()).unwrap(), scval.to_xdr_base64(Limits::none()).unwrap()]).unwrap();
                let row = entries.next().unwrap().unwrap();
                let xdr_entry: String = row.get(0).unwrap();
                let xdr_entry = LedgerEntry::from_xdr_base64(xdr_entry, Limits::none()).unwrap();

                Some((Rc::new(xdr_entry), Some(u32::MAX)))
            }

            _ => None
        };

        Ok(entry)
    }
}
