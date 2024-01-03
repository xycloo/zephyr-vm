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

## CLI

The Zephyr CLI allows you to:
- create tables.
- deploy Zephyr programs to Mercury.

Refer to the [docs](https://zephyr-b8t.pages.dev/) to learn how to use Zephyr on Mercury.

# Guide for early users.

We're keeping the access to the Zephyr integration restricted for now since the implementation is
live in Mercury's production deployment and there are still some areas of the code that are unsafe
and could compromise Mercury's overall functioning.

Keeping the integration private also means that the codebase remains private, which means
early testers must build both the CLI and the SDK from source, below there's a guide to help
you set up things.

Before starting, we ask you to:
- not share the codebase or the docs, though you're welcome to share results of using Zephyr or share
your queries so that other folks can see the results.
- don't deploy programs that use a tweaked SDK.
- not to spam the database, meaning for instance not storing big chunks of data (for instance each ledger close meta multiplied by a factor). If you're indecided or it seems that you need to store big chunks of data reach out to
tdep on discord first, chances are you don't need to store that much data. This is needed since as of now
we haven't introduced metering yet.

## Setup

1. Create your zephyr workspace in the shape of a directory and cd into it (`mkdir zephyr-testing; cd zephyr-testing`).
2. Clone this repo (`git clone https://github.com/xycloo/zephyr`).
3. Always inside the workspace clone the examples folder (`git clone https://github.com/xycloo/zephyr-examples`).
4. cd inside zephyr's CLI crate and build (`cd zephyr/zephyr-cli; cargo build --release; cd ../..`).
5. cd inside the starter template and create an alias for the zephyr cli (`cd zephyr-examples/zephyr-starter; alias zephyr="../../zephyr/target/release/zephyr-mercury-cli"`).

You're now all set to start working on the starter template. Now you are ready to visit 
[the docs](https://zephyr-b8t.pages.dev/) and start testing out Zephyr. 
