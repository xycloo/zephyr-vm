use rs_zephyr_common::{Account, ContractDataEntry};
use soroban_env_host::xdr::{
    AccountEntryExt, AccountId, LedgerEntry, LedgerEntryData, LedgerKey, LedgerKeyAccount,
    LedgerKeyContractData, Limits, ReadXdr, ScAddress, ScVal, Uint256, WriteXdr,
};
use zephyr_vm::{
    db::ledger::LedgerStateRead, snapshot::raw_endpoint::entry_and_ttl, ZephyrStandard,
};

#[derive(Clone)]
pub struct RawLedgerReader {}

impl ZephyrStandard for RawLedgerReader {
    fn zephyr_standard() -> anyhow::Result<Self>
    where
        Self: Sized,
    {
        Ok(Self {})
    }
}

impl LedgerStateRead for RawLedgerReader {
    fn read_contract_data_entry_by_contract_id_and_key(
        &self,
        contract: ScAddress,
        key: ScVal,
    ) -> Option<ContractDataEntry> {
        // todo change api to better support this.
        let persistent_key = LedgerKey::ContractData(LedgerKeyContractData {
            contract: contract.clone(),
            key: key.clone(),
            durability: soroban_env_host::xdr::ContractDataDurability::Persistent,
        });

        let temp_key = LedgerKey::ContractData(LedgerKeyContractData {
            contract: contract.clone(),
            key,
            durability: soroban_env_host::xdr::ContractDataDurability::Persistent,
        });

        if let Ok(Some((entry, _))) = entry_and_ttl(persistent_key.to_xdr(Limits::none()).unwrap())
        {
            let ledger_entry = LedgerEntry::from_xdr(entry, Limits::none()).unwrap();
            if let LedgerEntryData::ContractData(entry) = &ledger_entry.data {
                let durability = entry.durability as u8;
                return Some(ContractDataEntry {
                    durability: durability as i32,
                    contract_id: contract.clone(),
                    key: entry.key.clone(),
                    entry: ledger_entry.clone(),
                    last_modified: i32::MAX,
                });
            }
        } else {
            if let Ok(Some((entry, _))) = entry_and_ttl(temp_key.to_xdr(Limits::none()).unwrap()) {
                let ledger_entry = LedgerEntry::from_xdr(entry, Limits::none()).unwrap();
                if let LedgerEntryData::ContractData(entry) = &ledger_entry.data {
                    let durability = entry.durability as u8;
                    return Some(ContractDataEntry {
                        durability: durability as i32,
                        contract_id: contract,
                        key: entry.key.clone(),
                        entry: ledger_entry,
                        last_modified: i32::MAX,
                    });
                }
            }
        }
        None
    }

    /// not supported on raw reader
    fn read_contract_data_entries_by_contract_id(
        &self,
        contract: ScAddress,
    ) -> Vec<ContractDataEntry> {
        vec![]
    }

    fn read_account(&self, account: String) -> Option<Account> {
        let ledger_key = LedgerKey::Account(LedgerKeyAccount {
            account_id: AccountId(soroban_env_host::xdr::PublicKey::PublicKeyTypeEd25519(
                Uint256(
                    stellar_strkey::ed25519::PublicKey::from_string(&account)
                        .unwrap()
                        .0,
                ),
            )),
        });

        if let Ok(Some((entry, _))) = entry_and_ttl(ledger_key.to_xdr(Limits::none()).unwrap()) {
            let ledger_entry = LedgerEntry::from_xdr(entry, Limits::none()).unwrap();
            if let LedgerEntryData::Account(account_entry) = &ledger_entry.data {
                let (buying, selling) = match &account_entry.ext {
                    AccountEntryExt::V0 => (0, 0),
                    AccountEntryExt::V1(l) => (l.liabilities.buying, l.liabilities.selling),
                };

                return Some(Account {
                    account_id: account,
                    native_balance: account_entry.balance as f64,
                    buying_liabilities: buying as f64,
                    selling_liabilities: selling as f64,
                    seq_num: account_entry.seq_num.0 as f64,
                    num_sponsored: 0,
                    num_subentries: account_entry.num_sub_entries as i32,
                    num_sponsoring: 0,
                });
            }
        }

        None
    }
}
