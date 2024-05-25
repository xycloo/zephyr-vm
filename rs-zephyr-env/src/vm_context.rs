//! Defines the context for the host environment.
//! VM context is used when dealing with reading and
//! writing the guest memory.

use anyhow::Result;
use std::rc::{Rc, Weak};

use crate::{
    db::{database::ZephyrDatabase, ledger::LedgerStateRead},
    error::HostError,
    vm::Vm,
    ZephyrMock, ZephyrStandard,
};

/// VM Context.
/// The object is currently simply a wrapper for an
/// optional reference to the Virtual Machine.
#[derive(Clone)]
pub struct VmContext<DB: ZephyrDatabase, L: LedgerStateRead> {
    /// Optional Zephyr Virtual Machine.
    pub vm: Option<Weak<Vm<DB, L>>>,
}

impl<DB: ZephyrDatabase + ZephyrStandard, L: LedgerStateRead + ZephyrStandard> ZephyrStandard
    for VmContext<DB, L>
{
    fn zephyr_standard() -> Result<Self>
    where
        Self: Sized,
    {
        Ok(Self { vm: None })
    }
}

impl<DB: ZephyrDatabase + ZephyrMock, L: LedgerStateRead + ZephyrMock> ZephyrMock
    for VmContext<DB, L>
{
    fn mocked() -> Result<Self>
    where
        Self: Sized,
    {
        Ok(Self { vm: None })
    }
}

impl<DB: ZephyrDatabase, L: LedgerStateRead> VmContext<DB, L> {
    /// Writes the provided VM as the context's Virtual Machine.
    /// Errors when a VM is already present in the context.
    pub fn load_vm(&mut self, vm: Weak<Vm<DB, L>>) -> Result<()> {
        if self.vm.is_some() {
            return Err(HostError::ContextAlreadyExists.into());
        }

        self.vm = Some(vm);

        Ok(())
    }
}
