//! This module defines the interaction between the ZephyrVM
//! and Stellar's ledger state. This implementation is stricter
//! than the [`ZephyrDatabase`] implementation because it is not
//! implementation agnostic, as we're always talking about the Stellar
//! ledger.

use std::{cell::RefCell, rc::Rc};

use anyhow::Result;
use stellar_xdr::next::{ContractDataDurability, LedgerEntry, ScAddress, ScVal};

use crate::{ZephyrMock, ZephyrStandard};

#[derive(Debug)]
pub struct ContractDataEntry {
    pub contract_id: ScAddress,
    pub key: ScVal,
    pub entry: LedgerEntry,
    pub durability: ContractDataDurability,
    pub last_modified: i32
}

/// Reads state from the Stellar Ledger.
pub trait LedgerStateRead {
    /// Returns a vector of Contract Data Entries given a set of contract addresses.
    fn read_contract_data_entries_by_contract_ids(&self, contracts: impl IntoIterator<Item = ScAddress>) -> Vec<ContractDataEntry>;

    /// Returns a vector of contract instance entries given a set of contract addresses.
    fn read_contract_instance_by_contract_ids(&self, contracts: impl IntoIterator<Item = ScAddress>) -> Vec<ContractDataEntry>;

    /// Returns a contract instance entry given a contract address.
    fn read_contract_instance_by_contract_id(&self, contract: ScAddress) -> Option<ContractDataEntry>;
    
    /// Returns a contract data entry given a contract address and a ledger key.
    fn read_contract_data_entry_by_contract_id_and_key(&self, contract: ScAddress, key: ScVal) -> Option<ContractDataEntry>;
}

#[derive(Clone)]
pub struct LedgerImpl<L: LedgerStateRead> {
    /// Implementor's ledger.
    pub ledger: Box<L>,
}

/// Wrapper of the database implementation.
#[derive(Clone)]
pub struct Ledger<L: LedgerStateRead>(pub(crate) LedgerImpl<L>);

impl<L: LedgerStateRead + ZephyrStandard> ZephyrStandard for LedgerImpl<L> {
    fn zephyr_standard() -> Result<Self>
        where
            Self: Sized {
        Ok(Self {
            ledger: Box::new(L::zephyr_standard()?)
        })
    }
}

impl<L: LedgerStateRead + ZephyrStandard> ZephyrStandard for Ledger<L> {
    fn zephyr_standard() -> Result<Self>
        where
            Self: Sized {
        Ok(Self(LedgerImpl::zephyr_standard()?))
    }
}

impl<L: LedgerStateRead + ZephyrMock> ZephyrMock for LedgerImpl<L> {
    fn mocked() -> Result<Self>
        where
            Self: Sized {
        Ok(Self {
            ledger: Box::new(L::mocked()?)
        })
    }
}

impl<L: LedgerStateRead + ZephyrMock> ZephyrMock for Ledger<L> {
    fn mocked() -> Result<Self>
        where
            Self: Sized {
        Ok(Self(LedgerImpl::mocked()?))
    }
}
