const TAG: u8 = 14;

#[derive(Debug)]
pub enum SymbolError {
    InvalidSymbol,
}

pub struct Symbol(pub u64);

impl Symbol {
    pub fn to_string(&self) -> Result<String, SymbolError> {
        let mut body = self.0;

        if (body & (TAG as u64)) != (TAG as u64) {
            return Err(SymbolError::InvalidSymbol);
        }

        body >>= 8; // Remove the tag
        let mut result = String::new();

        while body > 0 {
            let index = (body & 0x3F) as u8;
            body >>= 6;
            let ch = match index {
                1 => '_',
                2..=11 => (b'0' + index - 2) as char,
                12..=37 => (b'A' + index - 12) as char,
                38..=63 => (b'a' + index - 38) as char,
                _ => return Err(SymbolError::InvalidSymbol),
            };
            result.push(ch);
        }

        Ok(result.chars().rev().collect())
    }

    pub fn from_body(body: u64) -> Self {
        Symbol((body << 8) | (TAG as u64))
    }

    fn encode_char(ch: char) -> Result<u64, SymbolError> {
        let v = match ch {
            '_' => 1,
            '0'..='9' => 2 + ((ch as u64) - ('0' as u64)),
            'A'..='Z' => 12 + ((ch as u64) - ('A' as u64)),
            'a'..='z' => 38 + ((ch as u64) - ('a' as u64)),
            _ => return Err(SymbolError::InvalidSymbol),
        };
        Ok(v)
    }

    pub fn try_from_bytes(b: &[u8]) -> Result<Self, SymbolError> {
        let mut n = 0;
        let mut accum: u64 = 0;
        while n < b.len() {
            let ch = b[n] as char;
            if n >= 9 {
                return Err(SymbolError::InvalidSymbol);
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
