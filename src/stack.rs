use std::{cell::{RefCell, Ref}, rc::Rc, borrow::BorrowMut};
use anyhow::Result;
use crate::ZephyrStandard;


#[derive(Clone)]
pub struct StackImpl(pub RefCell<Vec<i64>>);

#[derive(Clone)]
pub struct Stack(pub Rc<StackImpl>);

impl ZephyrStandard for StackImpl {
    fn zephyr_standard() -> Result<Self> {
        Ok(Self(RefCell::new(Vec::new())))
    }
}

impl StackImpl {
    pub fn push(&self, val: i64) {
        let mut stack = self.0.borrow_mut();
        stack.borrow_mut().push(val);
    }

    pub fn clear(&self) {
        let mut stack = self.0.borrow_mut();
        stack.borrow_mut().clear();
    }

    pub fn load_host(&self) -> &RefCell<Vec<i64>> {
        &self.0
    }

    pub fn load(&self) -> Vec<i64> {
        self.0.borrow().clone()
    }
}

impl ZephyrStandard for Stack {
    fn zephyr_standard() -> Result<Self>{
        Ok(Self(Rc::new(StackImpl::zephyr_standard()?)))
    }
}
