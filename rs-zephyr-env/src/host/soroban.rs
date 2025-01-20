use super::Host;
use crate::{
    db::{database::ZephyrDatabase, ledger::LedgerStateRead},
    error::{HostError, InternalError},
    snapshot::{snapshot_utils, DynamicSnapshot},
    trace::TracePoint,
};
use anyhow::Result;
use soroban_env_host::{
    budget::AsBudget,
    xdr::{
        AccountId, Hash, HostFunction, LedgerEntryData, Limits, PublicKey, ReadXdr, ScAddress,
        ScVal, Uint256, WriteXdr,
    },
    Env, LedgerInfo, Symbol, TryFromVal, Val,
};
use soroban_simulation::{simulation::SimulationAdjustmentConfig, NetworkConfig};
use std::rc::Rc;
use wasmi::Caller;

impl<DB: ZephyrDatabase + Clone + 'static, L: LedgerStateRead + 'static> Host<DB, L> {
    /// Returns the Soroban host object associated to the Zephyr host.
    pub fn soroban_host(caller: &Caller<Self>) -> soroban_env_host::Host {
        let host = caller.data();
        host.0.soroban.borrow().to_owned()
    }

    pub(crate) fn internal_read_contract_data_entry_by_contract_id_and_key(
        caller: Caller<Self>,
        contract: [u8; 32],
        key: ScVal,
    ) -> (Caller<Self>, Result<(i64, i64)>) {
        let host = caller.data();

        let contract = ScAddress::Contract(Hash(contract));
        let read = {
            let ledger = &host.0.ledger.0.ledger;
            bincode::serialize(
                &ledger.read_contract_data_entry_by_contract_id_and_key(contract, key),
            )
            .unwrap()
        };

        Self::write_to_memory(caller, read)
    }

    pub(crate) fn read_contract_data_entry_by_contract_id_and_key(
        caller: Caller<Self>,
        contract: [u8; 32],
        offset: i64,
        size: i64,
    ) -> (Caller<Self>, Result<(i64, i64)>) {
        let effect = (|| {
            let host = caller.data();

            let key = {
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

                ScVal::from_xdr(
                    Self::read_segment_from_memory(&memory, &caller, segment)?,
                    Limits::none(),
                )?
            };

            Ok(key)
        })();

        let key = if let Ok(key) = effect {
            key
        } else {
            return (caller, Err(effect.err().unwrap()));
        };

        Self::internal_read_contract_data_entry_by_contract_id_and_key(caller, contract, key)
    }

    pub(crate) fn read_contract_instance(
        caller: Caller<Self>,
        contract: [u8; 32],
    ) -> (Caller<Self>, Result<(i64, i64)>) {
        let key = ScVal::LedgerKeyContractInstance;

        Self::internal_read_contract_data_entry_by_contract_id_and_key(caller, contract, key)
    }

    pub(crate) fn read_contract_entries(
        caller: Caller<Self>,
        contract: [u8; 32],
    ) -> (Caller<Self>, Result<(i64, i64)>) {
        let host = caller.data();

        let contract = ScAddress::Contract(Hash(contract));
        let read = {
            let ledger = &host.0.ledger.0.ledger;
            bincode::serialize(&ledger.read_contract_data_entries_by_contract_id(contract)).unwrap()
        };

        Self::write_to_memory(caller, read)
    }

    pub(crate) fn read_account_object(
        caller: Caller<Self>,
        account: [u8; 32],
    ) -> (Caller<Self>, Result<(i64, i64)>) {
        let host = caller.data();
        let account = stellar_strkey::ed25519::PublicKey(account).to_string();

        let read = {
            let ledger = &host.0.ledger.0.ledger;
            bincode::serialize(&ledger.read_account(account)).unwrap()
        };

        Self::write_to_memory(caller, read)
    }

    pub(crate) fn scval_to_valid_host_val(
        caller: Caller<Self>,
        scval: ScVal,
    ) -> (Caller<Self>, Result<i64>) {
        let val = (|| {
            let host = caller.data();

            let (soroban, val) = {
                let soroban = host.0.soroban.borrow().to_owned();
                soroban.as_budget().reset_unlimited().unwrap();

                soroban.enable_debug().unwrap();

                let val = soroban
                    .with_test_contract_frame(
                        Hash([0; 32]),
                        Symbol::from_small_str("test"),
                        || soroban.to_valid_host_val(&scval),
                    )?
                    .get_payload() as i64;

                (soroban, val)
            };

            *host.0.soroban.borrow_mut() = soroban;

            Ok(val)
        })();

        (caller, val)
    }

