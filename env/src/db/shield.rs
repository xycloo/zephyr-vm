//! Implementation for the so called shield database.
//! This database is stored in memory for every VM
//! execution and serves the need of limiting writes
//! to the database that may be overwritten within the
//! same execution.
//!
//! It also serves for local testing, as the mocked VM
//! does not communicate with any database.

use std::{cell::RefCell, rc::Rc};

/// Shield store implementation.
#[derive(Clone)]
pub struct ShieldedStoreImpl {}

impl Default for ShieldedStoreImpl {
    fn default() -> Self {
        Self {}
    }
}

/// Shield store implementation wrapper.
#[derive(Clone, Default)]
pub struct ShieldedStore(pub(crate) Rc<RefCell<ShieldedStoreImpl>>);
