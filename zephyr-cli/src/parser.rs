use std::{fs::File, io::Read, path::Path, process::Command};
use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::{error::ParserError, MercuryClient};


impl Config {
    fn tables(&self) -> Vec<Table> {
        self.tables.clone()
    }
}


#[derive(Deserialize, Serialize, Clone)]
pub struct Config {
    pub name: String,

    /// Tables that the poject is writing or reading.
    pub tables: Vec<Table>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct Table {
    pub name: String,
    pub columns: Vec<Column>
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Column {
    pub name: String,
    pub col_type: String
}

pub struct ZephyrProjectParser {
    config: Config,
    client: MercuryClient
}

impl ZephyrProjectParser {
    pub fn from_path<P: AsRef<Path>>(client: MercuryClient, path: P) -> Result<Self> {
        let project_definition = {
            let mut content = String::new();
            File::open(path)?.read_to_string(&mut content)?;

            content
        };

        let parser = Self {
            client,
            config: toml::from_str(&project_definition)?
        };

        Ok(parser)
    }

    pub fn build_wasm(&self) -> Result<()> {
        let output = Command::new("cargo")
        .args(&["build", "--release", "--target=wasm32-unknown-unknown"])
        .output()?;

        if !output.status.success() {
            let error = if !output.stdout.is_empty() {
                String::from_utf8_lossy(&output.stdout).to_string()
            } else {
                String::new()
            };

            return Err(ParserError::WasmBuildError(error).into())
        }

        Ok(())
    }

    pub async fn deploy_tables(&self) -> Result<()> {
        for table in self.config.tables() {
            if let Err(_) = self.client.new_table(table).await {
                return Err(ParserError::TableCreationError.into())
            };
        }

        Ok(())
    }

    pub async fn deploy_wasm(&self, target: Option<String>) -> Result<()> {
        let project_name = &self.config.name;
        let path = if let Some(target_dir) = target {
            format!("{}/{}.wasm", target_dir, project_name.replace('-', "_"))
        } else {
            format!("./target/wasm32-unknown-unknown/release/{}.wasm", project_name.replace('-', "_"))
        };
        
        if let Err(_) = self.client.deploy(path, true).await {
            return Err(ParserError::WasmDeploymentError.into());
        };

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::{Column, Config, Table};

    #[test]
    pub fn sample_config() {
        let config = Config {
            name: "zephyr-soroban-op-ratio".into(),
            tables: vec![Table {
                name: "opratio".into(),
                columns: vec![Column {
                    name: "soroban".into(),
                    col_type: "BYTEA".into() // only supported type as of now 
                }, Column {
                    name: "ratio".into(),
                    col_type: "BYTEA".into() // only supported type as of now 
                }]
            }]
        };

        println!("{}", toml::to_string(&config).unwrap());
    }
}
