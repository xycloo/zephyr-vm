use std::borrow::Borrow;

use anyhow::Result;
use rs_zephyr_common::DatabaseError;
use wasmi::Caller;

use crate::db::{
    database::{DatabasePermissions, WhereCond, ZephyrDatabase},
    ledger::LedgerStateRead,
};

use super::{utils, Host};

impl<DB: ZephyrDatabase + Clone + 'static, L: LedgerStateRead + 'static> Host<DB, L> {
    pub(crate) fn write_database_raw(caller: Caller<Self>) -> Result<()> {
        let (memory, write_point_hash, columns, segments) = {
            let host = caller.data();
            let stack_impl = host.as_stack_mut();

            let id = {
                let value = host.get_host_id();
                utils::bytes::i64_to_bytes(value)
            };

            let write_point_hash: [u8; 16] = {
                let point_raw = stack_impl.0.get_with_step()?;
                let point_bytes = utils::bytes::i64_to_bytes(point_raw);
                md5::compute([point_bytes, id].concat()).into()
            };

            let columns = {
                let columns_size_idx = stack_impl.0.get_with_step()?;
                let mut columns: Vec<i64> = Vec::new();
                for _ in 0..columns_size_idx as usize {
                    columns.push(stack_impl.0.get_with_step()?);
                }
                columns
            };

            let data_segments = {
                let mut segments: Vec<(i64, i64)> = Vec::new();
                let data_segments_size_idx = {
                    let non_fixed = stack_impl.0.get_with_step()?;
                    (non_fixed * 2) as usize
                };
                for _ in (0..data_segments_size_idx).step_by(2) {
                    let offset = stack_impl.0.get_with_step()?;
                    let size = stack_impl.0.get_with_step()?;
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

    pub(crate) fn update_database_raw(caller: Caller<Self>) -> Result<()> {
        let (memory, write_point_hash, columns, segments, conditions, conditions_args) = {
            let host = caller.data();

            let stack_impl = host.as_stack_mut();

            let id = {
                let value = host.get_host_id();
                utils::bytes::i64_to_bytes(value)
            };

            let write_point_hash: [u8; 16] = {
                let point_raw = stack_impl.0.get_with_step()?;
                let point_bytes = utils::bytes::i64_to_bytes(point_raw);
                md5::compute([point_bytes, id].concat()).into()
            };

            let columns = {
                let columns_size_idx = stack_impl.0.get_with_step()?;
                let mut columns: Vec<i64> = Vec::new();

                for _ in 0..columns_size_idx as usize {
                    columns.push(stack_impl.0.get_with_step()?);
                }

                columns
            };

            let data_segments = {
                let mut segments: Vec<(i64, i64)> = Vec::new();

                let data_segments_size_idx = {
                    let non_fixed = stack_impl.0.get_with_step()?;
                    (non_fixed * 2) as usize
                };

                for _ in (0..data_segments_size_idx).step_by(2) {
                    let offset = stack_impl.0.get_with_step()?;
                    let size = stack_impl.0.get_with_step()?;
                    segments.push((offset, size))
                }
                segments
            };

            let conditions = {
                let mut conditions = Vec::new();

                let conditions_length = {
                    let non_fixed = stack_impl.0.get_with_step()?;
                    (non_fixed * 2) as usize
                };

                for _ in (0..conditions_length).step_by(2) {
                    let column = stack_impl.0.get_with_step()?;
                    let operator = stack_impl.0.get_with_step()?;
                    conditions.push(WhereCond::from_column_and_operator(column, operator)?);
                }

                conditions
            };

            let conditions_args = {
                let mut segments = Vec::new();

                let args_length = {
                    let non_fixed = stack_impl.0.get_with_step()?;
                    (non_fixed * 2) as usize
                };

                for _ in (0..args_length).step_by(2) {
                    let offset = stack_impl.0.get_with_step()?;
                    let size = stack_impl.0.get_with_step()?;
                    segments.push((offset, size))
                }

                segments
            };

            let context = host.0.context.borrow();
            let vm = context.vm.as_ref().unwrap().upgrade().unwrap();
            let mem_manager = &vm.memory_manager;

            stack_impl.0.clear();

            (
                mem_manager.memory,
                write_point_hash,
                columns,
                data_segments,
                conditions,
                conditions_args,
            )
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
            aggregated_conditions_args,
        )?;

        Ok(())
    }

    // todo: read from other id notice for payment.
    pub(crate) fn read_database_as_id(caller: Caller<Self>, host_id: i64) -> Result<(i64, i64)> {
        let host = caller.data();

        let read = host.read_database_raw(host_id, &caller)?;
        Self::write_to_memory(caller, read.as_slice())
    }

    pub(crate) fn read_database_self(caller: Caller<Self>) -> Result<(i64, i64)> {
        let host = caller.data();
        let host_id = host.get_host_id();

        let read = host.read_database_raw(host_id, &caller)?;
        Self::write_to_memory(caller, read.as_slice())
    }

    pub(crate) fn read_database_raw(&self, host_id: i64, caller: &Caller<Self>) -> Result<Vec<u8>> {
        //let host = caller.data();
        let host = self;
        let read = {
            let db_obj = host.0.database.borrow();
            let db_impl = db_obj.0.borrow();

            let stack_impl = &host.as_stack_mut().0;

            if let DatabasePermissions::WriteOnly = db_impl.permissions {
                return Err(DatabaseError::ReadOnWriteOnly.into());
            }

            let id = utils::bytes::i64_to_bytes(host_id);

            let read_point_hash: [u8; 16] = {
                let point_raw = stack_impl.get_with_step()?;
                let point_bytes = utils::bytes::i64_to_bytes(point_raw);

                md5::compute([point_bytes, id].concat()).into()
            };

            let read_data = {
                let data_size_idx = stack_impl.get_with_step()?;
                let mut retrn = Vec::new();

                for _ in 0..data_size_idx {
                    retrn.push(stack_impl.get_with_step()?);
                }
                retrn
            };

            let conditions = {
                let mut conditions = Vec::new();
                let non_fixed = stack_impl.get_with_step();

                // Note: if there is an extra argument here specifying the conditions length
                // we assume that it's safe to halt execution if the subsequent stack is malformed
                if let Ok(non_fixed) = non_fixed {
                    let conditions_length = (non_fixed * 2) as usize;

                    for _ in (0..conditions_length).step_by(2) {
                        let column = stack_impl.get_with_step()?;
                        let operator = stack_impl.get_with_step()?;
                        conditions.push(WhereCond::from_column_and_operator(column, operator)?);
                    }

                    Some(conditions)
                } else {
                    None
                }
            };
            let has_conditions = conditions.is_some();

            let conditions_args = if has_conditions {
                let mut segments = Vec::new();

                let args_length = {
                    let non_fixed = stack_impl.get_with_step()?;
                    (non_fixed * 2) as usize
                };

                for _ in (0..args_length).step_by(2) {
                    let offset = stack_impl.get_with_step()?;
                    let size = stack_impl.get_with_step()?;
                    segments.push((offset, size))
                }

                Some(segments)
            } else {
                None
            };

            let aggregated_conditions_args = if has_conditions {
                let memory = Self::get_memory(caller);
                Some(
                    conditions_args
                        .unwrap()
                        .iter()
                        .map(|segment| Self::read_segment_from_memory(&memory, &caller, *segment))
                        .collect::<Result<Vec<_>, _>>()?,
                )
            } else {
                None
            };

            let user_id = host.get_host_id();
            stack_impl.clear();

            db_impl.db.read_raw(
                user_id,
                read_point_hash,
                &read_data,
                conditions.as_ref().map(Vec::as_slice),
                aggregated_conditions_args,
            )?
        };

        Ok(read)
    }
}
