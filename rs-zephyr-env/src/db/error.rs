use thiserror::Error;

#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("Invalid permissions. Tried reading when in write-only")]
    ReadOnWriteOnly,

    #[error("Invalid permissions. Tried writing when in read-only")]
    WriteOnReadOnly,

    #[error("Zephyr query malformed.")]
    ZephyrQueryMalformed,

    #[error("Zephyr query error.")]
    ZephyrQueryError,

    #[error("Unable to write to DB.")]
    WriteError,
}
