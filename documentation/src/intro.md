# Zephyr: The First Blockchain Data Focused Cloud Computing Virtual Machine.

> The Zephyr Virtual Machine was built and is being maintained by [Xycloo Labs](https://xycloo.com).

The ZephyrVM is a wasmi virtual machine built for enabling efficient cloud computing 
of blockchain-data related activities such as:
- indexing
- monitoring
- automation
and more.

This book is meant to be a documentation of the codebase itself and is not a recommended read
for standard users.

# Concepts

Below are some of the concepts to keep in mind while reading the codebase and understanding the VM.

### Allocation as a first-class citzen

Zephyr programs will inevitably become quite large in size. This is because all the logic that is abstracted
in smart contracts needs to be reconciled in off-chain applications, such as zephyr programs.

These programs have to deal with raw XDR, user input, and potentially third-party libraries such as
for example the `charming` charting library (as long as they are wasm compatible).

Unfortunately, all of this means that binary size will generally be much bigger than the binaries
produced by soroban contracts. A possible approach to counter this is to rely more on host-side
allocation building custom containerized storage structures on the host environment, similarly to
what the Soroban VM does for instance. This would allow for much less structures on the guest side,
resulting in smaller binary sizes and consequently faster VM instantiation times.

However, due to the nature of zephyr programs which need to iterate and build complex logic around
structures like XDR, all the speed gained by decreasing binary size would be lost and completely
out-weighted by all of the environment I/O calls which ultimately lead to much higher latency.

Additionally, the no-std flag would be soon lost by using external crates that need allocation such
as `charming` for instance. 

As a result, zephyr binaries are large and often require much more guest-side allocation than for example
soroban contracts.

### Zephyr is a db bridge

Among other things, at its core, zephyr is a vm that takes user data and translates it to a database-interpretable
instruction. Zephyr programs will in fact ofter read from and write to a database.

Database interactions are crucial in Zephyr as they allow for data ingestion and querying.

### Chain abstraction

The ZephyrVM has two entry-points for blockchain data:
- block/ledger transitions, i.e the meta of each block/ledger close. 
- state access.

While the ZephyrVM was built purposefully to work with the Stellar network, the implementation
for the state access is abstracted from the VM and up to the implementing server to implement.
Additionally, transitions are just bytes that can be accessed by the guest environment, meaning
that they can potentially have other formats than the `LedgerCloseMeta` used by Stellar. 

### Database abstraction

While we recommend using postgres to work with Zephyr, the database implementation is also abstracted
from the virtual machine, allowing the implementing server to work with other databases.

### Soroban as a first class citzen

Again, even if it's chain-agnostic, the current implementation of the ZephyrVM natively interoperates
with the Soroban Virtual Machine. The xyclooLabs team has created a [zephyr-compatible fork](https://github.com/heytdep/rs-soroban-env) 
of the sorboan host environment and modified soroban functions for sharing memory with the ZephyrVM. 

This enables clients to build with hooks to the soroban host environment, effectively enabling
them to use all the utilities offered by the native soroban sdk used to build smart contracts. 

A main point of advantage here is being able to use the same structures (`Symbol`, `Address`, custom types, etc)
on directly within a zephyr program.

### Multival

The post-mvp multival wasm feature is crucial in the ZephyrVM, since most host-guest interactions
are built upon providing directly as host function argument the position and size of a certain object
in the shared linear memory.
