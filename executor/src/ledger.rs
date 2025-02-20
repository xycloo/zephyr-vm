use rs_zephyr_common::{Account, ContractDataEntry};
use rusqlite::{params, Connection};
use soroban_env_host::xdr::{AccountEntryExt, AccountEntryExtensionV1, AccountEntryExtensionV1Ext, LedgerEntry, Limits, ReadXdr, ScAddress, ScVal, WriteXdr};
use zephyr_vm::{db::ledger::LedgerStateRead, ZephyrStandard};

#[derive(Clone)]
pub struct LedgerReader {
    path: String,
}

impl ZephyrStandard for LedgerReader {
    fn zephyr_standard() -> anyhow::Result<Self>
    where
        Self: Sized,
    {
        Ok(Self {
            path: "/tmp/rs_ingestion_temp/stellar.db".into(),
        })
    }
}

impl LedgerStateRead for LedgerReader {
    fn read_contract_data_entry_by_contract_id_and_key(
        &self,
        contract: ScAddress,
        key: ScVal,
    ) -> Option<ContractDataEntry> {
        let conn = Connection::open(&self.path).unwrap();
        let query_string = format!("SELECT contractid, key, ledgerentry, \"type\", lastmodified FROM contractdata where contractid = ?1 AND key = ?2");

        let mut stmt = conn.prepare(&query_string).unwrap();
        let entries = stmt.query_map(
            params![
                contract.to_xdr_base64(Limits::none()).unwrap(),
                key.to_xdr_base64(Limits::none()).unwrap()
            ],
            |row| {
                Ok(ContractDataEntry {
                    contract_id: contract.clone(),
                    key: ScVal::from_xdr_base64(
                        row.get::<usize, String>(1).unwrap(),
                        Limits::none(),
                    )
                    .unwrap(),
                    entry: LedgerEntry::from_xdr_base64(
                        row.get::<usize, String>(2).unwrap(),
                        Limits::none(),
                    )
                    .unwrap(),
                    durability: row.get(3).unwrap(),
                    last_modified: row.get(4).unwrap(),
                })
            },
        );

        let entries = entries
            .unwrap()
            .map(|r| r.unwrap())
            .collect::<Vec<ContractDataEntry>>();

        entries.get(0).cloned()
    }

    fn read_contract_data_entries_by_contract_id(
        &self,
        contract: ScAddress,
    ) -> Vec<ContractDataEntry> {
        let conn = Connection::open(&self.path).unwrap();
        let query_string = format!("SELECT contractid, key, ledgerentry, \"type\", lastmodified FROM contractdata where contractid = ?1");
        let mut stmt = conn.prepare(&query_string).unwrap();
        let entries = stmt.query_map(
            params![contract.to_xdr_base64(Limits::none()).unwrap()],
            |row| {
                let entry = ContractDataEntry {
                    contract_id: contract.clone(),
                    key: ScVal::from_xdr_base64(
                        row.get::<usize, String>(1).unwrap(),
                        Limits::none(),
                    )
                    .unwrap(),
                    entry: LedgerEntry::from_xdr_base64(
                        row.get::<usize, String>(2).unwrap(),
                        Limits::none(),
                    )
                    .unwrap(),
                    durability: row.get(3).unwrap(),
                    last_modified: row.get(4).unwrap(),
                };

                Ok(entry)
            },
        );

        entries
            .unwrap()
            .map(|r| r.unwrap())
            .collect::<Vec<ContractDataEntry>>()
    }

    fn read_account(&self, account: String) -> Option<Account> {
        let conn = Connection::open(&self.path).unwrap();
        let mut stmt = conn
            .prepare("SELECT * FROM accounts where accountid = ?1")
            .unwrap();

        let account = stmt.query_map(params![account], |row| {
            let (buying_liabilities, selling_liabilities, num_sponsored, num_sponsoring) = {
                let extension = {
                    let b64: String = row.get(12).unwrap_or("".into()); // note, this will get caught by the following line.
                    AccountEntryExt::from_xdr_base64(b64, Limits::none())
                };

                match extension {
                    Ok(AccountEntryExt::V0) => {
                        (row.get(2).unwrap_or(0.0), row.get(3).unwrap_or(0.0), 0, 0)
                    }
                    Ok(AccountEntryExt::V1(AccountEntryExtensionV1 { ext, liabilities })) => {
                        let (sponsored, sponsoring) = match ext {
                            AccountEntryExtensionV1Ext::V0 => (0, 0),
                            AccountEntryExtensionV1Ext::V2(v2_ext) => {
                                (v2_ext.num_sponsored as i32, v2_ext.num_sponsoring as i32)
                            }
                        };

                        (
                            liabilities.buying as f64,
                            liabilities.selling as f64,
                            sponsored,
                            sponsoring,
                        )
                    }
                    Err(_) => (row.get(2).unwrap_or(0.0), row.get(3).unwrap_or(0.0), 0, 0),
                }
            };

            Ok(Account {
                account_id: row.get(0).unwrap(),
                native_balance: row.get(1).unwrap(),
                buying_liabilities,
                selling_liabilities,
                seq_num: row.get(4).unwrap(),
                num_subentries: row.get(5).unwrap(),
                num_sponsored,
                num_sponsoring,
            })
        });

        let last = if let Ok(account) = account {
            if let Some(Ok(account)) = account.last() {
                Some(account)
            } else {
                None
            }
        } else {
            None
        };

        last
    }
}
