use thiserror::Error;

#[derive(Error, Debug)]
pub enum HostError {
    #[error("Binary does not export Zephyr entry function")]
    NoEntryPointExport,

    #[error("Extern is not a function")]
    ExternNotAFunction,

    #[error("No value on stack")]
    NoValOnStack
}
