# Setup your Project

> Note: this section assumes that you have already gone through at least the first steps of the [Soroban's setup](https://soroban.stellar.org/docs/getting-started/setup).
> If you haven't, you'll need to install rust and add `wasm32-unknown-unknown` as target.

## Install the Zephyr CLI

The Zephyr CLI will be needed to upload your programs and create the tables these will access.

```
cargo install zephyr-cli
```

## Initialize the project

First, you need to create a new cargo library:

```
cargo new --lib zephyr-hello-ledger 
```

## Add Zephyr SDK as Dependency

Next, you'll need to add the zephyr sdk to your dependencies.
This will enable you to easily access the environment without directly communicating with it or with shared linear memory.

```toml
[package]
name = "zephyr-hello-ledger"
version = "0.1.0"
edition = "2021"

# See more keys and their definitions at https://doc.rust-lang.org/cargo/reference/manifest.html

[dependencies]
rs-zephyr-sdk = { path="../zephyr/rs-zephyr-sdk" }

[lib]
crate-type = ["cdylib"]

[profile.release]
opt-level = "z"
overflow-checks = true
debug = 0
strip = "symbols"
debug-assertions = false
panic = "abort"
codegen-units = 1
lto = true

```

Also, we've set `cdylib` as crate type to produce a dynamic library and set some release flags in order
not to produce md-large binaries.
