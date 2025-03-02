use async_trait::async_trait;
use query::{get_query_generic, ResponseAfterLedger};

use super::EventFeed;

pub mod query;

pub struct MercuryClient {
    pub jwt: String,
    pub network: String
}

#[async_trait]
impl EventFeed for MercuryClient {
    // todo: add cursors
    async fn events(
        &self,
        contracts_ids: Vec<String>,
        topics: [Vec<String>; 4],
        start: i64,
    ) -> anyhow::Result<query::ResponseAfterLedger> {
        let jwt = &self.jwt;
        let topic1s = topics.get(0).cloned().unwrap_or(vec![]);
        let topic2s = topics.get(1).cloned().unwrap_or(vec![]);
        let topic3s = topics.get(2).cloned().unwrap_or(vec![]);
        let topic4s = topics.get(3).cloned().unwrap_or(vec![]);
        
        let client = reqwest::Client::new();
        let graphql_endpoint = if &self.network == "Public Global Stellar Network ; September 2015" {
            "https://mainnet.mercurydata.app:2083/graphql"
        } else {
            "https://api.mercurydata.app:2083/graphql"
        };

        let res = client
            .post(graphql_endpoint)
            .bearer_auth(jwt)
            .json(&get_query_generic(
                &contracts_ids,
                &topic1s,
                &topic2s,
                &topic3s,
                &topic4s,
                Some(start),
            ))
            .send()
            .await?;

        let resp: ResponseAfterLedger = res.json().await?;
        Ok(resp)
    }
}