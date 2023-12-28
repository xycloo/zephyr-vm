# Concepts SDK API Definition

Below there are some important concepts about the ZephyrVM integration. We recommend reading
this section before getting hands-on with the tutorial in the next sections.


## Runs for every ledger

Zephyr programs run sequentially for every new ledger that is closed in the network.
This means that once your program is deployed it will be run for every new ledger close
since the moment of deployment. This is at least until we introduce conditional execution
that will make Zephyr less expensive.


## Before you deploy

Before you deploy your program and expect for it to start working immediately, it's important 
to know that you are going to need to correctly create all the tables that your program accesses.

In fact the general workflow for working with Zephyr is:
1. write the program and take note of the tables and columns you access.
2. create these tables with the columns in the database through the CLI.
3. deploy the program.

If you deploy the program without having first created the **correct** tables through the CLI
the execution of the program will always exit unsuccessfully, and currently there isn't a loggin
infra available to users that makes this easy to detect.


## Symbols inherited from Soroban

For ease of implementation all table names and table columns are pretty much Soroban symbols. This means
that they undergo all of the constraints that Soroban symbols have. They cannot exceed 9 characters and valid characters are `a-zA-Z0-9_`. This is an efficiency-driven decision. 

Since unlike the Soroban VM we are using WASM's multivalue feature, we're considering extending the lenght 
of a symbol but it's not currently implemented. 

## Slice-based communication

Currently we haven't implemented any Zephyr types yet, so when you write and read from the database, you're
in charge of serializing/deserializing the contents to/from a bytes slice. This will become clearer once you
take a look at the next sections.

## Environemnt

The guest program, i.e your program will access the database and ledger close metas thorugh the `Env` object
exported by the SDK.

This object has currently the following functions:

- `db_write(&self, table_name: &str, columns: &[&str], segments: &[&[u8]]) -> Result<(), SdkError>` which writes 
to the DB's `table` table the specified columns with the specified `segments` of data (always as byte slices).

- `db_read(&self, table_name: &str, columns: &[&str]) -> Result<TableRows, SdkError>` which reads from 
the DB's `table` table the specified columns. This returns a `TableRows` object which wraps all the rows and columns:

```rust
// Current implementation treats these as named structs, but could change.
#[derive(Clone, Deserialize, Serialize)]
pub struct TableRows {
    pub rows: Vec<TableRow>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct TableRow {
    pub row: Vec<TypeWrap>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct TypeWrap(pub Vec<u8>);
```

As you can see `TablesRows` wraps the rows, and each `TableRow` wraps each requested column. The value in each 
column for each row is `TypeWrap`, so a vectory of bytes.  

- `reader(&mut self) -> MetaReader` returns the `MetaReader` object, which should help you in getting
sections of the ledger meta more easily. However, the current implementation of the `MetaReader` is very limited.
If you're searching for a part of the metadata that isn't easily extracted through the reader then use the `last_ledger_meta_xdr()` function.

- `last_ledger_meta_xdr(&mut self) -> &stellar_xdr::next::LedgerCloseMeta` which returns the whole `LedgerCloseMeta`
object (from Stellar's XDR definition).

## Zephyr's DB Access

Of course, Zephyr-executed programs have strict limitations on database writes and reads. Mainly access limitations
for now. Only zephyr tables (created through the CLI) created by your account are accessible. Mercury built-in tables
(contract events, ledger entries, payments, etc) will never be able to be written by Zephyr and are currently not able
to be read (though we plan on enabling this once we write the authorization part for this). 

## Querying Zephyr Tables

Zephyr tables your create and access don't actually show up with the name you give them in the database. Rather they 
are a md5 hash of the table name you provide and your Mercury user id. This means that when querying your Zephyr table 
through GraphQL you don't query the table name rather the hash.

This is how the hash is generated:

```rust
let id = {
    let value = host.get_host_id();
    byte_utils::i64_to_bytes(value)
};

let write_point_hash: [u8; 16] = {
    let point_raw = stack.first().ok_or(HostError::NoValOnStack)?;
    let point_bytes = byte_utils::i64_to_bytes(*point_raw);

    md5::compute([point_bytes, id].concat()).into()
};
```

Basically, you compute the hash of the i64 repr of the table name symbol concatenated with your user id. Anyways, the
CLI will output the actual table name queryable from GraphQL once you deploy the table.  
