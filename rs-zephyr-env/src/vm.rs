//! Structures and implementations for the Zephyr Virtual Machine.
//!

use anyhow::{anyhow, Result};
use std::{cell::RefCell, rc::Rc};
use wasmi::{Engine, Instance, Linker, Memory, Module, StackLimits, Store};

use crate::{
    db::{database::ZephyrDatabase, ledger::LedgerStateRead},
    error::{HostError, InternalError},
    host::{Host, InvokedFunctionInfo},
};

const MIN_VALUE_STACK_HEIGHT: usize = 1024;

// Allowing for more stack height than default. Currently shouldn't be
// required by most programs, but better to keep these configurable on our
// end
const MAX_VALUE_STACK_HEIGHT: usize = 2 * 1024 * MIN_VALUE_STACK_HEIGHT;
const MAX_RECURSION_DEPTH: usize = 1024;

/// MemoryManager object. Stored in the VM object.
#[derive(Clone)]
pub struct MemoryManager {
    /// VM memory object.
    pub memory: Memory,

    /// Latest written offset to the module's memory.
    /// This value is updated for every time the memory is written.
    pub offset: RefCell<usize>,
}

impl MemoryManager {
    /// Creates a new memory manager offset.
    pub fn new(memory: Memory, offset: usize) -> Self {
        Self {
            memory,
            offset: RefCell::new(offset),
        }
    }
}

/// The Zephyr VM.
pub struct Vm<DB: ZephyrDatabase, L: LedgerStateRead> {
    /// Module object.
    #[allow(dead_code)]
    module: Module, // currently not used.

    /// VM's store object. Provides bindings to the host.
    pub store: RefCell<Store<Host<DB, L>>>,

    /// Memory manager.
    pub memory_manager: MemoryManager,

    instance: Instance,
}

#[allow(dead_code)]
impl<DB: ZephyrDatabase + Clone + 'static, L: LedgerStateRead + Clone + 'static> Vm<DB, L> {
    /// Creates and instantiates the VM.
    pub fn new(host: &Host<DB, L>, wasm_module_code_bytes: &[u8]) -> Result<Rc<Self>> {
        let mut config = wasmi::Config::default();
        let stack_limits = StackLimits::new(
            MIN_VALUE_STACK_HEIGHT,
            MAX_VALUE_STACK_HEIGHT,
            MAX_RECURSION_DEPTH,
        )
        .map_err(|_| HostError::InternalError(InternalError::WasmiConfig))?;

        // TODO: decide which post-mvp features to override.
        // For now we use wasmtime's defaults.
        config.consume_fuel(true);
        config.set_stack_limits(stack_limits);

        let engine = Engine::new(&config);
        let module = Module::new(&engine, wasm_module_code_bytes)?;

        let mut store = Store::new(&engine, host.clone());
        if let Err(error) = host.as_budget().infer_fuel(&mut store) {
            return Err(anyhow!(error));
        };

        // TODO: set Store::limiter() once host implements ResourceLimiter

        let mut linker = <Linker<Host<DB, L>>>::new(&engine);

        for func_info in host.host_functions(&mut store) {
            // Note: this is just a current workaround.
            let _ = linker.define(func_info.module, func_info.func, func_info.wrapped);
        }

        // NOTE
        // We are not starting instance already.
        let instance = linker.instantiate(&mut store, &module)?;
        let instance = instance.start(&mut store)?; // handle
        let memory = instance
            .get_export(&mut store, "memory")
            .ok_or_else(|| HostError::NoMemoryExport)?
            .into_memory()
            .ok_or_else(|| HostError::NoMemoryExport)?;

        let memory_manager = MemoryManager::new(memory, 0);

        Ok(Rc::new(Self {
            module,
            store: RefCell::new(store),
            memory_manager,
            instance,
        }))
    }

    /// Entry point of a Zephyr VM invocation.
    /// By default, the called function is defined in the host as the InvokedFunctionInfo.
    /// The function itself won't return anything but will have access to the Database
    /// implementation and the ledger metadata through Host bindings.
    pub fn metered_call(self: &Rc<Self>, host: &Host<DB, L>) -> Result<()> {
        let store = &self.store;
        let entry_point_info = host.get_entry_point_info();
        let mut retrn = entry_point_info.retrn.clone();

        let ext = match self
            .instance
            .get_export(&mut *store.borrow_mut(), &entry_point_info.fname)
        {
            Some(ext) => ext,
            None => return Err(HostError::NoEntryPointExport.into()),
        };

        let func = match ext.into_func() {
            Some(func) => func,
            None => return Err(HostError::ExternNotAFunction.into()),
        };

        func.call(
            &mut *store.borrow_mut(),
            entry_point_info.params.as_slice(),
            &mut retrn,
        )?;

        Ok(())
    }

    /// Executes the requested exported function of the binary.
    pub fn metered_function_call(
        self: &Rc<Self>,
        host: &Host<DB, L>,
        fname: &str,
    ) -> Result<String> {
        let invoked_function_info = InvokedFunctionInfo::serverless_defaults(fname);

        let store: &RefCell<Store<Host<DB, L>>> = &self.store;
        let mut retrn = invoked_function_info.retrn.clone();

        let ext = match self
            .instance
            .get_export(&mut *store.borrow_mut(), &invoked_function_info.fname)
        {
            Some(ext) => ext,
            None => return Err(HostError::NoEntryPointExport.into()),
        };

        let func = match ext.into_func() {
            Some(func) => func,
            None => return Err(HostError::ExternNotAFunction.into()),
        };

        func.call(
            &mut *store.borrow_mut(),
            invoked_function_info.params.as_slice(),
            &mut retrn,
        )?;

        Ok(host.read_result())
    }
}
