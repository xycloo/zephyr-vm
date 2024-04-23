use clap::Parser;
use zephyr_mercury_cli::{Cli, Commands, MercuryClient, ZephyrProjectParser};

const BACKEND_ENDPOINT: &str = "https://api.mercurydata.app:8443";
const MAINNET_BACKEND_ENDPOINT: &str = "https://mainnet.mercurydata.app:8443";
const LOCAL_BACKEND: &str = "http://127.0.0.1:8443";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let client = if let Some(true) = cli.local {
        MercuryClient::new(LOCAL_BACKEND.to_string(), cli.jwt) 
    } else {
        if let Some(true) = cli.mainnet {
            MercuryClient::new(MAINNET_BACKEND_ENDPOINT.to_string(), cli.jwt) 
        } else {
            MercuryClient::new(BACKEND_ENDPOINT.to_string(), cli.jwt) 
        }
    };

    match cli.command {
        Some(Commands::Deploy {target, old_api, force}) => {
            if let Some(true) = old_api {
                println!("Deploying wasm ...");
                client.deploy(target.unwrap(), force.unwrap_or(false)).await.unwrap();
                println!("Successfully deployed Zephyr program.");
            } else {
                println!("Parsing project configuration ...");
                let parser = ZephyrProjectParser::from_path(client, "./zephyr.toml").unwrap();
                println!("Building binary ...");
                parser.build_wasm().unwrap();
                println!("Deploying tables ...");
                parser.deploy_tables().await.unwrap();
                
                println!("Deploying wasm ...");
                parser.deploy_wasm(target).await.unwrap();

                println!("Successfully deployed Zephyr program.");
            }
        },

        None => {
            println!("Usage: zephyr deploy")
        }
    };

    Ok(())
}
