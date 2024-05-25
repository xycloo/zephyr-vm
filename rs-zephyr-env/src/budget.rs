//! This module defines the budget-related structures and
//! implementations for the Zephyr host.
//!
//! Metering is defined within this module.

use anyhow::Result;
use std::{cell::RefCell, rc::Rc};
use wasmi::{errors::FuelError, Store};

use crate::{
    db::{database::ZephyrDatabase, ledger::LedgerStateRead},
    host::Host,
    ZephyrStandard,
};

const STANDARD_FUEL: u64 = 1_000_000_000;
const STANDARD_WRITE_MAX: usize = 64_000;

/// Limits in the budget allocated to every Zephyr VM
/// execution.
#[derive(Clone)]
pub struct DimensionLimits {
    fuel: u64,

    #[allow(dead_code)]
    write_max: usize,
}

impl ZephyrStandard for DimensionLimits {
    fn zephyr_standard() -> Result<Self> {
        Ok(Self {
            fuel: STANDARD_FUEL,
            write_max: STANDARD_WRITE_MAX,
        })
    }
}

/// Budget implementation.
#[derive(Clone)]
pub struct BudgetImpl {
    limits: DimensionLimits,
}

/// Budget implementation wrapper.
#[derive(Clone)]
pub struct Budget(pub(crate) Rc<RefCell<BudgetImpl>>); // Again, wrapping for ownership and mutability.

impl ZephyrStandard for BudgetImpl {
    fn zephyr_standard() -> Result<Self> {
        Ok(Self {
            limits: DimensionLimits::zephyr_standard()?,
        })
    }
}

impl ZephyrStandard for Budget {
    fn zephyr_standard() -> Result<Self>
    where
        Self: Sized,
    {
        Ok(Self(Rc::new(RefCell::new(BudgetImpl::zephyr_standard()?))))
    }
}

impl Budget {
    /// Allocates the maximum fuel to the provided store object.
    pub fn infer_fuel<DB: ZephyrDatabase, L: LedgerStateRead>(
        &self,
        store: &mut Store<Host<DB, L>>,
    ) -> Result<(), FuelError> {
        store.add_fuel(self.0.borrow().limits.fuel)
    }
}
