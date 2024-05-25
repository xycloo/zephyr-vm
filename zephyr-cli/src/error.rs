use thiserror::Error;

#[derive(Error, Debug)]
pub enum ParserError {
    #[error("Error when creating new table.")]
    TableCreationError,

    #[error("Error when deploying binary.")]
    WasmDeploymentError,

    #[error("Error when compiling program: {0}.")]
    WasmBuildError(String),
}
