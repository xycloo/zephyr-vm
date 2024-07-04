use crate::testutils::{invoke_vm, symbol::Symbol, Column, MercuryDatabaseSetup};

#[tokio::test]
async fn soroban_host() {
    let mut dbsetup =
        MercuryDatabaseSetup::setup_local("postgres://postgres:postgres@localhost:5432").await;

    let invocation =
        invoke_vm("../target/wasm32-unknown-unknown/release/soroban_host.wasm".into()).await;
    assert!(invocation.is_ok());
    let invocation = invocation.unwrap();
    assert!(invocation.is_ok());

    // Note
    dbsetup.close().await
}
