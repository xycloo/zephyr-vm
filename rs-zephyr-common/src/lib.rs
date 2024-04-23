//! Common structures between the environment and the SDK.
//! This crate omits the structures that are shared between
//! Zephyr and Mercury due to the latter's closed-source nature.

pub mod wrapping;
pub mod http;
pub mod log;

pub fn to_fixed<T, const N: usize>(v: Vec<T>) -> [T; N] {
    v.try_into()
        .unwrap_or_else(|v: Vec<T>| panic!("Expected a Vec of length {} but it was {}", N, v.len()))
}

#[repr(u32)]
pub enum ZephyrStatus {
    Unknown = 0,
    Success = 1,
    DbWriteError = 2,
    DbReadError = 3,
    NoValOnStack = 4,
    HostConfiguration = 5
}

use http::AgnosticRequest;
use log::ZephyrLog;
use serde::{Deserialize, Serialize};
use stellar_xdr::next::{LedgerEntry, ScAddress, ScVal};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("Invalid permissions. Tried reading when in write-only")]
    ReadOnWriteOnly,

    #[error("Invalid permissions. Tried writing when in read-only")]
    WriteOnReadOnly,

    #[error("Zephyr query malformed.")]
    ZephyrQueryMalformed,

    #[error("Zephyr query error.")]
    ZephyrQueryError,

    #[error("Unable to write to DB.")]
    WriteError,

    #[error("Unable to parse operator.")]
    OperatorError,
}


impl From<anyhow::Error> for ZephyrStatus {
    fn from(value: anyhow::Error) -> Self {
        match value.downcast_ref() {
            Some(DatabaseError::WriteError) => ZephyrStatus::DbWriteError,
            Some(DatabaseError::ZephyrQueryError) => ZephyrStatus::DbReadError,
            Some(DatabaseError::ZephyrQueryMalformed) => ZephyrStatus::DbReadError,
            Some(DatabaseError::ReadOnWriteOnly) => ZephyrStatus::HostConfiguration,
            Some(DatabaseError::WriteOnReadOnly) => ZephyrStatus::HostConfiguration,
            Some(DatabaseError::OperatorError) => ZephyrStatus::DbWriteError, // todo: specific error
            None => ZephyrStatus::Unknown
        } 
    }
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

#[derive(Deserialize, Serialize, Clone)]
pub enum ZephyrVal {
    I128(i128),
    I64(i64),
    U64(u64),
    F64(f64),
    U32(u32),
    I32(i32),
    F32(f32),
    String(String),
    Bytes(Vec<u8>)
}

#[derive(Debug)]
pub enum ZephyrValError {
    ConversionError
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ContractDataEntry {
    pub contract_id: ScAddress,
    pub key: ScVal,
    pub entry: LedgerEntry,
    pub durability: i32,
    pub last_modified: i32
}

macro_rules! impl_inner_from {
    ($variant:ident, $inner:ty) => {
        impl From<$inner> for ZephyrVal {
            fn from(value: $inner) -> Self {
                ZephyrVal::$variant(value)
            }
        }

        impl From<ZephyrVal> for $inner {
            fn from(value: ZephyrVal) -> Self {
                match value {
                    ZephyrVal::$variant(inner_val) => inner_val,
                    _ => panic!("Attempted to convert ZephyrVal variant to different inner type"),
                }
            }
        }
    };
}

impl_inner_from!(I128, i128);
impl_inner_from!(I64, i64);
impl_inner_from!(U64, u64);
impl_inner_from!(F64, f64);
impl_inner_from!(U32, u32);
impl_inner_from!(I32, i32);
impl_inner_from!(F32, f32);
impl_inner_from!(String, String);
impl_inner_from!(Bytes, Vec<u8>);


#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum RelayedMessageRequest {
    Http(AgnosticRequest),
    Log(ZephyrLog)
}
