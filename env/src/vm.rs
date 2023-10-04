use std::{cell::RefCell, rc::Rc};
use wasmtime::{Module, Store, Engine, Linker, Instance, Memory};
use anyhow::Result;

use crate::{host::Host, error::HostError, db::database::ZephyrDatabase};

#[derive(Clone)]
pub struct MemoryManager {
    pub memory: Memory,
    pub offset: RefCell<usize>,
}

impl MemoryManager {
    pub fn new(memory: Memory, offset: usize) -> Self {
        Self { 
            memory, 
            offset: RefCell::new(offset) 
        }
    }
}

/// The Zephyr VM.
pub struct Vm<DB: ZephyrDatabase> {
    module: Module,
    pub store: RefCell<Store<Host<DB>>>,
    pub memory_manager: MemoryManager,
    instance: Instance,
}

impl<DB: ZephyrDatabase + Clone> Vm<DB> {
    pub fn new(host: &Host<DB>, wasm_module_code_bytes: &[u8]) -> Result<Rc<Self>> {
        let mut config = wasmtime::Config::default();

        // TODO: decide which post-mvp features to override.
        // For now we use wasmtime's defaults.
        config
            .consume_fuel(true);

        let engine = Engine::new(&config)?;
        let module = Module::new(&engine, &wasm_module_code_bytes)?;


        let mut store = Store::new(&engine, host.clone());
        host.as_budget().infer_fuel(&mut store)?;

        // TODO: set Store::limiter() once host implements ResourceLimiter

        let mut linker = <Linker<Host<DB>>>::new(&engine);

        for func_info in host.host_functions(&mut store) {
            linker.define(&mut store, func_info.module, func_info.func, func_info.wrapped)?;
        }

        // TODO: add host functions to linker

        // NOTE
        // We are not starting instance already. 
        let instance = linker.instantiate(&mut store, &module)?;
        let memory = instance.get_export(&mut store, "memory").unwrap().into_memory().unwrap();
        
        
        let memory_manager = MemoryManager::new(memory, 0);

        Ok(
            Rc::new(
                Self {
                    module,
                    store: RefCell::new(store),
                    memory_manager,
                    instance,
                }
            )
        )
    }

    pub fn metered_call(self: &Rc<Self>, host: &Host<DB>) -> Result<()> {
        let store = &self.store;
        let entry_point_info = host.get_entry_point_info();
        let mut retrn = entry_point_info.retrn.clone();
        
        let ext = match self.instance.get_export(&mut *store.borrow_mut(), &entry_point_info.fname) {
            Some(ext) => ext,
            None => return Err(HostError::NoEntryPointExport.into())
        };

        let func = match ext.into_func() {
            Some(func) => func,
            None => return Err(HostError::ExternNotAFunction.into())
        };

        func.call(
            &mut *store.borrow_mut(), 
            entry_point_info.params.as_slice(), 
            &mut retrn
        )?;
        
        Ok(())
    }
}


#[cfg(test)]
mod otf_test {
    use std::rc::Rc;
    use std::fs::read;

    use crate::{host::{Host}, native::database::MercuryDatabase, ZephyrMock};

    use super::Vm;

    #[test]
    fn simple_vm_invocation() {
        let code = &[0, 97, 115, 109, 1, 0, 0, 0, 1, 4, 1, 96, 0, 0, 3, 2, 1, 0, 7, 12, 1, 8, 111, 110, 95, 99, 108, 111, 115, 101, 0, 0, 10, 10, 1, 8, 0, 65, 0, 65, 1, 106, 26, 11, 0, 10, 4, 110, 97, 109, 101, 2, 3, 1, 0, 0];

        let host = Host::<MercuryDatabase>::mocked().unwrap();

        let start = std::time::Instant::now();

        let vm = Vm::new(&host, code).unwrap();
        
        host.load_context(Rc::clone(&vm)).unwrap();
        
        vm.metered_call(&host).unwrap();

        println!("elapsed {:?}", start.elapsed());
    }

    #[test]
    fn alloc_invocation() {
        let code = {
            read("./../target/wasm32-unknown-unknown/release/alloc.wasm").unwrap()
        };

        let host = Host::<MercuryDatabase>::mocked().unwrap();

        let start = std::time::Instant::now();

        let vm = Vm::new(&host, code.as_slice()).unwrap();
        
        host.load_context(Rc::clone(&vm)).unwrap();
        
        vm.metered_call(&host).unwrap();

        println!("elapsed {:?}", start.elapsed());
    }

