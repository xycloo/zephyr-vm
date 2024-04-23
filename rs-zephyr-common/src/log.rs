use serde::{Deserialize, Serialize};

/// Zephyr permitted log levels.
/// More may be added in the future.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum LogLevel {
    Error,
    Warning,
    Debug,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ZephyrLog {
    pub level: LogLevel,
    pub message: String,
    pub data: Option<Vec<u8>>,
}
