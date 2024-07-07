# Zephyr VM: The First Blockchain Data Focused Cloud Computing Virtual Machine.

> The Zephyr Virtual Machine was built and is being maintained by [Xycloo Labs](https://xycloo.com).

The ZephyrVM is a wasmi virtual machine built for enabling efficient cloud computing 
of blockchain-data related activities such as:
- indexing
- monitoring
- automation
and more.

Zephyr is the VM at the core of [Mercury](https://mercurydatap.app/)'s cloud execution environment,
but can be also implemented, built and run locally.

- User docs can be found at https://docs.mercurydata.app/zephyr-full-customization/introduction.
- Stellar/Soroban client-side tooling is at https://github.com/xycloo/rs-zephyr-toolkit.
- Developers/auditors (not complete yet) documentation is in [./documentation](./documentation/src/). 

### Soroban environment fork

The ZephyrVM (ZVM) relies on a forked version of the soroban host environment that accepts the ZVM
as a generic VM implementation for all-things related to linmem shared access. The Soroban host used by 
Zephyr can be found in @heytdep's fork: https://github.com/heytdep/rs-soroban-env
