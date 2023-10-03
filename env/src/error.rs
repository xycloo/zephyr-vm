use thiserror::Error;

#[derive(Error, Debug)]
pub enum HostError {
    #[error("Binary does not export Zephyr entry function")]
    NoEntryPointExport,

    #[error("Extern is not a function")]
    ExternNotAFunction,

    #[error("Tried loading contex where context already exists")]
    ContextAlreadyExists,

    #[error("Zephyr cannot operate without memory export")]
    NoMemoryExport,

    #[error("Tried reading stack at an index where no value is on it")]
    NoValOnStack
}
