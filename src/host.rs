use std::{rc::Rc, cell::{RefCell, RefMut, Ref}, borrow::BorrowMut};
use wasmtime::{Val, Store, Func, Caller};
use anyhow::Result;
use sha2::{Digest, Sha256};

use crate::{budget::Budget, db::{shield::ShieldedStore, database::{Database, ZephyrDatabase, DatabasePermissions}, error::DatabaseError, self}, ZephyrStandard, ZephyrMock, stack::Stack, error::HostError};

mod byte_utils {
    pub fn i64_to_bytes(value: i64) -> [u8; 8] {
        let byte0 = ((value >> 0) & 0xFF) as u8;
        let byte1 = ((value >> 8) & 0xFF) as u8;
        let byte2 = ((value >> 16) & 0xFF) as u8;
        let byte3 = ((value >> 24) & 0xFF) as u8;
        let byte4 = ((value >> 32) & 0xFF) as u8;
        let byte5 = ((value >> 40) & 0xFF) as u8;
        let byte6 = ((value >> 48) & 0xFF) as u8;
        let byte7 = ((value >> 56) & 0xFF) as u8;

        [byte0, byte1, byte2, byte3, byte4, byte5, byte6, byte7]
    }
}

#[derive(Clone)]
pub struct EntryPointInfo {
    pub fname: String,
    pub params: Vec<Val>,
    pub retrn: Vec<Val>
}

impl Default for EntryPointInfo {
    fn default() -> Self {
        Self { fname: "on_close".to_string(), params: [].into(), retrn: [].into() }
    }
}

/// Zephyr Host State Implementation.
#[derive(Clone)]
pub struct HostImpl<DB: ZephyrDatabase> {
    pub id: i64,
    pub shielded_store: RefCell<ShieldedStore>,
    pub database: RefCell<Database<DB>>,
    pub budget: RefCell<Budget>,
    pub entry_point_info: RefCell<EntryPointInfo>,
    pub stack: RefCell<Stack>
}

/// Zephyr Host State.
#[derive(Clone)]
pub struct Host<DB: ZephyrDatabase>(Rc<HostImpl<DB>>); // We wrap [`HostImpl`] here inside an rc pointer for multi ownership.

impl<DB: ZephyrDatabase + ZephyrStandard> Host<DB> {
    fn host_from_id(id: i64) -> Result<Self> {
        Ok(Self(Rc::new(
            HostImpl {
                id,
                shielded_store: RefCell::new(ShieldedStore::default()), 
                database: RefCell::new(Database::zephyr_standard()?),
                budget: RefCell::new(Budget::zephyr_standard()?),
                entry_point_info: RefCell::new(EntryPointInfo::default()),
                stack: RefCell::new(Stack::zephyr_standard()?)
            })
        ))
    }
}

impl<DB: ZephyrDatabase + ZephyrMock> ZephyrMock for Host<DB> {
    fn mocked() -> Result<Self> {
        Ok(Self(Rc::new(
            HostImpl {
                id: 0,
                shielded_store: RefCell::new(ShieldedStore::default()), 
                database: RefCell::new(Database::mocked()?),
                budget: RefCell::new(Budget::zephyr_standard()?),
                entry_point_info: RefCell::new(EntryPointInfo::default()),
                stack: RefCell::new(Stack::zephyr_standard()?)
            })
        ))
    }
}

#[derive(Clone)]
pub struct FunctionInfo {
    pub module: &'static str,
    pub func: &'static str,
    pub wrapped: Func
}

impl<DB: ZephyrDatabase> Host<DB> {
    pub fn get_host_id(&self) -> i64 {
        self.0.id
    }
    
    pub fn get_entry_point_info(&self) -> Ref<EntryPointInfo> {
        self.0.entry_point_info.borrow()
    }

    pub fn as_budget(&self) -> Ref<Budget> {
        self.0.budget.borrow()
    }

    pub fn as_stack_mut(&self) -> RefMut<Stack> {
        self.0.stack.borrow_mut()
    }

    pub fn as_stack(&self) -> Ref<Stack> {
        self.0.stack.borrow()
    }

    fn read_database_raw(&self) -> Result<()> {
        let db_obj = self.0.database.borrow();
        let db_impl = db_obj.0.borrow();
        
        if let DatabasePermissions::WriteOnly = db_impl.permissions {
            return Err(DatabaseError::ReadOnWriteOnly.into());
        }

        let stack = self.0.stack.borrow();
        let stack = stack.0.load_host().borrow();

        let id = {
            let value = self.get_host_id();
            byte_utils::i64_to_bytes(value)
        };

        let read_point_hash: [u8; 32] = {
            let read_point_raw = stack.get(0).ok_or(HostError::NoValOnStack)?;
            let read_point_bytes = byte_utils::i64_to_bytes(*read_point_raw);

            let mut hasher = Sha256::new();
            hasher.update(id);
            hasher.update(read_point_bytes);
            hasher.finalize().into()
        };

        {
            let mut columns: Vec<> = Vec::new();
        }

        Ok(())
    }

    fn stack_load(&self) -> Result<Vec<i64>> {
        let stack = self.as_stack();
        
        Ok(stack.0.load())
    }

    fn stack_push(&self, val: i64) -> Result<()> {
        let mut stack = self.as_stack_mut();
        let stack_impl = stack.0.borrow_mut();
        stack_impl.push(val);

        Ok(())
    }

    fn stack_clear(&self) -> Result<()> {
        let mut stack = self.as_stack_mut();
        let stack_impl = stack.0.borrow_mut();
        stack_impl.clear();

        Ok(())
    }

    pub fn host_functions(&self, store: &mut Store<Host<DB>>) -> [FunctionInfo; 2] {
        let mut store = store;
        
        let db_read_fn = {
            let db_read_fn_wrapped = Func::wrap(&mut store, |caller: Caller<_>| {
                let host: &Host<DB> = caller.data();

                host.read_database_raw()
            });

            FunctionInfo {
                module: "db",
                func: "read_raw",
                wrapped: db_read_fn_wrapped
            }
        };

        let stack_push_fn = {
            let stack_push_fn_wrapped = Func::wrap(&mut store, |caller: Caller<'_, Host<DB>>, val: i64| {
                let host: &Host<DB> = caller.data();

                host.stack_push(val)
            });

            FunctionInfo {
                module: "stack",
                func: "push",
                wrapped: stack_push_fn_wrapped
            }
        };

        [db_read_fn, stack_push_fn]
    }
}
