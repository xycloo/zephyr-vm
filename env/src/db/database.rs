use std::{rc::Rc, cell::RefCell};
use anyhow::Result;

use crate::{ZephyrStandard, ZephyrMock};
use super::error::DatabaseError;


/// Zephyr-compatible database trait.
/// Implementations of Zephyr that allow from a database from within
/// a Zephyr execution must implement this trait.
pub trait ZephyrDatabase {
    fn read_raw(&self, user_id: i64, read_point_hash: [u8; 32], read_data: &[u8]) -> Result<(), DatabaseError>;
    
    fn write_raw(&self, user_id: i64, written_point_hash: [u8; 32], written: &[u8]) -> Result<(), DatabaseError>;
}

#[derive(Clone)]
pub enum DatabasePermissions {
    ReadOnly,
    WriteOnly,
    ReadWrite
}

#[derive(Clone)]
pub struct DatabaseImpl<DB: ZephyrDatabase> {
    pub permissions: DatabasePermissions,
    pub db: Box<DB>
}

#[derive(Clone)]
pub struct Database<DB: ZephyrDatabase>(pub(crate) Rc<RefCell<DatabaseImpl<DB>>>);


impl<DB: ZephyrDatabase + ZephyrStandard> ZephyrStandard for DatabaseImpl<DB> {
    fn zephyr_standard() -> Result<Self> {
        Ok(Self { 
            permissions: DatabasePermissions::ReadWrite,
            db: Box::new(DB::zephyr_standard()?)
        })
    }
}

impl<DB: ZephyrDatabase + ZephyrStandard> ZephyrStandard for Database<DB> {
    fn zephyr_standard() -> Result<Self> {
        Ok(Self(Rc::new(RefCell::new(DatabaseImpl::zephyr_standard()?))))
    }
}

impl<DB: ZephyrDatabase + ZephyrMock> ZephyrMock for DatabaseImpl<DB> {
    fn mocked() -> Result<Self> {
        Ok(Self { 
            permissions: DatabasePermissions::ReadWrite,
            db: Box::new(DB::mocked()?)
        })
    }
}

impl<DB: ZephyrDatabase + ZephyrMock> ZephyrMock for Database<DB> {
    fn mocked() -> Result<Self> {
        Ok(Self(Rc::new(RefCell::new(DatabaseImpl::mocked()?))))
    }
}

