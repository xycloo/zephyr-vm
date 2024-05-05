//! Structures and implementations for the Zephyr
//! host environment. This module defines all the interactions
//! between the binary code executed within the VM and
//! the implementor.

use anyhow::{Result, anyhow};
use rs_zephyr_common::{to_fixed, wrapping::WrappedMaxBytes, DatabaseError, ZephyrStatus};
use soroban_env_host::budget::AsBudget;
use soroban_env_host::vm::CustomContextVM;
use soroban_env_host::{CheckedEnvArg, ContractFunctionSet, Env, MapObject, Symbol, TryFromVal, TryIntoVal, U32Val, Val, VmCaller, WasmiMarshal};
use soroban_env_host::wasmi as soroban_wasmi;
//use soroban_sdk::{IntoVal, Symbol, Val};
use stellar_xdr::next::{Hash, LedgerEntry, LedgerEntryData, Limits, ReadXdr, ScAddress, ScVal, WriteXdr};
use tokio::sync::mpsc::UnboundedSender;

//use sha2::{Digest, Sha256};
use std::{
    borrow::{Borrow, BorrowMut}, cell::{Ref, RefCell, RefMut}, num::Wrapping, rc::{Rc, Weak}, sync::mpsc::Sender
};
use wasmi::{core::Pages, Caller, Func, Memory, Store, Value};

