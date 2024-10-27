//! Structures and implementations for the Zephyr
//! host environment. This module defines all the interactions
//! between the binary code executed within the VM and
//! the implementor.

use crate::error::InternalError;
use crate::snapshot::snapshot_utils;
use crate::soroban_host_gen::{self, build_u32val, with_frame, RelativeObjectConversion};
use crate::trace::{StackTrace, TracePoint};
use crate::{
    budget::Budget,
    db::{
        database::{Database, ZephyrDatabase},
        ledger::{Ledger, LedgerStateRead},
    },
    error::HostError,
    stack::Stack,
    vm::Vm,
    vm_context::VmContext,
    ZephyrMock, ZephyrStandard,
};
use anyhow::Result;
use memory::CustomVMCtx;
use rs_zephyr_common::{wrapping::WrappedMaxBytes, ZephyrStatus};
use soroban_env_host::budget::AsBudget;
use soroban_env_host::xdr::{Hash, Limits, ReadXdr, ScAddress, ScVal};
use soroban_env_host::{wasmi as soroban_wasmi, BytesObject, VecObject, VmCaller};
use soroban_env_host::{CheckedEnvArg, MapObject, Symbol, Val};
use std::{
    borrow::BorrowMut,
    cell::{Ref, RefCell, RefMut},
    rc::{Rc, Weak},
};
use tokio::sync::mpsc::UnboundedSender;
use utils::soroban::ZephyrTestContract;
use wasmi::{Caller, Func, Store, Val as Value};

pub(crate) mod database;
pub(crate) mod memory;
pub(crate) mod soroban;
pub(crate) mod utils;

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
            retrn: vec![],
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

    /// Network id hashed.
    pub network_id: [u8; 32],

    /// Transmitter
    pub transmitter: RefCell<Option<ZephyrRelayer>>,

    /// Result of the invocation. Currently this can only be a string.
    pub result: RefCell<String>,

    /// Latest ledger close meta. This is set as optional as
    /// some Zephyr programs might not need the ledger meta.
    ///
    /// NB: naming probably needs to change as this is used
    /// to just communicate starting input to a program, which could
    /// be both:
    /// - a ledger close meta (state transition) < for ingestors
    /// - a request body < for functions
    pub latest_close: RefCell<Option<Vec<u8>>>, // some zephyr programs might not need the ledger close meta

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

    /// VM stack trace.
    pub stack_trace: RefCell<StackTrace>,
}

/// Zephyr Host State.
#[derive(Clone)]
pub struct Host<DB: ZephyrDatabase, L: LedgerStateRead>(Rc<HostImpl<DB, L>>); // We wrap [`HostImpl`] here inside an rc pointer for multi ownership.

// Tracing-friendly utils implementations
impl<DB: ZephyrDatabase, L: LedgerStateRead> Host<DB, L> {
    /// Returns a reference to the host's stack implementation.
    pub fn as_stack_mut(&self) -> RefMut<Stack> {
        // self.0.stack_trace.borrow_mut().maybe_add_trace(TracePoint::ZephyrEnvironment, "Reading through the ZVM stack.", false);
        self.0.stack.borrow_mut()
    }
}

#[allow(dead_code)]
impl<DB: ZephyrDatabase + ZephyrStandard, L: LedgerStateRead + ZephyrStandard> Host<DB, L> {
    /// Creates a standard Host object starting from a given
    /// host ID. The host ID is the only relation between the VM
    /// and the entity it is bound to. For instance, in Mercury
    /// the host id is the id of a Mercury user. This is needed to
    /// implement role constraints in Zephyr.
    pub fn from_id(id: i64, network_id: [u8; 32]) -> Result<Self> {
        let host = soroban_env_host::Host::test_host_with_recording_footprint();
        host.as_budget().reset_unlimited().unwrap();
        host.with_mut_ledger_info(|li| {
            let (sequence, timestamp) = snapshot_utils::get_current_ledger_sequence();
            li.sequence_number = sequence as u32;
            li.timestamp = timestamp as u64;
            li.network_id = network_id;

            li.protocol_version = 21;
        })?;
        host.enable_debug()?;

        let test_contract = Rc::new(ZephyrTestContract::new());
        let contract_id_bytes = [0; 32];
        let contract_address = ScAddress::Contract(Hash(contract_id_bytes));
        let contract_id = host.add_host_object(contract_address)?;

        // Since Soroban's Host relies on a contract to give context to the execution actions
        // performed in the ZephyrVM are connected to a non-existing sample contract address.
        host.register_test_contract(contract_id, test_contract)?;

        Ok(Self(Rc::new(HostImpl {
            id,
            network_id,
            transmitter: RefCell::new(None),
            result: RefCell::new(String::new()),
            latest_close: RefCell::new(None),
            database: RefCell::new(Database::zephyr_standard()?),
            ledger: Ledger::zephyr_standard()?,
            budget: RefCell::new(Budget::zephyr_standard()?),
            entry_point_info: RefCell::new(InvokedFunctionInfo::zephyr_standard()?),
            context: RefCell::new(VmContext::zephyr_standard()?),
            stack: RefCell::new(Stack::zephyr_standard()?),
            soroban: RefCell::new(host),
            stack_trace: RefCell::new(Default::default()),
        })))
    }
}

