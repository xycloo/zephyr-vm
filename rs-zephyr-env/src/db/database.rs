//! This module defines how the database implementation in
//! Zephyr should be implemented.
//!
//! Implementors that wish to use the Zephyr VM in their code
//! must provide the Zephyr host environment with a valid implementation
//! of the Database.

use crate::{ZephyrMock, ZephyrStandard};
use anyhow::Result;
use rs_zephyr_common::DatabaseError;

/// Allowed column conditions
pub enum WhereCond {
    /// Where column i64 is equal to the corresponding condition
    /// argument.
    ColEq(i64),

    /// Where column i64 is greater than the corresponding condition
    /// argument.
    ColGt(i64),

    /// Where column i64 is less than the corresponding condition
    /// argument.
    ColLt(i64),
}

impl WhereCond {
    pub(crate) fn from_column_and_operator(col: i64, operator: i64) -> Result<Self> {
        match operator {
            0 => Ok(Self::ColEq(col)),
            1 => Ok(Self::ColGt(col)),
            2 => Ok(Self::ColLt(col)),
            _ => Err(DatabaseError::OperatorError.into()),
        }
    }
}

/// Zephyr-compatible database trait.
/// Implementations of Zephyr that allow from a database from within
/// a Zephyr execution must implement this trait.
pub trait ZephyrDatabase {
    /// Reads the database from raw data.
    /// - user id is the identifier of the host, which might be
    /// needed for database access control depending on how the
    /// implementor initializes the host.
    /// - read point hash is the identifier of the slot Zephyr
    /// is trying to read from the database.
    /// - read data is a slice of integers that define the read
    /// instruction that Zephyr is providing to the database implementation
    fn read_raw(
        &self,
        user_id: i64,
        read_point_hash: [u8; 16],
        read_data: &[i64],
        condition: Option<&[WhereCond]>,
        condition_args: Option<Vec<Vec<u8>>>,
    ) -> Result<Vec<u8>, DatabaseError>;

    /// Writes the database from raw data.
    /// - user id is the identifier of the host, which might be
    /// needed for database access control depending on how the
    /// implementor initializes the host.
    /// - written point hash is the identifier of the slot in
    /// the database that Zephyr is writing to.
    /// - write data is a slice of integers with instructions
    /// about the write operation.
    /// - written is a multidimensional vector with bytes being
    /// written as a single value in the database.
    fn write_raw(
        &self,
        user_id: i64,
        written_point_hash: [u8; 16],
        write_data: &[i64],
        written: Vec<Vec<u8>>,
    ) -> Result<(), DatabaseError>;

    /// Updates database rows from raw data.
    /// - user id is the identifier of the host, which might be
    /// needed for database access control depending on how the
    /// implementor initializes the host.
    /// - written point hash is the identifier of the slot in
    /// the database that Zephyr is writing to.
    /// - write data is a slice of integers with instructions
    /// about the write operation.
    /// - written is a multidimensional vector with bytes being
    /// written as a single value in the database.
    fn update_raw(
        &self,
        user_id: i64,
        written_point_hash: [u8; 16],
        write_data: &[i64],
        written: Vec<Vec<u8>>,
        condition: &[WhereCond],
        condition_args: Vec<Vec<u8>>,
    ) -> Result<(), DatabaseError>;
}

/// Specify the database permissions that the implementor
/// is granting to Zephyr.
#[derive(Clone)]
pub enum DatabasePermissions {
    /// Zephyr can only read the database.
    ReadOnly,

    /// Zephyr can only write the database.
    WriteOnly,

    /// Zephyr can both read and write the database.
    ReadWrite,
}

/// Database implementation.
/// Wraps the implementor-supplied DB implementation that
/// Zephyr will use to communicate with the database.
#[derive(Clone)]
pub struct DatabaseImpl<DB: ZephyrDatabase> {
    /// Permissions granted.
    pub permissions: DatabasePermissions,

    /// Implementor's database.
    pub db: Box<DB>,
}

/// Wrapper of the database implementation.
#[derive(Clone)]
pub struct Database<DB: ZephyrDatabase>(pub(crate) DatabaseImpl<DB>);

impl<DB: ZephyrDatabase + ZephyrStandard> ZephyrStandard for DatabaseImpl<DB> {
    fn zephyr_standard() -> Result<Self> {
        Ok(Self {
            permissions: DatabasePermissions::ReadWrite,
            db: Box::new(DB::zephyr_standard()?),
        })
    }
}

impl<DB: ZephyrDatabase + ZephyrStandard> ZephyrStandard for Database<DB> {
    fn zephyr_standard() -> Result<Self> {
        Ok(Self(DatabaseImpl::zephyr_standard()?))
    }
}

impl<DB: ZephyrDatabase + ZephyrMock> ZephyrMock for DatabaseImpl<DB> {
    fn mocked() -> Result<Self> {
        Ok(Self {
            permissions: DatabasePermissions::ReadWrite,
            db: Box::new(DB::mocked()?),
        })
    }
}

impl<DB: ZephyrDatabase + ZephyrMock> ZephyrMock for Database<DB> {
    fn mocked() -> Result<Self> {
        Ok(Self(DatabaseImpl::mocked()?))
    }
}
