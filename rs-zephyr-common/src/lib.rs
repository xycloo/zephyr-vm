//! Common structures between the environment and the SDK.
//! This crate omits the structures that are shared between
//! Zephyr and Mercury due to the latter's closed-source nature.

#[repr(u32)]
pub enum ZephyrStatus {
    Unknown = 0,
    Success = 1,
    DbWriteError = 2,
    DbReadError = 3,
    NoValOnStack = 4,
    HostConfiguration = 5
}

impl From<u32> for ZephyrStatus {
    fn from(value: u32) -> Self {
        match value {
            0 => Self::Unknown,
            1 => Self::Success,
            2 => Self::DbWriteError,
            3 => Self::DbReadError,
            4 => Self::NoValOnStack,
            5 => Self::HostConfiguration,
            _ => panic!("Unrecoverable status"),
        }
    }
}