    pub(crate) fn valid_host_val_to_scval(
        caller: Caller<Self>,
        val: Val,
    ) -> (Caller<Self>, Result<(i64, i64)>) {
        let host = caller.data();

        let res = {
            let soroban = host.0.soroban.borrow().to_owned();
            soroban.as_budget().reset_unlimited().unwrap();
            soroban.enable_debug().unwrap();

            caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                TracePoint::SorobanEnvironment,
                format!("Converting host value to SCVal."),
                false,
            );

            let scval = ScVal::try_from_val(&soroban, &val)
                .map_err(|e| HostError::SorobanHostWithContext(e));
            let scval = if let Ok(scval) = scval {
                scval
            } else {
                return (caller, Err(scval.err().unwrap().into()));
            };

            Self::write_to_memory(caller, scval.to_xdr(Limits::none()).unwrap())
        };

        res
    }

    pub(crate) fn simulate_soroban_transaction(
        caller: Caller<Self>,
        source: [u8; 32],
        offset: i64,
        size: i64,
    ) -> (Caller<Self>, Result<(i64, i64)>) {
        let resp = (|| {
            let host = caller.data();
            let host_fn = {
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
                let bytes = Self::read_segment_from_memory(&memory, &caller, segment)?;

                HostFunction::from_xdr(bytes, Limits::none())?
            };

            caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                TracePoint::SorobanEnvironment,
                format!("Simulating host function {:?}.", host_fn),
                false,
            );

            let snapshot_source = Rc::new(DynamicSnapshot {});
            let source = AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(source)));
            let mut ledger_info = LedgerInfo::default();
            ledger_info.protocol_version = 21;
            let ledger_from_state = snapshot_utils::get_current_ledger_sequence();
            ledger_info.sequence_number = ledger_from_state.0 as u32;
            ledger_info.timestamp = ledger_from_state.1 as u64;
            ledger_info.network_id = host.0.network_id;
            ledger_info.max_entry_ttl = 3110400;
            let bucket_size: u64 = {
                let string = std::fs::read_to_string("/tmp/currentbucketsize")?; // unrecoverable: todo handle this
                string.parse()?
            };

            caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                TracePoint::SorobanEnvironment,
                format!("Current bucket size is {}.", bucket_size),
                false,
            );
            let network_config =
                NetworkConfig::load_from_snapshot(&DynamicSnapshot {}, bucket_size)?;
            network_config.fill_config_fields_in_ledger_info(&mut ledger_info);
            let random_prng_seed = rand::Rng::gen(&mut rand::thread_rng());

            let resp = soroban_simulation::simulation::simulate_invoke_host_function_op(
                snapshot_source,
                Some(network_config),
                &SimulationAdjustmentConfig::default_adjustment(),
                &ledger_info,
                host_fn,
                None,
                &source,
                random_prng_seed,
                true,
            )?;

            caller.data().0.stack_trace.borrow_mut().maybe_add_trace(
                TracePoint::SorobanEnvironment,
                format!("Simulated with result {:?}.", resp.invoke_result),
                false,
            );

            Ok(resp)
        })();

        let resp = if let Ok(resp) = resp {
            resp
        } else {
            return (caller, Err(resp.err().unwrap()));
        };

        Self::write_to_memory(caller, bincode::serialize(&resp).unwrap())
    }

    /// Reads contract entries to a memory slot on the Soroban Host environment.
    pub(crate) fn read_contract_entries_to_env(
        caller: Caller<Self>,
        contract: [u8; 32],
    ) -> Result<i64> {
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

            let val = soroban
                .with_test_contract_frame(Hash([0; 32]), Symbol::from_small_str("test"), || {
                    let mut map = soroban.map_new()?;

                    for entry in data {
                        let LedgerEntryData::ContractData(d) = entry.entry.data else {
                            panic!("invalid xdr")
                        };

                        if d.key != ScVal::LedgerKeyContractInstance {
                            let key = soroban.to_valid_host_val(&d.key)?;
                            let val = soroban.to_valid_host_val(&d.val)?;

                            map = soroban.map_put(map, key, val)?;
                        }
                    }

                    soroban.enable_debug().unwrap();

                    Ok(map.into())
                })?
                .get_payload() as i64;

            (soroban, val)
        };

        *host.0.soroban.borrow_mut() = soroban;

        Ok(val)
    }
}
