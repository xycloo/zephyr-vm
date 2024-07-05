//! Utilities for testing Zephyr programs and the ZephyrVM.
//!
//! Note:
//! Testing on the ZephyrVM is currently quite difficult as the host doesn't spawn VMs.
//! A Zephyr host is completely contained by the executing VM and cannot spawn other VMs
//! unlike VMs such as the Soroban VM were cross-host calls are allowed through spawning a new
//! VM to execute the binaries.
//!
pub mod database;
pub mod symbol;

use crate::{
    host::{utils, Host},
    vm::Vm,
    ZephyrMock,
};
use anyhow::Result as AnyResult;
use database::{LedgerReader, MercuryDatabase};
use postgres::NoTls;
use rs_zephyr_common::RelayedMessageRequest;
use std::{fs::File, io::Read, rc::Rc};
use symbol::Symbol;
use tokio::task::JoinError;

#[derive(Default)]
pub struct TestHost;

impl TestHost {
    pub fn database(&self, path: &str) -> MercuryDatabaseSetup {
        MercuryDatabaseSetup::setup_local(path)
    }

    pub fn new_program(&self, wasm_path: &str) -> TestVM {
        TestVM::import(wasm_path)
    }
}

pub fn read_wasm(path: &str) -> Vec<u8> {
    // todo: make this a compile-time macro.
    let mut file = File::open(path).unwrap();
    let mut binary = Vec::new();
    file.read_to_end(&mut binary).unwrap();

    binary.to_vec()
}

pub struct TestVM {
    wasm_path: String,
}

impl TestVM {
    pub fn import(path: &str) -> Self {
        Self {
            wasm_path: path.to_string(),
        }
    }

    pub async fn invoke_vm(&self, fname: impl ToString) -> Result<AnyResult<String>, JoinError> {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        let fname = fname.to_string();
        let wasm_path = self.wasm_path.clone();

        let invocation = tokio::runtime::Handle::current()
            .spawn_blocking(move || {
                let mut host: Host<MercuryDatabase, LedgerReader> = Host::mocked().unwrap();
                let vm = Vm::new(&host, &read_wasm(&wasm_path)).unwrap();
                host.load_context(Rc::downgrade(&vm)).unwrap();
                host.add_transmitter(tx);

                vm.metered_function_call(&host, &fname)
            })
            .await;

        let _ = tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                println!("Received message");
                let request: RelayedMessageRequest = bincode::deserialize(&message).unwrap();

                match request {
                    RelayedMessageRequest::Http(_) => {
                        // Note: http testing needs more thought since it's less rigorous
                        // than DB testing.
                    }

                    RelayedMessageRequest::Log(log) => {
                        println!("{:?}", log);
                    }
                }
            }
        })
        .await;

        invocation
    }
}

pub struct MercuryDatabaseSetup {
    dir: String,
    tables: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Column {
    name: String,
    col_type: String,
}

impl Column {
    pub fn with_name(name: &impl ToString) -> Self {
        Column {
            name: name.to_string(),
            col_type: "BYTEA".to_string(),
        }
    }
}

impl MercuryDatabaseSetup {
    pub fn setup_local(dir: &str) -> Self {
        Self {
            dir: dir.to_string(),
            tables: vec![],
        }
    }

    pub async fn get_rows_number(&self, id: i64, symbol: Symbol) -> anyhow::Result<usize> {
        let id = utils::bytes::i64_to_bytes(id);
        let bytes = utils::bytes::i64_to_bytes(symbol.0 as i64);
        let table_name = format!(
            "zephyr_{}",
            hex::encode::<[u8; 16]>(md5::compute([bytes, id].concat()).into()).as_str()
        );
        let postgres_args: String = self.dir.clone();
        let (client, connection) = tokio_postgres::connect(&postgres_args, NoTls)
            .await
            .unwrap();
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("connection error: {}", e);
            }
        });
        let query = String::from(&format!("SELECT * FROM {};", table_name));
        let resp = client.query(&query, &[]).await?;
        Ok(resp.len())
    }

    pub async fn load_table(
        &mut self,
        id: i64,
        symbol: Symbol,
        columns: Vec<Column>,
    ) -> anyhow::Result<()> {
        let id = utils::bytes::i64_to_bytes(id);
        let bytes = utils::bytes::i64_to_bytes(symbol.0 as i64);
        let table_name = format!(
            "zephyr_{}",
            hex::encode::<[u8; 16]>(md5::compute([bytes, id].concat()).into()).as_str()
        );
        self.tables.push(table_name.clone());

        let postgres_args: String = self.dir.clone();
        let (client, connection) = tokio_postgres::connect(&postgres_args, NoTls)
            .await
            .unwrap();

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("connection error: {}", e);
            }
        });

        let mut new_table_stmt = String::from(&format!("CREATE TABLE {} (", table_name));

        for (index, column) in columns.iter().enumerate() {
            new_table_stmt.push_str(&format!("{} {}", column.name, column.col_type));

            if index < columns.len() - 1 {
                new_table_stmt.push_str(", ");
            }
        }

        new_table_stmt.push(')');
        client.execute(&new_table_stmt, &[]).await?;

        Ok(())
    }

    pub async fn close(&self) {
        let tables = &self.tables;
        for table_name in tables.clone() {
            let directory = self.dir.clone();

            let drop_table_statement = String::from(&format!("DROP TABLE {}", table_name.clone()));

            let postgres_args: String = directory;
            let (client, connection) = tokio_postgres::connect(&postgres_args, NoTls)
                .await
                .unwrap();

            tokio::spawn(async move {
                if let Err(e) = connection.await {
                    eprintln!("connection error: {}", e);
                }
            });

            client.execute(&drop_table_statement, &[]).await.unwrap();
        }
    }
}