impl<DB: ZephyrDatabase + ZephyrMock, L: LedgerStateRead + ZephyrMock> ZephyrMock for Host<DB, L> {
    /// Creates a Host object designed to be used in tests with potentially
    /// mocked data such as host id, databases and context.
    fn mocked() -> Result<Self> {
        let host = soroban_env_host::Host::test_host_with_recording_footprint();
        host.as_budget().reset_unlimited().unwrap();
        host.with_mut_ledger_info(|li| {
            li.protocol_version = 21;
        })?;
        let test_contract = Rc::new(ZephyrTestContract {});
        let contract_id_bytes = [0; 32];
        let contract_address = ScAddress::Contract(Hash(contract_id_bytes));
        let contract_id = host.add_host_object(contract_address)?;

        // Since Soroban's Host relies on a contract to give context to the execution actions
        // performed in the ZephyrVM are connected to a non-existing sample contract address.
        let _ = host.register_test_contract(contract_id, test_contract);

        Ok(Self(Rc::new(HostImpl {
            id: 0,
            network_id: [0; 32],
            transmitter: RefCell::new(None),
            result: RefCell::new(String::new()),
            latest_close: RefCell::new(None),
            database: RefCell::new(Database::mocked()?),
            ledger: Ledger::mocked()?,
            budget: RefCell::new(Budget::zephyr_standard()?),
            entry_point_info: RefCell::new(InvokedFunctionInfo::zephyr_standard()?),
            context: RefCell::new(VmContext::mocked()?),
            stack: RefCell::new(Stack::zephyr_standard()?),
            soroban: RefCell::new(host),
            stack_trace: RefCell::new(Default::default()),
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
pub struct SorobanTempFunctionInfo<
    DB: ZephyrDatabase + Clone + 'static,
    L: LedgerStateRead + 'static,
> {
    /// Module name.
    pub module: &'static str,

    /// Function name.
    pub func: &'static str,

    /// Func object. Contains the function's implementation.
    pub wrapped: fn(&mut Store<Host<DB, L>>) -> Func,
}

#[allow(dead_code)]
impl<DB: ZephyrDatabase + Clone + 'static, L: LedgerStateRead + 'static> Host<DB, L> {
    /// Loads the ledger close meta bytes of the ledger the Zephyr VM will have
    /// access to.
    ///
    /// The ledger close meta is stored as a slice and currenty no type checks occur.
    /// The functions returns a [`HostError::LedgerCloseMetaOverridden`] error when a ledger
    /// close meta is already present in the host object. This is because VMs are not re-usable
    /// between ledgers and need to be created and instantiated for each new invocation to
    /// prevent memory issues.
    pub fn add_ledger_close_meta(&mut self, ledger_close_meta: Vec<u8>) -> Result<()> {
        self.0.stack_trace.borrow_mut().maybe_add_trace(
            TracePoint::ZephyrEnvironment,
            "Adding ledger close meta to ZVM.",
            false,
        );
        let current = &self.0.latest_close;
        if current.borrow().is_some() {
            return Err(HostError::LedgerCloseMetaOverridden.into());
        }

        *current.borrow_mut() = Some(ledger_close_meta);

        Ok(())
    }

    /// Allow configuring the stack trace.
    pub fn set_stack_trace(&mut self, active: bool) {
        if active {
            self.0.stack_trace.borrow_mut().enable();
        } else {
            self.0.stack_trace.borrow_mut().disable();
        }
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
        self.0.stack_trace.borrow_mut().maybe_add_trace(
            TracePoint::ZephyrEnvironment,
            "Loading ZVM context to the host.",
            false,
        );
        let mut vm_context = self.0.context.borrow_mut();

        vm_context.load_vm(vm)
    }

    fn get_stack(&self) -> Result<Vec<i64>> {
        let stack = self.as_stack_mut();
        Ok(stack.0.load())
    }

    fn stack_clear(&self) -> Result<()> {
        self.0.stack_trace.borrow_mut().maybe_add_trace(
            TracePoint::ZephyrEnvironment,
            "Clearing the ZVM's stack.",
            false,
        );
        let mut stack = self.as_stack_mut();
        let stack_impl = stack.0.borrow_mut();
        stack_impl.clear();

        Ok(())
    }

    fn read_ledger_meta(caller: Caller<Self>) -> Result<(i64, i64)> {
        caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
            TracePoint::ZephyrEnvironment,
            "Reading the ledger close meta.",
            false,
        );
        let host = caller.data();
        let ledger_close_meta = {
            let current = host.0.latest_close.borrow();
            current
                .clone()
                .ok_or_else(|| HostError::NoLedgerCloseMeta)?
        };

        Self::write_to_memory(caller, ledger_close_meta).1
    }

    /// Sends a message to any receiver whose sender has been provided to the
    /// host object.
    pub fn send_message(caller: Caller<Self>, offset: i64, size: i64) -> Result<()> {
        caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
            TracePoint::ZephyrEnvironment,
            "Relaying message to inner transmitter.",
            false,
        );
        let host = caller.data();

        let message = {
            let memory = {
                let context = host.0.context.borrow();
                let vm = context
                    .vm
                    .as_ref()
                    .ok_or_else(|| HostError::NoContext)?
                    .upgrade()
                    .ok_or_else(|| HostError::InternalError(InternalError::CannotUpgradeRc))?;
                let mem_manager = &vm.memory_manager;

                mem_manager.memory
            };

            let segment = (offset, size);
            Self::read_segment_from_memory(&memory, &caller, segment)?
        };

        caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
            TracePoint::ZephyrEnvironment,
            "Successfully read user message, sending to transmitter.",
            false,
        );

        let tx = host.0.transmitter.borrow();
        let tx = if let Some(tx) = tx.as_ref() {
            tx
        } else {
            caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                TracePoint::ZephyrEnvironment,
                "Couldn't find transmitter in virtual machine.",
                true,
            );
            return Err(HostError::NoTransmitter.into());
        };

        tx.send(message)?;

        Ok(())
    }

    fn write_result(caller: Caller<Self>, offset: i64, size: i64) -> Result<()> {
        caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
            TracePoint::ZephyrEnvironment,
            "Writing invocation result object.",
            false,
        );
        let host = caller.data();

        let memory = {
            let context = host.0.context.borrow();
            let vm = context
                .vm
                .as_ref()
                .ok_or_else(|| HostError::NoContext)?
                .upgrade()
                .ok_or_else(|| HostError::InternalError(InternalError::CannotUpgradeRc))?;
            let mem_manager = &vm.memory_manager;

            mem_manager.memory
        };

        let segment = (offset, size);
        let seg = Self::read_segment_from_memory(&memory, &caller, segment)?;
        let res: String = bincode::deserialize(&seg)?;

        host.0.result.borrow_mut().push_str(&res);

        Ok(())
    }

    /// Read a result string potentially written from the guest environment.
    pub fn read_result(&self) -> String {
        self.0.result.borrow().clone()
    }

    /// Read the VM's stack trace.
    pub fn read_stack_trace(&self) -> StackTrace {
        self.0.stack_trace.borrow().to_owned()
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
            let wrapped = Func::wrap(&mut store, |caller: Caller<Host<DB, L>>| {
                caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                    TracePoint::DatabaseImpl,
                    format!("Writing to the database implementation."),
                    false,
                );
                let (caller, result) = Self::write_database_raw(caller);
                let res = if let Some(err) = result.err() {
                    caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                        TracePoint::DatabaseImpl,
                        format!(
                            "Hit error {:?} while writing to the database implementation.",
                            err
                        ),
                        true,
                    );
                    ZephyrStatus::from(err) as i64
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
            let wrapped = Func::wrap(&mut store, |caller: Caller<Host<DB, L>>| {
                caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                    TracePoint::DatabaseImpl,
                    format!("Updating to the database implementation."),
                    false,
                );

                let (caller, result) = Self::update_database_raw(caller);
                let res = if let Some(err) = result.err() {
                    caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                        TracePoint::DatabaseImpl,
                        format!(
                            "Hit error {:?} while updating to the database implementation.",
                            err
                        ),
                        true,
                    );
                    ZephyrStatus::from(err) as i64
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
            let db_read_fn_wrapped = Func::wrap(&mut store, |caller: Caller<Host<DB, L>>| {
                caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                    TracePoint::DatabaseImpl,
                    format!("Reading from the database implementation."),
                    false,
                );

                let (caller, result) = Host::read_database_self(caller);
                let res = if let Some(err) = result.err() {
                    caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                        TracePoint::DatabaseImpl,
                        format!(
                            "Hit error {:?} while updating to the database implementation.",
                            err
                        ),
                        true,
                    );
                    ZephyrStatus::from(err) as i64
                } else {
                    ZephyrStatus::Success as i64
                };
            });

            FunctionInfo {
                module: "env",
                func: "read_raw",
                wrapped: db_read_fn_wrapped,
            }
        };

        let db_read_as_id_fn = {
            let db_read_fn_wrapped =
                Func::wrap(&mut store, |caller: Caller<Host<DB, L>>, id: i64| {
                    let (caller, result) = Host::read_database_as_id(caller, id);
                    if let Ok(res) = result {
                        (ZephyrStatus::Success as i64, res.0, res.1)
                    } else {
                        (ZephyrStatus::from(result.err().unwrap()) as i64, 0, 0)
                    }
                });

            FunctionInfo {
                module: "env",
                func: "read_as_id",
                wrapped: db_read_fn_wrapped,
            }
        };

        let read_contract_data_entry_by_contract_id_and_key_fn = {
            let wrapped = Func::wrap(
                &mut store,
                |caller: Caller<Host<DB, L>>,
                 contract_part_1: i64,
                 contract_part_2: i64,
                 contract_part_3: i64,
                 contract_part_4: i64,
                 offset: i64,
                 size: i64| {
                    let contract = WrappedMaxBytes::array_from_max_parts::<32>(&[
                        contract_part_1,
                        contract_part_2,
                        contract_part_3,
                        contract_part_4,
                    ]);

                    caller.data().0.stack_trace.borrow_mut().maybe_add_trace(TracePoint::LedgerImpl, format!("Reading contract data entry for contract {:?} and key with size of {}.", contract, size), false);

                    let (caller, result) = Host::read_contract_data_entry_by_contract_id_and_key(
                        caller, contract, offset, size,
                    );

                    if let Ok(res) = result {
                        (ZephyrStatus::Success as i64, res.0, res.1)
                    } else {
                        (ZephyrStatus::from(result.err().unwrap()) as i64, 0, 0)
                    }
                },
            );

            FunctionInfo {
                module: "env",
                func: "read_contract_data_entry_by_contract_id_and_key",
                wrapped,
            }
        };

        let read_contract_instance_fn = {
            let wrapped = Func::wrap(
                &mut store,
                |caller: Caller<Host<DB, L>>,
                 contract_part_1: i64,
                 contract_part_2: i64,
                 contract_part_3: i64,
                 contract_part_4: i64| {
                    let contract = WrappedMaxBytes::array_from_max_parts::<32>(&[
                        contract_part_1,
                        contract_part_2,
                        contract_part_3,
                        contract_part_4,
                    ]);

                    caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                        TracePoint::LedgerImpl,
                        format!("Reading contract instance for contract {:?}.", contract),
                        false,
                    );

                    let (caller, result) = Host::read_contract_instance(caller, contract);

                    if let Ok(res) = result {
                        (ZephyrStatus::Success as i64, res.0, res.1)
                    } else {
                        (ZephyrStatus::from(result.err().unwrap()) as i64, 0, 0)
                    }
                },
            );

            FunctionInfo {
                module: "env",
                func: "read_contract_instance",
                wrapped,
            }
        };

