#![warn(missing_docs)]

//! ## The Zephyr VM host environment.
//!
//! Implementation of the Zephyr VM, the core of Mercury's code execution environment.
//! Even if Zephyr is built to be used in Mercury, it is implementation-agnostic
//! and can be integrated in any kind of implementation.

pub mod budget;
pub mod db;
pub mod host;
pub mod vm;

mod soroban_host_gen;

#[allow(missing_docs)]
pub mod error;

pub mod stack;
pub mod vm_context;

use anyhow::Result;

/// This only for testing
#[cfg(feature = "testutils")]
pub mod testutils;

/// Standard object for Zephyr. This trait must be implemented for all
/// components that are encompassed by the Zephyr VM, specifically
/// the database implementation.
pub trait ZephyrStandard {
    /// Returns the standard zephyr object.
    fn zephyr_standard() -> Result<Self>
    where
        Self: Sized;
}

// TODO: make mocks testutils only.
/// Standard mocked Zephyr object. This trait must be implemented for all
/// components that are encompassed by the Zephyr VM that required mocks
/// for testing.
pub trait ZephyrMock {
    /// Returns the mocked object.
    fn mocked() -> Result<Self>
    where
        Self: Sized;
}
