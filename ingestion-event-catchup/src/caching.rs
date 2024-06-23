use anyhow::Result;
use persy::{Config, Persy};
use serde_json::json;

pub struct CacheClient {
    pub storage: String
}

impl CacheClient {
    pub fn new() -> Self {
        let path: String = "/tmp/mercury-dashboards.persy".into();
        let _ = Persy::create(&path);
        
        Self { storage: path }
    }

    pub fn get_cahched(&self, id: u32) -> Result<String> {
        let connection = Persy::open(&self.storage, Config::new())?;

        let mut response = json!({"error": "no cached dashboard available with this id."}).to_string();
        let mut count = 0;

        for (_, content) in connection.scan(id.to_string())? {
            if count > 1 {
                panic!("Fatal, got two dashboard versions");
            }
            response.clear();
            response.push_str(&String::from_utf8(content)?);
            
            count += 1;
        }

        Ok(response)
    }

    pub fn insert_or_update(&self, id: u32, content: &str) -> Result<()> {
        let connection = Persy::open(&self.storage, Config::new())?;
        let mut persy_id = None;
        for (id, _) in connection.scan(id.to_string())? {
            persy_id = Some(id);
        };

        if let Some(persy_id) = persy_id {
            let mut tx = connection.begin()?;
            tx.update(id.to_string(), &persy_id, content.as_bytes())?;
            tx.prepare()?.commit()?;
        } else {
            let mut tx = connection.begin()?;
            tx.create_segment(&id.to_string())?;
            tx.insert(id.to_string(), content.as_bytes())?;
            tx.prepare()?.commit()?;
        }

        Ok(())
    }
}