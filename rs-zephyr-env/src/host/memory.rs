use super::Host;
use crate::{
    db::{database::ZephyrDatabase, ledger::LedgerStateRead},
    error::{HostError, InternalError},
};
use anyhow::{anyhow, Result};
use soroban_env_host::vm::CustomContextVM;
use wasmi::{core::Pages, Caller, Memory};

const KEEP_FREE: usize = 16384;

pub struct CustomVMCtx<'a, DB: ZephyrDatabase + 'static, L: LedgerStateRead + 'static> {
    caller: Option<&'a Caller<'a, Host<DB, L>>>,
    caller_mut: Option<Caller<'a, Host<DB, L>>>,
}

impl<'a, DB: ZephyrDatabase + 'static, L: LedgerStateRead + 'static> CustomVMCtx<'a, DB, L> {
    pub fn new(ctx: &'a Caller<Host<DB, L>>) -> Self {
        Self {
            caller: Some(ctx),
            caller_mut: None,
        }
    }

    pub fn new_mut(ctx: Caller<'a, Host<DB, L>>) -> Self {
        Self {
            caller: None,
            caller_mut: Some(ctx),
        }
    }

    pub fn into_inner(self) -> Option<Caller<'a, Host<DB, L>>> {
        self.caller_mut
    }
}

impl<'a, DB: ZephyrDatabase + Clone + 'static, L: LedgerStateRead + 'static> CustomContextVM
    for CustomVMCtx<'a, DB, L>
{
    // Note: we prefer not to handle potential VM memory errors here since
    // they would need to be handled by our SVM fork and we're trying to keep
    // as less logic as possible there.
    fn read(&self, mem_pos: usize, buf: &mut [u8]) {
        if let Some(caller) = self.caller {
            let _ = Host::get_memory(caller).read(caller, mem_pos, buf);
        } else {
            let _ = Host::get_memory(self.caller_mut.as_ref().unwrap()).read(
                self.caller_mut.as_ref().unwrap(),
                mem_pos,
                buf,
            );
        }
    }

    fn data(&self) -> &[u8] {
        if let Some(caller) = self.caller {
            Host::get_memory(caller).data(caller)
        } else {
            Host::get_memory(self.caller_mut.as_ref().unwrap())
                .data(self.caller_mut.as_ref().unwrap())
        }
    }

    fn write(&mut self, pos: u32, slice: &[u8]) -> i64 {
        Host::write_to_memory_mut(self.caller_mut.as_mut().unwrap(), pos, slice).unwrap()
    }

    fn data_mut(&mut self) -> &mut [u8] {
        if let Some(caller) = self.caller_mut.as_mut() {
            Host::get_memory(caller).data_mut(caller)
        } else {
            &mut []
        }
    }
}

impl<DB: ZephyrDatabase + Clone + 'static, L: LedgerStateRead + 'static> Host<DB, L> {
    /// Returns wasmi's VM memory handler.
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

    pub(crate) fn write_to_memory(mut caller: Caller<Self>, contents: Vec<u8>) -> (Caller<Self>, Result<(i64, i64)>) {
        let effect = (|| {
            let (memory, offset, data) = {
                let host = caller.data();

                let context = host.0.context.borrow();
                let vm = context
                    .vm
                    .as_ref()
                    .ok_or_else(|| HostError::NoContext)?
                    .upgrade()
                    .ok_or_else(|| HostError::InternalError(InternalError::CannotUpgradeRc))?;

                let manager = &vm.memory_manager;
                let memory = manager.memory;

                let mut offset_mut = manager.offset.borrow_mut();
                let new_offset = offset_mut
                    .checked_add(contents.len())
                    .ok_or_else(|| HostError::InternalError(InternalError::ArithError))?;

                *offset_mut = new_offset;

                (memory, new_offset, contents)
            };

            Self::grow_memory_pages_if_needed(memory, &mut caller, data.len());

            if let Err(error) = memory.write(&mut caller, data.len(), data.as_slice()) {
                return Err(anyhow!(error));
            };

            Ok((data.len() as i64, data.len() as i64))
        })();

        (caller, effect)
    }

    pub(crate) fn write_to_memory_mut(
        caller: &mut Caller<Self>,
        pos: u32,
        contents: &[u8],
    ) -> Result<i64> {
        let memory = Self::get_memory(caller);
        Self::grow_memory_pages_if_needed(memory, caller, contents.len());
        
        if let Err(error) = memory.write(caller, pos as usize, contents) {
            return Err(anyhow!(error));
        };

        Ok((pos + contents.len() as u32) as i64)
    }

    pub(crate) fn read_segment_from_memory(
        memory: &Memory,
        caller: &Caller<Self>,
        segment: (i64, i64),
    ) -> Result<Vec<u8>> {
        let mut written_vec = vec![0; segment.1 as usize];
        if let Err(error) = memory.read(caller, segment.0 as usize, &mut written_vec) {
            return Err(anyhow!(error));
        }

        Ok(written_vec)
    }

    pub(crate) fn grow_memory_pages_if_needed(memory: Memory, caller: &mut Caller<Self>, buf_len: usize) {
        // Estimating free allocated memory.
        let current_estimated_free = memory.data(&caller).iter().filter(|byte| **byte == 0x00_u8).count();
        
        if current_estimated_free < buf_len + KEEP_FREE {
            let _ = memory.grow(caller, Pages::new(100).unwrap());
        }
    }
}
