//! Structures and implementations for the Zephyr Virtual Machine.
//!

use anyhow::{Result, anyhow};
use rs_zephyr_common::ContractDataEntry;
use stellar_xdr::next::{LedgerEntry, VecM};
use std::{cell::RefCell, rc::Rc};
use wasmi::{Engine, Instance, Linker, Memory, Module, StackLimits, Store, Value};

use crate::{db::{database::ZephyrDatabase, ledger::LedgerStateRead}, error::HostError, host::{InvokedFunctionInfo, Host}};


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
        let stack_limits = StackLimits::new(MIN_VALUE_STACK_HEIGHT, MAX_VALUE_STACK_HEIGHT, MAX_RECURSION_DEPTH).unwrap();

        // TODO: decide which post-mvp features to override.
        // For now we use wasmtime's defaults.
        config.consume_fuel(true);
        config.set_stack_limits(stack_limits);
        
        let engine = Engine::new(&config);
        let module = Module::new(&engine, wasm_module_code_bytes)?;


        let mut store = Store::new(&engine, host.clone());
        if let Err(error) = host.as_budget().infer_fuel(&mut store) {
            return Err(anyhow!(error))
        };

        // TODO: set Store::limiter() once host implements ResourceLimiter

        let mut linker = <Linker<Host<DB, L>>>::new(&engine);
        
        for func_info in host.host_functions(&mut store) {
            // Note: this is just a current workaround.
            let _ = linker.define(
                func_info.module,
                func_info.func,
                func_info.wrapped,
            );
        }
        
        // NOTE
        // We are not starting instance already.
        let instance = linker.instantiate(&mut store, &module)?;
        let instance  = instance.start(&mut store)?; // handle
        let memory = instance
            .get_export(&mut store, "memory")
            .unwrap()
            .into_memory()
            .unwrap();
        
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

        println!("\nrunning function\n");
        func.call(
            &mut *store.borrow_mut(),
            entry_point_info.params.as_slice(),
            &mut retrn,
        )?;

        Ok(())
    }

    pub fn metered_function_call(self: &Rc<Self>, host: &Host<DB, L>, fname: &str) -> Result<String> {
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

        let _ = func.call(
            &mut *store.borrow_mut(),
            invoked_function_info.params.as_slice(),
            &mut retrn,
        );

        println!("{:?}",host.read_result());

        Ok(host.read_result())
    }
}
/* 
#[cfg(test)]
mod tests {
    use std::fs::{read, read_to_string};
    use std::rc::Rc;
    use stellar_xdr::curr::{Limits, LedgerCloseMeta, ReadXdr, WriteXdr};
    use crate::{host::Host, testutils::database::MercuryDatabase, ZephyrMock};

    use super::Vm;
    // Previous tests were removed due to being unstructured
    // and unorganized.
    //
    // Tests of the Mercury integration are currently in Mercury's 
    // codebase.
    //
    // TODO: rewrite Zephyr-only tests.

    #[test]
    fn mainnet_ledger() {
        // todo rewrite test with proper configs
        let code = { read("/mnt/storagehdd/projects/master/zephyr-examples/zephyr-hello-ledger/target/wasm32-unknown-unknown/release/zephyr_hello_ledger.wasm").unwrap() };
        let mainnet_ledger = LedgerCloseMeta::from_xdr_base64(read_to_string("../../mercury/ledger.txt").unwrap(), Limits::none()).unwrap().to_xdr(Limits::none()).unwrap();
        
        assert!(LedgerCloseMeta::from_xdr(&mainnet_ledger, Limits::none()).is_ok());
        let mut host = Host::<MercuryDatabase>::mocked().unwrap();
        host.add_ledger_close_meta(mainnet_ledger).unwrap();

        let start = std::time::Instant::now();

        let vm = Vm::new(&host, code.as_slice()).unwrap();

        host.load_context(Rc::downgrade(&vm)).unwrap();

        vm.metered_call(&host).unwrap();

        println!("elapsed {:?}", start.elapsed());
    }
}*/