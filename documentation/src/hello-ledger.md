# Write a Hello Ledger Program

Now it's time to write a very simple zephyr program that for every ledger that passes will write to a table `ledgers` the ledger sequence and the amount of transactions in the ledger's tx processing.


## Entry point

The Zephyr environment will automatically only call a `on_close() -> ()` function. If such function isn't exported by the WASM module there will be no execution.

```rust
#[no_mangle]
pub extern "C" fn on_close() {}
```

We disable mangling and tell the compiler this function has a c interface.

## Getting ledger number and tx processing count

Next up is accessing the ledger meta to read the ledger sequence and number of transactions in the ledger's transaction processing section. 

```rust
let mut env = EnvClient::default();
let reader = env.reader();

let sequence = reader.ledger_sequence();
let processing = reader.tx_processing();
let processing_length = processing.len();
```

> Note that the `reader` is currently very incomplete and there are high chances that you'll
> have to deal with the hole ledger meta object yourself.
> In such cases, the usage of `let meta = env.last_ledger_meta_xdr()` is recommended.


## Writing to the database

Lastly, we want to write the sequence and the processing to the database's `ledgers` table:

```rust
env.db_write("ledgers", 
    &[
        "sequence", 
        "proc"
    ], 
    &[
        &sequence.to_be_bytes(), 
        &processing_length.to_be_bytes()]
    ).unwrap();
```

> Note that we're transforming the sequence and processing lenght to an array of bytes.
> This is needed as currently the only type we've defined to work with Zephyr host <> guest
> communication is raw bytes. As SDK development goes further there will be more types (Arrays, Strings, Numbers, etc) that can be sent.  

## Summary

In the end, our Zephyr program should look like this:

```rust
use rs_zephyr_sdk::EnvClient;


#[no_mangle]
pub extern "C" fn on_close() {
    let mut env = EnvClient::default();
    let reader = env.reader();

    let sequence = reader.ledger_sequence();
    let processing = reader.tx_processing();
    let processing_length = processing.len();

    env.db_write("ledgers", 
    &[
        "sequence", 
        "proc"
    ], 
    &[
        &sequence.to_be_bytes(), 
        &processing_length.to_be_bytes()]
    ).unwrap();
}
```

# Compiling

To compile the program to WASM:

```
cargo +nightly rustc --release --target=wasm32-unknown-unknown -- -C target-feature=+multivalue
```

As you can see, we're targeting a WASM release and we're also enabling WASM's multivalue compilation.
Multivalue is used in Zephyr for efficiency of host <> guest interop. 

This should compile the program to `target/wasm32-unknown-unknown/release/zephyr_hello_ledger.wasm`.

If you wish to optimize the program size, you can also use:

```
wasm-opt -Oz -o ./target/wasm32-unknown-unknown/release/zephyr_hello_ledger.optimized.wasm  ./target/wasm32-unknown-unknown/release/zephyr_hello_ledger.wasm --enable-multivalue
```

<hr/>

The last step is to crete the `ledgers` table and upload the program. See the next chapter.
