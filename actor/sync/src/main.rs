use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use ingest::{CaptiveCore, IngestionConfig, SupportedNetwork};
use serde::{Deserialize, Serialize};
use stellar_xdr::next::{
    ContractCodeEntry, ContractDataEntry, ContractExecutable, ExtensionPoint, Hash, HostFunction,
    InvokeContractArgs, InvokeHostFunctionOp, LedgerEntry, LedgerEntryChanges, LedgerEntryData,
    LedgerEntryExt, LedgerFootprint, LedgerKey, LedgerKeyContractCode, LedgerKeyContractData,
    Limits, MuxedAccount, Operation, OperationBody, OperationMeta, ScAddress, ScContractInstance,
    ScMap, ScSymbol, ScVal, ScVec, SequenceNumber, SorobanResources, SorobanTransactionDataExt,
    SorobanTransactionMeta, Transaction, TransactionMetaV3, TransactionV1Envelope, Uint256,
    WriteXdr,
};
use tokio::fs;
use tokio::sync::broadcast;
use tokio::time::sleep;
use warp::filters::ws::Message;
use warp::{ws::WebSocket, Filter};

use crate::sample::ledger_with_tx;

mod sample;

const PUBNET: &str = "Public Global Stellar Network ; September 2015";

#[derive(Serialize)]
pub struct SyncRequest {
    meta: Vec<u8>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct Config {
    pub network: String,
    pub min: Option<u32>,
    pub max: Option<u32>,
    pub frequency: u32,
}

/// This function starts the WebSocket server. It accepts a broadcast sender,
/// which is passed to each connection so that clients receive messages broadcast
/// by the ledger meta processing loop.
async fn serve_ws(ws_broadcast: broadcast::Sender<Message>) {
    let ws_route = warp::path("ws")
        .and(warp::ws())
        .and(with_ws_sender(ws_broadcast))
        .map(|ws: warp::ws::Ws, sender: broadcast::Sender<Message>| {
            ws.on_upgrade(move |socket| handle_connection(socket, sender))
        });

    println!("WebSocket server listening on ws://0.0.0.0:4000/ws");
    warp::serve(ws_route).run(([0, 0, 0, 0], 4000)).await;
}

/// This helper filter makes a clone of the broadcast sender available to each connection.
fn with_ws_sender(
    sender: broadcast::Sender<Message>,
) -> impl Filter<Extract = (broadcast::Sender<Message>,), Error = std::convert::Infallible> + Clone
{
    warp::any().map(move || sender.clone())
}

/// Handles an individual WebSocket connection by subscribing to the broadcast channel.
/// It spawns a task that continuously receives broadcast messages and sends them to the client.
async fn handle_connection(ws: WebSocket, ws_sender: broadcast::Sender<Message>) {
    println!("handling connection");
    let mut rx = ws_sender.subscribe();
    let (mut ws_tx, mut ws_rx) = ws.split();

    let forward_task = tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(msg) => {
                    if let Err(e) = ws_tx.send(msg.clone()).await {
                        eprintln!("Error sending message to client: {:?}", e);
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("Broadcast receive error: {:?}", e);
                    break;
                }
            }
        }
    });

    while let Some(result) = ws_rx.next().await {
        match result {
            Ok(msg) => {
                println!("Received message from client: {:?}", msg);
            }
            Err(e) => {
                eprintln!("WebSocket error: {:?}", e);
                break;
            }
        }
    }
    let _ = forward_task.await;
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt::init();

    use std::sync::Arc;
    use tokio::sync::broadcast;
    use tokio::task::LocalSet;

    let (ws_broadcast, _) = broadcast::channel(32);
    let ws_broadcast = Arc::new(ws_broadcast);

    let ws_server_handle = {
        let ws_broadcast_clone = ws_broadcast.clone();
        tokio::spawn(async move {
            serve_ws(ws_broadcast_clone.as_ref().clone()).await;
        })
    };

    let local = LocalSet::new();
    local
        .run_until(async move {
            #[cfg(not(feature = "mock"))]
            run_stellar_core(ws_broadcast).await;

            #[cfg(feature = "mock")]
            mock_ledger_closes(ws_broadcast).await
        })
        .await;

    let _ = tokio::join!(ws_server_handle);

    Ok(())
}

