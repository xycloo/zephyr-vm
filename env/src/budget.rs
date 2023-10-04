use std::{cell::RefCell, rc::Rc};
use anyhow::Result;
use wasmtime::Store;

use crate::{host::Host, db::database::ZephyrDatabase, ZephyrStandard};

const STANDARD_FUEL: u64 = 1_000_000_000;

#[derive(Clone)]
pub struct DimensionLimits {
    fuel: u64
}

impl ZephyrStandard for DimensionLimits {
    fn zephyr_standard() -> Result<Self> {
        Ok(Self { 
            fuel: STANDARD_FUEL
        })
    }
}

#[derive(Clone)]
pub struct BudgetImpl {
    limits: DimensionLimits
}

#[derive(Clone)]
pub struct Budget(pub(crate) Rc<RefCell<BudgetImpl>>); // Again, wrapping for ownership and mutability.


impl ZephyrStandard for BudgetImpl {
    fn zephyr_standard() -> Result<Self> {
        Ok(Self {
            limits: DimensionLimits::zephyr_standard()?
        })
    }
}

impl ZephyrStandard for Budget {
    fn zephyr_standard() -> Result<Self> where Self: Sized {
        Ok(Self(Rc::new(RefCell::new(BudgetImpl::zephyr_standard()?))))
    }
}

impl Budget {
    pub fn infer_fuel<DB: ZephyrDatabase>(&self, store: &mut Store<Host<DB>>) -> Result<()> {
        store.add_fuel(self.0.borrow().limits.fuel)
    }
}

