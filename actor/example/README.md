# Actor example setup for hello-leger program

Within actor directory:

```
INGESTOR_DB="dbname=myzephyr user=postgres" ../target/release/server newtable
```

```
INGESTOR_DB="host=127.0.0.1 dbname=myzephyr user=postgres" WASM_PATH="ACTOR_DIR/example/zephyr-hello-ledger/target/wasm32-unknown-unknown/release/zephyr_hello_ledger.wasm" BINARY_PATH="../target/release/zephyr_binary" ../target/release/server start
```

## For catchup

```
INGESTOR_DB="host=127.0.0.1 dbname=myzephyr user=postgres" WASM_PATH="ACTOR_DIR/example/zephyr-hello-ledger/target/wasm32-unknown-unknown/release/zephyr_hello_ledger.wasm" BINARY_PATH="../target/release/zephyr_binary" ../target/release/server catchup
```