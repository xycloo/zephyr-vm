//! Structures and implementations for the Zephyr Virtual Machine.
//!

use anyhow::Result;
use std::{cell::RefCell, rc::Rc};
use wasmtime::{Engine, Instance, Linker, Memory, Module, Store};

use crate::{db::database::ZephyrDatabase, error::HostError, host::Host};

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
pub struct Vm<DB: ZephyrDatabase> {
    /// Module object.
    #[allow(dead_code)]
    module: Module, // currently not used.

    /// VM's store object. Provides bindings to the host.
    pub store: RefCell<Store<Host<DB>>>,

    /// Memory manager.
    pub memory_manager: MemoryManager,

    instance: Instance,
}

#[allow(dead_code)]
impl<DB: ZephyrDatabase + Clone> Vm<DB> {
    /// Creates and instantiates the VM.
    pub fn new(host: &Host<DB>, wasm_module_code_bytes: &[u8]) -> Result<Rc<Self>> {
        let mut config = wasmtime::Config::default();

        // TODO: decide which post-mvp features to override.
        // For now we use wasmtime's defaults.
        config.consume_fuel(true);

        let engine = Engine::new(&config)?;
        let module = Module::new(&engine, wasm_module_code_bytes)?;

        let mut store = Store::new(&engine, host.clone());
        host.as_budget().infer_fuel(&mut store)?;

        // TODO: set Store::limiter() once host implements ResourceLimiter

        let mut linker = <Linker<Host<DB>>>::new(&engine);

        for func_info in host.host_functions(&mut store) {
            linker.define(
                &mut store,
                func_info.module,
                func_info.func,
                func_info.wrapped,
            )?;
        }

        // NOTE
        // We are not starting instance already.
        let instance = linker.instantiate(&mut store, &module)?;
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
    /// By default, the called function is defined in the host as the EntryPointInfo.
    /// The function itself won't return anything but will have access to the Database
    /// implementation and the ledger metadata through Host bindings.
    pub fn metered_call(self: &Rc<Self>, host: &Host<DB>) -> Result<()> {
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
}

#[cfg(test)]
mod tests {
    use std::fs::read;
    use std::rc::Rc;

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
    fn alloc_invocation() {
        let code = { read("/Users/tommasodeponti/Desktop/projects/master/zephyr-examples/zephyr-track-all-sac/target/wasm32-unknown-unknown/release/zephyr_track_all_sac.wasm").unwrap() };

        let host = Host::<MercuryDatabase>::mocked().unwrap();

        let start = std::time::Instant::now();

        let vm = Vm::new(&host, code.as_slice()).unwrap();

        host.load_context(Rc::clone(&vm)).unwrap();

        //vm.metered_call(&host).unwrap();

        println!("elapsed {:?}", start.elapsed());
    }
}
