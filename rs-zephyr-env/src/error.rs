use thiserror::Error;

#[derive(Error, Debug)]
pub enum InternalError {
    #[error("Faulty wasmi configuration")]
    WasmiConfig,

    #[error("Error while performing arithmetic calc")]
    ArithError,

    #[error("Cannot upgrade weak to rc")]
    CannotUpgradeRc,
}

#[derive(Error, Debug)]
pub enum HostError {
    #[error("Binary does not export Zephyr entry function")]
    NoEntryPointExport,

    #[error("Extern is not a function")]
    ExternNotAFunction,

    #[error("Tried loading contex where context already exists")]
    ContextAlreadyExists,

    #[error("Tried using VM contex where none exists")]
    NoContext,

    #[error("Zephyr cannot operate without memory export")]
    NoMemoryExport,

    #[error("Tried reading stack at an index where no value is on it")]
    NoValOnStack,

    #[error("Tried overwriting ledger close meta")]
    LedgerCloseMetaOverridden,

    #[error("Requested ledger close meta but it is none")]
    NoLedgerCloseMeta,

    #[error("Requested ledger entry doesn't exist")]
    NoLedgerEntry,

    #[error("Invalid types found on function result")]
    InvalidFunctionResult,

    #[error("Tried using the transmitter but didn't provide one")]
    NoTransmitter,

    #[error("Internal Error")]
    InternalError(InternalError),

    #[error("Error on the Soroban host side")]
    SorobanHost,
}
