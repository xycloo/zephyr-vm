use crate::to_fixed;

pub struct WrappedMaxBytes(pub [u8; 8]);

impl From<i64> for WrappedMaxBytes {
    fn from(value: i64) -> Self {
        let mut buf = [0; 8];
        buf[..8].copy_from_slice(&value.to_be_bytes());

        Self(buf)
    }
}

impl From<&i64> for WrappedMaxBytes {
    fn from(value: &i64) -> Self {
        let mut buf = [0; 8];
        buf[..8].copy_from_slice(&value.to_be_bytes());

        Self(buf)
    }
}

impl Into<i64> for WrappedMaxBytes {
    fn into(self) -> i64 {
        u64::from_be_bytes(self.0[..8].try_into().unwrap()) as i64
    }
}

impl WrappedMaxBytes {
    pub fn array_from_max_parts<const N: usize>(parts: &[i64]) -> [u8; N] {
        let mut buf = [0_u8; N];
        let mut i = 0;

        for part in parts {
            let arr = Self::from(part).0;
            buf[i..i + 8].copy_from_slice(&arr);
            i += 8;
        }

        buf
    }

    pub fn array_to_max_parts<const N: usize>(array: &[u8]) -> [i64; N] {
        let mut buf = [0_i64; N];
        let mut i = 0;
        
        for n in (0..array.len()).step_by(8) {
            let part: i64 = Self(to_fixed(array[n..n+8].to_vec())).into();
            buf[i] = part;
            i += 1;
        }
        
        buf
    }

}