use std::{cell::{RefCell, Ref}, rc::Rc, borrow::BorrowMut};
use anyhow::Result;
use crate::{ZephyrStandard, vm::Vm, db::database::ZephyrDatabase, ZephyrMock, error::HostError};


#[derive(Copy, Clone)]
pub enum Op {
    OnClose
}

#[derive(Clone)]
pub struct VmContext<DB: ZephyrDatabase> {
    pub vm: Option<Rc<Vm<DB>>>,
    
}

impl<DB: ZephyrDatabase + ZephyrStandard> ZephyrStandard for VmContext<DB> {
    fn zephyr_standard() -> Result<Self> where Self: Sized {
        Ok(Self {
            vm: None
        })
    }
}

impl<DB: ZephyrDatabase + ZephyrMock> ZephyrMock for VmContext<DB> {
    fn mocked() -> Result<Self> where Self: Sized {
        Ok(Self {
            vm: None
        })
    }
}

impl<DB: ZephyrDatabase> VmContext<DB> {
    pub fn load_vm(&mut self, vm: Rc<Vm<DB>>) -> Result<()> {
        if self.vm.is_some() {
            return Err(HostError::ContextAlreadyExists.into());
        }

        self.vm = Some(vm);
        
        Ok(())
    }
}
