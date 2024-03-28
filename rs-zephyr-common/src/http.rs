use serde::{Deserialize, Serialize};

/// A generic request object meant to be easily reusable by any HTTP client
/// request.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AgnosticRequest {
    pub body: Option<String>,
    pub url: String,
    pub method: Method,
    pub headers: Vec<(String, String)>
}

/// Methods currently supported are Get and Post.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum Method {
    Get,
    Post,
}

