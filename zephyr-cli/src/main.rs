use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;

use clap::{Parser, Subcommand};

const BACKEND_ENDPOINT: &str = "http://ec2-13-50-173-187.eu-north-1.compute.amazonaws.com:3030";

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(short, long)]
    jwt: String,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    NewTable {
        #[arg(short, long)]
        name: String,

        #[clap(short, long, value_parser, num_args = 1.., value_delimiter = ' ')]
        columns: Vec<String>,
    },

    Deploy {
        #[arg(short, long)]
        wasm: String,
    },
}

#[derive(Clone, Deserialize, Serialize, Debug)]
struct Column {
    name: String,
    col_type: String,
}

#[derive(Deserialize, Serialize, Debug)]
struct NewZephyrTableClient {
    table: Option<String>,
    columns: Option<Vec<Column>>,
}

#[derive(Deserialize, Serialize, Debug)]
struct CodeUploadClient {
    code: Option<Vec<u8>>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let client = MercuryClient::new(BACKEND_ENDPOINT.to_string(), cli.jwt);

    match cli.command {
        Some(Commands::NewTable { name, columns }) => client.new_table(name, &columns).await?,

        Some(Commands::Deploy { wasm }) => client.deploy(wasm).await?,

        None => {
            println!("--newtable or --deploy");
        }
    };

    Ok(())
}

pub struct MercuryClient {
    pub base_url: String,
    pub jwt: String,
}

impl MercuryClient {
    pub fn new(base_url: String, jwt: String) -> Self {
        Self { base_url, jwt }
    }

    async fn new_table(
        &self,
        name: String,
        columns: &[String],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut cols = Vec::new();

        for col in columns {
            cols.push(Column {
                name: col.to_string(),
                col_type: "BYTEA".to_string(),
            });
        }

        let code = NewZephyrTableClient {
            table: Some(name),
            columns: Some(cols),
        };

        // Convert the code object to JSON
        let json_code = serde_json::to_string(&code)?;

        // Define the URL for your POST request
        let url = format!("{}/zephyr_table_new", &self.base_url);

        // Define the authorization header
        let authorization = format!("Bearer {}", &self.jwt);

        // Create a reqwest Client
        let client = reqwest::Client::new();

        // Make a POST request with the JSON data
        let response = client
            .post(url)
            .header("Content-Type", "application/json")
            .header("Authorization", authorization)
            .body(json_code)
            .send()
            .await
            .unwrap();

        if response.status().is_success() {
            println!(
                "[+] Table \"{}\" created successfully",
                response.text().await.unwrap()
            );
        } else {
            println!(
                "[-] Request failed with status code: {:?}",
                response.status()
            );
        };

        Ok(())
    }

    pub async fn deploy(&self, wasm: String) -> Result<(), Box<dyn std::error::Error>> {
        // Replace "input.wasm" with the path to your Wasm file.
        let mut input_file = File::open(wasm)?;

        let mut buffer = Vec::new();
        input_file.read_to_end(&mut buffer)?;
        println!("(Size of program is {})", buffer.len());

        let code = CodeUploadClient { code: Some(buffer) };

        // Convert the code object to JSON
        let json_code = serde_json::to_string(&code)?;

        // Define the URL for your POST request
        let url = format!("{}/zephyr_upload", &self.base_url);

        // Define the authorization header
        let authorization = format!("Bearer {}", &self.jwt);

        // Create a reqwest Client
        let client = reqwest::Client::new();

        // Make a POST request with the JSON data
        let response = client
            .post(url)
            .header("Content-Type", "application/json")
            .header("Authorization", authorization)
            .body(json_code)
            .send()
            .await
            .unwrap();

        if response.status().is_success() {
            println!("[+] Deployed was successful!");
        } else {
            println!(
                "[-] Request failed with status code: {:?}",
                response.status()
            );
        };

        Ok(())
    }
}

