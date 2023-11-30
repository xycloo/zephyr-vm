//! Structures and implementations for the Zephyr
//! host environment. This module defines all the interactions
//! between the binary code executed within the VM and
//! the implementor.

use anyhow::Result;
//use sha2::{Digest, Sha256};
use std::{
    borrow::BorrowMut,
    cell::{Ref, RefCell, RefMut},
    rc::Rc,
};
use wasmtime::{Caller, Func, Store, Val};

use crate::{
    budget::Budget,
    db::{
        database::{Database, DatabasePermissions, ZephyrDatabase},
        error::DatabaseError,
        shield::ShieldedStore,
    },
    error::HostError,
    stack::Stack,
    vm::Vm,
    vm_context::VmContext,
    ZephyrMock, ZephyrStandard,
};

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

/// Information about the entry point function. This
/// function is exported by the binary with the given
/// argument types.
#[derive(Clone)]
pub struct EntryPointInfo {
    /// Name of the function.
    pub fname: String,

    /// Function parameter types.
    pub params: Vec<Val>,

    /// Function return types.
    pub retrn: Vec<Val>,
}

/// By default, Zephyr infers a standard entry point:
/// the `on_close() -> ()` function.
impl ZephyrStandard for EntryPointInfo {
    fn zephyr_standard() -> Result<Self>
    where
        Self: Sized,
    {
        Ok(Self {
            fname: "on_close".to_string(),
            params: [].into(),
            retrn: [].into(),
        })
    }
}

/// Zephyr Host State Implementation.
#[derive(Clone)]
pub struct HostImpl<DB: ZephyrDatabase> {
    /// Host id.
    pub id: i64,

    /// Latest ledger close meta. This is set as optional as
    /// some Zephyr programs might not need the ledger meta.
    pub latest_close: RefCell<Option<Vec<u8>>>, // some zephyr programs might not need the ledger close meta

    /// Implementation of the Shielded Store.
    pub shielded_store: RefCell<ShieldedStore>,

    /// Database implementation.
    pub database: RefCell<Database<DB>>,

    /// Budget implementation.
    pub budget: RefCell<Budget>,

    /// Entry point info.
    pub entry_point_info: RefCell<EntryPointInfo>,

    /// VM context.
    pub context: RefCell<VmContext<DB>>,

    /// Host pseudo stack implementation.
    pub stack: RefCell<Stack>,
}

/// Zephyr Host State.
#[derive(Clone)]
pub struct Host<DB: ZephyrDatabase>(Rc<HostImpl<DB>>); // We wrap [`HostImpl`] here inside an rc pointer for multi ownership.

#[allow(dead_code)]
impl<DB: ZephyrDatabase + ZephyrStandard> Host<DB> {
    /// Creates a standard Host object starting from a given
    /// host ID. The host ID is the only relation between the VM
    /// and the entity it is bound to. For instance, in Mercury
    /// the host id is the id of a Mercury user. This is needed to
    /// implement role constraints in Zephyr.
    pub fn from_id(id: i64) -> Result<Self> {
        Ok(Self(Rc::new(HostImpl {
            id,
            latest_close: RefCell::new(None),
            shielded_store: RefCell::new(ShieldedStore::default()),
            database: RefCell::new(Database::zephyr_standard()?),
            budget: RefCell::new(Budget::zephyr_standard()?),
            entry_point_info: RefCell::new(EntryPointInfo::zephyr_standard()?),
            context: RefCell::new(VmContext::zephyr_standard()?),
            stack: RefCell::new(Stack::zephyr_standard()?),
        })))
    }
}

