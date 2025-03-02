use base64::prelude::*;
use client::EventFeed;
use executor::db::execution::NewZephyrTable;
use sha2::{Digest, Sha256};
use tokio::{fs, process::Command, sync::mpsc::{UnboundedReceiver, UnboundedSender}};
use tokio_tungstenite::connect_async;
use futures::StreamExt;
use url::Url;
use serde::{Deserialize, Serialize};

mod client;

#[derive(Deserialize, Serialize, Debug)]
pub struct SyncRequest {
    meta: Vec<u8>,
}

#[derive(Deserialize, Serialize, Debug)]
pub enum BinaryType {
    Path(String),
    Code(Vec<u8>)
}

#[derive(Deserialize, Serialize, Clone)]
pub struct Config {
    pub network: String,
    pub min: Option<u32>,
    pub max: Option<u32>,
    pub frequency: u32,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct FInput {
    pub associated_data: Vec<u8>,
    pub binary: BinaryType,
    pub user_id: u64,
    pub network_id: [u8; 32],
    pub fname: String
}

async fn get_network_id_from_env() -> [u8; 32] {
    let project_definition = fs::read_to_string("./config/mercury.toml").await.unwrap();
    let config: Config = toml::from_str(&project_definition).unwrap();

    let mut hasher = Sha256::new();
    hasher.update(&config.network);
    hasher.finalize().as_slice().try_into().unwrap()
}

async fn fill_metas(tx: UnboundedSender<Vec<u8>>) {
    let url = Url::parse("ws://127.0.0.1:4000/ws").unwrap();
    let (ws_stream, _) = connect_async(url)
        .await
        .expect("Failed to connect to the server");
    println!("Connected to the server.");
    let (_, mut read) = ws_stream.split();
    
    while let Some(message) = read.next().await {
        match message {
            Ok(msg) if msg.is_text() => {
                let text = msg.to_text().unwrap();

                match serde_json::from_str::<SyncRequest>(text) {
                    Ok(sync_req) => {
                        println!(
                            "Received SyncRequest with meta length: {} bytes",
                            sync_req.meta.len()
                        );

                        let _ = tx.send(sync_req.meta);
                    }
                    Err(e) => {
                        eprintln!("Failed to parse SyncRequest: {:?}", e);
                    }
                }
            }
            Ok(msg) if msg.is_close() => {
                println!("Server closed the connection.");
                break;
            }
            Ok(_) => {
                //
            }
            Err(e) => {
                eprintln!("Error receiving message: {:?}", e);
                break;
            }
        }
    }
}

async fn meta_executor(rx: &mut UnboundedReceiver<Vec<u8>>) {
    // NB: subprocess exec makes it simpler to control the constraints.
    while let Some(data) = rx.recv().await {
        let function = FInput {
            associated_data: data,
            user_id: 0,
            network_id: get_network_id_from_env().await,
            fname: "on_close".into(),
            binary: BinaryType::Path(std::env::var("WASM_PATH").expect("missing WASM_PATH from enviornment"))
        };
        
        let _ = execute_function(function).await;
    }
}

async fn execute_function(function: FInput) -> anyhow::Result<()> {
    let serialized = bincode::serialize(&function).unwrap();
    let encoded = BASE64_STANDARD.encode(&serialized);
    let child = Command::new(std::env::var("BINARY_PATH").expect("missing BINARY_PATH from enviornment"))
        .arg(encoded.clone())
        .spawn().unwrap();

    let output = child.wait_with_output().await?;
    if output.status.success() {
        println!("[+] successfully called zephyr function")
    } else {
        println!("Zephyr function error: {:?}", output)
    }

    Ok(())
}

#[derive(Deserialize)]
pub struct CatchupConfig {
    mercury_jwt: String,
    network: String,
    contracts: Vec<String>,
    topic1s: Vec<String>,
    topic2s: Vec<String>,
    topic3s: Vec<String>,
    topic4s: Vec<String>,
    start: i64,
}

#[tokio::main]
async fn main() {
    let server_action: String = std::env::args().nth(1).unwrap_or_default();
    
    match server_action.as_str() {
        "catchup" => {
            println!("Starting catchup job, reading config.json");
            let config: CatchupConfig = serde_json::from_str(&tokio::fs::read_to_string("catchup.json").await.expect("No catchup.json found.")).expect("Invalid config.json format.");
            println!("Retrieving events (using mercury plug)");
            let plug = client::mercury::MercuryClient {
                network: config.network,
                jwt: config.mercury_jwt
            };
            let events = plug.events(config.contracts, [config.topic1s, config.topic2s, config.topic3s, config.topic4s], config.start).await.expect("failed to retreive events");
            let last_ledger_processed = client::do_catchups_on_events(events.data, config.start).await;
            println!("Finished catchup, processed until ledger {}", last_ledger_processed);
        },
        "newtable" => {
            let tables: Vec<NewZephyrTable> = serde_json::from_str(&tokio::fs::read_to_string("tables.json").await.expect("No tables.json found.")).expect("Invalid tables.json format.");
            println!("Creating new tables");
            
            for table in tables {
                match executor::db::execution::new_zephyr_table(table).await {
                    Ok(name) => println!("Created table {name}"),
                    Err(e) => println!("Failed to create zephyr table: {:?}", e)
                }
            }
        }
        _ => {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
            let fill_metas = tokio::spawn(async move {
                fill_metas(tx).await
            });
            let spawn_executor = tokio::spawn(async move {
                meta_executor(&mut rx).await
            });

            let _ = tokio::join!(fill_metas, spawn_executor);
        }
    }
}
