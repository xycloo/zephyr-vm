//! We use Soroban's Symbol type for ease of implementation.
//!
//! This does not mean Zephyr encompasses in any way the Soroban environoment.

/// Wrapper around the inner small symbol value.
/// Decodes the integer to a string with at
/// maximum 9 characters. The idea and implementation
/// are taken from the Soroban implementation.
const TAG: u8 = 14;

#[derive(Debug, thiserror::Error)]
pub enum SymbolError {
    /// Returned when attempting to form a [SymbolSmall] from a string with more
    /// than 9 characters.
    #[error(
        "Returned when attempting to form a Symbol from a string with more than 9 characters."
    )]
    TooLong(usize),

    /// Returned when attempting to form a [SymbolObject] or [SymbolSmall] from
    /// a string with characters outside the range `[a-zA-Z0-9_]`.
    #[error("Used bad characters")]
    BadChar(char),
}

pub struct Symbol(pub u64);

impl Symbol {
    pub fn from_body(body: u64) -> Self {
        Symbol((body << 8) | (TAG as u64))
    }

    fn encode_char(ch: char) -> Result<u64, SymbolError> {
        let v = match ch {
            '_' => 1,
            '0'..='9' => 2 + ((ch as u64) - ('0' as u64)),
            'A'..='Z' => 12 + ((ch as u64) - ('A' as u64)),
            'a'..='z' => 38 + ((ch as u64) - ('a' as u64)),
            _ => return Err(SymbolError::BadChar(ch)),
        };
        Ok(v)
    }

    pub fn try_from_bytes(b: &[u8]) -> Result<Self, SymbolError> {
        let mut n = 0;
        let mut accum: u64 = 0;
        while n < b.len() {
            let ch = b[n] as char;
            if n >= 9 {
                return Err(SymbolError::TooLong(b.len()));
            }
            n += 1;
            accum <<= 6;
            let v = match Self::encode_char(ch) {
                Ok(v) => v,
                Err(e) => return Err(e),
            };
            accum |= v;
        }
        Ok(Self::from_body(accum))
    }
}