impl<DB: ZephyrDatabase + ZephyrMock> ZephyrMock for Host<DB> {
    /// Creates a Host object designed to be used in tests with potentially
    /// mocked data such as host id, databases and context.
    fn mocked() -> Result<Self> {
        Ok(Self(Rc::new(HostImpl {
            id: 0,
            latest_close: RefCell::new(None),
            shielded_store: RefCell::new(ShieldedStore::default()),
            database: RefCell::new(Database::mocked()?),
            budget: RefCell::new(Budget::zephyr_standard()?),
            entry_point_info: RefCell::new(EntryPointInfo::zephyr_standard()?),
            context: RefCell::new(VmContext::mocked()?),
            stack: RefCell::new(Stack::zephyr_standard()?),
        })))
    }
}

/// Wrapper function information.
/// This object is sent to the VM object when the Virtual Machine
/// is created to tell the linker which host functions to define.
#[derive(Clone)]
pub struct FunctionInfo {
    /// Module name.
    pub module: &'static str,

    /// Function name.
    pub func: &'static str,

    /// Func object. Contains the function's implementation.
    pub wrapped: Func,
}

#[allow(dead_code)]
impl<DB: ZephyrDatabase + Clone> Host<DB> {
    /// Loads the ledger close meta bytes of the ledger the Zephyr VM will have
    /// access to.
    ///
    /// The ledger close meta is stored as a slice and currenty no type checks occur.
    /// The functions returns a [`HostError::LedgerCloseMetaOverridden`] error when a ledger
    /// close meta is already present in the host object. This is because VMs are not re-usable
    /// between ledgers and need to be created and instantiated for each new invocation to
    /// prevent memory issues.
    pub fn add_ledger_close_meta(&mut self, ledger_close_meta: Vec<u8>) -> Result<()> {
        let current = &self.0.latest_close;
        if current.borrow().is_some() {
            return Err(HostError::LedgerCloseMetaOverridden.into());
        }

        *current.borrow_mut() = Some(ledger_close_meta);

        Ok(())
    }

    /// Returns a reference to the host's budget implementation.
    pub fn as_budget(&self) -> Ref<Budget> {
        self.0.budget.borrow()
    }

    /// Returns a reference to the host's stack implementation.
    pub fn as_stack_mut(&self) -> RefMut<Stack> {
        self.0.stack.borrow_mut()
    }

    /// Returns the id assigned to the host.
    pub fn get_host_id(&self) -> i64 {
        self.0.id
    }

    /// Returns a reference to the host's entry point information.
    pub fn get_entry_point_info(&self) -> Ref<EntryPointInfo> {
        self.0.entry_point_info.borrow()
    }

    /// Loads VM context in the host if needed.
    pub fn load_context(&self, vm: Rc<Vm<DB>>) -> Result<()> {
        let mut vm_context = self.0.context.borrow_mut();

        vm_context.load_vm(vm)
    }

    fn get_stack(&self) -> Result<Vec<i64>> {
        let stack = self.as_stack_mut();

        Ok(stack.0.load())
    }

    fn stack_clear(&self) -> Result<()> {
        let mut stack = self.as_stack_mut();
        let stack_impl = stack.0.borrow_mut();
        stack_impl.clear();

        Ok(())
    }

    fn read_ledger_meta(caller: Caller<Self>) -> Result<(i64, i64)> {
        let host = caller.data();
        let ledger_close_meta = {
            let current = host.0.latest_close.borrow();

            if current.is_none() {
                return Err(HostError::NoLedgerCloseMeta.into());
            }

            current.clone().unwrap()
        };

        Self::write_to_memory(caller, ledger_close_meta.as_slice())
    }

    fn write_to_memory(mut caller: Caller<Self>, contents: &[u8]) -> Result<(i64, i64)> {
        let (memory, offset, data) = {
            let host = caller.data();

            let context = host.0.context.borrow();
            let vm = context.vm.as_ref().unwrap(); // todo: make safe

            let manager = &vm.memory_manager;
            let memory = manager.memory;

            let mut offset_mut = manager.offset.borrow_mut();
            let new_offset = offset_mut.checked_add(contents.len()).unwrap();

            *offset_mut = new_offset;

            (memory, new_offset, contents)
        };

        memory.write(&mut caller, offset, data)?;

        Ok((offset as i64, data.len() as i64))
    }

