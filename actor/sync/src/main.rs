use futures::{SinkExt, StreamExt};
use ingest::{CaptiveCore, IngestionConfig, SupportedNetwork};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stellar_xdr::next::{Limits, WriteXdr};
use tokio::fs;
use tokio::sync::broadcast;
use warp::filters::ws::Message;
use warp::{ws::WebSocket, Filter};

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
    local.run_until(async move {
        log4rs::init_file("./config/log4rs.yml", Default::default()).unwrap();
        let project_definition = fs::read_to_string("./config/mercury.toml").await.unwrap();
        let config: Config = toml::from_str(&project_definition).unwrap();

        let network = if config.network == PUBNET {
            SupportedNetwork::Pubnet
        } else {
            SupportedNetwork::Testnet
        };

        log::info!(target: "info", "Booting up service for network {:?}", network);

        let ingestion_config = IngestionConfig {
            executable_path: "/usr/local/bin/stellar-core".to_string(),
            context_path: Default::default(),
            network,
            bounded_buffer_size: None,
            staggered: None,
        };

        let mut captive = CaptiveCore::new(ingestion_config);
        log::info!(target: "info", "Starting to receive streamed ledger metas from stellar core");
        let mut rv = captive.async_start_online_no_range().await.expect("failed to start ingesting");
        log::info!(target: "info", "Started online streaming.");

        while let Some(result) = rv.recv().await {
            log::info!(target: "info", "Got new meta object.");
            let ledger = if let Some(ledger_wrapper) = result.ledger_close_meta {
                ledger_wrapper.ledger_close_meta
            } else {
                log::error!("Core stopped catching up");
                captive.close_runner_process().unwrap();
                std::process::exit(0);
            };

            let encoded = ledger.to_xdr(Limits::none()).unwrap();
            let body = SyncRequest { meta: encoded };

            let json_body = serde_json::to_string(&body).unwrap();
            log::info!(target: "info", "Forwarding meta object via WebSocket.");
            let sent = ws_broadcast.send(Message::text(json_body));
        }
    }).await;

    let _ = tokio::join!(ws_server_handle);

    Ok(())
}
