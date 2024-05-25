use std::fmt::Debug;

use crate::{
    db::{database::ZephyrDatabase, ledger::LedgerStateRead},
    host::{FunctionInfo, Host, SorobanTempFunctionInfo},
};
use soroban_env_host::wasmi::{
    core::{Trap, TrapCode::BadSignature},
    Value,
};
use soroban_env_host::{
    AddressObject, Bool, BytesObject, DurationObject, Error, HostError, I128Object, I256Object,
    I256Val, I64Object, MapObject, StorageType, StringObject, Symbol, SymbolObject,
    TimepointObject, TryFromVal, U128Object, U256Object, U256Val, U32Val, U64Object, U64Val, Val,
    VecObject, VmCaller, Void, WasmiMarshal,
};
use soroban_env_macros::generate_call_macro_with_all_host_functions;

use soroban_env_host::{
    xdr::{ContractCostType, Hash, ScErrorCode, ScErrorType},
    CheckedEnvArg, EnvBase, Host as SorobanHost, VmCallerEnv,
};

use wasmi::{Func, Store};

pub(crate) trait RelativeObjectConversion: WasmiMarshal + Clone {
    fn absolute_to_relative(self, _host: &SorobanHost) -> Result<Self, HostError> {
        Ok(self)
    }
    fn relative_to_absolute(self, _host: &SorobanHost) -> Result<Self, HostError> {
        Ok(self)
    }
    fn try_marshal_from_relative_value(
        v: soroban_env_host::wasmi::Value,
        host: &SorobanHost,
    ) -> Result<Self, Trap> {
        let val = Self::try_marshal_from_value(v).ok_or_else(|| {
            Trap::from(HostError::from(Error::from_type_and_code(
                ScErrorType::Value,
                ScErrorCode::InvalidInput,
            )))
        })?;

        let backup = val.clone();

        Ok(val.relative_to_absolute(host).unwrap_or(backup))
    }
    fn marshal_relative_from_self(
        self,
        host: &SorobanHost,
    ) -> Result<soroban_env_host::wasmi::Value, Trap> {
        let backup = self.clone();

        let rel = self.absolute_to_relative(host).unwrap_or(backup);

        Ok(Self::marshal_from_self(rel))
    }
}

macro_rules! impl_relative_object_conversion {
    ($T:ty) => {
        impl RelativeObjectConversion for $T {
            fn absolute_to_relative(self, host: &SorobanHost) -> Result<Self, HostError> {
                Ok(Self::try_from(host.absolute_to_relative(self.into())?)?)
            }

            fn relative_to_absolute(self, host: &SorobanHost) -> Result<Self, HostError> {
                Ok(Self::try_from(host.relative_to_absolute(self.into())?)?)
            }
        }
    };
}

enum TraceArg<T: Debug> {
    Bad(i64),
    Ok(T),
}
impl<T> Debug for TraceArg<T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TraceArg::Bad(i) => write!(f, "bad:{:?}", i),
            TraceArg::Ok(t) => write!(f, "{:?}", t),
        }
    }
}

macro_rules! homogenize_tuple {
    ($u:ident, ()) => {
        &[]
    };
    ($u:ident, ($_a:expr)) => {
        &[&$u]
    };
    ($u:ident, ($_a:expr, $_b:expr)) => {
        &[&$u.0, &$u.1]
    };
    ($u:ident, ($_a:expr, $_b:expr, $_c:expr)) => {
        &[&$u.0, &$u.1, &$u.2]
    };
    ($u:ident, ($_a:expr, $_b:expr, $_c:expr, $_d:expr)) => {
        &[&$u.0, &$u.1, &$u.2, &$u.3]
    };
    ($u:ident, ($_a:expr, $_b:expr, $_c:expr, $_d:expr, $_e:expr)) => {
        &[&$u.0, &$u.1, &$u.2, &$u.3, &$u.4]
    };
}

// Define a relative-to-absolute impl for any type that is (a) mentioned
// in a host function type signature in env and (b) might possibly carry an
// object reference. If you miss one, this file won't compile, so it's safe.
impl_relative_object_conversion!(Val);
impl_relative_object_conversion!(Symbol);

impl_relative_object_conversion!(AddressObject);
impl_relative_object_conversion!(BytesObject);
impl_relative_object_conversion!(DurationObject);

