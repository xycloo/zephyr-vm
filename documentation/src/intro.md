# Zephyr: Empowering Mercury Users with Custom Extensions to our Ingestion.

Welcome to the documentation for Zephyr, a code execution environment designed to enhance the capabilities of Mercury, the data indexer built by [xyclooLabs](https://xycloo.com) designed
to fit all the needs of developers and users of the Stellar Network, and where Soroban is a first-class citizen.


## Overview

Zephyr is a Virtual Machine developed on top of WebAssembly. This technology enables the execution of WASM modules
within Mercury's ingestion flow, providing seamless access to the Stellar network's data stored in Mercury's database and ledger metadata. 

The decision to build on WebAssembly stems from its proven efficiency, safety features developed in Rust, and its 
ability to offer a secure execution environment for users and infrastructure alike.

Zephyr's integration with Mercury unlocks a plethora of possibilities for the Stellar ecosystem. 
Users can now build specialized services without the need for setup or infrastructure. 
From protocol-centered services to user-specific applications, the Mercury + Zephyr combination opens up a world of innovative use cases.

Some use cases that are currently **only partially possible*** are:
- Advanced Alert Systems: Empower traders and arbitrageurs to build highly customized alert and trading strategies without the complexities of managing databases or running instances.
- Trackers: Effortlessly create watcher services to track the movement of funds across the Stellar network.
- Multi-step Workflows: Facilitate complex processes by enabling workflows where each step depends on the result of the previous one.
- Customized Indexing: Tailor database structures to specific querying needs with Zephyr's ingestion mechanisms.
- User-defined Data Aggregations: Define personalized aggregation functions and calculations for unique requirements.
- On-the-fly Subscriptions: Dynamically create subscriptions for specific data, allowing for real-time monitoring.
- Custom Data Retention Policies: Empower users to manage data retention based on custom policies, optimizing costs in the long run.
- Protocol Health Checks: Easily deploy watcher programs to monitor and maintain the health of protocols within the Stellar ecosystem.

**The latest release of the Zephyr VM and its integration within Mercury is still very experimental and a lot of key features are
 still missing (websockets, custom querying, more advanced DB access, making subscriptions, conditional triggering and others).*


<hr/>

If this looks interesting enough and you whish to try it out, proceed to the next section to setup your projects to
work with Zephyr. 
