# Track SAC Events

> Note: this program relies on the previous SAC tracker program.

This program tracks all events of the SACs registered from the [previous](./contracts-map.md) program.

### Currently deployed version

This program is already live on Zephyr and is tracking events for SACs wrapped in and after ledger `177442` 
and its data can be accessed as follows:

```graphql
query SACEvents {
  allZephyr86882807C5E507349D54F6F33Fc8229As {
    edges {
      node {
        sequence
        contract
        topic1
        topic2
        topic3
        topic4
        data
      }
    }
  }
}
```

```json
{
  "data": {
    "allZephyr86882807C5E507349D54F6F33Fc8229As": {
      "edges": [
        {
          "node": {
            "sequence": "\\x0002b522",
            "contract": "\\xc184ad97f64befba0907b3cef6b570155c16658fcef92df221147d1592faa3e0",
            "topic1": "\\x0000000f000000046d696e74",
            "topic2": "\\x000000120000000000000000d08a167b577b96595971d6884e6a3097affb238ca5947585ead11660fa23676f",
            "topic3": "\\x00000012000000000000000047447cded9fa966bd551e683c1d39d5e9b32361f1a6483c15382f7684751bea0",
            "topic4": "\\x0000000e0000003c54454d3a47444949554654334b35355a4d574b5a4f484c495154544b47434c3237365a445253535a49354d46354c49524d594832454e545737374248",
            "data": "\\x0000000a00000000000000000000000000000190"
          }
        }
      ]
    }
  }
}
```

Remember that all this data is hex encoded bytes. `topicn` and `data` should be parsed as ScVals 
(`ScVal::from_xdr(...)`), `contract` is the inner `Hash(<this is contract>)` so you can easily build the 
corresponding string using the stellar-strkey libs. `sequence` is the big endian bytes repr for an `i64`. 
For example you can parse it in js as follows

```js
const hex = "\\x0002b522";
const cleanHex = hex.replace(/\\x/g, '');
const result = parseInt(cleanHex, 16);

console.log(result);
```

# Code

```rust
use rs_zephyr_sdk::{
    stellar_xdr::next::{ContractEventBody, Limits, TransactionMeta, WriteXdr},
    EnvClient,
};

#[no_mangle]
pub extern "C" fn on_close() {
    let mut env = EnvClient::default();
    let reader = env.reader();

    let sequence = reader.ledger_sequence();
    let processing = reader.tx_processing();

    let sacs = env.db_read("sacs", &["contract"]).unwrap();
    let tracked_deployed_sacs: Vec<&Vec<u8>> = sacs.rows.iter().map(|row| &row.row[0].0).collect();

    for tx_processing in processing {
        if let TransactionMeta::V3(meta) = &tx_processing.tx_apply_processing {
            if let Some(soroban) = &meta.soroban_meta {
                if !soroban.events.is_empty() {
                    for event in soroban.events.iter() {
                        let contract_id = event.contract_id.as_ref().unwrap().0;
                        if tracked_deployed_sacs.contains(&contract_id.to_vec().as_ref()) {
                            let (topics, data) = match &event.body {
                                ContractEventBody::V0(v0) => (
                                    v0.topics
                                        .iter()
                                        .map(|topic| topic.to_xdr(Limits::none()).unwrap())
                                        .collect::<Vec<Vec<u8>>>(),
                                    v0.data.to_xdr(Limits::none()).unwrap(),
                                ),
                            };
                            env.db_write(
                                "sac_event",
                                &[
                                    "sequence", "contract", "topic1", "topic2", "topic3", "topic4",
                                    "data",
                                ],
                                &[
                                    &sequence.to_be_bytes(),
                                    &contract_id,
                                    &topics.get(0).unwrap_or(&vec![]),
                                    &topics.get(1).unwrap_or(&vec![]),
                                    &topics.get(2).unwrap_or(&vec![]),
                                    &topics.get(3).unwrap_or(&vec![]),
                                    &data,
                                ],
                            )
                            .unwrap()
                        }
                    }
                }
            }
        }
    }
}
```
