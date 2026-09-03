use std::{str::FromStr, time::Duration};

use reqwest::{
    header::{HeaderMap, HeaderName},
    RequestBuilder, Response,
};
use rs_zephyr_common::http::{AgnosticRequest, Method};

pub struct WebhookJob {
    pub builder_cloned: Option<RequestBuilder>,
    pub response: Result<Response, reqwest::Error>,
    pub request: AgnosticRequest,
}

impl WebhookJob {
    pub fn build_request(request: AgnosticRequest) -> RequestBuilder {
        let client = reqwest::Client::new();
        let mut headers = HeaderMap::new();
        for (k, v) in &request.headers {
            headers.insert(HeaderName::from_str(&k).unwrap(), v.parse().unwrap());
        }

        let builder = match request.method {
            Method::Get => {
                let builder = client.get(&request.url).headers(headers);

                if let Some(body) = &request.body {
                    builder.body(body.clone())
                } else {
                    builder
                }
            }

            Method::Post => {
                let builder = client.post(&request.url).headers(headers);

                if let Some(body) = &request.body {
                    builder.body(body.clone())
                } else {
                    builder
                }
            }
        };
        let builder = builder.timeout(Duration::from_secs(10));

        builder
    }

    pub fn is_success(&self) -> bool {
        if let Ok(resp) = self.response.as_ref() {
            resp.status().is_success()
        } else {
            false
        }
    }

    pub fn inspect(&self) -> Option<String> {
        if let Ok(resp) = self.response.as_ref() {
            let inspection = format!(
                "Status: {}\nHeaders: {:?}\nRequest: {:?}",
                resp.status().as_str(),
                resp.headers(),
                self.request
            );
            Some(inspection)
        } else {
            None
        }
    }

    pub async fn handle_retry(&mut self, max_retries: u32) -> Result<u32, String> {
        // Should already be triggered
        // but keeping extra precaution.
        if self.is_success() {
            return Ok(0);
        }

        let mut attempts = 0;
        while attempts < max_retries {
            let backoff_duration = Duration::from_secs(2u64.pow(attempts));
            tokio::time::sleep(backoff_duration).await;

            if let Some(builder) = self.builder_cloned.take() {
                let new_response = builder.send().await;
                self.response = new_response;

                if self.is_success() {
                    return Ok(attempts);
                }
            } else {
                let new_builder = Self::build_request(self.request.clone());
                let new_response = new_builder.send().await;
                self.response = new_response;

                if self.is_success() {
                    return Ok(attempts);
                }
            }

            attempts += 1;
        }

        Err(self.inspect().unwrap_or("Reqwest error".into()))
    }
}
