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
mod otf_test {
    use std::fs::read;
    use std::rc::Rc;

    use crate::{host::Host, native::database::MercuryDatabase, ZephyrMock};

    use super::Vm;

    #[test]
    fn simple_vm_invocation() {
        let code = &[
            0, 97, 115, 109, 1, 0, 0, 0, 1, 4, 1, 96, 0, 0, 3, 2, 1, 0, 7, 12, 1, 8, 111, 110, 95,
            99, 108, 111, 115, 101, 0, 0, 10, 10, 1, 8, 0, 65, 0, 65, 1, 106, 26, 11, 0, 10, 4,
            110, 97, 109, 101, 2, 3, 1, 0, 0,
        ];

        let host = Host::<MercuryDatabase>::mocked().unwrap();

        let start = std::time::Instant::now();

        let vm = Vm::new(&host, code).unwrap();

        host.load_context(Rc::clone(&vm)).unwrap();

        vm.metered_call(&host).unwrap();

        println!("elapsed {:?}", start.elapsed());
    }

    #[test]
    fn alloc_invocation() {
        let code = { read("./../target/wasm32-unknown-unknown/release/alloc.wasm").unwrap() };

        let host = Host::<MercuryDatabase>::mocked().unwrap();

        let start = std::time::Instant::now();

        let vm = Vm::new(&host, code.as_slice()).unwrap();

        host.load_context(Rc::clone(&vm)).unwrap();

        vm.metered_call(&host).unwrap();

        println!("elapsed {:?}", start.elapsed());
    }

    #[test]
    fn with_ledger_close_meta() {
        let code = { read("./../target/wasm32-unknown-unknown/release/alloc.wasm").unwrap() };

        let mut host = Host::<MercuryDatabase>::mocked().unwrap();

        {
            let ledger_close_meta = &[0, 0, 0, 0, 0, 0, 0, 1];
            host.add_ledger_close_meta(ledger_close_meta.to_vec())
                .unwrap();
        }

        let start = std::time::Instant::now();

        let vm = Vm::new(&host, code.as_slice()).unwrap();

        host.load_context(Rc::clone(&vm)).unwrap();

        vm.metered_call(&host).unwrap();

        println!("elapsed {:?}", start.elapsed());
    }

    #[test]
    fn dbread_mocked_invocation() {
        let code = &[
            0, 97, 115, 109, 1, 0, 0, 0, 1, 8, 2, 96, 0, 0, 96, 1, 126, 0, 2, 28, 2, 2, 100, 98, 8,
            114, 101, 97, 100, 95, 114, 97, 119, 0, 0, 5, 115, 116, 97, 99, 107, 4, 112, 117, 115,
            104, 0, 1, 3, 2, 1, 0, 7, 12, 1, 8, 111, 110, 95, 99, 108, 111, 115, 101, 0, 2, 10, 12,
            1, 10, 0, 66, 177, 242, 7, 16, 1, 16, 0, 11, 0, 36, 4, 110, 97, 109, 101, 1, 20, 2, 0,
            6, 100, 98, 114, 101, 97, 100, 1, 9, 115, 116, 97, 99, 107, 112, 117, 115, 104, 2, 7,
            3, 0, 0, 1, 0, 2, 0,
        ];

        let host = Host::<MercuryDatabase>::mocked().unwrap();

        let start = std::time::Instant::now();

        let vm = Vm::new(&host, code).unwrap();

        host.load_context(Rc::clone(&vm)).unwrap();

        vm.metered_call(&host).unwrap();

        println!("elapsed {:?}", start.elapsed());
    }
}
