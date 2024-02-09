use parser::{Column, Table};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;

use clap::{Parser, Subcommand};

mod parser;
mod error;

pub use parser::ZephyrProjectParser;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[arg(short, long)]
    pub jwt: String,

    #[arg(short, long)]
    pub local: Option<bool>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    Deploy {
        #[arg(short, long)]
        target: Option<String>,
    },
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

pub struct MercuryClient {
    pub base_url: String,
    pub jwt: String,
}

impl MercuryClient {
    pub fn new(base_url: String, jwt: String) -> Self {
        Self { base_url, jwt }
    }

    pub async fn new_table(
        &self,
        table: Table,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let columns = table.columns;
        let mut cols = Vec::new();

        for col in columns {
            cols.push(Column {
                name: col.name.to_string(),
                col_type: col.col_type.to_string(),
            });
        }

        let code = NewZephyrTableClient {
            table: Some(table.name),
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
        println!("Reading wasm {}", wasm);
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