use crate::soroban_host_gen::{self, RelativeObjectConversion};
use crate::vm::MemoryManager;
use crate::{
    budget::Budget,
    db::{
        database::{Database, DatabasePermissions, WhereCond, ZephyrDatabase}, ledger::{Ledger, LedgerStateRead}, shield::ShieldedStore
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


pub struct ZephyrTestContract;

impl ContractFunctionSet for ZephyrTestContract {
    fn call(&self, _func: &Symbol, host: &soroban_env_host::Host, _args: &[Val]) -> Option<Val> {
        None
    }
}

type ZephyrRelayer = UnboundedSender<Vec<u8>>;

/// Information about the entry point function. This
/// function is exported by the binary with the given
/// argument types.
#[derive(Clone)]
pub struct InvokedFunctionInfo {
    /// Name of the function.
    pub fname: String,

    /// Function parameter types.
    pub params: Vec<Value>,

    /// Function return types.
    pub retrn: Vec<Value>,
}

impl InvokedFunctionInfo {
    pub(crate) fn serverless_defaults(name: &str) -> Self {
        Self { 
            fname: name.into(), 
            params: vec![], 
            retrn: vec![] 
        }
    }
}

/// By default, Zephyr infers a standard entry point:
/// the `on_close() -> ()` function.
impl ZephyrStandard for InvokedFunctionInfo {
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
pub struct HostImpl<DB: ZephyrDatabase, L: LedgerStateRead> {
    /// Host id.
    pub id: i64,

    /// Transmitter
    pub transmitter: RefCell<Option<ZephyrRelayer>>,
    
    /// Result of the invocation. Currently this can only be a string.
    pub result: RefCell<String>,

    /// Latest ledger close meta. This is set as optional as
    /// some Zephyr programs might not need the ledger meta.
    pub latest_close: RefCell<Option<Vec<u8>>>, // some zephyr programs might not need the ledger close meta

    /// Implementation of the Shielded Store.
    pub shielded_store: RefCell<ShieldedStore>,

    /// Database implementation.
    pub database: RefCell<Database<DB>>,

    /// Ledger state.
    pub ledger: Ledger<L>,

    /// Budget implementation.
    pub budget: RefCell<Budget>,

    /// Entry point info.
    pub entry_point_info: RefCell<InvokedFunctionInfo>,

    /// VM context.
    pub context: RefCell<VmContext<DB, L>>,

    /// Host pseudo stack implementation.
    pub stack: RefCell<Stack>,

    /// Wrapper for the Soroban Host Environment
    pub soroban: RefCell<soroban_env_host::Host>,
}

/// Zephyr Host State.
#[derive(Clone)]
pub struct Host<DB: ZephyrDatabase, L: LedgerStateRead>(Rc<HostImpl<DB, L>>); // We wrap [`HostImpl`] here inside an rc pointer for multi ownership.

#[allow(dead_code)]
impl<DB: ZephyrDatabase + ZephyrStandard, L: LedgerStateRead + ZephyrStandard> Host<DB, L> {
    /// Creates a standard Host object starting from a given
    /// host ID. The host ID is the only relation between the VM
    /// and the entity it is bound to. For instance, in Mercury
    /// the host id is the id of a Mercury user. This is needed to
    /// implement role constraints in Zephyr.
    pub fn from_id(id: i64) -> Result<Self> {
        let host = soroban_env_host::Host::test_host_with_recording_footprint();
        host.as_budget().reset_unlimited().unwrap();
        host.enable_debug();
        
        let test_contract = Rc::new(ZephyrTestContract {});
        let dummy_id = [0; 32];
        let dummy_address = ScAddress::Contract(Hash(dummy_id));
        let contract_id = host.add_host_object(dummy_address).unwrap();
        
        host.register_test_contract(contract_id, test_contract)?;
        
        Ok(Self(Rc::new(HostImpl {
            id,
            transmitter: RefCell::new(None),
            result: RefCell::new(String::new()),
            latest_close: RefCell::new(None),
            shielded_store: RefCell::new(ShieldedStore::default()),
            database: RefCell::new(Database::zephyr_standard()?),
            ledger: Ledger::zephyr_standard()?,
            budget: RefCell::new(Budget::zephyr_standard()?),
            entry_point_info: RefCell::new(InvokedFunctionInfo::zephyr_standard()?),
            context: RefCell::new(VmContext::zephyr_standard()?),
            stack: RefCell::new(Stack::zephyr_standard()?),
            soroban: RefCell::new(host)
        })))
    }
}

impl<DB: ZephyrDatabase + ZephyrMock, L: LedgerStateRead + ZephyrMock> ZephyrMock for Host<DB, L> {
    /// Creates a Host object designed to be used in tests with potentially
    /// mocked data such as host id, databases and context.
    fn mocked() -> Result<Self> {
        let host = soroban_env_host::Host::test_host_with_recording_footprint();
        host.as_budget().reset_unlimited().unwrap();
        let test_contract = Rc::new(ZephyrTestContract {});
        let dummy_id = [0; 32];
        let dummy_address = ScAddress::Contract(Hash(dummy_id));
        let contract_id = host.add_host_object(dummy_address).unwrap();
        host.register_test_contract(contract_id, test_contract);

        Ok(Self(Rc::new(HostImpl {
            id: 0,
            transmitter: RefCell::new(None),
            result: RefCell::new(String::new()),
            latest_close: RefCell::new(None),
            shielded_store: RefCell::new(ShieldedStore::default()),
            database: RefCell::new(Database::mocked()?),
            ledger: Ledger::mocked()?,
            budget: RefCell::new(Budget::zephyr_standard()?),
            entry_point_info: RefCell::new(InvokedFunctionInfo::zephyr_standard()?),
            context: RefCell::new(VmContext::mocked()?),
            stack: RefCell::new(Stack::zephyr_standard()?),
            soroban: RefCell::new(host)
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

/// Wrapper function information.
/// This object is sent to the VM object when the Virtual Machine
/// is created to tell the linker which host functions to define.
#[derive(Clone)]
pub struct SorobanTempFunctionInfo<DB: ZephyrDatabase + Clone + 'static, L: LedgerStateRead + 'static> {
    /// Module name.
    pub module: &'static str,

    /// Function name.
    pub func: &'static str,

    /// Func object. Contains the function's implementation.
    pub wrapped: fn(&mut Store<Host<DB, L>>) -> Func,
}

#[allow(dead_code)]
impl<DB: ZephyrDatabase + Clone + 'static, L: LedgerStateRead + 'static> Host<DB, L> {
    pub fn soroban_host(caller: &Caller<Self>) -> soroban_env_host::Host {
        let host = caller.data();
        host.0.soroban.borrow().to_owned()
    }

    pub fn get_memory(caller: &Caller<Self>) -> Memory {
        let host = caller.data();

        let memory = {
            let context = host.0.context.borrow();
            let vm = context.vm.as_ref().unwrap().upgrade().unwrap();
            let mem_manager = &vm.memory_manager;

            mem_manager.memory
        };

        memory
    }


    pub fn get_memory_mut(caller: &mut Caller<Self>) -> Memory {
        let host = caller.data();

        let memory = {
            let context = host.0.context.borrow();
            let vm = context.vm.as_ref().unwrap().upgrade().unwrap();
            let mem_manager = &vm.memory_manager;

            mem_manager.memory
        };

        memory
    }

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

    /// Adds a transmitter that will be used to send message to the
    /// associated receiver once every time the [`Self::send_message`]
    /// host is called.
    /// 
    /// Current behaviour replaces any existing transmitter.
    pub fn add_transmitter(&mut self, transmitter: ZephyrRelayer) {
        let current = &self.0.transmitter;
        
        *current.borrow_mut() = Some(transmitter);
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
    pub fn get_entry_point_info(&self) -> Ref<InvokedFunctionInfo> {
        self.0.entry_point_info.borrow()
    }

    /// Loads VM context in the host if needed.
    pub fn load_context(&self, vm: Weak<Vm<DB, L>>) -> Result<()> {
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

            current.clone().unwrap() // this is unsafe and can easily break the execution
                                     // we should either not make the close meta an option
                                     // or handle the error here and return the error to
                                     // the guest.
        };

        Self::write_to_memory(caller, ledger_close_meta.as_slice())
    }

    fn write_to_memory(mut caller: Caller<Self>, contents: &[u8]) -> Result<(i64, i64)> {
        let (memory, offset, data) = {
            let host = caller.data();

            let context = host.0.context.borrow();
            let vm = context.vm.as_ref().unwrap().upgrade().unwrap(); // todo: make safe

            let manager = &vm.memory_manager;
            let memory = manager.memory;

            let mut offset_mut = manager.offset.borrow_mut();
            let new_offset = offset_mut.checked_add(contents.len()).unwrap();

            *offset_mut = new_offset;

            (memory, new_offset, contents)
        };

        // TODO: this should actually only grow the linear memory when needed, so check the current
        // pages and the size of the contents to compute a safe pages size (else error with a growth error).
        // Currently we don't unwrap this and allow the program to grow unbounded <- this is unsafe and only temporary.
        let _ = memory.grow(&mut caller, Pages::new(1000).unwrap());
        
        if let Err(error) = memory.write(&mut caller, offset, data) {
            return Err(anyhow!(error))
        };

        Ok((offset as i64, data.len() as i64))
    }

    fn write_to_memory_mut(caller: &mut Caller<Self>, pos: u32, contents: &[u8]) -> Result<i64> {
        let memory = Host::get_memory(caller);

        // TODO: this should actually only grow the linear memory when needed, so check the current
        // pages and the size of the contents to compute a safe pages size (else error with a growth error).
        // Currently we don't unwrap this and allow the program to grow unbounded <- this is unsafe and only temporary.
        //let _ = memory.grow(caller, Pages::new(1000).unwrap());
        
        if let Err(error) = memory.write(caller, pos as usize, contents) {
            return Err(anyhow!(error))
        };

        Ok((pos + contents.len() as u32) as i64)
    }


    // **Note**
    // `write_database_raw` currently directly calls the database implementation
    // to write the database. Once the shield store implementation is ready this will
    // await for end of execution and handle + execute the transaction.
    fn write_database_raw(caller: Caller<Self>) -> Result<()> {
        let (memory, write_point_hash, columns, segments) = {
            let host = caller.data();
            let stack_impl = host.as_stack_mut();

            let id = {
                let value = host.get_host_id();
                byte_utils::i64_to_bytes(value)
            };

            let write_point_hash: [u8; 16] = {
                let point_raw = stack_impl.0.get_with_step().ok_or(HostError::NoValOnStack)?;
                let point_bytes = byte_utils::i64_to_bytes(point_raw);
                md5::compute([point_bytes, id].concat()).into()
            };

            let columns = {
                let columns_size_idx = stack_impl.0.get_with_step().ok_or(HostError::NoValOnStack)?;
                let mut columns: Vec<i64> = Vec::new();
                for _ in 0..columns_size_idx as usize {
                    columns.push(stack_impl.0.get_with_step().ok_or(HostError::NoValOnStack)?);
                }
                columns
            };

            let data_segments = {
                let mut segments: Vec<(i64, i64)> = Vec::new();
                let data_segments_size_idx = {
                    let non_fixed = stack_impl.0.get_with_step().ok_or(HostError::NoValOnStack)?;
                    (non_fixed * 2) as usize
                };
                for _ in (0..data_segments_size_idx).step_by(2) {
                    let offset = stack_impl.0.get_with_step().ok_or(HostError::NoValOnStack)?;
                    let size = stack_impl.0.get_with_step().ok_or(HostError::NoValOnStack)?;
                    segments.push((offset, size))
                }
                segments
            };

            let context = host.0.context.borrow();
            let vm = context.vm.as_ref().unwrap().upgrade().unwrap();
            let mem_manager = &vm.memory_manager;
            stack_impl.0.clear();

            (mem_manager.memory, write_point_hash, columns, data_segments)
        };

        let aggregated_data = segments
            .iter()
            .map(|segment| Self::read_segment_from_memory(&memory, &caller, *segment))
            .collect::<Result<Vec<_>, _>>()?;


        let host = caller.data();
        let db_obj = host.0.database.borrow();
        let db_impl = &db_obj.0;

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

    fn update_database_raw(caller: Caller<Self>) -> Result<()> {
        let (memory, write_point_hash, columns, segments, conditions, conditions_args) = {
            let host = caller.data();

            let stack_impl = host.as_stack_mut();

            let id = {
                let value = host.get_host_id();
                byte_utils::i64_to_bytes(value)
            };

            let write_point_hash: [u8; 16] = {
                let point_raw = stack_impl.0.get_with_step().ok_or(HostError::NoValOnStack)?;
                let point_bytes = byte_utils::i64_to_bytes(point_raw);
                md5::compute([point_bytes, id].concat()).into()
            };

            let columns = {
                let columns_size_idx = stack_impl.0.get_with_step().ok_or(HostError::NoValOnStack)?;
                let mut columns: Vec<i64> = Vec::new();

                for _ in 0..columns_size_idx as usize {
                    columns.push(stack_impl.0.get_with_step().ok_or(HostError::NoValOnStack)?);
                }

                columns
            };

            let data_segments = {
                let mut segments: Vec<(i64, i64)> = Vec::new();

                let data_segments_size_idx = {
                    let non_fixed = stack_impl.0.get_with_step().ok_or(HostError::NoValOnStack)?;
                    (non_fixed * 2) as usize
                };

                for _ in (0..data_segments_size_idx).step_by(2) {
                    let offset = stack_impl.0.get_with_step().ok_or(HostError::NoValOnStack)?;
                    let size = stack_impl.0.get_with_step().ok_or(HostError::NoValOnStack)?;
                    segments.push((offset, size))
                }
                segments
            };

            let conditions = {
                let mut conditions = Vec::new();

                let conditions_length = {
                    let non_fixed = stack_impl.0.get_with_step().ok_or(HostError::NoValOnStack)?;
                    (non_fixed * 2) as usize
                };

                for _ in (0..conditions_length).step_by(2) {
                    let column = stack_impl.0.get_with_step().ok_or(HostError::NoValOnStack)?;    
                    let operator = stack_impl.0.get_with_step().ok_or(HostError::NoValOnStack)?;                    
                    conditions.push(WhereCond::from_column_and_operator(column, operator)?);
                }

                conditions
            };

            let conditions_args = {
                let mut segments = Vec::new();

                let args_length = {
                    let non_fixed = stack_impl.0.get_with_step().ok_or(HostError::NoValOnStack)?;
                    (non_fixed * 2) as usize
                };

                for _ in (0..args_length).step_by(2) {
                    let offset = stack_impl.0.get_with_step().ok_or(HostError::NoValOnStack)?;
                    let size = stack_impl.0.get_with_step().ok_or(HostError::NoValOnStack)?;
                    segments.push((offset, size))
                }

                segments
            };

            let context = host.0.context.borrow();
            let vm = context.vm.as_ref().unwrap().upgrade().unwrap();
            let mem_manager = &vm.memory_manager;

            stack_impl.0.clear();

            (mem_manager.memory, write_point_hash, columns, data_segments, conditions, conditions_args)
        };

        let aggregated_data = segments
            .iter()
            .map(|segment| Self::read_segment_from_memory(&memory, &caller, *segment))
            .collect::<Result<Vec<_>, _>>()?;

        let aggregated_conditions_args = conditions_args
            .iter()
            .map(|segment| Self::read_segment_from_memory(&memory, &caller, *segment))
            .collect::<Result<Vec<_>, _>>()?;


        let host = caller.data();
        let db_obj = host.0.database.borrow();
        let db_impl = db_obj.0.borrow();

        if let DatabasePermissions::ReadOnly = db_impl.permissions {
            return Err(DatabaseError::WriteOnReadOnly.into());
        }

        db_impl.db.update_raw(
            host.get_host_id(),
            write_point_hash,
            &columns,
            aggregated_data,
            &conditions,
            aggregated_conditions_args
        )?;

        Ok(())
    }

    fn read_database_raw(caller: Caller<Self>) -> Result<(i64, i64)> {
        let host = caller.data();
        
        let read = {
            let db_obj = host.0.database.borrow();
            let db_impl = db_obj.0.borrow();
            let stack_obj = host.0.stack.borrow_mut();
            let stack = stack_obj.0.inner.borrow_mut();

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

            drop(stack);
            stack_obj.0.clear();
            
            db_impl.db.read_raw(user_id, read_point_hash, &read_data)?
        };

        Self::write_to_memory(caller, read.as_slice())
    }

    fn internal_read_contract_data_entry_by_contract_id_and_key(caller: Caller<Self>, contract: [u8; 32], key: ScVal) -> Result<(i64, i64)> {
        let host = caller.data();
    
        let contract = ScAddress::Contract(Hash(contract));
        let read = {
            let ledger = &host.0.ledger.0.ledger;
            bincode::serialize(&ledger.read_contract_data_entry_by_contract_id_and_key(contract, key)).unwrap()
        };
        
        Self::write_to_memory(caller, &read)
    }


    pub fn read_contract_data_entry_by_contract_id_and_key(caller: Caller<Self>, contract: [u8; 32], offset: i64, size: i64) -> Result<(i64, i64)> {
        let host = caller.data();
        
        let key = {
            let memory = {
                let context = host.0.context.borrow();
                let vm = context.vm.as_ref().unwrap().upgrade().unwrap();
                let mem_manager = &vm.memory_manager;

                mem_manager.memory
            };

            let segment = (offset, size);
            
            ScVal::from_xdr(Self::read_segment_from_memory(&memory, &caller, segment)?, Limits::none())?
        };

        Self::internal_read_contract_data_entry_by_contract_id_and_key(caller, contract, key)
    }

    pub fn read_contract_instance(caller: Caller<Self>, contract: [u8; 32]) -> Result<(i64, i64)> {
        let key = ScVal::LedgerKeyContractInstance;

        Self::internal_read_contract_data_entry_by_contract_id_and_key(caller, contract, key)
    }

    pub fn read_contract_entries(caller: Caller<Self>, contract: [u8; 32]) -> Result<(i64, i64)> {
        let host = caller.data();
    
        let contract = ScAddress::Contract(Hash(contract));
        let read = {
            let ledger = &host.0.ledger.0.ledger;
            bincode::serialize(&ledger.read_contract_data_entries_by_contract_id(contract)).unwrap()
        };
        
        Self::write_to_memory(caller, &read)
    }

    pub fn scval_to_valid_host_val(caller: Caller<Self>, scval: &ScVal) -> Result<i64> {
        let host = caller.data();
    
        let (soroban, val) = {
            let soroban = host.0.soroban.borrow().to_owned();
            soroban.as_budget().reset_unlimited().unwrap();

            soroban.enable_debug().unwrap();
            
            let val = soroban.with_test_contract_frame(Hash([0;32]), Symbol::from_small_str("test"), || {
                Ok(soroban.to_valid_host_val(scval).unwrap())
            }).unwrap().get_payload() as i64;
            
            (soroban, val)
        };

        *host.0.soroban.borrow_mut() = soroban;

        Ok(val)
    }

    pub fn valid_host_val_to_scval(caller: Caller<Self>, val: Val) -> Result<(i64, i64)> {
        let host = caller.data();
    
        let res = {
            let soroban = host.0.soroban.borrow().to_owned();
            soroban.as_budget().reset_unlimited().unwrap();

            soroban.enable_debug().unwrap();
            let scval = ScVal::try_from_val(&soroban, &val).unwrap();
            Self::write_to_memory(caller, &scval.to_xdr(Limits::none()).unwrap())
        };

        res
    }


    pub fn read_contract_entries_to_env(caller: Caller<Self>, contract: [u8; 32]) -> Result<i64> {
        let host = caller.data();
    
        let (soroban, val) = {
            let contract = ScAddress::Contract(Hash(contract));
            let ledger = &host.0.ledger.0.ledger;

            let data = ledger.read_contract_data_entries_by_contract_id(contract);

            let soroban = host.0.soroban.borrow().to_owned();
            soroban.as_budget().reset_unlimited().unwrap();

            soroban.enable_debug().unwrap();
            //let mut current = soroban.get_ledger_info().unwrap().unwrap_or_default();
            //let map = soroban.map_new().unwrap();
            
            let val = soroban.with_test_contract_frame(Hash([0;32]), Symbol::from_small_str("test"), || {
                let mut map = soroban.map_new().unwrap();
                
                for entry in data {
                    let LedgerEntryData::ContractData(d) = entry.entry.data else {
                        panic!("invalid xdr")
                    };

                    if d.key != ScVal::LedgerKeyContractInstance {
                        let key = soroban.to_valid_host_val(&d.key).unwrap();
                        let val = soroban.to_valid_host_val(&d.val).unwrap();

                        map = soroban.map_put(map, key, val).unwrap();
                    }
                };

                soroban.enable_debug().unwrap();
                
                Ok(map.into())
            }).unwrap().get_payload() as i64;

            let map = MapObject::try_from_val(&soroban, &Val::from_payload(val as u64)).unwrap();
            
            (soroban, val)
        };

        *host.0.soroban.borrow_mut() = soroban;

        Ok(val)
    }

    /// Sends a message to any receiver whose sender has been provided to the
    /// host object.
    pub fn send_message(caller: Caller<Self>, offset: i64, size: i64) -> Result<()> {
        let host = caller.data();

        let message = {
            let memory = {
                let context = host.0.context.borrow();
                let vm = context.vm.as_ref().unwrap().upgrade().unwrap();
                let mem_manager = &vm.memory_manager;

                mem_manager.memory
            };

            let segment = (offset, size);
            Self::read_segment_from_memory(&memory, &caller, segment)?
        };

        
        let tx = host.0.transmitter.borrow();
        let tx = tx.as_ref().ok_or_else(|| HostError::NoTransmitter)?;
    
        tx.send(message)?;

        Ok(())
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
    /// - Database update: Retrieves and structures instructions and data used by the [`DB`]
    /// implementation to update a table.  
    /// - Log function: takes an integer from the module and logs it in the host.
    /// - Stack push function: pushes an integer from the module to the host's pseudo
    /// stack. This is currently the means of communication for unbound intructions between
    /// the guest and the host environment.
    /// - Read ledger close meta: Reads the host's latest ledger meta (if present) and
    /// writes it to the module's memory. Returns the offset and the size of the bytes
    /// written in the binary's memory.
    pub fn host_functions(&self, store: &mut Store<Host<DB, L>>) -> Vec<FunctionInfo> {
        let mut store = store;

        let db_write_fn = {
            let wrapped = Func::wrap(&mut store, |caller: Caller<_>| {
                let result = Self::write_database_raw(caller);
                let res = if result.is_err() {
                    ZephyrStatus::from(result.err().unwrap()) as i64
                } else {
                    ZephyrStatus::Success as i64
                };

                res
            });

            FunctionInfo {
                module: "env",
                func: "write_raw",
                wrapped,
            }
        };

        let db_update_fn = {
            let wrapped = Func::wrap(&mut store, |caller: Caller<_>| {
                let result = Self::update_database_raw(caller);
                let res = if result.is_err() {
                    ZephyrStatus::from(result.err().unwrap()) as i64
                } else {
                    ZephyrStatus::Success as i64
                };

                res
            });

            FunctionInfo {
                module: "env",
                func: "update_raw",
                wrapped,
            }
        };

        let db_read_fn = {
            let db_read_fn_wrapped = Func::wrap(&mut store, |caller: Caller<_>| {
                let result = Host::read_database_raw(caller);
                if let Ok(res) = result {
                    (ZephyrStatus::Success as i64, res.0, res.1)
                } else {
                    (ZephyrStatus::from(result.err().unwrap()) as i64, 0, 0)
                }
            });

            FunctionInfo {
                module: "env",
                func: "read_raw",
                wrapped: db_read_fn_wrapped,
            }
        };

        let read_contract_data_entry_by_contract_id_and_key_fn = {
            let wrapped = Func::wrap(&mut store, |caller: Caller<_>, contract_part_1: i64, contract_part_2: i64, contract_part_3: i64, contract_part_4: i64, offset: i64, size: i64| {
                let contract = WrappedMaxBytes::array_from_max_parts::<32>(&[contract_part_1, contract_part_2, contract_part_3, contract_part_4]);

                let result = Host::read_contract_data_entry_by_contract_id_and_key(caller, contract, offset, size);
                if let Ok(res) = result {
                    (ZephyrStatus::Success as i64, res.0, res.1)
                } else {
                    (ZephyrStatus::from(result.err().unwrap()) as i64, 0, 0)
                }
            });

            FunctionInfo {
                module: "env",
                func: "read_contract_data_entry_by_contract_id_and_key",
                wrapped
            }
        };

        let read_contract_instance_fn = {
            let wrapped = Func::wrap(&mut store, |caller: Caller<_>, contract_part_1: i64, contract_part_2: i64, contract_part_3: i64, contract_part_4: i64| {
                let contract = WrappedMaxBytes::array_from_max_parts::<32>(&[contract_part_1, contract_part_2, contract_part_3, contract_part_4]);
                let result = Host::read_contract_instance(caller, contract);
                
                if let Ok(res) = result {
                    (ZephyrStatus::Success as i64, res.0, res.1)
                } else {
                    (ZephyrStatus::from(result.err().unwrap()) as i64, 0, 0)
                }
            });

            FunctionInfo {
                module: "env",
                func: "read_contract_instance",
                wrapped
            }
        };

        let read_contract_entries_fn = {
            let wrapped = Func::wrap(&mut store, |caller: Caller<_>, contract_part_1: i64, contract_part_2: i64, contract_part_3: i64, contract_part_4: i64| {
                let contract = WrappedMaxBytes::array_from_max_parts::<32>(&[contract_part_1, contract_part_2, contract_part_3, contract_part_4]);
                let result = Host::read_contract_entries(caller, contract);
                
                if let Ok(res) = result {
                    (ZephyrStatus::Success as i64, res.0, res.1)
                } else {
                    (ZephyrStatus::from(result.err().unwrap()) as i64, 0, 0)
                }
            });

            FunctionInfo {
                module: "env",
                func: "read_contract_entries_by_contract",
                wrapped
            }
        };

        let read_contract_entries_to_env_fn = {
            let wrapped = Func::wrap(&mut store, |caller: Caller<_>, contract_part_1: i64, contract_part_2: i64, contract_part_3: i64, contract_part_4: i64| {
                let contract = WrappedMaxBytes::array_from_max_parts::<32>(&[contract_part_1, contract_part_2, contract_part_3, contract_part_4]);
                let result = Host::read_contract_entries_to_env(caller, contract);
                
                if let Ok(res) = result {
                    (ZephyrStatus::Success as i64, res)
                } else {
                    (ZephyrStatus::from(result.err().unwrap()) as i64, 0)
                }
            });

            FunctionInfo {
                module: "env",
                func: "read_contract_entries_by_contract_to_env",
                wrapped
            }
        };

        let scval_to_valid_host_val = {
            let wrapped = Func::wrap(&mut store, |caller: Caller<_>, offset: i64, size: i64| {
                let bytes = {
                    let host: &Self = caller.data();
                    let memory = {
                        let context = host.0.context.borrow();
                        let vm = context.vm.as_ref().unwrap().upgrade().unwrap();
                        let mem_manager = &vm.memory_manager;

                        mem_manager.memory
                    };

                    let segment = (offset, size);
                    Self::read_segment_from_memory(&memory, &caller, segment).unwrap()
                };
                let scval = ScVal::from_xdr(bytes, Limits::none()).unwrap();

                let result = Host::scval_to_valid_host_val(caller, &scval);
                
                
                if let Ok(res) = result {
                    (ZephyrStatus::Success as i64, res)
                } else {
                    (ZephyrStatus::from(result.err().unwrap()) as i64, 0)
                }
            });

            FunctionInfo {
                module: "env",
                func: "scval_to_valid_host_val",
                wrapped
            }
        };

        let valid_host_val_to_scval = {
            let wrapped = Func::wrap(&mut store, |caller: Caller<_>, val: i64| {
                let result = Host::valid_host_val_to_scval(caller, Val::from_payload(val as u64));
                
                if let Ok(res) = result {
                    (ZephyrStatus::Success as i64, res.0, res.1)
                } else {
                    (ZephyrStatus::from(result.err().unwrap()) as i64, 0, 0)
                }
            });

            FunctionInfo {
                module: "env",
                func: "valid_host_val_to_scval",
                wrapped
            }
        };


        let conclude_fn = {
            let wrapped = Func::wrap(&mut store, |caller: Caller<_>, offset: i64, size: i64| {
                Host::write_result(caller, offset, size);
            });

            FunctionInfo {
                module: "env",
                func: "conclude",
                wrapped
            }
        };

        let send_message_fn = {
            let wrapped = Func::wrap(&mut store, |caller: Caller<_>, offset: i64, size: i64| {
                let result = Host::send_message(caller, offset, size);

                if let Ok(_) = result {
                    ZephyrStatus::Success as i64
                } else {
                    ZephyrStatus::from(result.err().unwrap()) as i64
                }
            });

            FunctionInfo {
                module: "env",
                func: "tx_send_message",
                wrapped
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

        let wbin_descr_fn = {
            let wrapped = Func::wrap(&mut store, |_: Caller<_>, param: i32| {
                println!("placeholder describe: {}", param);
            });

            FunctionInfo {
                module: "__wbindgen_placeholder__",
                func: "__wbindgen_describe",
                wrapped,
            }
        };

        let wbin_throw_fn = {
            let wrapped = Func::wrap(&mut store, |_: Caller<_>, param: i32, param1: i32| {
                println!("placeholder throw: {}", param);
            });

            FunctionInfo {
                module: "__wbindgen_placeholder__",
                func: "__wbindgen_throw",
                wrapped,
            }
        };

        let wbin_tab_grow_fn = {
            let wrapped = Func::wrap(&mut store, |_: Caller<_>, param: i32| {
                println!("table grow: {}", param);

                param
            });

            FunctionInfo {
                module: "__wbindgen_externref_xform__",
                func: "__wbindgen_externref_table_grow",
                wrapped,
            }
        };

        let wbin_tab_set_null_fn = {
            let wrapped = Func::wrap(&mut store, |_: Caller<_>, param: i32| {
                println!("table grow: {}", param);
            });

            FunctionInfo {
                module: "__wbindgen_externref_xform__",
                func: "__wbindgen_externref_table_set_null",
                wrapped,
            }
        };

        let stack_push_fn = {
            let wrapped = Func::wrap(&mut store, |caller: Caller<_>, param: i64| {
                let host: &Host<DB, L> = caller.data();
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
                    // this is also unsafe
                    // panic!()

                    // current implementation is faulty
                    // and only serves mocked testing
                    // purposes. Any attempt to run
                    // Zephyr without providing the latest
                    // close meta has a high probability of
                    // breaking.

                    (0, 0)
                }
            });

            FunctionInfo {
                module: "env",
                func: "read_ledger_meta",
                wrapped,
            }
        };

        let string_from_linmem = {
            let wrapped = Func::wrap(&mut store, |caller: Caller<Host<DB, L>>, lm_pos: i64, len: i64| {
                let vm_ctx = CustomVMCtx::new(&caller);
                
                let host: soroban_env_host::Host = Host::<DB, L>::soroban_host(&caller);
                host.enable_debug();
                
                let effects = || {
                    let res: Result<_, soroban_env_host::HostError> = host.string_new_from_linear_memory_mem(vm_ctx, U32Val::check_env_arg(U32Val::try_marshal_from_relative_value(soroban_wasmi::Value::I64(lm_pos), &host).unwrap(), &host).unwrap(), U32Val::check_env_arg(U32Val::try_marshal_from_relative_value(soroban_wasmi::Value::I64(len), &host).unwrap(), &host).unwrap());
                    res
                };

                let res = host.with_test_contract_frame(Hash([0;32]), Symbol::from_small_str("test"), || {
                    let res = effects();
                    let res = match res {
                        Ok(ok) => {
                            let ok = ok.check_env_arg(&host).unwrap();
                            
                            let val: soroban_wasmi::Value = ok.marshal_relative_from_self(&host).unwrap();
                            
                            if let soroban_wasmi::Value::I64(v) = val {
                                Ok((v,))
                            } else {
                                Err(0)
                            }
                        },
                        Err(hosterr) => {
                            panic!("todo")
                        }
                    };

                    Ok(Val::from_payload(res.unwrap().0 as u64))
                });

                res.unwrap().get_payload() as i64
            });

            FunctionInfo {
                module: "b",
                func: "i",
                wrapped,
            }
        };

        let symbol_from_linmem = {
            let wrapped = Func::wrap(&mut store, |caller: Caller<Host<DB, L>>, lm_pos: i64, len: i64| {
                let vm_ctx = CustomVMCtx::new(&caller);
                
                let host: soroban_env_host::Host = Host::<DB, L>::soroban_host(&caller);
                host.enable_debug();
                
                let effects = || {
                    let res: Result<_, soroban_env_host::HostError> = host.symbol_new_from_linear_memory_mem(vm_ctx, U32Val::check_env_arg(U32Val::try_marshal_from_relative_value(soroban_wasmi::Value::I64(lm_pos), &host).unwrap(), &host).unwrap(), U32Val::check_env_arg(U32Val::try_marshal_from_relative_value(soroban_wasmi::Value::I64(len), &host).unwrap(), &host).unwrap());
                    res
                };

                let res = host.with_test_contract_frame(Hash([0;32]), Symbol::from_small_str("test"), || {
                    let res = effects();
                    let res = match res {
                        Ok(ok) => {
                            let ok = ok.check_env_arg(&host).unwrap();
                            
                            let val: soroban_wasmi::Value = ok.marshal_relative_from_self(&host).unwrap();
                            
                            if let soroban_wasmi::Value::I64(v) = val {
                                Ok((v,))
                            } else {
                                Err(0)
                            }
                        },
                        Err(hosterr) => {
                            panic!("todo")
                        }
                    };

                    Ok(Val::from_payload(res.unwrap().0 as u64))
                });

                res.unwrap().get_payload() as i64
            });

            FunctionInfo {
                module: "b",
                func: "j",
                wrapped,
            }
        };

        let symbol_index_from_linmem = {
            let wrapped = Func::wrap(&mut store, |caller: Caller<Host<DB, L>>, sym: i64,  lm_pos: i64, len: i64| {
                let vm_ctx = CustomVMCtx::new(&caller);
                
                let host: soroban_env_host::Host = Host::<DB, L>::soroban_host(&caller);
                host.enable_debug();
                
                let effects = || {
                    let res: Result<_, soroban_env_host::HostError> = host.symbol_index_in_linear_memory_mem(vm_ctx, Symbol::check_env_arg(Symbol::try_marshal_from_relative_value(soroban_wasmi::Value::I64(sym), &host).unwrap(), &host).unwrap(), U32Val::check_env_arg(U32Val::try_marshal_from_relative_value(soroban_wasmi::Value::I64(lm_pos), &host).unwrap(), &host).unwrap(), U32Val::check_env_arg(U32Val::try_marshal_from_relative_value(soroban_wasmi::Value::I64(len), &host).unwrap(), &host).unwrap());
                    res
                };

                let res = host.with_test_contract_frame(Hash([0;32]), Symbol::from_small_str("test"), || {
                    let res = effects();
                    let res = match res {
                        Ok(ok) => {
                            let ok = ok.check_env_arg(&host).unwrap();
                            
                            let val: soroban_wasmi::Value = ok.marshal_relative_from_self(&host).unwrap();
                            
                            if let soroban_wasmi::Value::I64(v) = val {
                                Ok((v,))
                            } else {
                                Err(0)
                            }
                        },
                        Err(hosterr) => {
                            panic!("{:?}", hosterr)
                        }
                    };

                    Ok(Val::from_payload(res.unwrap().0 as u64))
                });

                res.unwrap().get_payload() as i64
            });

            FunctionInfo {
                module: "b",
                func: "m",
                wrapped,
            }
        };

        let vec_new_from_linear_memory_mem = {
            let wrapped = Func::wrap(&mut store, |caller: Caller<Host<DB, L>>, lm_pos: i64, len: i64| {
                let vm_ctx = CustomVMCtx::new(&caller);
                
                let host: soroban_env_host::Host = Host::<DB, L>::soroban_host(&caller);
                host.enable_debug();
                
                let effects = || {
                    let res: Result<_, soroban_env_host::HostError> = host.vec_new_from_linear_memory_mem(vm_ctx, U32Val::check_env_arg(U32Val::try_marshal_from_relative_value(soroban_wasmi::Value::I64(lm_pos), &host).unwrap(), &host).unwrap(), U32Val::check_env_arg(U32Val::try_marshal_from_relative_value(soroban_wasmi::Value::I64(len), &host).unwrap(), &host).unwrap());
                    res
                };

                let res = host.with_test_contract_frame(Hash([0;32]), Symbol::from_small_str("test"), || {
                    let res = effects();
                    let res = match res {
                        Ok(ok) => {
                            let ok = ok.check_env_arg(&host).unwrap();
                            
                            let val: soroban_wasmi::Value = ok.marshal_relative_from_self(&host).unwrap();
                            
                            if let soroban_wasmi::Value::I64(v) = val {
                                Ok((v,))
                            } else {
                                Err(0)
                            }
                        },
                        Err(hosterr) => {
                            panic!("{:?}", hosterr)
                        }
                    };

                    Ok(Val::from_payload(res.unwrap().0 as u64))
                });

                res.unwrap().get_payload() as i64
            });

            FunctionInfo {
                module: "v",
                func: "g",
                wrapped,
            }
        };

        let map_unpack_to_linear_memory_fn_mem = {
            let wrapped = Func::wrap(&mut store, |caller: Caller<Host<DB, L>>, map: i64, keys_pos: i64, vals_pos: i64, len: i64| {
                
                
                let host: soroban_env_host::Host = Host::<DB, L>::soroban_host(&caller);
                host.enable_debug();
                
                let mut effects = || {
                    let mut vm_ctx = CustomVMCtx::new_mut(caller);
                    let res: Result<_, soroban_env_host::HostError> = host.map_unpack_to_linear_memory_fn_mem(&mut vm_ctx, MapObject::check_env_arg(MapObject::try_marshal_from_relative_value(soroban_wasmi::Value::I64(map), &host).unwrap(), &host).unwrap(), U32Val::check_env_arg(U32Val::try_marshal_from_relative_value(soroban_wasmi::Value::I64(keys_pos), &host).unwrap(), &host).unwrap(), U32Val::check_env_arg(U32Val::try_marshal_from_relative_value(soroban_wasmi::Value::I64(vals_pos), &host).unwrap(), &host).unwrap(), U32Val::check_env_arg(U32Val::try_marshal_from_relative_value(soroban_wasmi::Value::I64(len), &host).unwrap(), &host).unwrap());
                    res
                };

                let res = host.with_test_contract_frame(Hash([0;32]), Symbol::from_small_str("test"), || {
                    let res = effects();
                    let res = match res {
                        Ok(ok) => {
                            let ok = ok.check_env_arg(&host).unwrap();
                            
                            
                            let val: soroban_wasmi::Value = ok.marshal_relative_from_self(&host).unwrap();
                            
                            if let soroban_wasmi::Value::I64(v) = val {
                                Ok((v,))
                            } else {
                                Err(0)
                            }
                        },
                        Err(hosterr) => {
                            panic!("{:?}", hosterr)
                        }
                    };

                    Ok(Val::from_payload(res.unwrap().0 as u64))
                });

                res.unwrap().get_payload() as i64
            });

            FunctionInfo {
                module: "m",
                func: "a",
                wrapped,
            }
        };

        /*let memobj_copy_from_linear_memory = {
            let wrapped = Func::wrap(&mut store, |caller: Caller<Host<DB, L>>, obj: i64, obj_pos: i64,  lm_pos: i64, len: i64| {
                let vm_ctx = CustomVMCtx::new(&caller);
                
                let host: soroban_env_host::Host = Host::<DB, L>::soroban_host(&caller);
                host.enable_debug();
                
                let effects = || {
                    let res: Result<_, soroban_env_host::HostError> = host.symbol_index_in_linear_memory_mem(vm_ctx, Symbol::check_env_arg(Symbol::try_marshal_from_relative_value(soroban_wasmi::Value::I64(sym), &host).unwrap(), &host).unwrap(), U32Val::check_env_arg(U32Val::try_marshal_from_relative_value(soroban_wasmi::Value::I64(lm_pos), &host).unwrap(), &host).unwrap(), U32Val::check_env_arg(U32Val::try_marshal_from_relative_value(soroban_wasmi::Value::I64(len), &host).unwrap(), &host).unwrap());
                    res
                };

                let res = host.with_test_contract_frame(Hash([0;32]), Symbol::from_small_str("test"), || {
                    let res = effects();
                    let res = match res {
                        Ok(ok) => {
                            let ok = ok.check_env_arg(&host).unwrap();
                            
                            let val: soroban_wasmi::Value = ok.marshal_relative_from_self(&host).unwrap();
                            
                            if let soroban_wasmi::Value::I64(v) = val {
                                Ok((v,))
                            } else {
                                Err(0)
                            }
                        },
                        Err(hosterr) => {
                            panic!("{:?}", hosterr)
                        }
                    };

                    Ok(Val::from_payload(res.unwrap().0 as u64))
                });

                res.unwrap().get_payload() as i64
            });

            FunctionInfo {
                module: "b",
                func: "m",
                wrapped,
            }
        };*/

        let mut soroban_functions = soroban_host_gen::generate_host_fn_infos(store);
        
        let mut arr = vec![
            db_write_fn,
            db_read_fn,
            db_update_fn,
            log_fn,
            stack_push_fn,
            read_ledger_meta_fn,
            read_contract_data_entry_by_contract_id_and_key_fn,
            read_contract_instance_fn,
            read_contract_entries_fn,
            
            scval_to_valid_host_val,
            valid_host_val_to_scval,
            read_contract_entries_to_env_fn,
            conclude_fn,
            send_message_fn,

            wbin_descr_fn,
            wbin_throw_fn,
            wbin_tab_grow_fn,
            wbin_tab_set_null_fn,

            string_from_linmem,
            symbol_index_from_linmem,
            vec_new_from_linear_memory_mem,
            symbol_from_linmem,
            map_unpack_to_linear_memory_fn_mem
        ];

        soroban_functions.append(&mut arr);
        soroban_functions.reverse();

        soroban_functions
    }
    
    fn write_result(caller: Caller<Self>, offset: i64, size: i64) {
        let host = caller.data();

        let memory = {
            let context = host.0.context.borrow();
            let vm = context.vm.as_ref().unwrap().upgrade().unwrap();
            let mem_manager = &vm.memory_manager;

            mem_manager.memory
        };

        let segment = (offset, size);
        let seg = Self::read_segment_from_memory(&memory, &caller, segment).unwrap();
        let res: String = bincode::deserialize(&seg).unwrap();
        
        host.0.result.borrow_mut().push_str(&res);
    }

    fn read_segment_from_memory(memory: &Memory, caller: &Caller<Self>, segment: (i64, i64)) -> Result<Vec<u8>> {
        let mut written_vec = vec![0; segment.1 as usize];
        if let Err(error) = memory.read(caller, segment.0 as usize, &mut written_vec) {
            return Err(anyhow!(error));
        }
        
        Ok(written_vec)
    }

    pub fn read_result(&self) -> String {
        self.0.result.borrow().clone()
    }
}


pub struct CustomVMCtx<'a, DB: ZephyrDatabase + 'static, L: LedgerStateRead + 'static> {
    caller: Option<&'a Caller<'a, Host<DB, L>>>,
    caller_mut: Option<Caller<'a, Host<DB, L>>>,
}

impl<'a, DB: ZephyrDatabase + 'static, L: LedgerStateRead + 'static> CustomVMCtx<'a, DB, L> {
    fn new(ctx: &'a Caller<Host<DB, L>>) -> Self {
        Self {
            caller: Some(ctx),
            caller_mut: None
        }
    }

    fn new_mut(ctx: Caller<'a, Host<DB, L>>) -> Self {
        Self {
            caller: None,
            caller_mut: Some(ctx)
        }
    }
}

impl<'a, DB: ZephyrDatabase + Clone + 'static, L: LedgerStateRead + 'static> CustomContextVM for CustomVMCtx<'a, DB, L> {
    fn read(&self, mem_pos: usize, buf: &mut [u8]) {
        if let Some(caller) = self.caller {
            Host::get_memory(caller).read(caller, mem_pos, buf);
        } else {
            Host::get_memory(self.caller_mut.as_ref().unwrap()).read(self.caller_mut.as_ref().unwrap(), mem_pos, buf);
        }
    }


    fn data(&self) -> &[u8] {
        if let Some(caller) = self.caller {
            Host::get_memory(caller).data(caller)
        } else {
            Host::get_memory(self.caller_mut.as_ref().unwrap()).data(self.caller_mut.as_ref().unwrap())
        }
    }

    fn write(&mut self, pos: u32, slice: &[u8]) -> i64 {
        Host::write_to_memory_mut(self.caller_mut.as_mut().unwrap(), pos, slice).unwrap()
    }

    fn data_mut(&mut self) -> &mut [u8] {
        if let Some(caller) = self.caller_mut.as_mut() {
            Host::get_memory_mut(caller).data_mut(caller)
        } else {
            &mut []
        }
    }
}