    fn write_database_raw(mut caller: Caller<Self>) -> Result<()> {
        let (memory, write_point_hash, columns, segments) = {
            let host = caller.data();

            let stack_obj = host.0.stack.borrow_mut();
            let stack = stack_obj.0 .0.borrow_mut();

            let id = {
                let value = host.get_host_id();
                byte_utils::i64_to_bytes(value)
            };

            // insert into point (columns...) values (from memory)

            let write_point_hash: [u8; 16] = {
                let point_raw = stack.first().ok_or(HostError::NoValOnStack)?;
                let point_bytes = byte_utils::i64_to_bytes(*point_raw);

                md5::compute([point_bytes, id].concat()).into()
            };

            let columns = {
                let columns_size_idx = stack.get(1).ok_or(HostError::NoValOnStack)? + 2;
                let mut columns: Vec<i64> = Vec::new();

                for idx in 2..columns_size_idx {
                    columns.push(*stack.get(idx as usize).ok_or(HostError::NoValOnStack)?);
                }

                columns
            };

            let data_segments = {
                let mut segments: Vec<(i64, i64)> = Vec::new();
                let mut start = 2 + columns.len();

                let data_segments_size_idx = {
                    let non_fixed = stack.get(start).ok_or(HostError::NoValOnStack)?;

                    (*non_fixed * 2) as usize + start
                };

                println!("data segments size: {}", data_segments_size_idx);

                start += 1;

                for idx in (start..data_segments_size_idx).step_by(2) {
                    let offset = *stack.get(idx as usize).ok_or(HostError::NoValOnStack)?;
                    let size = *stack.get(idx + 1).ok_or(HostError::NoValOnStack)?;
                    segments.push((offset, size))
                }

                segments
            };

            let context = host.0.context.borrow();
            let vm = context.vm.as_ref().unwrap();
            let mem_manager = &vm.memory_manager;

            drop(stack);
            stack_obj.0.clear();

            (mem_manager.memory, write_point_hash, columns, data_segments)
        };

        let aggregated_data = {
            let mut aggregated = Vec::new();

            for segment in segments {
                let data = {
                    let mut written_vec = Vec::new();
                    for _ in 0..segment.1 {
                        written_vec.push(0)
                    }

                    memory.read(&mut caller, segment.0 as usize, &mut written_vec)?;

                    written_vec
                };

                aggregated.push(data);
            }

            aggregated
        };

        let host = caller.data();
        let db_obj = host.0.database.borrow();
        let db_impl = db_obj.0.borrow();

        if let DatabasePermissions::ReadOnly = db_impl.permissions {
            return Err(DatabaseError::WriteOnReadOnly.into());
        }

        db_impl.db.write_raw(
            host.get_host_id(),
            write_point_hash,
            &columns,
            aggregated_data,
        )?;

        Ok(())
    }