        let read_contract_entries_fn = {
            let wrapped = Func::wrap(
                &mut store,
                |caller: Caller<Host<DB, L>>,
                 contract_part_1: i64,
                 contract_part_2: i64,
                 contract_part_3: i64,
                 contract_part_4: i64| {
                    let contract = WrappedMaxBytes::array_from_max_parts::<32>(&[
                        contract_part_1,
                        contract_part_2,
                        contract_part_3,
                        contract_part_4,
                    ]);

                    caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                        TracePoint::LedgerImpl,
                        format!(
                            "Reading all non-instance contract entries for contract {:?}.",
                            contract
                        ),
                        false,
                    );

                    let (caller, result) = Host::read_contract_entries(caller, contract);

                    if let Ok(res) = result {
                        (ZephyrStatus::Success as i64, res.0, res.1)
                    } else {
                        (ZephyrStatus::from(result.err().unwrap()) as i64, 0, 0)
                    }
                },
            );

            FunctionInfo {
                module: "env",
                func: "read_contract_entries_by_contract",
                wrapped,
            }
        };

        let read_contract_entries_to_env_fn = {
            let wrapped = Func::wrap(
                &mut store,
                |caller: Caller<Host<DB, L>>,
                 contract_part_1: i64,
                 contract_part_2: i64,
                 contract_part_3: i64,
                 contract_part_4: i64| {
                    let contract = WrappedMaxBytes::array_from_max_parts::<32>(&[
                        contract_part_1,
                        contract_part_2,
                        contract_part_3,
                        contract_part_4,
                    ]);

                    caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                        TracePoint::LedgerImpl,
                        format!(
                            "Reading to soroban value all contract entries for contract {:?}.",
                            contract
                        ),
                        false,
                    );

                    let result = Host::read_contract_entries_to_env(caller, contract);

                    if let Ok(res) = result {
                        (ZephyrStatus::Success as i64, res)
                    } else {
                        (ZephyrStatus::from(result.err().unwrap()) as i64, 0)
                    }
                },
            );

            FunctionInfo {
                module: "env",
                func: "read_contract_entries_by_contract_to_env",
                wrapped,
            }
        };

        let read_account_from_ledger_fn = {
            let wrapped = Func::wrap(
                &mut store,
                |caller: Caller<Host<DB, L>>,
                 account_part_1: i64,
                 account_part_2: i64,
                 account_part_3: i64,
                 account_part_4: i64| {
                    let account = WrappedMaxBytes::array_from_max_parts::<32>(&[
                        account_part_1,
                        account_part_2,
                        account_part_3,
                        account_part_4,
                    ]);

                    caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                        TracePoint::LedgerImpl,
                        format!("Fetching account {:?} from the ledger.", account),
                        false,
                    );

                    let (caller, result) = Host::read_account_object(caller, account);

                    if let Ok(res) = result {
                        (ZephyrStatus::Success as i64, res.0, res.1)
                    } else {
                        (ZephyrStatus::from(result.err().unwrap()) as i64, 0, 0)
                    }
                },
            );

            FunctionInfo {
                module: "env",
                func: "read_account_from_ledger",
                wrapped,
            }
        };

        let scval_to_valid_host_val = {
            let wrapped = Func::wrap(
                &mut store,
                |caller: Caller<Host<DB, L>>, offset: i64, size: i64| {
                    let bytes = {
                        let host: &Self = caller.data();
                        let memory = {
                            let context = host.0.context.borrow();
                            let vm = context
                                .vm
                                .as_ref()
                                .ok_or_else(|| HostError::NoContext)
                                .unwrap()
                                .upgrade()
                                .ok_or_else(|| {
                                    HostError::InternalError(InternalError::CannotUpgradeRc)
                                })
                                .unwrap();
                            let mem_manager = &vm.memory_manager;

                            mem_manager.memory
                        };

                        let segment = (offset, size);
                        Self::read_segment_from_memory(&memory, &caller, segment).unwrap()
                    };

                    caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                        TracePoint::SorobanEnvironment,
                        format!("Building ScVal from bytes {:?}.", bytes),
                        false,
                    );
                    let scval = ScVal::from_xdr(bytes, Limits::none()).unwrap();

                    caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                        TracePoint::SorobanEnvironment,
                        format!("Converting ScVal {:?} to a valid host value.", scval),
                        false,
                    );
                    let (caller, result) = Host::scval_to_valid_host_val(caller, scval.clone());

                    if let Ok(res) = result {
                        (ZephyrStatus::Success as i64, res)
                    } else {
                        let error = result.err();
                        caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                            TracePoint::SorobanEnvironment,
                            format!(
                                "Hit error {:?} while converting ScVal {:?} to a valid host value.",
                                error, scval
                            ),
                            true,
                        );
                        (ZephyrStatus::from(error.unwrap()) as i64, 0)
                    }
                },
            );

            FunctionInfo {
                module: "env",
                func: "scval_to_valid_host_val",
                wrapped,
            }
        };

        let valid_host_val_to_scval = {
            let wrapped = Func::wrap(&mut store, |caller: Caller<Host<DB, L>>, val: i64| {
                caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                    TracePoint::SorobanEnvironment,
                    format!("Converting host val {:?} to ScVal.", val),
                    false,
                );
                let (caller, result) =
                    Host::valid_host_val_to_scval(caller, Val::from_payload(val as u64));

                if let Ok(res) = result {
                    (ZephyrStatus::Success as i64, res.0, res.1)
                } else {
                    let error = result.err();
                    caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                        TracePoint::SorobanEnvironment,
                        format!(
                            "Hit error {} while converting host val {:?} to ScVal.",
                            error.as_ref().unwrap(),
                            val
                        ),
                        true,
                    );
                    (ZephyrStatus::from(error.unwrap()) as i64, 0, 0)
                }
            });

            FunctionInfo {
                module: "env",
                func: "valid_host_val_to_scval",
                wrapped,
            }
        };

        let conclude_fn = {
            let wrapped = Func::wrap(
                &mut store,
                |caller: Caller<Host<DB, L>>, offset: i64, size: i64| {
                    caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                        TracePoint::ZephyrEnvironment,
                        format!("Writing object of size {:?} to result slot.", size),
                        false,
                    );
                    Host::write_result(caller, offset, size).unwrap();
                },
            );

            FunctionInfo {
                module: "env",
                func: "conclude",
                wrapped,
            }
        };

        let send_message_fn = {
            let wrapped = Func::wrap(
                &mut store,
                |caller: Caller<Host<DB, L>>, offset: i64, size: i64| {
                    let result = Host::send_message(caller, offset, size);

                    if let Ok(_) = result {
                        ZephyrStatus::Success as i64
                    } else {
                        ZephyrStatus::from(result.err().unwrap()) as i64
                    }
                },
            );

            FunctionInfo {
                module: "env",
                func: "tx_send_message",
                wrapped,
            }
        };

        let log_fn = {
            let wrapped = Func::wrap(&mut store, |_: Caller<Host<DB, L>>, param: i64| {
                println!("Logged: {}", param);
            });

            FunctionInfo {
                module: "env",
                func: "zephyr_logger",
                wrapped,
            }
        };

        let stack_push_fn = {
            let wrapped = Func::wrap(&mut store, |caller: Caller<Host<DB, L>>, param: i64| {
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
            let wrapped = Func::wrap(&mut store, |caller: Caller<Host<DB, L>>| {
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
            let wrapped = Func::wrap(
                &mut store,
                |caller: Caller<Host<DB, L>>, lm_pos: i64, len: i64| {
                    let vm_ctx = CustomVMCtx::new(&caller);
                    let host: soroban_env_host::Host = Host::<DB, L>::soroban_host(&caller);

                    caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                        TracePoint::SorobanEnvironment,
                        "Creating soroban string from ZVM linear memory.",
                        false,
                    );

                    let effect = |host: soroban_env_host::Host| {
                        let result: Result<_, soroban_env_host::HostError> = host
                            .string_new_from_linear_memory_mem(
                                vm_ctx,
                                build_u32val(&host, lm_pos)?,
                                build_u32val(&host, len)?,
                            );

                        with_frame(host, result)
                    };

                    let val = effect(host);
                    match val {
                        Ok(val) => val.get_payload() as i64,
                        Err(host_error) => {
                            caller.data().0.stack_trace.borrow_mut().maybe_add_trace(TracePoint::SorobanEnvironment, format!("Hit error {:?} while reating soroban string from ZVM linear memory.", host_error), true);
                            // todo log error.
                            // Note: this will panic on the guest.
                            0
                        }
                    }
                },
            );

            FunctionInfo {
                module: "b",
                func: "i",
                wrapped,
            }
        };

        let symbol_from_linmem = {
            let wrapped = Func::wrap(
                &mut store,
                |caller: Caller<Host<DB, L>>, lm_pos: i64, len: i64| {
                    let vm_ctx = CustomVMCtx::new(&caller);
                    let host: soroban_env_host::Host = Host::<DB, L>::soroban_host(&caller);

                    caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                        TracePoint::SorobanEnvironment,
                        format!("Creating soroban symbol from ZVM linear memory."),
                        false,
                    );

                    let effect = |host: soroban_env_host::Host| {
                        let result: Result<_, soroban_env_host::HostError> = host
                            .symbol_new_from_linear_memory_mem(
                                vm_ctx,
                                build_u32val(&host, lm_pos)?,
                                build_u32val(&host, len)?,
                            );

                        with_frame(host, result)
                    };

                    let val = effect(host);
                    match val {
                        Ok(val) => val.get_payload() as i64,
                        Err(host_error) => {
                            caller.data().0.stack_trace.borrow_mut().maybe_add_trace(TracePoint::SorobanEnvironment, format!("Hit error {:?} while creating soroban string from ZVM linear memory.", host_error), true);
                            // todo log error.
                            // Note: this will panic on the guest.
                            0
                        }
                    }
                },
            );

            FunctionInfo {
                module: "b",
                func: "j",
                wrapped,
            }
        };

        let symbol_index_from_linmem = {
            let wrapped = Func::wrap(
                &mut store,
                |caller: Caller<Host<DB, L>>, sym: i64, lm_pos: i64, len: i64| {
                    let vm_ctx = CustomVMCtx::new(&caller);
                    let host: soroban_env_host::Host = Host::<DB, L>::soroban_host(&caller);

                    caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                        TracePoint::SorobanEnvironment,
                        "Finding soroban symbol in ZVM linear memory slices.",
                        false,
                    );

                    let effect = |host: soroban_env_host::Host| {
                        let res: Result<_, soroban_env_host::HostError> = host
                            .symbol_index_in_linear_memory_mem(
                                vm_ctx,
                                Symbol::check_env_arg(
                                    Symbol::try_marshal_from_relative_value(
                                        soroban_wasmi::Value::I64(sym),
                                        &host,
                                    )
                                    .unwrap(),
                                    &host,
                                )
                                .unwrap(),
                                build_u32val(&host, lm_pos)?,
                                build_u32val(&host, len)?,
                            );

                        with_frame(host, res)
                    };

                    let val = effect(host);
                    match val {
                        Ok(val) => val.get_payload() as i64,
                        Err(host_error) => {
                            caller.data().0.stack_trace.borrow_mut().maybe_add_trace(TracePoint::SorobanEnvironment, format!("Hit error {:?} while finding soroban symbol in ZVM linear memory slices.", host_error), true);
                            // todo log error.
                            // Note: this will panic on the guest.
                            0
                        }
                    }
                },
            );

            FunctionInfo {
                module: "b",
                func: "m",
                wrapped,
            }
        };

        let vec_new_from_linear_memory_mem = {
            let wrapped = Func::wrap(
                &mut store,
                |caller: Caller<Host<DB, L>>, lm_pos: i64, len: i64| {
                    let vm_ctx = CustomVMCtx::new(&caller);
                    let host: soroban_env_host::Host = Host::<DB, L>::soroban_host(&caller);

                    caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                        TracePoint::SorobanEnvironment,
                        format!("Creating soroban vector from ZVM linear memory."),
                        false,
                    );

                    let effect = |host: soroban_env_host::Host| {
                        let res: Result<_, soroban_env_host::HostError> = host
                            .vec_new_from_linear_memory_mem(
                                vm_ctx,
                                build_u32val(&host, lm_pos)?,
                                build_u32val(&host, len)?,
                            );

                        with_frame(host, res)
                    };

                    let val = effect(host);
                    match val {
                        Ok(val) => val.get_payload() as i64,
                        Err(host_error) => {
                            caller.data().0.stack_trace.borrow_mut().maybe_add_trace(TracePoint::SorobanEnvironment, format!("Hit error {:?} while creating soroban vector from ZVM linear memory.", host_error), true);

                            // todo log error.
                            // Note: this will panic on the guest.
                            0
                        }
                    }
                },
            );

            FunctionInfo {
                module: "v",
                func: "g",
                wrapped,
            }
        };

        let map_new_from_linear_memory_mem = {
            let wrapped = Func::wrap(
                &mut store,
                |caller: Caller<Host<DB, L>>, key_pos: i64, val_pos: i64, len: i64| {
                    let vm_ctx = CustomVMCtx::new(&caller);
                    let host: soroban_env_host::Host = Host::<DB, L>::soroban_host(&caller);
                    let effect = |host: soroban_env_host::Host| {
                        caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                            TracePoint::SorobanEnvironment,
                            format!("Creating soroban map from ZVM linear memory."),
                            false,
                        );

                        let res: Result<_, soroban_env_host::HostError> = host
                            .map_new_from_linear_memory_mem(
                                vm_ctx,
                                build_u32val(&host, key_pos)?,
                                build_u32val(&host, val_pos)?,
                                build_u32val(&host, len)?,
                            );

                        with_frame(host, res)
                    };

                    let val = effect(host);
                    match val {
                        Ok(val) => val.get_payload() as i64,
                        Err(host_error) => {
                            caller.data().0.stack_trace.borrow_mut().maybe_add_trace(TracePoint::SorobanEnvironment, format!("Hit error {:?} while creating soroban map from ZVM linear memory.", host_error), true);
                            // todo log error.
                            // Note: this will panic on the guest.
                            0
                        }
                    }
                },
            );

            FunctionInfo {
                module: "m",
                func: "9",
                wrapped,
            }
        };

        let bytes_new_from_linear_memory_mem = {
            let wrapped = Func::wrap(
                &mut store,
                |caller: Caller<Host<DB, L>>, lm_pos: i64, len: i64| {
                    let vm_ctx = CustomVMCtx::new(&caller);
                    let host: soroban_env_host::Host = Host::<DB, L>::soroban_host(&caller);

                    caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                        TracePoint::SorobanEnvironment,
                        format!("Creating soroban bytes from ZVM linear memory."),
                        false,
                    );

                    let effect = |host: soroban_env_host::Host| {
                        let res: Result<_, soroban_env_host::HostError> = host
                            .bytes_new_from_linear_memory_mem(
                                vm_ctx,
                                build_u32val(&host, lm_pos)?,
                                build_u32val(&host, len)?,
                            );
                        with_frame(host, res)
                    };

                    let val = effect(host);
                    match val {
                        Ok(val) => val.get_payload() as i64,
                        Err(host_error) => {
                            caller.data().0.stack_trace.borrow_mut().maybe_add_trace(TracePoint::SorobanEnvironment, format!("Hit error {:?} while creating soroban bytes from ZVM linear memory.", host_error), true);
                            // todo log error.
                            // Note: this will panic on the guest.
                            0
                        }
                    }
                },
            );

            FunctionInfo {
                module: "b",
                func: "3",
                wrapped,
            }
        };

        let bytes_copy_to_linear_memory_mem = {
            let wrapped = Func::wrap(
                &mut store,
                |caller: Caller<Host<DB, L>>, b: i64, b_pos: i64, lm_pos: i64, len: i64| {
                    let host: soroban_env_host::Host = Host::<DB, L>::soroban_host(&caller);

                    caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                        TracePoint::SorobanEnvironment,
                        format!("Copying soroban bytes to ZVM linear memory."),
                        false,
                    );

                    let effect = |host: soroban_env_host::Host| {
                        let mut vm_ctx = CustomVMCtx::new_mut(caller);
                        let res: Result<_, soroban_env_host::HostError> = (|| {
                            let bytes_obj = BytesObject::check_env_arg(
                                BytesObject::try_marshal_from_relative_value(
                                    soroban_wasmi::Value::I64(b),
                                    &host,
                                )
                                .unwrap(),
                                &host,
                            )?;

                            let b_pos_val = build_u32val(&host, b_pos)?;
                            let lm_pos_val = build_u32val(&host, lm_pos)?;
                            let len_val = build_u32val(&host, len)?;

                            host.bytes_copy_to_linear_memory_mem(
                                &mut vm_ctx,
                                bytes_obj,
                                b_pos_val,
                                lm_pos_val,
                                len_val,
                            )
                        })(
                        );

                        match with_frame(host, res) {
                            Ok(val) => Ok((vm_ctx.into_inner(), val)),
                            Err(host_error) => Err((vm_ctx.into_inner(), host_error)),
                        }
                    };

                    let val = effect(host);
                    match val {
                        Ok((_maybe_vm_ctx, val)) => val.get_payload() as i64,
                        Err((maybe_caller, host_error)) => {
                            if let Some(caller) = maybe_caller {
                                caller.data().0.stack_trace.borrow_mut().maybe_add_trace(TracePoint::SorobanEnvironment, format!("Hit error {:?} while creating soroban bytes from ZVM linear memory.", host_error), true);
                            };

                            // todo log error.
                            // Note: this will panic on the guest.
                            0
                        }
                    }
                },
            );

            FunctionInfo {
                module: "b",
                func: "1",
                wrapped,
            }
        };

        let map_unpack_to_linear_memory_fn_mem = {
            let wrapped = Func::wrap(
                &mut store,
                |caller: Caller<Host<DB, L>>, map: i64, keys_pos: i64, vals_pos: i64, len: i64| {
                    let host: soroban_env_host::Host = Host::<DB, L>::soroban_host(&caller);

                    caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                        TracePoint::SorobanEnvironment,
                        format!("Unpacking soroban map to ZVM linear memory."),
                        false,
                    );

                    let effect = |host: soroban_env_host::Host| {
                        let mut vm_ctx = CustomVMCtx::new_mut(caller);
                        let res: Result<_, soroban_env_host::HostError> = (|| {
                            host.map_unpack_to_linear_memory_fn_mem(
                                &mut vm_ctx,
                                MapObject::check_env_arg(
                                    MapObject::try_marshal_from_relative_value(
                                        soroban_wasmi::Value::I64(map),
                                        &host,
                                    )
                                    .unwrap(),
                                    &host,
                                )
                                .unwrap(),
                                build_u32val(&host, keys_pos)?,
                                build_u32val(&host, vals_pos)?,
                                build_u32val(&host, len)?,
                            )
                        })(
                        );

                        match with_frame(host, res) {
                            Ok(val) => Ok((vm_ctx.into_inner(), val)),
                            Err(host_error) => Err((vm_ctx.into_inner(), host_error)),
                        }
                    };

                    let val = effect(host);
                    match val {
                        Ok((_maybe_vm_ctx, val)) => val.get_payload() as i64,
                        Err((maybe_caller, host_error)) => {
                            if let Some(caller) = maybe_caller {
                                caller.data().0.stack_trace.borrow_mut().maybe_add_trace(TracePoint::SorobanEnvironment, format!("Hit error {:?} while unpacking soroban map to ZVM linear memory.", host_error), true);
                            };

                            // todo log error.
                            // Note: this will panic on the guest.
                            0
                        }
                    }
                },
            );

            FunctionInfo {
                module: "m",
                func: "a",
                wrapped,
            }
        };

        let vec_unpack_to_linear_memory_fn_mem = {
            let wrapped = Func::wrap(
                &mut store,
                |caller: Caller<Host<DB, L>>, vec: i64, vals_pos: i64, len: i64| {
                    let host: soroban_env_host::Host = Host::<DB, L>::soroban_host(&caller);

                    caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                        TracePoint::SorobanEnvironment,
                        format!("Unpacking soroban vector to ZVM linear memory."),
                        false,
                    );

                    let effect = |host: soroban_env_host::Host| {
                        let mut vm_ctx = CustomVMCtx::new_mut(caller);
                        let res: Result<_, soroban_env_host::HostError> = (|| {
                            host.vec_unpack_to_linear_memory_mem(
                                &mut vm_ctx,
                                VecObject::check_env_arg(
                                    VecObject::try_marshal_from_relative_value(
                                        soroban_wasmi::Value::I64(vec),
                                        &host,
                                    )
                                    .unwrap(),
                                    &host,
                                )
                                .unwrap(),
                                build_u32val(&host, vals_pos)?,
                                build_u32val(&host, len)?,
                            )
                        })(
                        );

                        match with_frame(host, res) {
                            Ok(val) => Ok((vm_ctx.into_inner(), val)),
                            Err(host_error) => Err((vm_ctx.into_inner(), host_error)),
                        }
                    };

                    let val = effect(host);
                    match val {
                        Ok((_maybe_vm_ctx, val)) => val.get_payload() as i64,
                        Err((maybe_caller, host_error)) => {
                            if let Some(caller) = maybe_caller {
                                caller.data().0.stack_trace.borrow_mut().maybe_add_trace(TracePoint::SorobanEnvironment, format!("Hit error {:?} while creating soroban bytes from ZVM linear memory.", host_error), true);
                            };

                            // todo log error.
                            // Note: this will panic on the guest.
                            0
                        }
                    }
                },
            );

            FunctionInfo {
                module: "v",
                func: "h",
                wrapped,
            }
        };

        let soroban_simulate_tx_fn = {
            let wrapped = Func::wrap(
                &mut store,
                |caller: Caller<Host<DB, L>>,
                 account_part_1: i64,
                 account_part_2: i64,
                 account_part_3: i64,
                 account_part_4: i64,
                 offset: i64,
                 size: i64| {
                    let source = WrappedMaxBytes::array_from_max_parts::<32>(&[
                        account_part_1,
                        account_part_2,
                        account_part_3,
                        account_part_4,
                    ]);

                    let (caller, result) =
                        Host::simulate_soroban_transaction(caller, source, offset, size);
                    if let Ok(res) = result {
                        (ZephyrStatus::Success as i64, res.0, res.1)
                    } else {
                        (ZephyrStatus::from(result.err().unwrap()) as i64, 0, 0)
                    }
                },
            );

            FunctionInfo {
                module: "env",
                func: "soroban_simulate_tx",
                wrapped,
            }
        };

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
            string_from_linmem,
            symbol_index_from_linmem,
            vec_new_from_linear_memory_mem,
            bytes_new_from_linear_memory_mem,
            symbol_from_linmem,
            map_unpack_to_linear_memory_fn_mem,
            vec_unpack_to_linear_memory_fn_mem,
            soroban_simulate_tx_fn,
            bytes_copy_to_linear_memory_mem,
            db_read_as_id_fn,
            read_account_from_ledger_fn,
            map_new_from_linear_memory_mem,
        ];

        soroban_functions.append(&mut arr);
        soroban_functions.reverse();

        soroban_functions
    }
}
