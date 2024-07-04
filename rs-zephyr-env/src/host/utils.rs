pub(crate) mod bytes {
    pub fn i64_to_bytes(value: i64) -> [u8; 8] {
        let byte0 = ((value >> 0) & 0xFF) as u8;
        let byte1 = ((value >> 8) & 0xFF) as u8;
        let byte2 = ((value >> 16) & 0xFF) as u8;
        let byte3 = ((value >> 24) & 0xFF) as u8;
        let byte4 = ((value >> 32) & 0xFF) as u8;
        let byte5 = ((value >> 40) & 0xFF) as u8;
        let byte6 = ((value >> 48) & 0xFF) as u8;
        let byte7 = ((value >> 56) & 0xFF) as u8;

        [byte0, byte1, byte2, byte3, byte4, byte5, byte6, byte7]
    }
}

pub(crate) mod soroban {
    use soroban_env_host::{ContractFunctionSet, Symbol, Val};

    pub struct ZephyrTestContract;

    impl ZephyrTestContract {
        pub fn new() -> Self {
            ZephyrTestContract {}
        }
    }

    impl ContractFunctionSet for ZephyrTestContract {
        fn call(
            &self,
            _func: &Symbol,
            _host: &soroban_env_host::Host,
            _args: &[Val],
        ) -> Option<Val> {
            None
        }
    }
}
