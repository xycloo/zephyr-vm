//! Note: tests need to be executed sequentially since they rely on the same db that is dropped
//! only at the end of each test:
//!
//! `cargo test -- --exact --nocapture --test-threads 1`
//!

use crate::testutils::{MercuryDatabaseSetup, TestHost};

#[tokio::test]
async fn tables_manager() {
    let mut dbsetup =
        MercuryDatabaseSetup::setup_local("postgres://postgres:postgres@localhost:5432");
    let created = dbsetup.load_table(0, "hello", vec!["tdep"], None).await;

    assert!(created.is_ok());

    dbsetup.close().await
}

#[tokio::test]
async fn write_read() {
    let env = TestHost::default();

    let mut dbsetup = env.database("postgres://postgres:postgres@localhost:5432");
    let program = env.new_program("../target/wasm32-unknown-unknown/release/db_write_read.wasm");

    let created = dbsetup.load_table(0, "hello", vec!["tdep"], None).await;

    assert!(created.is_ok());
    assert_eq!(dbsetup.get_rows_number(0, "hello").await.unwrap(), 0);

    let invocation = program.invoke_vm("on_close").await;
    assert!(invocation.is_ok());
    let invocation = invocation.unwrap();
    assert!(invocation.is_ok());
    assert_eq!(dbsetup.get_rows_number(0, "hello").await.unwrap(), 1);

    let invocation = program.invoke_vm("on_close").await;
    // Note:
    // due to the condition at line 25 of wasm-tests/db-write-read/src/lib.rs this call should panic on the guest.
    assert!(invocation.is_ok());
    let invocation = invocation.unwrap();
    assert!(invocation.is_err());

    dbsetup.close().await
}

#[tokio::test]
async fn write_update_read() {
    let env = TestHost::default();

    let mut dbsetup = env.database("postgres://postgres:postgres@localhost:5432");
    let program =
        env.new_program("../target/wasm32-unknown-unknown/release/db_write_update_read.wasm");

    let created = dbsetup.load_table(0, "hello", vec!["tdep"], None).await;

    assert!(created.is_ok());
    assert_eq!(dbsetup.get_rows_number(0, "hello").await.unwrap(), 0);

    let invocation = program.invoke_vm("on_close").await;
    assert!(invocation.is_ok());
    let invocation = invocation.unwrap();
    assert!(invocation.is_ok());
    assert_eq!(dbsetup.get_rows_number(0, "hello").await.unwrap(), 1);

    let invocation = program.invoke_vm("on_close").await;
    // Note:
    // due to the condition at line 31 of wasm-tests/db-write-update-read/src/lib.rs this call should panic on the guest.
    assert!(invocation.is_ok());
    let invocation = invocation.unwrap();
    assert!(invocation.is_err());

    dbsetup.close().await
}