    #[test]
    fn with_ledger_close_meta() {
        let code = {
            read("./../target/wasm32-unknown-unknown/release/alloc.wasm").unwrap()
        };

        let mut host = Host::<MercuryDatabase>::mocked().unwrap();
        
        {
            let ledger_close_meta = &[0, 0, 0, 2, 0, 0, 0, 0, 120, 138, 184, 150, 10, 84, 86, 168, 114, 108, 166, 243, 147, 153, 39, 56, 184, 50, 49, 162, 165, 61, 166, 156, 200, 177, 178, 46, 201, 178, 186, 81, 0, 0, 0, 20, 98, 133, 23, 65, 30, 157, 22, 240, 77, 203, 132, 132, 229, 121, 76, 91, 46, 22, 91, 236, 106, 165, 162, 74, 63, 142, 227, 62, 199, 62, 91, 189, 94, 171, 73, 4, 147, 139, 78, 81, 203, 38, 176, 65, 248, 186, 107, 24, 237, 127, 196, 59, 200, 190, 45, 142, 195, 154, 159, 99, 149, 100, 201, 197, 0, 0, 0, 0, 101, 29, 118, 81, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 181, 93, 16, 178, 187, 125, 40, 78, 213, 192, 58, 71, 165, 225, 181, 144, 201, 15, 87, 131, 85, 71, 28, 196, 80, 123, 190, 130, 4, 210, 60, 139, 0, 0, 0, 64, 47, 201, 80, 142, 95, 16, 162, 100, 27, 202, 252, 15, 56, 66, 36, 62, 239, 144, 114, 167, 140, 101, 231, 183, 56, 228, 61, 29, 110, 164, 143, 241, 54, 254, 44, 49, 45, 7, 100, 249, 155, 32, 58, 95, 107, 190, 94, 150, 211, 63, 0, 51, 211, 123, 18, 146, 103, 10, 55, 230, 18, 45, 64, 1, 83, 18, 246, 154, 218, 163, 104, 195, 197, 164, 36, 171, 155, 52, 59, 231, 114, 98, 172, 177, 74, 111, 22, 252, 166, 104, 81, 143, 47, 27, 251, 202, 241, 244, 250, 93, 202, 100, 206, 60, 111, 103, 167, 214, 117, 23, 138, 130, 80, 166, 245, 135, 166, 148, 208, 0, 128, 142, 92, 242, 4, 226, 138, 52, 0, 28, 36, 157, 13, 224, 182, 179, 167, 100, 0, 0, 0, 0, 0, 198, 215, 48, 241, 110, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 201, 124, 0, 0, 0, 100, 0, 76, 75, 64, 0, 0, 0, 105, 185, 90, 108, 226, 104, 56, 127, 230, 249, 28, 164, 236, 213, 220, 36, 92, 193, 68, 56, 215, 194, 35, 162, 183, 31, 223, 57, 127, 116, 199, 216, 221, 152, 182, 227, 68, 124, 154, 235, 146, 252, 38, 51, 5, 47, 31, 166, 66, 112, 49, 158, 122, 166, 209, 129, 241, 77, 17, 245, 92, 224, 163, 151, 238, 184, 101, 180, 193, 78, 64, 58, 163, 84, 225, 61, 91, 239, 56, 185, 197, 61, 228, 214, 185, 226, 15, 128, 176, 214, 65, 189, 28, 185, 249, 181, 140, 219, 182, 191, 95, 163, 31, 241, 171, 225, 154, 147, 42, 154, 178, 38, 206, 98, 13, 1, 24, 25, 52, 100, 177, 238, 108, 252, 240, 180, 54, 7, 168, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 98, 133, 23, 65, 30, 157, 22, 240, 77, 203, 132, 132, 229, 121, 76, 91, 46, 22, 91, 236, 106, 165, 162, 74, 63, 142, 227, 62, 199, 62, 91, 189, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 100, 0, 0, 0, 3, 0, 0, 0, 2, 0, 0, 0, 0, 23, 129, 191, 253, 240, 18, 133, 71, 3, 129, 29, 241, 24, 134, 190, 159, 35, 114, 64, 74, 227, 28, 112, 34, 62, 147, 195, 74, 109, 80, 242, 104, 0, 61, 9, 0, 0, 1, 67, 116, 0, 2, 127, 23, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 101, 29, 118, 136, 0, 0, 0, 1, 0, 0, 0, 9, 112, 115, 112, 98, 58, 49, 48, 53, 56, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0, 1, 0, 0, 0, 0, 138, 150, 235, 84, 55, 145, 147, 221, 67, 168, 19, 96, 84, 144, 219, 73, 19, 10, 34, 164, 99, 162, 156, 17, 108, 66, 213, 224, 164, 134, 76, 197, 0, 0, 0, 1, 0, 0, 0, 0, 227, 125, 232, 251, 42, 222, 153, 101, 55, 183, 207, 28, 42, 151, 233, 29, 31, 137, 48, 84, 120, 224, 145, 243, 152, 17, 167, 139, 168, 106, 8, 70, 0, 0, 0, 2, 65, 84, 85, 83, 68, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 103, 202, 214, 99, 120, 154, 14, 117, 141, 181, 250, 111, 46, 147, 90, 8, 70, 202, 116, 56, 235, 119, 104, 16, 56, 51, 174, 42, 153, 163, 214, 243, 0, 0, 0, 0, 6, 142, 119, 128, 0, 0, 0, 1, 0, 0, 0, 0, 138, 150, 235, 84, 55, 145, 147, 221, 67, 168, 19, 96, 84, 144, 219, 73, 19, 10, 34, 164, 99, 162, 156, 17, 108, 66, 213, 224, 164, 134, 76, 197, 0, 0, 0, 1, 0, 0, 0, 0, 0, 144, 201, 43, 191, 145, 132, 224, 52, 172, 15, 5, 110, 12, 17, 159, 178, 236, 90, 153, 50, 39, 80, 251, 202, 19, 123, 37, 17, 175, 117, 148, 0, 0, 0, 2, 65, 84, 85, 83, 68, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 103, 202, 214, 99, 120, 154, 14, 117, 141, 181, 250, 111, 46, 147, 90, 8, 70, 202, 116, 56, 235, 119, 104, 16, 56, 51, 174, 42, 153, 163, 214, 243, 0, 0, 0, 0, 0, 224, 77, 224, 0, 0, 0, 1, 0, 0, 0, 0, 138, 150, 235, 84, 55, 145, 147, 221, 67, 168, 19, 96, 84, 144, 219, 73, 19, 10, 34, 164, 99, 162, 156, 17, 108, 66, 213, 224, 164, 134, 76, 197, 0, 0, 0, 1, 0, 0, 0, 0, 227, 125, 232, 251, 42, 222, 153, 101, 55, 183, 207, 28, 42, 151, 233, 29, 31, 137, 48, 84, 120, 224, 145, 243, 152, 17, 167, 139, 168, 106, 8, 70, 0, 0, 0, 2, 65, 84, 85, 83, 68, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 103, 202, 214, 99, 120, 154, 14, 117, 141, 181, 250, 111, 46, 147, 90, 8, 70, 202, 116, 56, 235, 119, 104, 16, 56, 51, 174, 42, 153, 163, 214, 243, 0, 0, 0, 0, 6, 142, 119, 128, 0, 0, 0, 1, 0, 0, 0, 0, 138, 150, 235, 84, 55, 145, 147, 221, 67, 168, 19, 96, 84, 144, 219, 73, 19, 10, 34, 164, 99, 162, 156, 17, 108, 66, 213, 224, 164, 134, 76, 197, 0, 0, 0, 1, 0, 0, 0, 0, 0, 144, 201, 43, 191, 145, 132, 224, 52, 172, 15, 5, 110, 12, 17, 159, 178, 236, 90, 153, 50, 39, 80, 251, 202, 19, 123, 37, 17, 175, 117, 148, 0, 0, 0, 2, 65, 84, 85, 83, 68, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 103, 202, 214, 99, 120, 154, 14, 117, 141, 181, 250, 111, 46, 147, 90, 8, 70, 202, 116, 56, 235, 119, 104, 16, 56, 51, 174, 42, 153, 163, 214, 243, 0, 0, 0, 0, 0, 224, 77, 224, 0, 0, 0, 0, 0, 0, 0, 2, 109, 80, 242, 104, 0, 0, 0, 64, 255, 55, 246, 90, 1, 250, 224, 149, 230, 119, 120, 235, 235, 24, 171, 129, 131, 72, 107, 160, 157, 203, 249, 177, 116, 16, 117, 97, 57, 234, 53, 79, 43, 93, 25, 202, 7, 72, 41, 159, 47, 31, 161, 161, 158, 114, 57, 154, 237, 214, 76, 65, 62, 205, 197, 166, 53, 30, 201, 129, 250, 255, 4, 10, 164, 134, 76, 197, 0, 0, 0, 64, 84, 190, 156, 74, 39, 43, 117, 107, 57, 193, 58, 106, 218, 246, 60, 51, 119, 206, 79, 184, 128, 62, 8, 32, 58, 71, 19, 243, 222, 186, 252, 68, 9, 40, 220, 134, 34, 189, 53, 175, 186, 55, 95, 5, 189, 53, 17, 20, 62, 112, 66, 146, 223, 187, 75, 2, 23, 250, 132, 61, 94, 197, 230, 13, 0, 0, 0, 2, 0, 0, 0, 0, 107, 106, 43, 1, 92, 216, 17, 61, 135, 193, 215, 167, 55, 125, 239, 128, 24, 1, 250, 15, 245, 235, 37, 54, 73, 203, 140, 12, 124, 160, 68, 96, 0, 30, 132, 128, 0, 0, 2, 212, 0, 2, 142, 150, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 101, 29, 118, 136, 0, 0, 0, 1, 0, 0, 0, 10, 112, 115, 112, 58, 49, 48, 53, 50, 57, 49, 0, 0, 0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0, 0, 138, 150, 235, 84, 55, 145, 147, 221, 67, 168, 19, 96, 84, 144, 219, 73, 19, 10, 34, 164, 99, 162, 156, 17, 108, 66, 213, 224, 164, 134, 76, 197, 0, 0, 0, 1, 0, 0, 0, 0, 106, 208, 86, 26, 19, 9, 214, 51, 234, 241, 139, 130, 2, 125, 157, 78, 36, 50, 1, 132, 235, 11, 213, 206, 40, 40, 160, 112, 167, 213, 115, 137, 0, 0, 0, 2, 65, 84, 85, 90, 83, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 103, 202, 214, 99, 120, 154, 14, 117, 141, 181, 250, 111, 46, 147, 90, 8, 70, 202, 116, 56, 235, 119, 104, 16, 56, 51, 174, 42, 153, 163, 214, 243, 0, 0, 3, 172, 162, 226, 84, 128, 0, 0, 0, 1, 0, 0, 0, 0, 138, 150, 235, 84, 55, 145, 147, 221, 67, 168, 19, 96, 84, 144, 219, 73, 19, 10, 34, 164, 99, 162, 156, 17, 108, 66, 213, 224, 164, 134, 76, 197, 0, 0, 0, 1, 0, 0, 0, 0, 0, 144, 201, 43, 191, 145, 132, 224, 52, 172, 15, 5, 110, 12, 17, 159, 178, 236, 90, 153, 50, 39, 80, 251, 202, 19, 123, 37, 17, 175, 117, 148, 0, 0, 0, 2, 65, 84, 85, 90, 83, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 103, 202, 214, 99, 120, 154, 14, 117, 141, 181, 250, 111, 46, 147, 90, 8, 70, 202, 116, 56, 235, 119, 104, 16, 56, 51, 174, 42, 153, 163, 214, 243, 0, 0, 0, 37, 65, 41, 15, 192, 0, 0, 0, 0, 0, 0, 0, 2, 124, 160, 68, 96, 0, 0, 0, 64, 69, 91, 11, 21, 29, 77, 113, 88, 73, 75, 139, 126, 9, 127, 69, 62, 60, 115, 228, 107, 200, 92, 156, 60, 78, 178, 241, 26, 69, 72, 143, 107, 190, 101, 30, 165, 32, 244, 214, 120, 209, 156, 90, 164, 37, 39, 212, 164, 6, 71, 251, 195, 252, 165, 44, 68, 104, 20, 61, 10, 97, 241, 134, 13, 164, 134, 76, 197, 0, 0, 0, 64, 40, 153, 222, 110, 52, 158, 112, 115, 152, 117, 61, 182, 187, 62, 180, 235, 52, 148, 86, 255, 234, 47, 244, 89, 34, 203, 25, 159, 43, 142, 96, 16, 207, 253, 11, 160, 215, 105, 160, 9, 221, 14, 97, 232, 92, 186, 150, 189, 95, 12, 210, 237, 138, 43, 78, 235, 80, 192, 28, 21, 228, 31, 160, 3, 0, 0, 0, 2, 0, 0, 0, 0, 53, 23, 86, 26, 251, 67, 190, 121, 8, 86, 25, 102, 172, 95, 91, 79, 241, 121, 211, 240, 166, 193, 200, 99, 185, 20, 194, 251, 141, 78, 100, 150, 0, 15, 66, 64, 0, 0, 2, 211, 0, 2, 142, 164, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 101, 29, 118, 136, 0, 0, 0, 1, 0, 0, 0, 8, 112, 115, 112, 98, 58, 56, 55, 51, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 138, 150, 235, 84, 55, 145, 147, 221, 67, 168, 19, 96, 84, 144, 219, 73, 19, 10, 34, 164, 99, 162, 156, 17, 108, 66, 213, 224, 164, 134, 76, 197, 0, 0, 0, 1, 0, 0, 0, 0, 233, 61, 211, 119, 236, 80, 61, 41, 87, 9, 126, 99, 1, 184, 63, 41, 61, 198, 147, 230, 58, 165, 172, 199, 152, 16, 228, 207, 223, 166, 104, 216, 0, 0, 0, 2, 65, 84, 85, 90, 83, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 103, 202, 214, 99, 120, 154, 14, 117, 141, 181, 250, 111, 46, 147, 90, 8, 70, 202, 116, 56, 235, 119, 104, 16, 56, 51, 174, 42, 153, 163, 214, 243, 0, 0, 3, 25, 243, 214, 180, 0, 0, 0, 0, 0, 0, 0, 0, 2, 141, 78, 100, 150, 0, 0, 0, 64, 141, 78, 10, 139, 135, 205, 164, 179, 110, 196, 197, 54, 104, 190, 12, 7, 133, 184, 190, 203, 17, 39, 47, 158, 128, 85, 195, 138, 209, 251, 105, 136, 124, 152, 105, 174, 215, 11, 148, 131, 112, 10, 19, 5, 238, 158, 129, 197, 123, 202, 70, 147, 19, 224, 123, 58, 138, 126, 91, 250, 33, 194, 238, 5, 164, 134, 76, 197, 0, 0, 0, 64, 7, 34, 85, 148, 231, 74, 103, 139, 142, 173, 114, 93, 102, 181, 187, 214, 176, 227, 182, 29, 234, 169, 92, 152, 154, 68, 248, 124, 194, 36, 250, 87, 73, 20, 59, 46, 254, 145, 219, 21, 69, 71, 21, 107, 206, 164, 147, 234, 109, 29, 134, 136, 200, 21, 254, 223, 210, 35, 150, 125, 84, 224, 112, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 51, 46, 82, 63, 70, 5, 137, 159, 110, 156, 158, 252, 126, 140, 254, 247, 226, 14, 25, 160, 184, 140, 233, 208, 227, 135, 105, 96, 73, 71, 152, 194, 0, 0, 0, 0, 0, 0, 0, 100, 255, 255, 255, 255, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 255, 255, 255, 254, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 3, 0, 28, 36, 153, 0, 0, 0, 0, 0, 0, 0, 0, 53, 23, 86, 26, 251, 67, 190, 121, 8, 86, 25, 102, 172, 95, 91, 79, 241, 121, 211, 240, 166, 193, 200, 99, 185, 20, 194, 251, 141, 78, 100, 150, 0, 0, 0, 18, 81, 180, 76, 15, 0, 0, 2, 211, 0, 2, 142, 163, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 28, 36, 153, 0, 0, 0, 0, 101, 29, 118, 61, 0, 0, 0, 0, 0, 0, 0, 1, 0, 28, 36, 157, 0, 0, 0, 0, 0, 0, 0, 0, 53, 23, 86, 26, 251, 67, 190, 121, 8, 86, 25, 102, 172, 95, 91, 79, 241, 121, 211, 240, 166, 193, 200, 99, 185, 20, 194, 251, 141, 78, 100, 150, 0, 0, 0, 18, 81, 180, 75, 171, 0, 0, 2, 211, 0, 2, 142, 163, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 28, 36, 153, 0, 0, 0, 0, 101, 29, 118, 61, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 3, 0, 28, 36, 157, 0, 0, 0, 0, 0, 0, 0, 0, 53, 23, 86, 26, 251, 67, 190, 121, 8, 86, 25, 102, 172, 95, 91, 79, 241, 121, 211, 240, 166, 193, 200, 99, 185, 20, 194, 251, 141, 78, 100, 150, 0, 0, 0, 18, 81, 180, 75, 171, 0, 0, 2, 211, 0, 2, 142, 163, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 28, 36, 153, 0, 0, 0, 0, 101, 29, 118, 61, 0, 0, 0, 0, 0, 0, 0, 1, 0, 28, 36, 157, 0, 0, 0, 0, 0, 0, 0, 0, 53, 23, 86, 26, 251, 67, 190, 121, 8, 86, 25, 102, 172, 95, 91, 79, 241, 121, 211, 240, 166, 193, 200, 99, 185, 20, 194, 251, 141, 78, 100, 150, 0, 0, 0, 18, 81, 180, 75, 171, 0, 0, 2, 211, 0, 2, 142, 164, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 28, 36, 157, 0, 0, 0, 0, 101, 29, 118, 81, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 97, 155, 31, 106, 6, 239, 2, 136, 90, 37, 161, 185, 141, 85, 133, 91, 153, 110, 88, 171, 53, 189, 85, 183, 38, 217, 57, 232, 134, 119, 53, 138, 0, 0, 0, 0, 0, 0, 0, 200, 255, 255, 255, 255, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 1, 255, 255, 255, 254, 0, 0, 0, 0, 0, 0, 0, 1, 255, 255, 255, 250, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 3, 0, 28, 36, 155, 0, 0, 0, 0, 0, 0, 0, 0, 107, 106, 43, 1, 92, 216, 17, 61, 135, 193, 215, 167, 55, 125, 239, 128, 24, 1, 250, 15, 245, 235, 37, 54, 73, 203, 140, 12, 124, 160, 68, 96, 0, 0, 0, 18, 94, 177, 214, 51, 0, 0, 2, 212, 0, 2, 142, 149, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 28, 36, 155, 0, 0, 0, 0, 101, 29, 118, 71, 0, 0, 0, 0, 0, 0, 0, 1, 0, 28, 36, 157, 0, 0, 0, 0, 0, 0, 0, 0, 107, 106, 43, 1, 92, 216, 17, 61, 135, 193, 215, 167, 55, 125, 239, 128, 24, 1, 250, 15, 245, 235, 37, 54, 73, 203, 140, 12, 124, 160, 68, 96, 0, 0, 0, 18, 94, 177, 213, 107, 0, 0, 2, 212, 0, 2, 142, 149, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 28, 36, 155, 0, 0, 0, 0, 101, 29, 118, 71, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 3, 0, 28, 36, 157, 0, 0, 0, 0, 0, 0, 0, 0, 107, 106, 43, 1, 92, 216, 17, 61, 135, 193, 215, 167, 55, 125, 239, 128, 24, 1, 250, 15, 245, 235, 37, 54, 73, 203, 140, 12, 124, 160, 68, 96, 0, 0, 0, 18, 94, 177, 213, 107, 0, 0, 2, 212, 0, 2, 142, 149, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 28, 36, 155, 0, 0, 0, 0, 101, 29, 118, 71, 0, 0, 0, 0, 0, 0, 0, 1, 0, 28, 36, 157, 0, 0, 0, 0, 0, 0, 0, 0, 107, 106, 43, 1, 92, 216, 17, 61, 135, 193, 215, 167, 55, 125, 239, 128, 24, 1, 250, 15, 245, 235, 37, 54, 73, 203, 140, 12, 124, 160, 68, 96, 0, 0, 0, 18, 94, 177, 213, 107, 0, 0, 2, 212, 0, 2, 142, 150, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 28, 36, 157, 0, 0, 0, 0, 101, 29, 118, 81, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 38, 17, 63, 202, 149, 172, 222, 9, 56, 154, 100, 38, 68, 243, 245, 86, 62, 11, 86, 3, 96, 109, 3, 201, 10, 42, 240, 195, 68, 195, 40, 27, 0, 0, 0, 0, 0, 0, 1, 144, 255, 255, 255, 255, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 1, 255, 255, 255, 250, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 255, 255, 255, 250, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 3, 0, 28, 36, 153, 0, 0, 0, 0, 0, 0, 0, 0, 23, 129, 191, 253, 240, 18, 133, 71, 3, 129, 29, 241, 24, 134, 190, 159, 35, 114, 64, 74, 227, 28, 112, 34, 62, 147, 195, 74, 109, 80, 242, 104, 0, 0, 0, 18, 106, 58, 191, 180, 0, 1, 67, 116, 0, 2, 127, 22, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 28, 36, 153, 0, 0, 0, 0, 101, 29, 118, 61, 0, 0, 0, 0, 0, 0, 0, 1, 0, 28, 36, 157, 0, 0, 0, 0, 0, 0, 0, 0, 23, 129, 191, 253, 240, 18, 133, 71, 3, 129, 29, 241, 24, 134, 190, 159, 35, 114, 64, 74, 227, 28, 112, 34, 62, 147, 195, 74, 109, 80, 242, 104, 0, 0, 0, 18, 106, 58, 190, 36, 0, 1, 67, 116, 0, 2, 127, 22, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 28, 36, 153, 0, 0, 0, 0, 101, 29, 118, 61, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 3, 0, 28, 36, 157, 0, 0, 0, 0, 0, 0, 0, 0, 23, 129, 191, 253, 240, 18, 133, 71, 3, 129, 29, 241, 24, 134, 190, 159, 35, 114, 64, 74, 227, 28, 112, 34, 62, 147, 195, 74, 109, 80, 242, 104, 0, 0, 0, 18, 106, 58, 190, 36, 0, 1, 67, 116, 0, 2, 127, 22, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 28, 36, 153, 0, 0, 0, 0, 101, 29, 118, 61, 0, 0, 0, 0, 0, 0, 0, 1, 0, 28, 36, 157, 0, 0, 0, 0, 0, 0, 0, 0, 23, 129, 191, 253, 240, 18, 133, 71, 3, 129, 29, 241, 24, 134, 190, 159, 35, 114, 64, 74, 227, 28, 112, 34, 62, 147, 195, 74, 109, 80, 242, 104, 0, 0, 0, 18, 106, 58, 190, 36, 0, 1, 67, 116, 0, 2, 127, 23, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 28, 36, 157, 0, 0, 0, 0, 101, 29, 118, 81, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 9, 121, 194, 145, 0, 0, 0, 0, 0, 0, 0, 0];
            host.add_ledger_close_meta(ledger_close_meta).unwrap();
        }

        let start = std::time::Instant::now();

        let vm = Vm::new(&host, code.as_slice()).unwrap();
        
        host.load_context(Rc::clone(&vm)).unwrap();
        
        vm.metered_call(&host).unwrap();

        println!("elapsed {:?}", start.elapsed());
    }

