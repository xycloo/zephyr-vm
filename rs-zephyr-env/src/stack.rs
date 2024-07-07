//! This module defines the pseudo stack implementation
//! that is used in Zephyr for the guest environment
//! to provide instructions to host environment.

use crate::{error::HostError, ZephyrStandard};
use anyhow::Result;
use std::{borrow::BorrowMut, cell::RefCell, rc::Rc};

/// Stack implementation.
#[derive(Clone)]
pub struct StackImpl {
    /// Inner stack vector.
    pub inner: RefCell<Vec<i64>>,
    step: RefCell<usize>,
}

/// Stack implementation wrapper.
#[derive(Clone)]
pub struct Stack(pub Rc<StackImpl>);

impl ZephyrStandard for StackImpl {
    fn zephyr_standard() -> Result<Self> {
        Ok(Self {
            inner: RefCell::new(Vec::new()),
            step: RefCell::new(0),
        })
    }
}

impl StackImpl {
    /// Pushes a value to the stack.
    pub fn push(&self, val: i64) {
        let mut stack = self.inner.borrow_mut();
        stack.borrow_mut().push(val);
    }

    /// Clear the stack.
    pub fn clear(&self) {
        let mut stack = self.inner.borrow_mut();
        *self.step.borrow_mut() = 0;
        stack.borrow_mut().clear();
    }

    /// Load a mutable reference to the stack.
    pub fn load_host(&self) -> &RefCell<Vec<i64>> {
        &self.inner
    }

    /// Load the cloned stack.
    pub fn load(&self) -> Vec<i64> {
        self.inner.borrow().clone()
    }

    /// Reads the current value on stack and increments
    /// the count.
    pub fn get_with_step(&self) -> Result<i64, HostError> {
        let current = self.step.clone().into_inner();
        *self.step.borrow_mut() = current + 1;

        self.inner
            .borrow()
            .get(current)
            .copied()
            .ok_or(HostError::NoValOnStack)
    }

    /// Returns the current count.
    pub fn get_current_step(&self) -> usize {
        self.step.clone().into_inner()
    }
}

impl ZephyrStandard for Stack {
    fn zephyr_standard() -> Result<Self> {
        Ok(Self(Rc::new(StackImpl::zephyr_standard()?)))
    }
}
