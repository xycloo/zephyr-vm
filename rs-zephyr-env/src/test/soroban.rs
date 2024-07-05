use crate::testutils::TestHost;

#[tokio::test]
async fn soroban_host() {
    let env = TestHost::default();
    let program = env.new_program("../target/wasm32-unknown-unknown/release/soroban_host.wasm");

    let invocation = program.invoke_vm("on_close").await;
    assert!(invocation.is_ok());
    let invocation = invocation.unwrap();
    assert!(invocation.is_ok());
}
