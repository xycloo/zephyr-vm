# Zephyr VM

> **Warning**: this codebase should not be used or taken as reference in a production environment.

The Zephyr VM provides a WebAssembly host environment to execute sandboxed applications that have access to the Stellar Network's data. Zpehyr was built to operate within [Mercury](https://mercurydata.app/), our indexer service. 


## Current state of development

Zephyr is currently in ALPHA development stage, meaning that it's successfully running within Mercury but is nowhere near feature completion yet and still unstable.


## Purpose

This VM is being developed to enable safe execution of arbitrary programs inside the [Mercury](https://mercurydata.app/) environment. 
These programs are executed in parallel with the Stellar's Network execution (i.e, ledger by ledger) and are allowed to:

- access ledger close metas.
- read from the database. *
- write to the database. *

**Interactions with the database are restricted.*

The execution of these program is **going to be** metered in terms of resource usage and data access, and allows 
anyone to build customized implementation of the data processing flow they require (including also DB schema), for if
their use case is not covered by Mercury's built-in features. 

For a more in depth overview of Zephyr, refer to [this](https://blog.xycloo.com/blog/introducing-zephyr) blog post.

## Usage and CLI

The Zephyr CLI allows you to:
- create tables.
- deploy Zephyr programs to Mercury.

Refer to the [docs]() to learn how to use Zephyr on Mercury.
