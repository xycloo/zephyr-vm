use std::{
    fmt,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Result;

/// Wrapper around the trace implementation. None when stack is disable for memn-efficient mode, Some when enabled.
#[derive(Clone, Debug)]
pub struct StackTrace(Option<Vec<TraceImpl>>);

#[derive(Clone, Debug)]
pub enum TracePoint {
    SorobanEnvironment,
    ZephyrEnvironment,
    DatabaseImpl,
    LedgerImpl,
}

#[derive(Clone, Debug)]
struct TraceImpl {
    trace_point: TracePoint,
    time: u128,
    message: String,

    // We want to tag errors to better recognize them. We don't need further debug levels.
    is_error: bool,
}

impl StackTrace {
    pub fn maybe_add_trace(&mut self, point: TracePoint, message: impl ToString, is_error: bool) {
        if let Some(traces) = self.0.as_mut() {
            let start = SystemTime::now();
            let since_the_epoch = start
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards");

            traces.push(TraceImpl {
                trace_point: point,
                time: since_the_epoch.as_millis(),
                message: message.to_string(),
                is_error,
            });
        }
    }

    pub fn enable(&mut self) {
        self.0 = Some(vec![])
    }

    pub fn disable(&mut self) {
        self.0 = None
    }

    // No method to clear the trace is needed for now.
}

impl Default for StackTrace {
    fn default() -> Self {
        Self(None)
    }
}

impl fmt::Display for TracePoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TracePoint::SorobanEnvironment => write!(f, "Soroban"),
            TracePoint::ZephyrEnvironment => write!(f, "Zephyr"),
            TracePoint::DatabaseImpl => write!(f, "Database"),
            TracePoint::LedgerImpl => write!(f, "Ledger"),
        }
    }
}

impl fmt::Display for StackTrace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            None => writeln!(f, "Empty stack trace"),
            Some(traces) => {
                writeln!(f, "Stack Trace ({} entries):", traces.len())?;
                for (index, trace) in traces.iter().enumerate() {
                    let error_indicator = if trace.is_error { "ERROR" } else { "INFO" };
                    writeln!(
                        f,
                        "{:3}. [{}] {:7} | {:7} | {}",
                        index + 1,
                        trace.time,
                        error_indicator,
                        trace.trace_point,
                        trace.message,
                    )?;
                }
                Ok(())
            }
        }
    }
}
