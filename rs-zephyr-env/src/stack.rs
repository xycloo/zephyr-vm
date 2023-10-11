//! This module defines the pseudo stack implementation
//! that is used in Zephyr for the guest environment
//! to provide instructions to host environment.

use crate::ZephyrStandard;
use anyhow::Result;
use std::{borrow::BorrowMut, cell::RefCell, rc::Rc};

/// Stack implementation.
#[derive(Clone)]
pub struct StackImpl(pub RefCell<Vec<i64>>);

/// Stack implementation wrapper.
#[derive(Clone)]
pub struct Stack(pub Rc<StackImpl>);

impl ZephyrStandard for StackImpl {
    fn zephyr_standard() -> Result<Self> {
        Ok(Self(RefCell::new(Vec::new())))
    }
}

impl StackImpl {
    /// Pushes a value to the stack.
    pub fn push(&self, val: i64) {
        let mut stack = self.0.borrow_mut();
        stack.borrow_mut().push(val);
    }

    /// Clear the stack.
    pub fn clear(&self) {
        let mut stack = self.0.borrow_mut();
        stack.borrow_mut().clear();
    }

    /// Load a mutable reference to the stack.
    pub fn load_host(&self) -> &RefCell<Vec<i64>> {
        &self.0
    }

    /// Load the cloned stack.
    pub fn load(&self) -> Vec<i64> {
        self.0.borrow().clone()
    }
}

impl ZephyrStandard for Stack {
    fn zephyr_standard() -> Result<Self> {
        Ok(Self(Rc::new(StackImpl::zephyr_standard()?)))
    }
}