    #[test]
    fn dbread_mocked_invocation() {
        let code = &[0, 97, 115, 109, 1, 0, 0, 0, 1, 8, 2, 96, 0, 0, 96, 1, 126, 0, 2, 28, 2, 2, 100, 98, 8, 114, 101, 97, 100, 95, 114, 97, 119, 0, 0, 5, 115, 116, 97, 99, 107, 4, 112, 117, 115, 104, 0, 1, 3, 2, 1, 0, 7, 12, 1, 8, 111, 110, 95, 99, 108, 111, 115, 101, 0, 2, 10, 12, 1, 10, 0, 66, 177, 242, 7, 16, 1, 16, 0, 11, 0, 36, 4, 110, 97, 109, 101, 1, 20, 2, 0, 6, 100, 98, 114, 101, 97, 100, 1, 9, 115, 116, 97, 99, 107, 112, 117, 115, 104, 2, 7, 3, 0, 0, 1, 0, 2, 0];

        let host = Host::<MercuryDatabase>::mocked().unwrap();

        let start = std::time::Instant::now();

        let vm = Vm::new(&host, code).unwrap();
        
        host.load_context(Rc::clone(&vm)).unwrap();
        
        vm.metered_call(&host).unwrap();

        println!("elapsed {:?}", start.elapsed());
    }
}