impl_relative_object_conversion!(TimepointObject);
impl_relative_object_conversion!(SymbolObject);
impl_relative_object_conversion!(StringObject);

impl_relative_object_conversion!(VecObject);
impl_relative_object_conversion!(MapObject);

impl_relative_object_conversion!(I64Object);
impl_relative_object_conversion!(I128Object);
impl_relative_object_conversion!(I256Object);

impl_relative_object_conversion!(U64Object);
impl_relative_object_conversion!(U128Object);
impl_relative_object_conversion!(U256Object);

impl_relative_object_conversion!(U64Val);
impl_relative_object_conversion!(U256Val);
impl_relative_object_conversion!(I256Val);

// Trivial / non-relativizing impls are ok for types that can't carry objects.
impl RelativeObjectConversion for i64 {}
impl RelativeObjectConversion for u64 {}
impl RelativeObjectConversion for Void {}
impl RelativeObjectConversion for Bool {}
impl RelativeObjectConversion for Error {}
impl RelativeObjectConversion for StorageType {}
impl RelativeObjectConversion for U32Val {}

macro_rules! generate_dispatch_functions {
    {
        $(
            // This outer pattern matches a single 'mod' block of the token-tree
            // passed from the x-macro to this macro. It is embedded in a `$()*`
            // pattern-repetition matcher so that it will match all provided
            // 'mod' blocks provided.
            $(#[$mod_attr:meta])*
            mod $mod_name:ident $mod_str:literal
            {
                $(
                    // This inner pattern matches a single function description
                    // inside a 'mod' block in the token-tree passed from the
                    // x-macro to this macro. It is embedded in a `$()*`
                    // pattern-repetition matcher so that it will match all such
                    // descriptions.
                    $(#[$fn_attr:meta])*
                    { $fn_str:literal, $($min_proto:literal)?, $($max_proto:literal)?, fn $fn_id:ident ($($arg:ident:$type:ty),*) -> $ret:ty }
                )*
            }
        )*
    }

    =>  // The part of the macro above this line is a matcher; below is its expansion.

    {
        // This macro expands to multiple items: a set of free functions in the
        // current module, which are called by functions registered with the VM
        // to forward calls to the host.
        $(
            $(
                // This defines a "dispatch function" that does several things:
                //
                //  1. Transfers the running "VM fuel" balance from wasmi to the
                //     host's CPU budget.
                //  2. Charges the host budget for the call, failing if over.
                //  3. Attempts to convert incoming wasmi i64 args to Vals or
                //     Val-wrappers expected by host functions, failing if any
                //     conversions fail. This step also does
                //     relative-to-absolute object reference conversion.
                //  4. Calls the host function.
                //  5. Augments any error result with this calling context, so
                //     that we get at minimum a "which host function failed"
                //     context on error.
                //  6. Converts the result back to an i64 for wasmi, again
                //     converting from absolute object references to relative
                //     along the way.
                //  7. Checks the result is Ok, or escalates Err to a VM Trap.
                //  8. Transfers the residual CPU budget back to wasmi "VM
                //     fuel".
                //
                // It is embedded in two nested `$()*` pattern-repetition
                // expanders that correspond to the pattern-repetition matchers
                // in the match section, but we ignore the structure of the
                // 'mod' block repetition-level from the outer pattern in the
                // expansion, flattening all functions from all 'mod' blocks
                // into a set of functions.
                $(#[$fn_attr])*
                pub(crate) fn $fn_id<DB: ZephyrDatabase + Clone + 'static, L: LedgerStateRead + 'static>(mut caller: wasmi::Caller<Host<DB, L>>, $($arg:i64),*) ->
                    (i64,)
                {
                    //let _span = tracy_span!(core::stringify!($fn_id));

                    let host: soroban_env_host::Host = Host::<DB, L>::soroban_host(&caller);
                    host.enable_debug();
                    let effects = || {
                        // This is an additional protocol version guardrail that
                        // should not be necessary. Any wasm contract containing a
                        // call to an out-of-protocol-range host function should
                        // have been rejected by the linker during VM instantiation.
                        // This is just an additional guard rail for future proof.
                        //$( host.check_protocol_version_lower_bound($min_proto)?; )?
                        //$( host.check_protocol_version_upper_bound($max_proto)?; )?

                        /*if host.tracing_enabled()
                        {
                            #[allow(unused)]
                            let trace_args = ($(
                                match <$type>::try_marshal_from_relative_value(Value::I64($arg), &host) {
                                    Ok(val) => TraceArg::Ok(val),
                                    Err(_) => TraceArg::Bad($arg),
                                }
                            ),*);
                            let hook_args: &[&dyn std::fmt::Debug] = homogenize_tuple!(trace_args, ($($arg),*));
                            host.trace_env_call(&core::stringify!($fn_id), hook_args)?;
                        }*/

                        // This is where the VM -> Host boundary is crossed.
                        // We first return all fuels from the VM back to the host such that
                        // the host maintains control of the budget.
                        //FuelRefillable::return_fuel_to_host(&mut caller, &host).map_err(|he| Trap::from(he))?;

                        // Charge for the host function dispatching: conversion between VM fuel and
                        // host budget, marshalling values. This does not account for the actual work
                        // being done in those functions, which are metered individually by the implementation.
                        //host.charge_budget(ContractCostType::DispatchHostFunction, None)?;
                        let mut vmcaller = VmCaller::none();
                        // The odd / seemingly-redundant use of `soroban_env_host::wasmi::Value` here
                        // as intermediates -- rather than just passing Vals --
                        // has to do with the fact that some host functions are
                        // typed as receiving or returning plain _non-val_ i64 or
                        // u64 values. So the call here has to be able to massage
                        // both types into and out of i64, and `soroban_env_host::wasmi::Value`
                        // happens to be a natural switching point for that: we have
                        // conversions to and from both Val and i64 / u64 for
                        // soroban_env_host::wasmi::Value.
                        let res: Result<_, HostError> = host.$fn_id(&mut vmcaller, $(<$type>::check_env_arg(<$type>::try_marshal_from_relative_value(Value::I64($arg), &host).unwrap(), &host).unwrap()),*);
                        res
                    };


                    (host.with_test_contract_frame(Hash([0;32]), Symbol::from_small_str("test"), || {
                        let res = effects();
                        /*if host.tracing_enabled()
                        {
                            let dyn_res: Result<&dyn core::fmt::Debug,&HostError> = match &res {
                                Ok(ref ok) => Ok(ok),
                                Err(err) => Err(err)
                            };
                            host.trace_env_ret(&core::stringify!($fn_id), &dyn_res)?;
                        }*/

                        // On the off chance we got an error with no context, we can
                        // at least attach some here "at each host function call",
                        // fairly systematically. This will cause the context to
                        // propagate back through wasmi to its caller.
                        /*let res = host.augment_err_result(res);

                        let res = match res {
                            Ok(ok) => {
                                let ok = ok.check_env_arg(&host).unwrap();
                                let val: Value = ok.marshal_relative_from_self(&host).unwrap();
                                if let Value::I64(v) = val {
                                    Ok((v,))
                                } else {
                                    Err(BadSignature.into())
                                }
                            },
                            Err(hosterr) => {
                                // We make a new HostError here to capture the escalation event itself.
                                let escalation: HostError =
                                    host.error(hosterr.into(),
                                            concat!("escalating error to VM trap from failed host function call: ",
                                                    stringify!($fn_id)), &[]);
                                let trap: Trap = escalation.into();
                                Err(trap)
                            }
                        };

                        // This is where the Host->VM boundary is crossed.
                        // We supply the remaining host budget as fuel to the VM.
                        //let caller = vmcaller.try_mut().map_err(|e| Trap::from(HostError::from(e))).unwrap();
                        //FuelRefillable::add_fuel_to_vm(caller, &host).map_err(|he| Trap::from(he))?;

                        Ok(res.unwrap())*/
                        let res = match res {
                            Ok(ok) => {
                                let ok = ok.check_env_arg(&host).unwrap();

                                let val: Value = ok.marshal_relative_from_self(&host).unwrap();
                                if let Value::I64(v) = val {
                                    Ok((v,))
                                } else {
                                    Err(BadSignature.into())
                                }
                            },
                            Err(hosterr) => {
                                // We make a new HostError here to capture the escalation event itself.
                                let escalation: HostError =
                                    host.error(hosterr.into(),
                                            concat!("escalating error to VM trap from failed host function call: ",
                                                    stringify!($fn_id)), &[]);
                                let trap: Trap = escalation.into();
                                Err(trap)
                            }
                        };

                        Ok(Val::from_payload(res.unwrap().0 as u64))

                }).unwrap().get_payload() as i64, )
                }
            )*
        )*
    };
}

// Here we invoke the x-macro passing generate_dispatch_functions as its callback macro.
generate_call_macro_with_all_host_functions!("../soroban/env.json");

call_macro_with_all_host_functions! { generate_dispatch_functions }

macro_rules! host_function_info_helper {
    {$mod_str:literal, $fn_id:literal, $args:tt, $func_id:ident } => {
        SorobanTempFunctionInfo {
            module: $mod_str,
            func: $fn_id,
            wrapped: |store| Func::wrap(store, $func_id),
        }
    };
}

///////////////////////////////////////////////////////////////////////////////
/// X-macro use: static HOST_FUNCTIONS array of HostFuncInfo
///////////////////////////////////////////////////////////////////////////////

// This is a callback macro that pattern-matches the token-tree passed by the
// x-macro (call_macro_with_all_host_functions) and produces a suite of
// dispatch-function definitions.
macro_rules! generate_host_function_infos {
    {
        $(
            // This outer pattern matches a single 'mod' block of the token-tree
            // passed from the x-macro to this macro. It is embedded in a `$()*`
            // pattern-repetition matcher so that it will match all provided
            // 'mod' blocks provided.
            $(#[$mod_attr:meta])*
            mod $mod_id:ident $mod_str:literal
            {
                $(
                    // This inner pattern matches a single function description
                    // inside a 'mod' block in the token-tree passed from the
                    // x-macro to this macro. It is embedded in a `$()*`
                    // pattern-repetition matcher so that it will match all such
                    // descriptions.
                    $(#[$fn_attr:meta])*
                    { $fn_id:literal, $($min_proto:literal)?, $($max_proto:literal)?, fn $func_id:ident $args:tt -> $ret:ty }
                )*
            }
        )*
    }

    =>   // The part of the macro above this line is a matcher; below is its expansion.

    {
        // This macro expands to a single item: a static array of HostFuncInfo, used by
        // two places:
        //
        //   1. The VM WASM-module instantiation step to resolve all import functions to numbers
        //       and typecheck their signatures (represented here by a simple arity number, since
        //       every host function we have just takes N i64 values and returns an i64).
        //
        //   2. The function dispatch path when guest code calls out of the VM, where we
        //      look up the numbered function the guest is requesting in this array and
        //      call its associated dispatch function.

        pub(crate) fn get_all_host_functions<DB, L>() -> Vec<SorobanTempFunctionInfo<DB, L>> where DB: ZephyrDatabase + Clone + 'static, L: LedgerStateRead + 'static {
            let mut fns: Vec<SorobanTempFunctionInfo<DB, L>> = Vec::new();

            for f in [
                $(
                     $(
                         // This generates a HostFuncInfo struct directly
                         // for each function matched in the token-tree (invoking
                         // the arity_helper! macro above to calculate the arity
                         // of each function along the way). It is embedded in two
                         // nested `$()*` pattern-repetition expanders that
                         // correspond to the pattern-repetition matchers in the
                         // match section, but we ignore the structure of the 'mod'
                         // block repetition-level from the outer pattern in the
                         // expansion, flattening all functions from all 'mod' blocks
                         // into the a single array of HostFuncInfo structs.
                         host_function_info_helper!{$mod_str, $fn_id, $args, $func_id},
                     )*
                 )*
            ] {
                fns.push(f)
            }

            fns
        }
    };
}

call_macro_with_all_host_functions! { generate_host_function_infos }

pub fn generate_host_fn_infos<DB, L>(store: &mut Store<Host<DB, L>>) -> Vec<FunctionInfo>
where
    DB: ZephyrDatabase + Clone + 'static,
    L: LedgerStateRead + 'static,
{
    // Here we invoke the x-macro passing generate_host_function_infos as its callback macro.
    let store = store;

    let functions = get_all_host_functions::<DB, L>()
        .iter()
        .map(|temp| FunctionInfo {
            module: temp.module,
            func: temp.func,
            wrapped: (temp.wrapped)(store),
        })
        .collect();

    functions
}
