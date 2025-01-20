use std::borrow::Borrow;

use anyhow::Result;
use rs_zephyr_common::DatabaseError;
use wasmi::Caller;

use crate::{
    db::{
        database::{DatabasePermissions, WhereCond, ZephyrDatabase},
        ledger::LedgerStateRead,
    },
    error::{HostError, InternalError},
    trace::TracePoint,
};

use super::{utils, Host};

impl<DB: ZephyrDatabase + Clone + 'static, L: LedgerStateRead + 'static> Host<DB, L> {
    pub(crate) fn write_database_raw(caller: Caller<Self>) -> (Caller<Self>, Result<()>) {
        let effect = (|| {
            let (memory, write_point_hash, columns, segments) = {
                let host = caller.data();
                let stack_impl = host.as_stack_mut();

                let id = {
                    let value = host.get_host_id();
                    utils::bytes::i64_to_bytes(value)
                };

                caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                    TracePoint::DatabaseImpl,
                    "Reading the table name.",
                    false,
                );
                let write_point_hash: [u8; 16] = {
                    let point_raw = stack_impl.0.get_with_step()?;
                    let point_bytes = utils::bytes::i64_to_bytes(point_raw);
                    md5::compute([point_bytes, id].concat()).into()
                };

                caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                    TracePoint::DatabaseImpl,
                    format!("Reading column names for table {:?}.", write_point_hash),
                    false,
                );
                let columns = {
                    let columns_size_idx = stack_impl.0.get_with_step()?;
                    let mut columns: Vec<i64> = Vec::new();
                    for _ in 0..columns_size_idx as usize {
                        columns.push(stack_impl.0.get_with_step()?);
                    }
                    columns
                };

                caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                    TracePoint::DatabaseImpl,
                    format!(
                        "Reading data segments for table {:?} with columns {:?}.",
                        write_point_hash, columns
                    ),
                    false,
                );
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
                let vm = context
                    .vm
                    .as_ref()
                    .ok_or_else(|| HostError::NoContext)?
                    .upgrade()
                    .ok_or_else(|| HostError::InternalError(InternalError::CannotUpgradeRc))?;
                let mem_manager = &vm.memory_manager;
                stack_impl.0.clear();

                (mem_manager.memory, write_point_hash, columns, data_segments)
            };

            caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                TracePoint::DatabaseImpl,
                format!(
                    "Using {} segment pairs to retrieve the data from linear memory.",
                    segments.len()
                ),
                false,
            );
            let aggregated_data = segments
                .iter()
                .map(|segment| Self::read_segment_from_memory(&memory, &caller, *segment))
                .collect::<Result<Vec<_>, _>>()?;

            {
                let host = caller.data();
                let db_obj = host.0.database.borrow();
                let db_impl = &db_obj.0;

                if let DatabasePermissions::ReadOnly = db_impl.permissions {
                    return Err(DatabaseError::WriteOnReadOnly.into());
                }

                caller.data().0.stack_trace.borrow_mut().maybe_add_trace(TracePoint::DatabaseImpl, format!("Delegating database insertion instructions to generic database implementation."), false);
                db_impl.db.write_raw(
                    host.get_host_id(),
                    write_point_hash,
                    &columns,
                    aggregated_data,
                )?;
            };

            Ok(())
        })();

        (caller, effect)
    }

    pub(crate) fn update_database_raw(caller: Caller<Self>) -> (Caller<Self>, Result<()>) {
        let effect = (|| {
            let (memory, write_point_hash, columns, segments, conditions, conditions_args) = {
                let host = caller.data();

                let stack_impl = host.as_stack_mut();

                let id = {
                    let value = host.get_host_id();
                    utils::bytes::i64_to_bytes(value)
                };

                caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                    TracePoint::DatabaseImpl,
                    "Reading the table name.",
                    false,
                );
                let write_point_hash: [u8; 16] = {
                    let point_raw = stack_impl.0.get_with_step()?;
                    let point_bytes = utils::bytes::i64_to_bytes(point_raw);
                    md5::compute([point_bytes, id].concat()).into()
                };

                caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                    TracePoint::DatabaseImpl,
                    format!("Reading column names for table {:?}.", write_point_hash),
                    false,
                );
                let columns = {
                    let columns_size_idx = stack_impl.0.get_with_step()?;
                    let mut columns: Vec<i64> = Vec::new();

                    for _ in 0..columns_size_idx as usize {
                        columns.push(stack_impl.0.get_with_step()?);
                    }

                    columns
                };

                caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                    TracePoint::DatabaseImpl,
                    format!(
                        "Reading data segments for table {:?} with columns {:?}.",
                        write_point_hash, columns
                    ),
                    false,
                );
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

                caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                    TracePoint::DatabaseImpl,
                    format!(
                        "Reading conditions for table {:?} with columns {:?}.",
                        write_point_hash, columns
                    ),
                    false,
                );
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

                caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                    TracePoint::DatabaseImpl,
                    format!(
                        "Reading condition arguments for table {:?} with columns {:?}.",
                        write_point_hash, columns
                    ),
                    false,
                );
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
                let vm = context
                    .vm
                    .as_ref()
                    .ok_or_else(|| HostError::NoContext)?
                    .upgrade()
                    .ok_or_else(|| HostError::InternalError(InternalError::CannotUpgradeRc))?;
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

            caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                TracePoint::DatabaseImpl,
                format!(
                    "Using {} segment pairs to retrieve the data from linear memory.",
                    segments.len()
                ),
                false,
            );
            let aggregated_data = segments
                .iter()
                .map(|segment| Self::read_segment_from_memory(&memory, &caller, *segment))
                .collect::<Result<Vec<_>, _>>()?;

            caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                TracePoint::DatabaseImpl,
                format!(
                    "Using {} segment pairs to retrieve the condition args from linear memory.",
                    segments.len()
                ),
                false,
            );
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

            caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                TracePoint::DatabaseImpl,
                format!(
                    "Delegating database update instructions to generic database implementation."
                ),
                false,
            );
            db_impl.db.update_raw(
                host.get_host_id(),
                write_point_hash,
                &columns,
                aggregated_data,
                &conditions,
                aggregated_conditions_args,
            )?;

            Ok(())
        })();

        (caller, effect)
    }

    // todo: read from other id notice for payment.
    pub(crate) fn read_database_as_id(
        caller: Caller<Self>,
        host_id: i64,
    ) -> (Caller<Self>, Result<(i64, i64)>) {
        let host = caller.data();

        let raw_read = host.read_database_raw(host_id, &caller);
        let read = if let Ok(read) = raw_read {
            read
        } else {
            return (caller, Err(raw_read.err().unwrap()));
        };

        Self::write_to_memory(caller, read)
    }

    pub(crate) fn read_database_self(caller: Caller<Self>) -> (Caller<Self>, Result<(i64, i64)>) {
        let host = caller.data();
        let host_id = host.get_host_id();

        let raw_read = host.read_database_raw(host_id, &caller);
        let read = if let Ok(read) = raw_read {
            read
        } else {
            return (caller, Err(raw_read.err().unwrap()));
        };
        Self::write_to_memory(caller, read)
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

            caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                TracePoint::DatabaseImpl,
                "Reading the table name.",
                false,
            );
            let read_point_hash: [u8; 16] = {
                let point_raw = stack_impl.get_with_step()?;
                let point_bytes = utils::bytes::i64_to_bytes(point_raw);

                md5::compute([point_bytes, id].concat()).into()
            };

            caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                TracePoint::DatabaseImpl,
                format!("Reading column names for table {:?}.", read_point_hash),
                false,
            );
            let read_data = {
                let data_size_idx = stack_impl.get_with_step()?;
                let mut retrn = Vec::new();

                for _ in 0..data_size_idx {
                    retrn.push(stack_impl.get_with_step()?);
                }
                retrn
            };

            caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                TracePoint::DatabaseImpl,
                format!(
                    "Reading conditions for table {:?} with columns {:?}.",
                    read_point_hash, read_data
                ),
                false,
            );
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

            caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                TracePoint::DatabaseImpl,
                format!(
                    "Reading condition arguments for table {:?} with columns {:?}.",
                    read_point_hash, read_data
                ),
                false,
            );
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

            caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                TracePoint::DatabaseImpl,
                format!(
                    "Aggregating condition arguments for table {:?} with columns {:?}.",
                    read_point_hash, read_data
                ),
                false,
            );
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

            caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                TracePoint::DatabaseImpl,
                format!(
                    "Delegating database read instructions to generic database implementation."
                ),
                false,
            );
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
