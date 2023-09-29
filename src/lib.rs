mod vm;
mod host;
mod budget;
mod db;
mod error;
mod memory;
mod symbol;

use anyhow::Result;

#[cfg(feature="native")]
mod native;

pub trait ZephyrStandard {
    fn zephyr_standard() -> Result<Self> where Self: Sized;
}

// TODO: make mocks testutils only.
pub trait ZephyrMock {
    fn mocked() -> Result<Self> where Self:Sized;
}
