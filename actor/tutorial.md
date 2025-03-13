# Self‑Hosting Mercury Zephyr: A Step‑by‑Step Tutorial

This tutorial will walk you through the entire process of setting up a self‑hosted Zephyr environment. You’ll learn how to install and run Stellar Core (for streaming ledger events), clone and build the necessary Mercury‑Zephyr repositories, set up PostgreSQL for ingestion, and compile and run an example Zephyr program.

## Prerequisites:

- Familiarity with the command line, Git, and Cargo
- Rust toolchain installed
- PostgreSQL installed on your system
- A Unix‑like environment (macOS/Linux; Windows users can adapt these instructions)

## Table of Contents

1. [Installing Stellar Core](#1-installing-stellar-core)
2. [Cloning the Repositories](#2-cloning-the-repositories)
3. [Building the Actor Components](#3-building-the-actor-components)
4. [Installing and Configuring PostgreSQL](#4-installing-and-configuring-postgresql)
5. [Editing Configuration Files](#5-editing-configuration-files)
6. [Running the Actor Sync Client](#6-running-the-actor-sync-client)
7. [Initializing the Database Schema](#7-initializing-the-database-schema)
8. [Building and Running an Example Zephyr WASM Program](#8-building-and-running-an-example-zephyr-wasm-program)
9. [Starting the Multiuser Logging Service](#9-starting-the-multiuser-logging-service)
10. [Starting the Execution Server](#10-starting-the-execution-server)
11. [Verifying the Setup](#11-verifying-the-setup)

---

## 1. Installing Stellar Core

Running a Stellar Core watcher node is needed to stream ledger “close metas”. These are later consumed by the Mercury‑Zephyr sync client to trigger the execution of your Zephyr programs. Ensure Stellar Core is running locally before proceeding. A tutorial on how to run Core will be provided by our team later.

## 2. Cloning the Repositories

You’ll need to clone the Zephyr-VM repository (using the “actor” branch) along with its required dependency repositories so that all code can interact properly.

**Steps:**

```bash
# In your chosen parent directory (e.g., ~/zephyr)
git clone -b actor https://github.com/xycloo/zephyr-vm.git
git clone -b stellar-main-2 https://github.com/heytdep/rs-soroban-env.git
git clone https://github.com/xycloo/multiuser-logging-service.git
git clone https://github.com/xycloo/rs-ingest.git
```

**Note:** The correct branch for `rs-soroban-env` is `stellar-main-2`.

## 3. Building the Actor Components

The actor component contains two key binaries: one that streams events (sync client) and another that processes these events (execution server). Building in the actor directory isolates these components from other workspace members.

- **Actor/Sync Client Binary:** This binary connects to Stellar Core to receive live "close metas” and forwards them to the execution layer. Essentially, it acts as a bridge between Stellar Core and the ZVM processing logic.
- **Actor/Server Execution Layer Binary:** This binary is responsible for processing the events received by the sync client. Upon receiving an event, it spawns subprocesses that run the Zephyr Virtual Machine (ZVM) on your local machine, executing the desired logic.

**Steps:**

```bash
cd zephyr-vm/actor
cargo build --release
```

If you encounter missing manifest or version mismatch errors, review your cloned repositories and ensure you’re on the correct branches.

## 4. Installing and Configuring PostgreSQL

PostgreSQL serves as the database to store ledger sequences and execution metadata.

**Installation (macOS Example with Homebrew):**

```bash
brew update
brew install postgresql
brew services start postgresql
```

**Creating the Database:**

```bash
createdb zephyr_db
```

**Setting the Environment Variable:**

Set the connection string using the `INGESTOR_DB` environment variable. For example:

```bash
export INGESTOR_DB="postgres://<username>:<password>@localhost:5432/zephyr_db"
```

Replace `<username>` and `<password>` with your PostgreSQL credentials. If you use peer authentication (common on macOS), you may omit the password:

```bash
export INGESTOR_DB="postgres://your_username@localhost:5432/zephyr_db"
```

Verify by running:

```bash
echo $INGESTOR_DB
```

## 5. Editing Configuration Files

The configuration files (located in `./config/` in the Zephyr VM repository) define runtime parameters like network endpoints (testnet or pubnet), logging levels, and other settings.

**Steps:**

1. Open each configuration file in your editor.
2. Read comments to understand each setting.
3. Adjust values as needed to match your local environment (e.g., switching network endpoints or adjusting verbosity).
4. Save changes and restart affected components to apply the new settings.

## 6. Running the Actor Sync Client

The sync client is responsible for connecting to Stellar Core (or an event API) and streaming ledger “close metas” to the execution server.

**Steps:**

**Note:** If you haven’t initialized Stellar Core’s ingestion database, jump to [step 7](#7-initializing-the-database-schema) before proceeding.

From the actor directory’s build output:

```bash
./target/release/sync
```

**Key Points and Troubleshooting:**

- **What to Expect:** You should see log messages indicating that the WebSocket server is listening (e.g., on `ws://0.0.0.0:4000/ws`) and that ledger metas are being processed.
- **Error Handling:** If you receive an error about a missing file (for example, “No such file or directory”), review the code (around line 105 in `actor/sync/src/main.rs`) to determine which configuration file or asset is missing. Ensure that you’re running the binary from the correct working directory so that relative paths resolve correctly.
- **Database Schema Check:** An error such as “No DB schema version found, try stellar-core new-db” indicates that Stellar Core’s ingestion database has not been initialized. See the next section on initializing the DB schema.

## 7. Initializing the Database Schema

Before the sync client and execution server can operate, Stellar Core needs to initialize its ingestion database. This process creates the necessary tables and stores the schema version.

**Steps:**

Navigate to the temporary ingestion directory and run:

```bash
cd /tmp/rs_ingestion_temp
stellar-core new-db
```

This command creates the database schema that the sync client checks for.

## 8. Building and Running an Example Zephyr WASM Program

Zephyr’s execution layer uses custom logic compiled to WebAssembly. This section shows how to compile an example Zephyr program (in this case, "zephyr-hello-ledger") and prepare it for execution.

### A. Change to Supported Rust Version and Add the WASM Target

*Switching to Rust 1.81 might not be necessary if you’re not using macOS.*

```bash
rustup default 1.81
rustup target add wasm32-unknown-unknown
```

### B. Installing Mercury CLI (Optional)

Mercury CLI can be used to build the Zephyr program:

```bash
cargo install mercury-cli
```

### C. Building the Example Program

Change to the example directory and build the program. Theoretically, it could also be built with the Mercury CLI but as we’re investigating some issues, it’s better to use option 1:

```bash
cd zephyr-vm/actor/example/zephyr-hello-ledger
```

**Option 1:**

```bash
RUSTFLAGS="-C target-feature=+multivalue -C link-args=-zstack-size=10000000" cargo build --release --target wasm32-unknown-unknown
```

**Option 2:**

```bash
mercury-cli build
```

After the build completes, verify the WASM binary’s location:

```bash
pwd
```

Your WASM binary should be at:

```sql
.../target/wasm32-unknown-unknown/release/zephyr_hello_ledger.wasm
```

## 9. Starting the Multiuser Logging Service

This step builds and launches the multiuser logging service, which collects log messages from your Zephyr system and writes them into your PostgreSQL database. This helps you monitor system activity and troubleshoot issues by persisting logs for later analysis.

**Steps:**

1. Navigate to the Service Directory and build:

   ```bash
   cd multiuser-logging-service
   cargo build --release
   ```

2. Run the Service with Database Connection:

   Set the database connection string and start the logging service by running:

   ```bash
   DB="host=127.0.0.1 dbname=zephyr_db user=your_username" ./target/release/zephyr_service_storage
   ```

   The logs will be stored in the `mercury_user_logs` table of the database.

## 10. Starting the Execution Server

The execution server ties together the ingestion (database), the WASM-compiled logic, and the host execution binary. It reads ledger events and spawns processes to execute the corresponding Zephyr Program code.

**Steps:**

Set the following environment variables and start the server:

```bash
INGESTOR_DB="host=127.0.0.1 dbname=zephyr_db user=your_username" \
WASM_PATH="/absolute/path/to/zephyr_hello_ledger.wasm" \
BINARY_PATH="../target/release/zephyr_binary" \
../target/release/server start
```

**Explanation:**

- **INGESTOR_DB:** Connects to your PostgreSQL database.
- **WASM_PATH:** Specifies the full path to the compiled WASM binary.
- **BINARY_PATH:** Points to the host binary (the self-hosted Zephyr-VM) used for executing the WASM module.
- `server start`: Instructs the server binary to begin processing ledger metas and executing your logic.

## 11. Verifying the Setup

**What to Check:**

### Console Output:
The execution server should print log messages showing that it’s connected (e.g., “WebSocket server listening on ws://0.0.0.0:4000/ws”) and processing ledger metas.

### Database Inspection:
Use the `psql` command or a GUI tool (like pgAdmin) to connect to `zephyr_db` and list the tables:

```bash
psql -h 127.0.0.1 -d zephyr_db -U your_username
\dt
```

Then query the table (whose schema was created by the new-db command):

```sql
SELECT * FROM <table_name>;
```

New rows should appear corresponding to each processed ledger sequence.

---

## Conclusion

This tutorial has guided you through the complete process to self‑host Zephyr and provided an understanding of the key components involved. Feel free to refer back to this guide or ask for further clarifications as needed. Happy coding!
