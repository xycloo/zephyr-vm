use zephyr_sdk::{prelude::*, EnvClient, DatabaseDerive};

#[derive(DatabaseDerive)]
#[with_name("hello")]
pub struct Hello {
    tdep: i32    
}

impl Hello {
    pub fn t() -> Self {
        Self {
            tdep: true as i32
        }
    }

    pub fn f() -> Self {
        Self {
            tdep: false as i32
        }
    }
}

#[no_mangle]
pub extern "C" fn on_close() {
    let env = EnvClient::empty();

    env.put(&Hello::t());
    
    let read: Vec<Hello> = env.read();
    
    if read.len() != 1 {
        panic!()
    }

    if read[0].tdep == false as i32 {
        panic!()
    }

    env.update().column_equal_to("tdep", true as i32).execute(&Hello::f()).unwrap();

    let read: Vec<Hello> = env.read();
    
    if read.len() != 1 {
        panic!()
    }

    if read[0].tdep == true as i32 {
        panic!()
    }
}
