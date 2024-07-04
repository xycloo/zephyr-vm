//! Note: tests need to be executed sequentially since they rely on the same db that is dropped
//! only at the end of each test:
//!
//! `cargo test -- --exact --nocapture --test-threads 1`
//!

use crate::testutils::{invoke_vm, symbol::Symbol, Column, MercuryDatabaseSetup};

#[tokio::test]
async fn tables_manager() {
    let mut dbsetup =
        MercuryDatabaseSetup::setup_local("postgres://postgres:postgres@localhost:5432").await;
    let created = dbsetup
        .load_table(
            0,
            Symbol::try_from_bytes("hello".as_bytes()).unwrap(),
            vec![Column::with_name(&"tdep")],
        )
        .await;

    assert!(created.is_ok());

    dbsetup.close().await
}

#[tokio::test]
async fn write_read() {
    let mut dbsetup =
        MercuryDatabaseSetup::setup_local("postgres://postgres:postgres@localhost:5432").await;

    let created = dbsetup
        .load_table(
            0,
            Symbol::try_from_bytes("hello".as_bytes()).unwrap(),
            vec![Column::with_name(&"tdep")],
        )
        .await;

    assert!(created.is_ok());
    assert_eq!(
        dbsetup
            .get_rows_number(0, Symbol::try_from_bytes("hello".as_bytes()).unwrap())
            .await
            .unwrap(),
        0
    );

    let invocation =
        invoke_vm("../target/wasm32-unknown-unknown/release/db_write_read.wasm".into()).await;
    assert!(invocation.is_ok());
    let invocation = invocation.unwrap();
    assert!(invocation.is_ok());
    assert_eq!(
        dbsetup
            .get_rows_number(0, Symbol::try_from_bytes("hello".as_bytes()).unwrap())
            .await
            .unwrap(),
        1
    );

    let invocation =
        invoke_vm("../target/wasm32-unknown-unknown/release/db_write_read.wasm".into()).await;
    // Note:
    // due to the condition at line 25 of wasm-tests/db-write-read/src/lib.rs this call should panic on the guest.
    assert!(invocation.is_ok());
    let invocation = invocation.unwrap();
    assert!(invocation.is_err());

    // Note
    dbsetup.close().await
}

#[tokio::test]
async fn write_update_read() {
    let mut dbsetup =
        MercuryDatabaseSetup::setup_local("postgres://postgres:postgres@localhost:5432").await;

    let created = dbsetup
        .load_table(
            0,
            Symbol::try_from_bytes("hello".as_bytes()).unwrap(),
            vec![Column::with_name(&"tdep")],
        )
        .await;

    assert!(created.is_ok());
    assert_eq!(
        dbsetup
            .get_rows_number(0, Symbol::try_from_bytes("hello".as_bytes()).unwrap())
            .await
            .unwrap(),
        0
    );

    let invocation =
        invoke_vm("../target/wasm32-unknown-unknown/release/db_write_update_read.wasm".into())
            .await;
    assert!(invocation.is_ok());
    let invocation = invocation.unwrap();
    assert!(invocation.is_ok());
    assert_eq!(
        dbsetup
            .get_rows_number(0, Symbol::try_from_bytes("hello".as_bytes()).unwrap())
            .await
            .unwrap(),
        1
    );

    let invocation =
        invoke_vm("../target/wasm32-unknown-unknown/release/db_write_update_read.wasm".into())
            .await;
    // Note:
    // due to the condition at line 31 of wasm-tests/db-write-update-read/src/lib.rs this call should panic on the guest.
    assert!(invocation.is_ok());
    let invocation = invocation.unwrap();
    assert!(invocation.is_err());

    // Note
    dbsetup.close().await
}