    fn read_database_raw(mut caller: Caller<Self>) -> Result<(i64, i64)> {
        let (memory, offset, data) = {
            let host = caller.data();
            let db_obj = host.0.database.borrow();
            let db_impl = db_obj.0.borrow();
            let stack_obj = host.0.stack.borrow_mut();
            let stack = stack_obj.0 .0.borrow_mut();

            if let DatabasePermissions::WriteOnly = db_impl.permissions {
                return Err(DatabaseError::ReadOnWriteOnly.into());
            }

            let id = {
                let value = host.get_host_id();
                byte_utils::i64_to_bytes(value)
            };

            let read_point_hash: [u8; 16] = {
                let point_raw = stack.first().ok_or(HostError::NoValOnStack)?;
                let point_bytes = byte_utils::i64_to_bytes(*point_raw);

                md5::compute([point_bytes, id].concat()).into()
            };

            let read_data = {
                let data_size_idx = stack.get(1).ok_or(HostError::NoValOnStack)? + 2;
                let mut retrn = Vec::new();

                for n in 2..data_size_idx {
                    retrn.push(*stack.get(n as usize).ok_or(HostError::NoValOnStack)?);
                }

                retrn
            };

            let user_id = host.get_host_id();
            let read = db_impl.db.read_raw(user_id, read_point_hash, &read_data)?;

            let context = host.0.context.borrow();
            let vm = context.vm.as_ref().unwrap(); // todo: make safe

            let manager = &vm.memory_manager;
            let mut offset_mut = manager.offset.borrow_mut();
            let new_offset = offset_mut.checked_add(read.len()).unwrap();

            *offset_mut = new_offset;

            drop(stack);
            stack_obj.0.clear();

            (manager.memory, new_offset, read.to_vec())
        };

        memory.write(&mut caller, offset, &data)?;
        Ok((offset as i64, data.len() as i64))
    }

    /// Returns all the host functions that must be defined in the linker.
    /// This should be the only public function related to foreign functions
    /// provided by the VM, the specific host functions should remain private.
    ///
    /// ### Current host functions
    ///
    /// The functions are currently:
    ///  - Database write: retrieves instructions and data to be written specified
    /// by the module and calls the [`DB::write_raw()`] function. Writing to the database
    /// is streamlined to the [`DB`] implementation.
    /// - Database read: retrieves instructions for the data to be read by the module
    /// and calls the [`DB::read_raw()`] function. Reading from the database is streamlined
    /// to the [`DB`] implementation.
    /// - Log function: takes an integer from the module and logs it in the host.
    /// - Stack push function: pushes an integer from the module to the host's pseudo
    /// stack. This is currently the means of communication for unbound intructions between
    /// the guest and the host environment.
    /// - Read ledger close meta: Reads the host's latest ledger meta (if present) and
    /// writes it to the module's memory. Returns the offset and the size of the bytes
    /// written in the binary's memory.
    pub fn host_functions(&self, store: &mut Store<Host<DB>>) -> [FunctionInfo; 5] {
        let mut store = store;

        let db_write_fn = {
            let wrapped = Func::wrap(&mut store, |caller: Caller<_>| {
                Self::write_database_raw(caller)
            });

            FunctionInfo {
                module: "env",
                func: "write_raw",
                wrapped,
            }
        };

        let db_read_fn = {
            let db_read_fn_wrapped = Func::wrap(&mut store, |caller: Caller<_>| {
                if let Ok(res) = Host::read_database_raw(caller) {
                    res
                } else {
                    panic!()
                }
            });

            FunctionInfo {
                module: "env",
                func: "read_raw",
                wrapped: db_read_fn_wrapped,
            }
        };

        let log_fn = {
            let wrapped = Func::wrap(&mut store, |_: Caller<_>, param: i64| {
                println!("Logged: {}", param);
            });

            FunctionInfo {
                module: "env",
                func: "zephyr_logger",
                wrapped,
            }
        };

        let stack_push_fn = {
            let wrapped = Func::wrap(&mut store, |caller: Caller<_>, param: i64| {
                let host: &Host<DB> = caller.data();
                host.as_stack_mut().0.push(param);
            });

            FunctionInfo {
                module: "env",
                func: "zephyr_stack_push",
                wrapped,
            }
        };

        let read_ledger_meta_fn = {
            let wrapped = Func::wrap(&mut store, |caller: Caller<_>| {
                if let Ok(res) = Host::read_ledger_meta(caller) {
                    res
                } else {
                    panic!()
                }
            });

            FunctionInfo {
                module: "env",
                func: "read_ledger_meta",
                wrapped,
            }
        };

        [
            db_write_fn,
            db_read_fn,
            log_fn,
            stack_push_fn,
            read_ledger_meta_fn,
        ]
    }
}
