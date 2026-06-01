use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FetchOption {
    pub url: String,
    pub timeout_ms: Option<u64>,
    pub max_bytes: Option<usize>,
    pub accept: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchResult {
    pub content: String,
    pub media_type: String,
}

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("{0}")]
    HttpStatus(String),
    #[error("{0}")]
    Timeout(String),
    #[error("{0}")]
    Network(String),
    #[error("{0}")]
    Parse(String),
}

pub trait Fetcher: Send + Sync {
    fn fetch(&self, opt: FetchOption) -> anyhow::Result<FetchResult>;
}

#[cfg(test)]
#[path = "fetch_test.rs"]
mod fetch_test;
