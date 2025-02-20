use base64::prelude::*;
use sha2::{Digest, Sha256};
use tokio::{fs, process::Command};
use tokio_tungstenite::connect_async;
use futures::StreamExt;
use url::Url;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
pub struct SyncRequest {
    meta: Vec<u8>,
}

#[derive(Deserialize, Serialize)]
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

#[derive(Deserialize, Serialize)]
pub struct FInput {
    associated_data: Vec<u8>,
    binary: BinaryType,
    user_id: u64,
    network_id: [u8; 32],
    fname: String
}

async fn get_network_id_from_env() -> [u8; 32] {
    let project_definition = fs::read_to_string("./config/mercury.toml").await.unwrap();
    let config: Config = toml::from_str(&project_definition).unwrap();

    let mut hasher = Sha256::new();
    hasher.update(&config.network);
    hasher.finalize().as_slice().try_into().unwrap()
}

#[tokio::main]
async fn main() {
    let url = Url::parse("ws://127.0.0.1:4000/ws").unwrap();
    let (ws_stream, _) = connect_async(url)
        .await
        .expect("Failed to connect to the server");
    println!("Connected to the server.");

    let (_, mut read) = ws_stream.split();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();

    let fill_metas = tokio::spawn(async move {
        while let Some(message) = read.next().await {
            match message {
                Ok(msg) if msg.is_text() => {
                    let text = msg.to_text().unwrap();
                    println!("Received raw message: {}", text);

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
    });

    let spawn_executor = tokio::spawn(async move {
        // NB: subprocess exec makes it simpler to control the constraints.
        while let Some(data) = rx.recv().await {
            let function = FInput {
                associated_data: data,
                user_id: 0,
                network_id: get_network_id_from_env().await,
                fname: "on_close".into(),
                binary: BinaryType::Path(std::env::var("WASM_PATH").expect("missing WASM_PATH from enviornment"))
            };
            
            let serialized = bincode::serialize(&function).unwrap();
            let encoded = BASE64_STANDARD.encode(&serialized);

            let child = Command::new(std::env::var("BINARY_PATH").expect("missing BINARY_PATH from enviornment"))
                .arg(encoded)
                .spawn().unwrap();

            let output = child.wait_with_output().await;
            println!("Subprocess: {:?}", output);
        }
    });

    tokio::join!(fill_metas, spawn_executor);
}
