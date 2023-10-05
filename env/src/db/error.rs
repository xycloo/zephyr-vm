use thiserror::Error;

#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("Invalid permissions. Tried reading when in write-only")]
    ReadOnWriteOnly,

    #[error("Invalid permissions. Tried writing when in read-only")]
    WriteOnReadOnly,
}
