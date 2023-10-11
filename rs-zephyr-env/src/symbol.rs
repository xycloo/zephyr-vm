//! We use Soroban's Symbol type for ease of implementation.
//!
//! This does not mean Zephyr encompasses in any way the Soroban environoment.

/// Wrapper around the inner small symbol value.
/// Decodes the integer to a string with at
/// maximum 9 characters. The idea and implementation
/// are taken from the Soroban implementation.
pub struct Symbol(pub i64);

impl Symbol {
    /// Creates a new wrapper for a given val.
    pub fn new() -> Self {
        Self(0)
    }
}
