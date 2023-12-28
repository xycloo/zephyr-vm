# Zephyr CLI: Create table and Upload

## Create the ledgers table

```
zephyr --jwt $JWT_TOKEN new-table --name "ledgers" --columns 'sequence' 'proc'

[+] Table "zephyr_d625b7bb470ff3fe8cd1351a1cbb7187" created successfully
```

> Note that table name and columns must abide to Soroban's short symbol rules. In fact they cannot exceed 9 characters and valid characters are `a-zA-Z0-9_`. This is an efficiency-driven decision. We are also considering extending the lenght using multivalue but it's not currently implemented. 

The above command will create the ledgers table (pertinent to the user specified by the jwt token) with columns `sequence` and `proc`.

## Upload

Only after having created all the needed tables (with correct columns) you can upload your program:

```
zephyr --jwt $JWT_TOKEN deploy --wasm ./target/wasm32-unknown-unknown/release/zephyr_hello_ledger.optimized.wasm
```

## Query

Now I can query all rows in my zephyr table:

```graphql
query MyQuery {
  allZephyrD625B7Bb470Ff3Fe8Cd1351A1Cbb7187S {
    edges {
      node {
        sequence
        proc
      }
    }
  }
}
```

```json
{
  "data": {
    "allZephyrD625B7Bb470Ff3Fe8Cd1351A1Cbb7187S": {
      "edges": [
        {
          "node": {
            "sequence": "\\x000280d3",
            "proc": "\\x00000002"
          }
        },
        {"more": "nodes"}
      ]
    }
  }
}
```