async fn mock_ledger_closes(ws_broadcast: Arc<broadcast::Sender<Message>>) {
    loop {
        let envelope = TransactionV1Envelope {
            signatures: vec![].try_into().unwrap(),
            tx: Transaction {
                source_account: MuxedAccount::Ed25519(Uint256([0; 32])),
                fee: 0,
                seq_num: SequenceNumber(1),
                cond: stellar_xdr::next::Preconditions::None,
                memo: stellar_xdr::next::Memo::None,
                ext: stellar_xdr::next::TransactionExt::V1(
                    stellar_xdr::next::SorobanTransactionData {
                        ext: SorobanTransactionDataExt::V0,
                        resources: SorobanResources {
                            footprint: LedgerFootprint {
                                read_only: vec![
                                    LedgerKey::ContractCode(LedgerKeyContractCode {
                                        hash: Hash([0; 32]),
                                    }),
                                    LedgerKey::ContractData(LedgerKeyContractData {
                                        contract: ScAddress::Contract(Hash([0; 32]).into()),
                                        key: ScVal::LedgerKeyContractInstance,
                                        durability:
                                            stellar_xdr::next::ContractDataDurability::Persistent,
                                    }),
                                ]
                                .try_into()
                                .unwrap(),
                                read_write: vec![].try_into().unwrap(),
                            },
                            instructions: 1000000,
                            disk_read_bytes: 10000,
                            write_bytes: 0,
                        },
                        resource_fee: 10000000,
                    },
                ),
                operations: vec![Operation {
                    source_account: None,
                    body: OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
                        host_function: HostFunction::InvokeContract(InvokeContractArgs {
                            contract_address: ScAddress::Contract(Hash([0; 32]).into()),
                            function_name: ScSymbol("t".try_into().unwrap()),
                            args: vec![].try_into().unwrap(),
                        }),
                        auth: vec![].try_into().unwrap(),
                    }),
                }]
                .try_into()
                .unwrap(),
            },
        };

        let meta = TransactionMetaV3 {
            ext: ExtensionPoint::V0,
            tx_changes_before: LedgerEntryChanges(vec![].try_into().unwrap()),
            tx_changes_after: LedgerEntryChanges(vec![].try_into().unwrap()),
            soroban_meta: Some(SorobanTransactionMeta {
                ext: stellar_xdr::next::SorobanTransactionMetaExt::V0,
                events: vec![].try_into().unwrap(),
                return_value: ScVal::Vec(Some(ScVec(
                    vec![
                        ScVal::Symbol(ScSymbol("hello".try_into().unwrap())),
                        ScVal::Symbol(ScSymbol("tdep".try_into().unwrap())),
                    ]
                    .try_into()
                    .unwrap(),
                ))),
                diagnostic_events: vec![].try_into().unwrap(),
            }),
            operations: vec![OperationMeta {
                changes: LedgerEntryChanges(vec![].try_into().unwrap()), // the hello world contract doesn't change anything
            }]
            .try_into()
            .unwrap(),
        };

        let ledger = ledger_with_tx(envelope, meta).await;
        let encoded = ledger.to_xdr(Limits::none()).unwrap();
        let body = SyncRequest { meta: encoded };

        let json_body = serde_json::to_string(&body).unwrap();
        tracing::info!(target: "info", "Forwarding meta object via WebSocket.");
        if let Err(_) = ws_broadcast.send(Message::text(json_body)) {
            tracing::info!("no client subscribed yet")
        };

        sleep(Duration::from_secs(4)).await;
    }
}

async fn run_stellar_core(ws_broadcast: Arc<broadcast::Sender<Message>>) {
    let project_definition = fs::read_to_string("./config/mercury.toml").await.unwrap();
    let config: Config = toml::from_str(&project_definition).unwrap();

    let network = if config.network == PUBNET {
        SupportedNetwork::Pubnet
    } else {
        SupportedNetwork::Testnet
    };

    tracing::info!(target: "info", "Booting up service for network {:?}", network);

    let ingestion_config = IngestionConfig {
        executable_path: "/usr/local/bin/stellar-core".to_string(),
        context_path: Default::default(),
        network,
        bounded_buffer_size: None,
        staggered: None,
    };

    let mut captive = CaptiveCore::new(ingestion_config);
    tracing::info!(target: "info", "Starting to receive streamed ledger metas from stellar core");
    let mut rv = captive
        .async_start_online_no_range()
        .await
        .expect("failed to start ingesting");
    tracing::info!(target: "info", "Started online streaming.");

    while let Some(result) = rv.recv().await {
        tracing::info!(target: "info", "Got new meta object.");
        let ledger = if let Some(ledger_wrapper) = result.ledger_close_meta {
            ledger_wrapper.ledger_close_meta
        } else {
            tracing::error!("Core stopped catching up");
            captive.close_runner_process().unwrap();
            std::process::exit(0);
        };

        let encoded = ledger.to_xdr(Limits::none()).unwrap();
        let body = SyncRequest { meta: encoded };

        let json_body = serde_json::to_string(&body).unwrap();
        tracing::info!(target: "info", "Forwarding meta object via WebSocket.");

        if let Err(_) = ws_broadcast.send(Message::text(json_body)) {
            tracing::info!("no client subscribed yet")
        };
    }
}
